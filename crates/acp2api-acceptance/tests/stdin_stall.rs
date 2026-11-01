#![allow(
    clippy::disallowed_methods,
    reason = "Real-time safety caps must work while virtual time or the conductor is blocked"
)]
use acp2api_acceptance::{
    Scenario, Setup,
    client::{ApiCall, Call},
    journal_check, real_limit,
    setup::definitely_exited,
};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;

async fn observed(s: &Scenario, ready: impl Fn() -> bool) -> bool {
    let end = Instant::now() + real_limit(Duration::from_secs(3));
    while !ready() {
        if Instant::now() >= end {
            return false;
        }
        let hub = s.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(end))
            .await
            .unwrap();
    }
    true
}

struct Processes {
    runtime: u32,
    host: u32,
}

async fn require(s: &mut Scenario, ok: bool, message: &str) {
    if !ok {
        // Kill before asserting: a blocked conductor cannot run normal cleanup.
        s.kill().await;
        s.hub.close();
    }
    assert!(ok, "{message}");
}

async fn stalled(stream: bool) -> (Scenario, Call, Processes) {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let client = s.client().await.clone();
    let warmup = client
        .send(ApiCall::one_user_message(
            "POST /v1/messages",
            "Prepare an independent conversation before the stdin lifecycle check.",
        ))
        .finish(&mut s.time)
        .await;
    assert_eq!(warmup.status, 200);
    let server = s
        .hub
        .entries()
        .into_iter()
        .find(|entry| entry.event["kind"] == "hello" && entry.event["hello"]["role"] == "server")
        .unwrap()
        .event["hello"]
        .clone();
    let pid = u32::try_from(server["pid"].as_u64().unwrap()).unwrap();
    let host = u32::try_from(server["parent_pid"].as_u64().unwrap()).unwrap();
    let observer = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    s.hub.command(
        server["label"].as_str().unwrap(),
        json!({"op":"StopReadingStdinAfter", "contains":"\"method\":\"session/new\"", "notify":observer.local_addr().unwrap().to_string()}),
    );
    // 8 MiB exceeds the macOS and Windows pipe buffers after the fake reads 64 KiB.
    // The observer accepts once and captures 256 bytes; the hub caps captures at 16 MiB.
    let text = "stdin-lifecycle-prompt-content; ".repeat(300_000);
    let mut request = ApiCall::one_user_message("POST /v1/messages", &text[..8 * 1024 * 1024]);
    request.body["stream"] = json!(stream);
    let call = client.send(request);
    let mut prefix = [0; 256];
    tokio::time::timeout(real_limit(Duration::from_secs(5)), async {
        let (mut stream, _) = observer.accept().await.unwrap();
        stream.read_exact(&mut prefix).await.unwrap();
    })
    .await
    .expect("the fake must consume a partial prompt before the lifecycle check");
    assert!(
        std::str::from_utf8(&prefix)
            .unwrap()
            .contains("session/prompt")
    );
    if stream {
        assert!(observed(&s, || call.request_id().is_some()).await);
        assert_eq!(call.status(), Some(200));
    }
    assert!(!definitely_exited(pid));
    (s, call, Processes { runtime: pid, host })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_stdin_keeps_admission_responsive_and_recovers_on_a_new_process() {
    // Four HTTP requests. The observer accepts once and captures only 256 bytes;
    // the hub caps captures at 16 MiB, HTTP replies at 8 MiB, and the root at 128 MiB.
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let (mut s, first, processes) = stalled(false).await;
        let pid = processes.runtime;
        let client = s.client().await.clone();
        let origin = s.hub.now();

        // Do not settle before this observation: the defect itself holds the
        // conductor's RuntimeLine work open while it awaits the pipe.
        let catalog = client.model_names();
        let responsive = observed(&s, || catalog.wire().lock().unwrap().end_at.is_some()).await;
        let status = catalog.status();
        require(&mut s, responsive, "a stalled runtime stdin must not block GET /v1/models").await;
        assert_eq!(status, Some(200), "a stalled runtime stdin must not block GET /v1/models");
        let catalog = catalog.finish_current(&mut s.time).await;
        assert_eq!(catalog.json()["data"][0]["id"], "gemini-3.8-flash");

        s.time.advance_to(origin + 29_999).await;
        assert_eq!(first.status(), None, "the write deadline must allow the whole 30 s");
        assert!(!definitely_exited(pid));
        s.time.advance_to(origin + 30_000).await;
        assert!(observed(&s, || first.status().is_some()).await, "write deadline must end the request");
        let failed = first.finish_current(&mut s.time).await;
        assert_eq!(failed.status, 503);
        assert_eq!(failed.json()["error"], json!({"type":"api_error", "message":"acp2api: the runtime process failed, so acp2api ended the prompt turn."}));
        assert_eq!(failed.headers.get("retry-after").map(String::as_str), Some("30"));

        // macOS permits a 5 s process-stop grace; Windows stops the Job Object.
        s.time.advance_to(origin + 35_000).await;
        assert!(observed(&s, || definitely_exited(pid)).await, "the partial pipe's process must stop");
        let replacement = s
            .hub
            .entries()
            .into_iter()
            .rev()
            .find(|entry| entry.event["kind"] == "hello" && entry.event["hello"]["role"] == "server")
            .unwrap()
            .event["hello"]["pid"]
            .as_u64()
            .unwrap();
        assert_ne!(replacement, u64::from(pid), "a partial pipe must never be reused");
        let recovered = client
            .send(ApiCall::one_user_message("POST /v1/messages", "new conversation after stdin stall"))
            .finish(&mut s.time)
            .await;
        assert_eq!(recovered.status, 200);
        assert_eq!(recovered.json()["content"][0]["text"], "OK");
        s.finish(true).await;
    })
    .await
    .expect("stdin stall run cap: 30 s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disconnect_during_stalled_stdin_keeps_its_result_when_the_runtime_fails() {
    // Three HTTP requests, 30 s run cap, existing 8 MiB HTTP and 128 MiB root caps.
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let (mut s, call, processes) = stalled(true).await;
        let pid = processes.runtime;
        let id = call.request_id().unwrap();
        let origin = s.hub.now();
        drop(call);
        let advancing = tokio::time::timeout(
            real_limit(Duration::from_secs(3)),
            s.time.advance_to(origin + 30_000),
        )
        .await;
        require(
            &mut s,
            advancing.is_ok(),
            "disconnect and transport deadline must progress while stdin is stalled",
        )
        .await;
        let recorded = observed(&s, || {
            journal_check::terminal(&s)
                .iter()
                .any(|record| record["request_id"] == id && record["result"] == "disconnected")
        })
        .await;
        require(
            &mut s,
            recorded,
            "runtime failure must complete the stalled request as disconnected",
        )
        .await;
        s.time.advance_to(origin + 35_000).await;
        assert!(
            observed(&s, || definitely_exited(pid)).await,
            "the disconnected partial pipe must stop by the failure rule"
        );
        let recovered = s
            .client()
            .await
            .send(ApiCall::one_user_message(
                "POST /v1/messages",
                "Use a fresh process after the disconnected stdin stall.",
            ))
            .finish(&mut s.time)
            .await;
        assert_eq!(recovered.status, 200);
        assert_eq!(recovered.json()["content"][0]["text"], "OK");
        assert_eq!(
            journal_check::terminal(&s)
                .iter()
                .filter(|record| record["request_id"] == id)
                .count(),
            1
        );
        s.finish(true).await;
    })
    .await
    .expect("stalled stdin disconnect run cap: 30 s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_during_stalled_stdin_ends_the_request_and_stops_every_process() {
    // Two HTTP requests and one harness child; the fixture bounds captures.
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let (mut s, call, processes) = stalled(true).await;
        let Processes { runtime: pid, host } = processes;
        s.hub
            .command("normal", json!({"op":"StartHarness", "count":1}));
        assert!(
            observed(&s, || s
                .hub
                .entries()
                .iter()
                .any(|entry| entry.event["hello"]["role"] == "harness"))
            .await
        );
        let child = s
            .hub
            .entries()
            .into_iter()
            .find(|entry| entry.event["hello"]["role"] == "harness")
            .unwrap()
            .event["hello"]["pid"]
            .as_u64()
            .unwrap();
        let child = u32::try_from(child).unwrap();
        assert!(!definitely_exited(child));
        let origin = s.hub.now();
        s.hub.stop();
        let answered = observed(&s, || {
            call.events().iter().any(|event| event.event == "error")
        })
        .await;
        require(
            &mut s,
            answered,
            "shutdown must end the stalled request before the process-stop grace",
        )
        .await;
        let error = call
            .events()
            .into_iter()
            .find(|event| event.event == "error")
            .unwrap();
        let error: serde_json::Value = serde_json::from_str(&error.data).unwrap();
        assert_eq!(
            error["error"]["message"],
            "acp2api: acp2api stopped before the end of this request."
        );
        // The response can finish before the writer joins and process stop registers its grace.
        let stopping = observed(&s, || {
            definitely_exited(pid)
                || s.time.next_deadline() == Some(origin + acp2api::os::PROCESS_STOP_GRACE)
        })
        .await;
        require(
            &mut s,
            stopping,
            "shutdown must reach process stop without another clock wait",
        )
        .await;
        s.time
            .advance_to(origin + acp2api::os::PROCESS_STOP_GRACE)
            .await;
        s.stop().await;
        assert_eq!(
            s.hub.now(),
            origin + acp2api::os::PROCESS_STOP_GRACE,
            "shutdown must finish within the process-stop grace"
        );
        assert!(definitely_exited(host), "shutdown must reap the test host");
        assert!(
            definitely_exited(pid),
            "shutdown must leave no stalled runtime process"
        );
        assert!(
            definitely_exited(child),
            "shutdown must leave no harness child"
        );
        s.finish(true).await;
    })
    .await
    .expect("stalled stdin shutdown run cap: 30 s");
}
