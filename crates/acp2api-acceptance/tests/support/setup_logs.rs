use crate::{World, panel_tiles_support as tiles, places_support as queue};
use acp2api_acceptance::{
    TurnPlan, client::Call, content_check, endpoint_check, fake::ModelSpec, journal_check,
    system_check,
};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Default)]
pub struct SetupLogs {
    pub requests: Vec<Value>,
    pub read: Option<Call>,
    pub sessions_before: usize,
}
impl std::fmt::Debug for SetupLogs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupLogs").finish_non_exhaustive()
    }
}

pub async fn send(w: &mut World, endpoint: &str, field: Option<(&str, Value)>) {
    assert_eq!(endpoint, "POST /v1/chat/completions");
    assert!(
        w.setup_logs.requests.len() < 51,
        "setup/logs request cap: 51"
    );
    queue::start(w).await;
    let n = w.setup_logs.requests.len() + 1;
    w.scenario.time.advance_to(n as u64 * 1000).await;
    let marker = format!("PRIVATE-SETUP-LOGS-{n}");
    let mut call = endpoint_check::request(endpoint, &marker, false);
    let unknown = if let Some((field, value)) = field {
        call.body[field] = value;
        vec![field.to_owned()]
    } else {
        vec![]
    };
    w.request.updates.clear();
    w.request.plan = Some(
        TurnPlan::new()
            .after(Duration::from_millis(17))
            .chunk("OK")
            .usage(137, 1_048_576),
    );
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(answer.status, 200, "request reaches the HTTP seam");
    let prompts = w.scenario.fake().record().prompts();
    assert_eq!(prompts.len(), n, "one ACP prompt for each HTTP request");
    assert_eq!(prompts[n - 1].text, marker);
    let id = answer.headers["x-request-id"].clone();
    let session: String = prompts[n - 1].session.chars().take(12).collect();
    w.setup_logs.requests.push(json!({
        "time":format!("2026-10-18T00:00:{n:02}Z"),
        "request_id":id,"api":"chat.completions","model":"gemini-3.8-flash",
        "session":session,"session_use":"new","duration_ms":217,
        "input_tokens":137,"output_tokens":1,"result":"success",
        "error_code":null,"unknown_fields":unknown
    }));
    journal_check::wait_terminal(&w.scenario, n).await;
}

pub fn no_secrets(w: &World) {
    for line in journal_check::lines(&w.scenario) {
        assert!(
            !line.contains(&w.scenario.root.key) && !line.contains(&w.scenario.root.password),
            "FR-056 logs hold neither the local API key nor the management password (withheld)"
        );
    }
}

pub fn rows(html: &str) -> Vec<Vec<String>> {
    html.split("<tr")
        .skip(1)
        .map(|row| {
            let mut rest = row.split("</tr>").next().unwrap();
            let mut cells = Vec::new();
            while let Some(start) = rest.find("<td").into_iter().chain(rest.find("<th")).min() {
                let cell = &rest[start + 1..];
                let tag = if cell.starts_with("td") { "td" } else { "th" };
                let body = cell.split_once('>').unwrap().1;
                let (body, next) = body.split_once(&format!("</{tag}>")).unwrap();
                cells.push(tiles::visible(body));
                rest = next;
            }
            cells
        })
        .collect()
}

pub fn normalized(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn unknown(w: &World, name: &str, api: &str, count: usize) {
    no_secrets(w);
    let html = tiles::html(w);
    let rows = rows(tiles::tile(&html, "logs"));
    let matches: Vec<_> = rows
        .iter()
        .filter(|row| row.iter().any(|cell| cell == name) && row.iter().any(|cell| cell == api))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "FR-054 one unknown-field row per field and API"
    );
    let row = matches[0];
    assert!(
        row.contains(&count.to_string()),
        "FR-054 unknown-field count is {count}"
    );
    let time = w.setup_logs.requests.last().unwrap()["time"]
        .as_str()
        .unwrap();
    assert!(
        row.iter().any(|cell| cell == time),
        "FR-054 unknown-field last time uses the virtual clock: {time}"
    );
}

pub fn catalog(w: &World) {
    w.scenario.fake().catalog(&[ModelSpec {
        value: "gemini-3.8-flash".into(),
        name: "Gemini 3.8 Flash".into(),
        levels: vec!["low".into(), "medium".into(), "high".into()],
    }]);
}

pub async fn page(w: &mut World) {
    let before = system_check::requests(&w.scenario).len();
    w.panel.answer = Some(queue::view(w).await);
    assert_eq!(
        system_check::requests(&w.scenario).len(),
        before,
        "FR-054 a panel view sends no ACP request"
    );
}

pub fn snippets(w: &World) -> Vec<String> {
    no_secrets(w);
    let html = tiles::html(w);
    let tile = tiles::tile(&html, "setup");
    let blocks: Vec<_> = ["pre", "textarea"]
        .into_iter()
        .flat_map(|tag| {
            tile.split(&format!("<{tag}"))
                .skip(1)
                .map(move |part| {
                    let body = part
                        .split_once('>')
                        .unwrap()
                        .1
                        .split(&format!("</{tag}>"))
                        .next()
                        .unwrap();
                    let mut tag = false;
                    let text: String = body
                        .chars()
                        .filter(|c| match c {
                            '<' => {
                                tag = true;
                                false
                            }
                            '>' => {
                                tag = false;
                                false
                            }
                            _ => !tag,
                        })
                        .collect();
                    text.replace("&amp;", "&")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                        .replace("&quot;", "\"")
                        .replace("&#39;", "'")
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        !blocks.is_empty(),
        "FR-054 setup tile has copyable setup text"
    );
    blocks
}

pub fn setup(w: &World) -> String {
    snippets(w).join("\n")
}

pub fn assignment<'a>(text: &'a str, name: &str) -> &'a str {
    let values: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
            let (key, value) = line.split_once('=')?;
            (key.trim().trim_matches('"') == name).then(|| value.trim().trim_matches(['\'', '"']))
        })
        .collect();
    assert_eq!(values.len(), 1, "FR-054 setup assignment for {name}");
    values[0]
}

pub fn model(value: &str) {
    assert!(
        crate::model_names::NAMES.contains(&value),
        "FR-054 setup selects an offered model name: {value}"
    );
}

pub fn placeholder(value: &str, field: &str) {
    assert!(
        !value.is_empty()
            && (value.starts_with('<') && value.ends_with('>')
                || value.starts_with('[') && value.ends_with(']')
                || value.starts_with('{') && value.ends_with('}')
                || value.starts_with("YOUR_")
                || value.starts_with("REPLACE_")),
        "FR-054 {field} has a placeholder"
    );
}
