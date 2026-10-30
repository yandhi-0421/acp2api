#![allow(
    clippy::disallowed_methods,
    reason = "Real-time safety caps must work while the conductor is stuck in a pipe write"
)]
use acp2api_acceptance::{Scenario, Setup, client::ApiCall, real_limit, setup::definitely_exited};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;

async fn observed(s: &Scenario, ready: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + real_limit(Duration::from_secs(3));
    while !ready() {
        if Instant::now() >= deadline {
            return false;
        }
        let hub = s.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
    }
    true
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_stdin_keeps_admission_responsive_and_recovers_on_a_new_process() {
    // Four HTTP requests. The observer accepts once and captures only 256 bytes;
    // the hub caps captures at 16 MiB, HTTP replies at 8 MiB, and the root at 128 MiB.
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let client = s.client().await.clone();
        // Consume the session prepared at startup, so the next conversation
        // crosses session/new before its prompt enters the pipe.
        let warmup = client
            .send(ApiCall::one_user_message("POST /v1/messages", "warmup conversation"))
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
        let observer = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        s.hub.command(
            server["label"].as_str().unwrap(),
            json!({"op":"StopReadingStdinAfter", "contains":"\"method\":\"session/new\"", "notify":observer.local_addr().unwrap().to_string()}),
        );

        // 8 MiB exceeds anonymous-pipe buffers on macOS and Windows, even after
        // the fake consumes 64 KiB. It stays below the 32 MiB HTTP prompt cap.
        let request = ApiCall::one_user_message("POST /v1/messages", &"x".repeat(8 * 1024 * 1024));
        let first = client.send(request);
        let mut prefix = [0; 256];
        tokio::time::timeout(real_limit(Duration::from_secs(5)), async {
            let (mut socket, _) = observer.accept().await.unwrap();
            socket.read_exact(&mut prefix).await.unwrap();
        })
        .await
        .expect("fake must consume a partial prompt before it stops reading stdin");
        assert!(std::str::from_utf8(&prefix).unwrap().contains("session/prompt"));
        assert!(!definitely_exited(pid), "the stalled runtime must stay alive");
        let origin = s.hub.now();

        // Do not settle before this observation: the defect itself holds the
        // conductor's RuntimeLine work open while it awaits the pipe.
        let catalog = client.model_names();
        let responsive = observed(&s, || catalog.wire().lock().unwrap().end_at.is_some()).await;
        let status = catalog.status();
        if !responsive {
            s.kill().await;
            s.hub.close();
        }
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
