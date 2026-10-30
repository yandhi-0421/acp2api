#[path = "compatibility_facts.rs"]
mod facts;
use crate::fake_pipe::FakePipe;
use acp2api_acceptance::real_limit;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn script_supplies_oauth_mcp_permission_cancel_load_and_dynamic_reply_text() {
    // Three MCP requests, 16 KiB each, a 5 s base run cap, and no log file.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let preparing = std::sync::Arc::new(tokio::sync::Notify::new());
    let resume = std::sync::Arc::new(tokio::sync::Notify::new());
    let entered = preparing.clone();
    let released = resume.clone();
    let endpoint = tokio::spawn(async move {
        tokio::time::timeout(real_limit(Duration::from_secs(5)), async move {
            for index in 0..3 {
                let (stream, _) = listener.accept().await.unwrap();
                if index == 0 {
                    entered.notify_one();
                    released.notified().await;
                }
                let mut stream = BufReader::new(stream);
                let mut length = 0;
                let mut size = 0;
                loop {
                    let mut line = String::new();
                    let n = (&mut stream).take(16 * 1024 + 1).read_line(&mut line).await.unwrap();
                    size += n;
                    assert!(n > 0 && size <= 16 * 1024, "MCP request head cap");
                    if line == "\r\n" { break; }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                assert!(size + length <= 16 * 1024, "MCP request cap");
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
                let call: Value = serde_json::from_slice(&body).unwrap();
                let response = json!({"jsonrpc":"2.0","id":call["id"],"result":{"tools":[{"name":"lookup","description":"Compatibility probe","inputSchema":{"type":"object","properties":{"value":{"type":"string"}}}}]}}).to_string();
                stream.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
            }
        }).await.expect("MCP fixture run cap");
    });
    let mut pipe = FakePipe::new();
    facts::configure(&pipe.scenario.fake(), facts::echo);
    pipe.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    let initialized = pipe.stdout().await;
    assert_eq!(initialized["result"]["protocolVersion"], 1);
    assert_eq!(initialized["result"]["agentInfo"]["version"], "1.4.0");
    assert_eq!(
        initialized["result"]["authMethods"][0]["id"],
        "oauth-personal"
    );
    pipe.send(json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{"mcpServers":[{"name":"check","url":url}],"_meta":{"antigravity":{"enabledTools":[],"eagerMcpServers":["check"],"systemPrompt":{"append":"The codeword is FROST-62."}}}}})).await;
    let session = pipe.stdout().await["result"]["sessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    pipe.prompt(&session, 3).await;
    tokio::time::timeout(real_limit(Duration::from_secs(5)), preparing.notified())
        .await
        .expect("fake reply preparation cap");
    let acknowledged_while_preparing = pipe.scenario.hub.counts().stdin_got;
    resume.notify_one();
    assert!(
        pipe.stdout().await["params"]["update"]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("FROST-62")
    );
    assert_eq!(pipe.stdout().await["id"], 3);
    endpoint.await.unwrap();
    pipe.prompt(&session, 4).await;
    let permission = pipe.stdout().await;
    assert_eq!(permission["method"], "session/request_permission");
    assert_eq!(permission["params"]["toolCall"]["title"], "Run lookup?");
    assert_eq!(
        permission["params"]["toolCall"]["rawInput"],
        json!({"value":"checked text"})
    );
    pipe.send(
        json!({"jsonrpc":"2.0","id":permission["id"],"result":{"outcome":{"outcome":"cancelled"}}}),
    )
    .await;
    pipe.cancel(&session).await;
    assert_eq!(pipe.stdout().await["params"]["update"]["status"], "failed");
    let cancelled = pipe.stdout().await;
    assert_eq!(cancelled["id"], 4);
    assert_eq!(cancelled["result"]["stopReason"], "cancelled");
    pipe.send(
        json!({"jsonrpc":"2.0","id":5,"method":"session/load","params":{"sessionId":session}}),
    )
    .await;
    assert_eq!(pipe.stdout().await["result"]["sessionId"], session);
    pipe.send(json!({"jsonrpc":"2.0","id":6,"method":"session/prompt","params":{"sessionId":session,"prompt":[{"type":"text","text":"Tool result: RIVER-62"}]}})).await;
    let text = pipe.stdout().await["params"]["update"]["content"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(text.contains("FROST-62") && text.contains("RIVER-62"));
    assert_eq!(pipe.stdout().await["id"], 6);
    assert_eq!(
        pipe.scenario.fake().record().offered_tools()[0]["result"]["tools"][0]["name"],
        "lookup"
    );
    pipe.finish().await;
    assert_eq!(
        acknowledged_while_preparing, 2,
        "initialize and session/new are complete, but the first prompt still needs its reply"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scripted_prompt_preserves_the_gate_and_builtin_failure_evidence() {
    let mut pipe = FakePipe::new();
    facts::configure(&pipe.scenario.fake(), facts::held);
    pipe.send(json!({"jsonrpc":"2.0","id":1,"method":"session/new","params":{"_meta":{"antigravity":{"systemPrompt":{"append":"GATED-62"}}}}})).await;
    let session = pipe.stdout().await["result"]["sessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    pipe.prompt(&session, 2).await;
    tokio::time::timeout(real_limit(Duration::from_secs(2)), async {
        while !pipe.scenario.hub.entries().iter().any(|entry| {
            entry.event["kind"] == "gate" && entry.event["name"] == "compatibility-check"
        }) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("script gate observation cap");
    pipe.scenario.fake().open_gate("compatibility-check");
    assert!(
        pipe.stdout().await["params"]["update"]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("GATED-62")
    );
    assert_eq!(pipe.stdout().await["id"], 2);
    pipe.finish().await;

    let mut pipe = FakePipe::new();
    facts::failure(&pipe.scenario.fake());
    pipe.prompt("check-session", 3).await;
    let update = pipe.stdout().await;
    assert_eq!(update["params"]["update"]["title"], "view_file");
    assert_eq!(update["params"]["update"]["status"], "completed");
    assert_eq!(pipe.stdout().await["id"], 3);
    pipe.scenario
        .fake()
        .next_prompt(acp2api_acceptance::TurnPlan::new().chunk("restored reply"));
    pipe.prompt("restored-session", 4).await;
    assert_eq!(
        pipe.stdout().await["params"]["update"]["content"]["text"],
        "restored reply"
    );
    assert_eq!(pipe.stdout().await["id"], 4);
    pipe.finish().await;
}
