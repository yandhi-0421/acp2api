use crate::real_limit;
use crate::{Scenario, TurnPlan, endpoint_check};
use acp2api::journal::{Api, RequestRecord, ResultKind, SessionUse, Writer};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub fn wall(now: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(1_792_281_600_000 + now)
}
pub fn files(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "log"))
        .collect();
    files.sort();
    files
}
pub fn lines(s: &Scenario) -> Vec<String> {
    files(&s.root.path.join("data/logs"))
        .into_iter()
        .flat_map(|p| {
            fs::read_to_string(p)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}
pub fn records(s: &Scenario) -> Vec<Value> {
    lines(s)
        .iter()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
pub fn terminal(s: &Scenario) -> Vec<Value> {
    s.hub
        .entries()
        .iter()
        .filter_map(|e| {
            e.event["text"]
                .as_str()
                .and_then(|l| l.strip_prefix("acp2api: "))
                .and_then(|l| serde_json::from_str::<Value>(l).ok())
        })
        .filter(|v| v.get("request_id").is_some())
        .collect()
}
pub async fn wait_terminal(s: &Scenario, count: usize) {
    let end = Instant::now() + real_limit(Duration::from_secs(5));
    while terminal(s).len() < count && Instant::now() < end {
        let hub = s.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(end))
            .await
            .unwrap();
    }
}
pub async fn send(
    s: &mut Scenario,
    path: &str,
    model: &str,
    field: &str,
    marker: &str,
    usage: u64,
    delay: u64,
) -> Value {
    assert!(
        marker.len() <= 256
            && field.len() <= 256
            && model.len() <= 256
            && usage <= 1_000_000
            && delay <= 1000,
        "probe input caps"
    );
    s.fake().next_prompt(
        TurnPlan::new()
            .after(Duration::from_millis(delay))
            .chunk("META-REPLY")
            .usage(usage, 1_048_576)
            .end("end_turn"),
    );
    let mut call = endpoint_check::request(path, marker, false);
    call.body["model"] = model.into();
    call.body[field] = marker.into();
    let response = s.client().await.send(call).finish(&mut s.time).await;
    assert_eq!(response.status, 200, "metadata request reached production");
    let session = s
        .fake()
        .record()
        .prompts()
        .into_iter()
        .rev()
        .find(|p| p.text.contains(marker))
        .expect("actual ACP prompt for this request")
        .session;
    serde_json::json!({"session":session,"request_id":response.headers.get("x-request-id").cloned(),"api": match path {"POST /v1/messages"=>"messages", "POST /v1/responses"=>"responses", _=>"chat.completions"}, "model":model, "field":field, "usage":usage, "delay":delay})
}
pub fn verify(s: &Scenario, expected: &[Value]) {
    let records = records(s);
    let terminal = terminal(s);
    assert_eq!(
        records.len(),
        expected.len(),
        "FR-056 exactly one request record per request"
    );
    assert_eq!(
        terminal.len(),
        expected.len(),
        "FR-002 exactly one terminal line per request"
    );
    for expected in expected {
        let id = expected["request_id"]
            .as_str()
            .expect("actual HTTP request ID");
        assert!(id.starts_with("req_") && id.len() > 4);
        let found: Vec<_> = records.iter().filter(|r| r["request_id"] == id).collect();
        assert_eq!(found.len(), 1, "request correlation");
        let r = found[0];
        assert_eq!(
            terminal
                .iter()
                .filter(|t| t["request_id"] == id)
                .collect::<Vec<_>>(),
            vec![r],
            "terminal and file metadata agree"
        );
        assert_eq!(r["api"], expected["api"]);
        assert_eq!(r["model"], expected["model"]);
        let session = expected["session"]
            .as_str()
            .expect("captured per-request ACP session");
        assert_eq!(
            r["session"],
            session.chars().take(12).collect::<String>(),
            "actual ACP session identity"
        );
        assert_eq!(
            r["session_use"], "new",
            "no history continuation claim before #47"
        );
        assert_eq!(
            r["duration_ms"],
            expected["delay"].as_u64().unwrap() + 200,
            "monotonic request duration"
        );
        assert_eq!(r["input_tokens"], expected["usage"]);
        assert_eq!(r["output_tokens"], 3);
        assert_eq!(r["result"], "success");
        assert!(r["error_code"].is_null());
        assert_eq!(r["unknown_fields"], serde_json::json!([expected["field"]]));
        assert_eq!(
            r["time"], "2026-10-18T00:00:00Z",
            "injected UTC wall time to seconds"
        );
    }
}
#[derive(Debug)]
pub struct Retention {
    pub written: usize,
    pub first: PathBuf,
    pub retained: Vec<(PathBuf, u64)>,
}
pub fn stress(folder: &Path, marker: &str) -> Retention {
    assert!(
        marker.len() <= 64
            && marker
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "stress marker cap"
    );
    fs::create_dir_all(folder).unwrap();
    let end = Instant::now() + real_limit(Duration::from_secs(12));
    let mut writer = Writer::open(folder, wall(0)).unwrap();
    let mut written = 0;
    let mut first = None;
    for n in 0..60 {
        assert!(
            Instant::now() < end,
            "production journal stress cap: 12 s, 60 records, 61 MB input, 0 terminal bytes"
        );
        let record = RequestRecord {
            time: "2026-10-18T00:00:00Z".into(),
            request_id: format!("stress-{marker}-{n}"),
            api: Api::Chat,
            model: None,
            session: None,
            session_use: Some(SessionUse::New),
            duration_ms: n,
            input_tokens: Some(137),
            output_tokens: Some(3),
            result: ResultKind::Success,
            error_code: None,
            unknown_fields: vec![format!("field-{marker}-{}", "x".repeat(1_000_000))],
        };
        written += writer.write(&record).unwrap();
        if n == 0 {
            first = files(folder).first().cloned();
        }
    }
    let retained = files(folder)
        .into_iter()
        .map(|p| {
            let len = fs::metadata(&p).unwrap().len();
            (p, len)
        })
        .collect();
    Retention {
        written,
        first: first.unwrap_or_else(|| folder.join("absent-writer-file")),
        retained,
    }
}
pub fn verify_stress(r: &Retention, marker: &str) {
    assert!(
        !r.retained.is_empty(),
        "FR-057 retained files must come from the production writer"
    );
    assert!(
        r.written >= 60_000_000 && r.written <= 61_000_000,
        "FR-057 real writer crossed 60 MB"
    );
    assert!(
        r.retained.iter().map(|(_, n)| n).sum::<u64>() <= 50_000_000,
        "FR-057 total decimal byte cap"
    );
    let mut ids = Vec::new();
    for (path, len) in &r.retained {
        assert!(*len <= 10_000_000, "FR-057 each file decimal byte cap");
        for line in fs::read_to_string(path).unwrap().lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            let prefix = format!("stress-{marker}-");
            ids.push(
                v["request_id"]
                    .as_str()
                    .unwrap()
                    .strip_prefix(&prefix)
                    .expect("retained record came from writer")
                    .parse::<u64>()
                    .unwrap(),
            );
            assert!(
                v["unknown_fields"][0]
                    .as_str()
                    .unwrap()
                    .starts_with(&format!("field-{marker}-"))
            );
        }
    }
    assert!(!r.first.exists(), "FR-057 oldest writer file deleted first");
    assert!(ids.first().is_some_and(|n| *n > 0));
    assert_eq!(ids.last(), Some(&59));
    assert!(
        ids.windows(2).all(|p| p[1] == p[0] + 1),
        "oldest deletion leaves consecutive newest records"
    );
}
