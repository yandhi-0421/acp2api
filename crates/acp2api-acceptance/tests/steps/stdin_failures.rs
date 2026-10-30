use crate::{World, runtime_support as process};
use acp2api_acceptance::{client::ApiCall, real_limit, setup::definitely_exited};
use cucumber::when;
use serde_json::json;
use std::time::Duration;
use tokio::io::AsyncReadExt;

#[when("three runtime processes stop reading stdin at 0 minutes, 4 minutes, and 9 minutes")]
async fn three_stalls(w: &mut World) {
    // Seven HTTP requests including the Then step. Each observer accepts once,
    // reads 256 bytes, and uses the existing 16 MiB hub and 128 MiB root caps.
    tokio::time::timeout(real_limit(Duration::from_secs(15)), async {
        let client = w.scenario.client().await.clone();
        let origin = w.scenario.hub.now();
        for (index, minute) in [0, 4, 9].into_iter().enumerate() {
            w.scenario.time.advance_to(origin + minute * 60_000).await;
            let warmup = client
                .send(ApiCall::one_user_message(
                    "POST /v1/messages",
                    &format!("Prepare a separate conversation for stdin failure {index}."),
                ))
                .finish(&mut w.scenario.time)
                .await;
            assert_eq!(warmup.status, 200, "each earlier stall leaves a usable runtime");
            let servers = process::servers(w);
            assert_eq!(servers.len(), index + 1, "each stall uses a new runtime");
            let server = servers.last().unwrap();
            let pid = u32::try_from(server["pid"].as_u64().unwrap()).unwrap();
            let observer = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            w.scenario.hub.command(
                server["label"].as_str().unwrap(),
                json!({"op":"StopReadingStdinAfter", "contains":"\"method\":\"session/new\"", "notify":observer.local_addr().unwrap().to_string()}),
            );
            // Exceed the pipe buffer after the fake consumes 64 KiB of this frame.
            let text = format!("stdin-failure-{index}; ").repeat(500_000);
            let sent_at = w.scenario.hub.now();
            let call = client.send(ApiCall::one_user_message(
                "POST /v1/messages",
                &text[..8 * 1024 * 1024],
            ));
            let (mut stream, _) = observer.accept().await.unwrap();
            let mut prefix = [0; 256];
            stream.read_exact(&mut prefix).await.unwrap();
            assert!(std::str::from_utf8(&prefix).unwrap().contains("session/prompt"));
            assert!(!definitely_exited(pid), "the runtime stays alive with stalled stdin");
            w.scenario.time.advance_to(sent_at + 30_000).await;
            let failed = call.finish_current(&mut w.scenario.time).await;
            process::error(
                &failed,
                503,
                "acp2api: the runtime process failed, so acp2api ended the prompt turn.",
            );
            w.scenario.time.advance_to(sent_at + 35_000).await;
            assert!(definitely_exited(pid), "the stalled runtime must stop");
        }
    })
    .await
    .expect("three stdin failures run cap: 15 s");
}
