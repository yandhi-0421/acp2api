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

async fn stalled() -> (Scenario, Call, u32, u32) {
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
    request.body["stream"] = json!(true);
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
    assert!(observed(&s, || call.request_id().is_some()).await);
    assert_eq!(call.status(), Some(200));
    assert!(!definitely_exited(pid));
    (s, call, pid, host)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_disconnect_during_stalled_stdin_keeps_its_result_when_the_runtime_fails() {
    // Three HTTP requests, 30 s run cap, existing 8 MiB HTTP and 128 MiB root caps.
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let (mut s, call, pid, _) = stalled().await;
        let id = call.request_id().unwrap();
        let origin = s.hub.now();
        drop(call);
        let advancing = tokio::time::timeout(
            real_limit(Duration::from_secs(3)),
            s.time.advance_to(origin + 30_000),
        )
        .await;
        if advancing.is_err() {
            s.kill().await;
            s.hub.close();
        }
        assert!(
            advancing.is_ok(),
            "disconnect and transport deadline must progress while stdin is stalled"
        );
        let recorded = observed(&s, || {
            journal_check::terminal(&s)
                .iter()
                .any(|record| record["request_id"] == id && record["result"] == "disconnected")
        })
        .await;
        if !recorded {
            s.kill().await;
            s.hub.close();
        }
        assert!(
            recorded,
            "runtime failure must complete the stalled request as disconnected"
        );
        assert_eq!(s.hub.now(), origin + 30_000);
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
        let (mut s, call, pid, host) = stalled().await;
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
        if !answered {
            s.kill().await;
            s.hub.close();
        }
        assert!(
            answered,
            "shutdown must end the stalled request before the process-stop grace"
        );
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
        s.time.advance_to(origin + 5000).await;
        s.stop().await;
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
