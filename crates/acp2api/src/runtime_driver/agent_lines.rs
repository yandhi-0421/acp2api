use super::*;
use crate::conversation::PromptBlock;
use std::{
    io,
    sync::Mutex,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, DuplexStream, ReadBuf};

const DEADLINE: Duration = Duration::from_secs(10);
const RUN_CAP: Duration = Duration::from_secs(30);
const WRITE_CAP: usize = 16;

struct Agent {
    stdout: DuplexStream,
    stderr: DuplexStream,
    stdin: BufReader<DuplexStream>,
    writes: usize,
}
impl Agent {
    fn count_write(&mut self) {
        self.writes += 1;
        assert!(
            self.writes <= WRITE_CAP,
            "fake ACP write cap: {WRITE_CAP} lines"
        );
    }
    async fn stdout(&mut self, line: Value) {
        self.count_write();
        tokio::time::timeout(
            DEADLINE,
            self.stdout.write_all(format!("{line}\n").as_bytes()),
        )
        .await
        .expect("a stdout write")
        .unwrap();
    }
    async fn stderr(&mut self, bytes: &[u8]) {
        self.count_write();
        tokio::time::timeout(DEADLINE, self.stderr.write_all(bytes))
            .await
            .expect("a stderr write")
            .unwrap();
    }
    async fn stdin(&mut self) -> Value {
        let mut line = String::new();
        tokio::time::timeout(DEADLINE, self.stdin.read_line(&mut line))
            .await
            .expect("a line on stdin")
            .unwrap();
        serde_json::from_str(&line).unwrap()
    }
}
struct Broken;
impl AsyncRead for Broken {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::other("broken stdout")))
    }
}
struct OneChild(Mutex<Option<Launched>>);
impl ProcessLauncher for OneChild {
    fn launch(&self, _: LaunchSpec) -> Result<Launched, LaunchError> {
        Ok(self.0.lock().unwrap().take().expect("one fake ACP process"))
    }
}
struct NoTree;
impl ProcessTree for NoTree {
    fn pid(&self) -> u32 {
        std::process::id()
    }
    fn exited(&mut self) -> BoxFuture<'_, ExitStatus> {
        Box::pin(std::future::pending())
    }
    fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}
struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn result(id: u64, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
fn chunk(text: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}})
}
async fn start(before_session: Vec<Value>) -> (Runtime, Agent, Root) {
    let suffix = crate::random_hex::<8>().unwrap();
    let root = Root(
        Environment::from_process()
            .temp
            .join(format!("acp2api-agent-lines-{suffix}")),
    );
    let (stdout, stdout_agent) = tokio::io::duplex(64 * 1024);
    let (stderr, stderr_agent) = tokio::io::duplex(64 * 1024);
    let (stdin, stdin_agent) = tokio::io::duplex(64 * 1024);
    let mut agent = Agent {
        stdout: stdout_agent,
        stderr: stderr_agent,
        stdin: BufReader::new(stdin_agent),
        writes: 0,
    };
    agent
        .stdout(result(
            1,
            json!({"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}),
        ))
        .await;
    for line in before_session {
        agent.stdout(line).await;
    }
    agent
        .stdout(result(
            2,
            json!({"sessionId":"s1","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"},{"value":"other","name":"Other"}]}]}),
        ))
        .await;
    let mut ports = Ports::production();
    ports.launcher = Arc::new(OneChild(Mutex::new(Some(Launched {
        stdin: Box::pin(stdin),
        stdout: Box::pin(stdout.chain(Broken)),
        stderr: Box::pin(stderr),
        tree: Box::new(NoTree),
    }))));
    let runtime = tokio::time::timeout(
        DEADLINE,
        Runtime::start(
            &RuntimeStart {
                stopped: tokio::sync::watch::channel(false).1,
                clock: ports.clock.clone(),
                launcher: ports.launcher.clone(),
                release: ports.release.clone(),
                data: root.0.clone(),
                temporary: crate::account::Temporary::fixture(&root.0),
                tool_sets: mcp::ToolSets::new(1),
            },
            "1.4.0",
        ),
    )
    .await
    .expect("runtime start")
    .unwrap()
    .0;
    assert_eq!(agent.stdin().await["method"], "initialize");
    assert_eq!(agent.stdin().await["method"], "session/new");
    (runtime, agent, root)
}
async fn next(runtime: &mut Runtime) -> String {
    let (event, _hold) = tokio::time::timeout(DEADLINE, runtime.next())
        .await
        .expect("a runtime event");
    match event {
        RuntimeEvent::Turn { update, .. } => match update {
            TurnUpdate::Activity => "activity".into(),
            TurnUpdate::Text(text) => format!("text {text}"),
            TurnUpdate::ThoughtDropped => "thought dropped".into(),
            TurnUpdate::Usage { used, .. } => format!("usage {used}"),
            TurnUpdate::Permission { call, .. } => format!("permission {call:?}"),
            TurnUpdate::RefusedPermission { name, .. } => format!("permission of {name}"),
            TurnUpdate::UnpermittedTool(name) => format!("unpermitted {name}"),
            TurnUpdate::PromptEnded { .. } => "prompt ended".into(),
            TurnUpdate::Failed(error) => format!("failed {error}"),
        },
        RuntimeEvent::Prepared { .. } => "prepared".into(),
        RuntimeEvent::Retired { .. } => "retired".into(),
        RuntimeEvent::Failed(error) => format!("failed {error}"),
        RuntimeEvent::Stderr => "stderr".into(),
        RuntimeEvent::SignInAddress(_) => "sign-in address".into(),
        RuntimeEvent::Authenticated(_) => "authenticated".into(),
        RuntimeEvent::UpstreamError { error, .. } => format!(
            "upstream {}",
            error.code.map_or(error.status, |code| code.to_string())
        ),
        RuntimeEvent::Other => "other".into(),
        RuntimeEvent::Closed => "closed".into(),
        RuntimeEvent::WriteFailed(reason) => format!("write failed: {reason}"),
    }
}
async fn turn(runtime: &mut Runtime) -> Vec<String> {
    let mut events = Vec::new();
    while events.last().is_none_or(|last| last != "prompt ended") {
        assert!(events.len() < 8, "fake ACP event cap: {events:?}");
        let event = next(runtime).await;
        if event != "other" {
            events.push(event);
        }
    }
    events
}
async fn capped(test: impl std::future::Future<Output = ()>) {
    tokio::time::timeout(RUN_CAP, test)
        .await
        .expect("fake ACP run cap: 30 s");
}

#[tokio::test]
async fn a_config_option_that_stays_different_fails_without_a_retry() {
    capped(async {
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        runtime.start_configured_prompt(crate::conversation::PromptRequest {
            blocks: vec![PromptBlock::Text("Hello".into())],
            model: "other".into(),
            level: None,
        }, "s1").await.unwrap();
        let option = agent.stdin().await;
        assert_eq!(option["method"], "session/set_config_option");
        assert_eq!(option["params"], json!({"sessionId":"s1","configId":"model","value":"other"}));
        agent.stdout(result(option["id"].as_u64().unwrap(), json!({"configOptions":[
            {"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"},{"value":"other","name":"Other"}]}
        ]}))).await;
        let event = next(&mut runtime).await;
        assert_eq!(event, "failed acp2api: the official runtime did not apply the model value other, so acp2api ended the prompt turn.", "M-29 names the unapplied option and requested value");
    }).await;
}

#[tokio::test]
async fn model_configuration_uses_the_returned_level_without_losing_another_turn() {
    capped(async {
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        runtime.start_prompt(vec![PromptBlock::Text("Other turn".into())], "s1").await.unwrap();
        let other = agent.stdin().await;
        runtime.request_session("target".into(), &[], None, None).await.unwrap();
        let create = agent.stdin().await;
        agent.stdout(result(create["id"].as_u64().unwrap(), json!({"sessionId":"s2","models":{"currentModelId":"m-high","availableModels":[{"modelId":"m-high","name":"M high"},{"modelId":"other-high","name":"Other high"},{"modelId":"other-medium","name":"Other medium"}]},"configOptions":[
            {"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"},{"value":"other","name":"Other"}]},
            {"id":"thinking_level","name":"Thinking level","type":"select","currentValue":"high","options":[{"value":"medium","name":"Medium"},{"value":"high","name":"High"}]}
        ]}))).await;
        assert_eq!(next(&mut runtime).await, "prepared");
        runtime.start_configured_prompt(crate::conversation::PromptRequest {
            blocks: vec![PromptBlock::Text("Next".into())],
            model: "other".into(),
            level: Some(crate::conversation::ThinkingLevel::High),
        }, "s2").await.unwrap();
        let model = agent.stdin().await;
        assert_eq!(model["params"], json!({"sessionId":"s2","configId":"model","value":"other"}));
        agent.stdout(chunk("Other reply")).await;
        assert_eq!(next(&mut runtime).await, "text Other reply");
        agent.stdout(result(model["id"].as_u64().unwrap(), json!({"configOptions":[
            {"id":"thinking_level","name":"Thinking level","type":"select","currentValue":"medium","options":[{"value":"medium","name":"Medium"},{"value":"high","name":"High"}]}
        ]}))).await;
        assert_eq!(next(&mut runtime).await, "other");
        let level = agent.stdin().await;
        assert_eq!(level["method"], "session/set_config_option", "FR-013 applies the requested level after the model changes it");
        assert_eq!(level["params"], json!({"sessionId":"s2","configId":"thinking_level","value":"high"}));
        agent.stdout(result(level["id"].as_u64().unwrap(), json!({}))).await;
        assert_eq!(next(&mut runtime).await, "other");
        let prompt = agent.stdin().await;
        assert_eq!(prompt["method"], "session/prompt");
        assert_eq!(prompt["params"], json!({"sessionId":"s2","prompt":[{"type":"text","text":"Next"}]}));
        agent.stdout(result(other["id"].as_u64().unwrap(), json!({"stopReason":"end_turn"}))).await;
        agent.stdout(result(prompt["id"].as_u64().unwrap(), json!({"stopReason":"end_turn"}))).await;
        assert_eq!(turn(&mut runtime).await, ["prompt ended"]);
        assert_eq!(turn(&mut runtime).await, ["prompt ended"]);
    }).await;
}

#[tokio::test]
async fn unknown_or_missing_stop_reasons_end_the_turn_normally() {
    capped(async {
        for response in [json!({"stopReason":"a_future_stop_reason"}), json!({})] {
            let (mut runtime, mut agent, _root) = start(Vec::new()).await;
            runtime
                .start_prompt(vec![PromptBlock::Text("Hi".into())], "s1")
                .await
                .unwrap();
            agent.stdout(chunk("Hello")).await;
            agent.stdout(result(3, response)).await;
            assert_eq!(
                [next(&mut runtime).await, next(&mut runtime).await],
                ["text Hello", "prompt ended"]
            );
        }
    })
    .await;
}

#[tokio::test]
async fn stderr_bytes_do_not_fail_a_turn_and_a_stdout_read_error_closes_the_runtime() {
    capped(async {
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        agent.stderr(b"\xff\xfe code page 936\n").await;
        agent.stderr(b"next stderr line\n").await;
        assert_eq!(next(&mut runtime).await, "stderr");
        assert_eq!(next(&mut runtime).await, "stderr");
        runtime
            .start_prompt(vec![PromptBlock::Text("Hi".into())], "s1")
            .await
            .unwrap();
        agent.stdout(chunk("Hello")).await;
        agent
            .stdout(result(3, json!({"stopReason":"end_turn"})))
            .await;
        assert_eq!(turn(&mut runtime).await, ["text Hello", "prompt ended"]);
        drop(agent.stdout);
        drop(agent.stderr);
        assert_eq!(next(&mut runtime).await, "closed");
    })
    .await;
}

#[tokio::test]
async fn an_unknown_session_update_counts_as_activity_without_reply_content() {
    capped(async {
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        runtime
            .start_prompt(vec![PromptBlock::Text("Hi".into())], "s1")
            .await
            .unwrap();
        agent
            .stdout(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"a_later_kind"}}}))
            .await;
        agent.stdout(chunk("Hello")).await;
        agent
            .stdout(result(3, json!({"stopReason":"end_turn"})))
            .await;
        assert_eq!(turn(&mut runtime).await, ["activity", "text Hello", "prompt ended"]);
    })
    .await;
}

#[tokio::test]
async fn an_agent_request_with_an_unknown_method_gets_32601_and_is_no_reply() {
    capped(async {
        let request = |id: u64| json!({"jsonrpc":"2.0","id":id,"method":"x/unknown","params":{}});
        let (mut runtime, mut agent, _root) = start(vec![request(2)]).await;
        assert_eq!(
            agent.stdin().await,
            json!({"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found"}})
        );
        runtime
            .start_prompt(vec![PromptBlock::Text("Hi".into())], "s1")
            .await
            .unwrap();
        assert_eq!(agent.stdin().await["method"], "session/prompt");
        agent.stdout(request(3)).await;
        agent.stdout(chunk("Hello")).await;
        agent
            .stdout(result(3, json!({"stopReason":"end_turn"})))
            .await;
        assert_eq!(turn(&mut runtime).await, ["text Hello", "prompt ended"]);
        assert_eq!(
            agent.stdin().await,
            json!({"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"Method not found"}})
        );
    })
    .await;
}

#[tokio::test]
async fn a_permission_request_is_a_permission_event_and_not_refused() {
    capped(async {
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        runtime
            .start_prompt(vec![PromptBlock::Text("Hi".into())], "s1")
            .await
            .unwrap();
        assert_eq!(agent.stdin().await["method"], "session/prompt");
        agent
            .stdout(json!({"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{"sessionId":"s1","toolCall":{"toolCallId":"c1","title":"Run lookup?","rawInput":{"a":1}},"options":[]}}))
            .await;
        agent
            .stdout(result(3, json!({"stopReason":"end_turn"})))
            .await;
        assert_eq!(
            turn(&mut runtime).await,
            ["permission of lookup", "prompt ended"]
        );
    })
    .await;
}

#[allow(
    clippy::disallowed_methods,
    reason = "The test calls only its loopback MCP listener with a real-time cap."
)]
async fn listed_tools(address: std::net::SocketAddr, url: &str) -> (u16, Value) {
    tokio::time::timeout(DEADLINE, async {
        let secret = url.rsplit('/').next().unwrap();
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        socket.write_all(format!(
            "POST /mcp/{secret} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()
        ).as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        socket.take(8193).read_to_end(&mut bytes).await.unwrap();
        assert!(bytes.len() <= 8192, "MCP answer cap: 8 KiB");
        let split = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
        let status = std::str::from_utf8(&bytes[..split]).unwrap()
            .split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, serde_json::from_slice(&bytes[split + 4..]).unwrap())
    }).await.expect("MCP request cap: 10 s")
}

#[tokio::test]
async fn a_dropped_session_releases_its_mcp_tool_set_and_safety_tracker_but_keeps_its_folder() {
    capped(async {
        use agent_client_protocol_schema::v1::ToolCallStatus;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        let old_folder = std::fs::read_dir(&runtime.cwd)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        agent
            .stdout(result(
                3,
                json!({"sessionId":"s2","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}),
            ))
            .await;
        let tool = crate::conversation::ClientTool {
            name: "lookup".into(),
            kind: crate::conversation::ToolKind::Function,
            namespace: None,
            description: "A retained client tool".into(),
            schema: json!({"type":"object"}),
        };
        runtime.new_session(&[tool], None).await.unwrap();
        let created = agent.stdin().await;
        assert_eq!(created["method"], "session/new");
        let live_folder = PathBuf::from(created["params"]["cwd"].as_str().unwrap());
        let urls = ["s1", "s2"].map(|session| runtime.tools[session].mcp_url.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let router = runtime.tool_sets.clone().router().layer(
            axum::middleware::from_fn(move |request, next: axum::middleware::Next| {
                let requests = requests.clone();
                async move {
                    assert!(
                        requests.fetch_add(1, Ordering::SeqCst) < 4,
                        "MCP endpoint cap: 4 requests"
                    );
                    next.run(request).await
                }
            }),
        );
        let server = tokio::spawn(async move {
            let _ = tokio::time::timeout(
                RUN_CAP,
                axum::serve(listener, router).into_future(),
            ).await;
        });
        assert_eq!(listed_tools(address, &urls[0]).await.0, 200);
        let live_tools = listed_tools(address, &urls[1]).await;
        assert_eq!(live_tools.0, 200);
        assert_eq!(live_tools.1["result"]["tools"][0]["name"], "lookup");
        assert!(old_folder.is_dir() && live_folder.is_dir());
        runtime.tool_calls.permission("s1", "permission");
        runtime.tool_calls.update("s1", "pending", Some("view_file"), None, None);
        assert_eq!(
            runtime.tool_calls.update(
                "s1", "reported", Some("view_file"), None, Some(ToolCallStatus::Completed),
            ),
            Some("view_file".into()),
        );
        runtime.tool_calls.permission("s2", "permission");

        runtime.forget("s1");

        let dropped_tools = listed_tools(address, &urls[0]).await;
        let retained_tools = listed_tools(address, &urls[1]).await;
        let observations = ["permission", "pending", "reported"].map(|id| {
            runtime.tool_calls.update("s1", id, None, None, Some(ToolCallStatus::Completed))
        });
        let retained_permission = runtime.tool_calls.update(
            "s2", "permission", None, None, Some(ToolCallStatus::Completed),
        );
        let folders = (old_folder.exists(), live_folder.is_dir());
        let sessions = (runtime.tools.contains_key("s1"), runtime.tools.contains_key("s2"));
        server.abort();
        runtime.stop().await;
        assert_eq!(
            (dropped_tools.0, retained_tools, sessions),
            (404, live_tools, (false, true)),
            "only the dropped ACP session loses its MCP tool set and runtime resources"
        );
        assert_eq!(
            (observations, retained_permission, folders),
            (
                [Some("unknown".into()), Some("unknown".into()), Some("unknown".into())],
                None,
                (true, true),
            ),
            "issue #93: a dropped ACP session loses every safety tracker entry; both ACP session folders stay while the runtime process runs"
        );
    })
    .await;
}

#[tokio::test]
async fn captured_stderr_uses_only_the_first_error_chain() {
    tokio::time::timeout(RUN_CAP, async {
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        agent.stderr(b"Antigravity SDK error processing prompt for session s1\n").await;
        agent.stderr(concat!(r#"[t_ms=9029.7] google.antigravity.types.AntigravityConnectionError: Agent execution terminated due to error. ("agent executor error: Error 400, Message: Unable to process input image. Please retry or report in https://developers.generativeai.google/guide/troubleshooting, Status: INVALID_ARGUMENT, Details: [map[@type:type.googleapis.com/google.rpc.ErrorInfo domain:cloudcode-pa.googleapis.com metadata:map[error_number:2011] reason:INVALID_MEDIA]]: Error 400, Message: Unable to process input image. Please retry or report in https://developers.generativeai.google/guide/troubleshooting, Status: INVALID_ARGUMENT, Details: [map[@type:type.googleapis.com/google.rpc.ErrorInfo domain:cloudcode-pa.googleapis.com metadata:map[error_number:2011] reason:INVALID_MEDIA]]")"#, "\n").as_bytes()).await;
        assert!(matches!(runtime.next().await.0, RuntimeEvent::Stderr));
        let RuntimeEvent::UpstreamError { session, error } = runtime.next().await.0 else { panic!("parsed upstream error") };
        assert_eq!(session, "s1");
        assert_eq!(crate::messages::Message::UpstreamHttp(error).to_string(), "acp2api: upstream error 400 INVALID_ARGUMENT: Unable to process input image. Please retry or report in https://developers.generativeai.google/guide/troubleshooting");
    }).await.expect("captured stderr test run cap");
}

#[tokio::test]
async fn a_cancelled_config_call_completes_without_starting_its_prompt() {
    capped(async {
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        runtime
            .start_configured_prompt(
                crate::conversation::PromptRequest {
                    blocks: vec![PromptBlock::Text("Cancelled input".into())],
                    model: "other".into(),
                    level: None,
                },
                "s1",
            )
            .await
            .unwrap();
        let config = agent.stdin().await;
        assert_eq!(config["method"], "session/set_config_option");
        runtime.cancel("s1").await.unwrap();
        assert_eq!(agent.stdin().await["method"], "session/cancel");
        agent
            .stdout(result(config["id"].as_u64().unwrap(), json!({})))
            .await;
        let _ = runtime.next().await;
        runtime
            .start_prompt(vec![PromptBlock::Text("Connected input".into())], "s2")
            .await
            .unwrap();
        let prompt = agent.stdin().await;
        assert_eq!(prompt["method"], "session/prompt");
        assert_eq!(
            prompt["params"]["sessionId"], "s2",
            "FR-049/FR-031 a completed non-prompt call must not start disconnected work"
        );
        assert_eq!(prompt["params"]["prompt"][0]["text"], "Connected input");
    })
    .await;
}

#[tokio::test]
async fn buffered_stdout_is_drained_before_a_ready_stderr_error() {
    capped(async {
        let (mut runtime, mut agent, _root) = start(Vec::new()).await;
        runtime.start_prompt(vec![PromptBlock::Text("Hello".into())], "s1").await.unwrap();
        let _prompt = agent.stdin().await;
        // Separate pipes can wake their readers in a different order from the writes.
        agent.stderr(b"Antigravity SDK error processing prompt for session s1\n").await;
        agent.stderr(b"Error 503, Message: The service is unavailable., Status: UNAVAILABLE, Details: []\n").await;
        agent.stdout(chunk("Sure, ")).await;
        agent.stdout(chunk("503 The service is unavailable.")).await;
        let mut observed = Vec::new();
        for _ in 0..4 {
            observed.push(next(&mut runtime).await);
        }
        assert_eq!(observed, ["text Sure, ", "text 503 The service is unavailable.", "stderr", "upstream 503"], "FR-028 buffered reply text must reach the holdback before its stderr error");
    }).await;
}

#[tokio::test]
async fn a_stderr_error_is_delivered_before_both_closed_streams() {
    capped(async {
        // Use ordinary EOF; the shared fixture appends a stdout read error.
        let root = Root(Environment::from_process().temp.join(format!(
            "acp2api-stderr-eof-{}", crate::random_hex::<8>().unwrap()
        )));
        let (stdout, stdout_agent) = tokio::io::duplex(64 * 1024);
        let (stderr, stderr_agent) = tokio::io::duplex(64 * 1024);
        let (stdin, stdin_agent) = tokio::io::duplex(64 * 1024);
        let mut agent = Agent {
            stdout: stdout_agent,
            stderr: stderr_agent,
            stdin: BufReader::new(stdin_agent),
            writes: 0,
        };
        agent.stdout(result(1, json!({"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}))).await;
        agent.stdout(result(2, json!({"sessionId":"s1","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}))).await;
        let ports = Ports::production();
        let (mut runtime, _) = Runtime::start(&RuntimeStart {
            stopped: tokio::sync::watch::channel(false).1,
            clock: ports.clock.clone(),
            launcher: Arc::new(OneChild(Mutex::new(Some(Launched {
                stdin: Box::pin(stdin), stdout: Box::pin(stdout), stderr: Box::pin(stderr), tree: Box::new(NoTree),
            })))),
            release: ports.release.clone(),
            data: root.0.clone(),
            temporary: crate::account::Temporary::fixture(&root.0),
            tool_sets: mcp::ToolSets::new(1),
        }, "1.4.0").await.unwrap();
        assert_eq!(agent.stdin().await["method"], "initialize");
        assert_eq!(agent.stdin().await["method"], "session/new");
        runtime.start_prompt(vec![PromptBlock::Text("Hello".into())], "s1").await.unwrap();
        let _prompt = agent.stdin().await;
        agent.stdout(chunk("Sure, ")).await;
        agent.stderr(b"Antigravity SDK error processing prompt for session s1\n").await;
        agent.stderr(b"Error 503, Message: The service is unavailable., Status: UNAVAILABLE, Details: []").await;
        drop(agent.stdout);
        drop(agent.stderr);
        let mut observed = Vec::new();
        for _ in 0..4 {
            observed.push(next(&mut runtime).await);
        }
        assert_eq!(observed, ["text Sure, ", "stderr", "upstream 503", "closed"], "stdout EOF must not discard a final stderr error, and buffered reply text must still precede that error");
    }).await;
}

#[tokio::test]
async fn session_cleanup_keeps_only_recorded_files_and_folders() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let root = Root(Environment::from_process().temp.join(format!(
            "acp2api-session-cleanup-{}",
            crate::random_hex::<12>().unwrap()
        )));
        let builds = crate::runtime_builds::Builds::fixture(&root.0, Ports::production().clock);
        let kept = builds.start.temporary.sessions().join("kept");
        let orphan = builds.start.temporary.sessions().join("orphan");
        for folder in [&kept, &orphan] {
            crate::account::private_folder(folder).unwrap();
        }
        let home = root.0.join("runtime-home");
        let files = home.join("antigravity-acp/conversations");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(home.join("settings.json"), b"sign-in").unwrap();
        let suffixes = ["db", "db-wal", "db-shm", "meta"];
        for session in ["kept", "orphan"] {
            for suffix in suffixes {
                std::fs::write(files.join(format!("{session}.{suffix}")), b"history").unwrap();
            }
        }
        builds
            .start
            .clean_sessions(vec![("kept".into(), kept.clone())])
            .await
            .unwrap();
        assert!(
            kept.is_dir()
                && !orphan.exists()
                && suffixes
                    .iter()
                    .all(|suffix| files.join(format!("kept.{suffix}")).is_file()
                        && !files.join(format!("orphan.{suffix}")).exists()),
            "ADR-0005 a later start removes only unrecorded sessions"
        );
        builds.start.clean_sessions(Vec::new()).await.unwrap();
        assert!(
            !kept.exists()
                && std::fs::read_dir(&files).unwrap().next().is_none()
                && std::fs::read(home.join("settings.json")).unwrap() == b"sign-in",
            "ADR-0005 a first start removes all session data and keeps sign-in files"
        );
    })
    .await
    .expect("session cleanup cap: 5 seconds, eight tiny session files, no requests or processes");
}

#[tokio::test]
async fn load_uses_the_requested_session_id_and_recreates_its_missing_cwd() {
    capped(async {
        let (mut runtime, mut agent, root) = start(Vec::new()).await;
        let cwd = root.0.join("missing-cwd");
        runtime.request_session("resume".into(), &[], Some("Be kind."), Some(("saved-session".into(), cwd.clone()))).await.unwrap();
        let load = agent.stdin().await;
        assert_eq!(load["method"], "session/load");
        assert_eq!(load["params"]["sessionId"], "saved-session");
        assert_eq!(load["params"]["cwd"], json!(cwd));
        assert!(cwd.is_dir());
        assert_eq!(load["params"]["_meta"]["antigravity"]["enabledTools"], json!([]));
        assert_eq!(load["params"]["_meta"]["antigravity"]["systemPrompt"]["append"], "Be kind.");
        agent.stdout(result(load["id"].as_u64().unwrap(), json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}))).await;
        let (event, _hold) = runtime.next().await;
        assert!(matches!(event, RuntimeEvent::Prepared { work, result: Ok((session, _)) } if work == "resume" && session == "saved-session"),
            "ACP session/load returns the catalog without a sessionId; the request identifies the loaded session");
    }).await;
}

#[tokio::test]
async fn a_sign_in_address_accepts_four_kib_and_rejects_a_longer_line() {
    let (mut runtime, _agent, _root) = start(vec![]).await;
    let prefix = "https://accounts.example.com/";
    let address = format!("{prefix}{}", "a".repeat(4096 - prefix.len()));
    let line = format!("Open the following link to authenticate the ACP server: {address}");
    assert!(
        matches!(
            Runtime::stderr(&mut runtime.stderr_session, &format!("{line}b")),
            RuntimeEvent::Stderr
        ),
        "the product rejects a sign-in address over 4 KiB"
    );
    assert!(
        matches!(Runtime::stderr(&mut runtime.stderr_session, &line), RuntimeEvent::SignInAddress(value) if value == address),
        "the product accepts a sign-in address of exactly 4 KiB"
    );
}

#[tokio::test]
async fn a_stderr_lockout_after_the_acp_call_fails_wins_over_start_failure() {
    struct LateStderr(tokio::io::DuplexStream);
    impl ProcessTree for LateStderr {
        fn pid(&self) -> u32 {
            std::process::id()
        }
        fn exited(&mut self) -> BoxFuture<'_, ExitStatus> {
            Box::pin(std::future::pending())
        }
        fn stop(mut self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
            Box::pin(async move {
                self.0.write_all(concat!(
                    "Antigravity SDK error processing prompt for session startup\n",
                    "Error 403, Message: Verify your account, Status: PERMISSION_DENIED, Details: []\n",
                ).as_bytes()).await.unwrap();
            })
        }
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        let root = Root(Environment::from_process().temp.join(format!("acp2api-late-stderr-{}", crate::random_hex::<8>().unwrap())));
        let (stderr, writer) = tokio::io::duplex(4096);
        let mut ports = Ports::production();
        ports.launcher = Arc::new(OneChild(Mutex::new(Some(Launched {
            stdin: Box::pin(tokio::io::sink()),
            stdout: Box::pin(std::io::Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32603,\"message\":\"Internal error\"}}\n")),
            stderr: Box::pin(stderr),
            tree: Box::new(LateStderr(writer)),
        }))));
        let start = RuntimeStart {
            stopped: tokio::sync::watch::channel(false).1,
            clock: ports.clock, launcher: ports.launcher, release: ports.release,
            data: root.0.clone(), temporary: crate::account::Temporary::fixture(&root.0), tool_sets: mcp::ToolSets::new(0),
        };
        let result = Runtime::start(&start, "1.4.0").await;
        assert!(matches!(result, Err(LaunchFailure::Lockout { signal: "403", ref error }) if error.to_string() == "acp2api: upstream error 403 PERMISSION_DENIED: Verify your account"), "FR-034: stderr emitted only when the failed process stops must override the ACP failure");
    }).await.expect("late startup stderr cap: five seconds, one process, one ACP error, two stderr lines under four KiB");
}

#[tokio::test]
async fn buffered_shutdown_lockout_evidence_overrides_stopped() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct QueueClock(Arc<dyn Clock>, Arc<tokio::sync::Notify>, AtomicUsize);
    impl Clock for QueueClock {
        fn now(&self) -> Moment {
            self.0.now()
        }
        fn wall(&self) -> SystemTime {
            self.0.wall()
        }
        fn sleep_until(&self, at: Moment) -> BoxFuture<'static, Hold> {
            self.0.sleep_until(at)
        }
        fn hold(&self, kind: HoldKind) -> Hold {
            if matches!(kind, HoldKind::RuntimeLine) && self.2.fetch_add(1, Ordering::SeqCst) == 1 {
                self.1.notify_one();
            }
            self.0.hold(kind)
        }
    }
    struct QueuedStderr(Arc<tokio::sync::Notify>);
    impl ProcessTree for QueuedStderr {
        fn pid(&self) -> u32 {
            std::process::id()
        }
        fn exited(&mut self) -> BoxFuture<'_, ExitStatus> {
            Box::pin(std::future::pending())
        }
        fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
            Box::pin(async move { self.0.notified().await })
        }
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        let root = Root(Environment::from_process().temp.join(format!("acp2api-stopped-lockout-{}", crate::random_hex::<8>().unwrap())));
        let queued = Arc::new(tokio::sync::Notify::new());
        let mut ports = Ports::production();
        ports.clock = Arc::new(QueueClock(ports.clock, queued.clone(), AtomicUsize::new(0)));
        ports.launcher = Arc::new(OneChild(Mutex::new(Some(Launched {
            stdin: Box::pin(tokio::io::sink()), stdout: Box::pin(tokio::io::empty()),
            stderr: Box::pin(std::io::Cursor::new(concat!(
                "Antigravity SDK error processing prompt for session startup\n",
                "Error 403, Message: Verify your account, Status: PERMISSION_DENIED, Details: []\n",
            ).as_bytes())),
            tree: Box::new(QueuedStderr(queued)),
        }))));
        let start = RuntimeStart {
            stopped: tokio::sync::watch::channel(true).1,
            clock: ports.clock, launcher: ports.launcher, release: ports.release,
            data: root.0.clone(), temporary: crate::account::Temporary::fixture(&root.0), tool_sets: mcp::ToolSets::new(0),
        };
        assert!(matches!(Runtime::start(&start, "1.4.0").await, Err(LaunchFailure::Lockout { signal: "403", .. })), "FR-034: buffered lockout evidence must still override Stopped during shutdown");
    }).await.expect("shutdown lockout cap: five seconds, one process, no ACP requests, two stderr lines under one KiB");
}

#[cfg(unix)]
mod escaped_stderr {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tokio::sync::{mpsc, watch};

    struct DrainClock {
        time: watch::Sender<Moment>,
        waits: mpsc::UnboundedSender<Moment>,
    }
    impl Clock for DrainClock {
        fn now(&self) -> Moment {
            *self.time.borrow()
        }
        fn wall(&self) -> SystemTime {
            SystemTime::UNIX_EPOCH + Duration::from_millis(self.now())
        }
        fn sleep_until(&self, at: Moment) -> BoxFuture<'static, Hold> {
            let mut time = self.time.subscribe();
            let waits = self.waits.clone();
            Box::pin(async move {
                waits.send(at).unwrap();
                time.wait_for(|now| *now >= at).await.unwrap();
                Hold::empty()
            })
        }
        fn hold(&self, _: HoldKind) -> Hold {
            Hold::empty()
        }
    }

    struct Fixture {
        root: Root,
        start: RuntimeStart,
        clock: Arc<DrainClock>,
        waits: mpsc::UnboundedReceiver<Moment>,
        stop: watch::Sender<bool>,
        helper_pid: PathBuf,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Ok(pid) = std::fs::read_to_string(&self.helper_pid)
                && let Ok(pid) = pid.trim().parse::<i32>()
            {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }
    fn fixture() -> Fixture {
        let root = Root(Environment::from_process().temp.join(format!(
            "acp2api-escaped-stderr-{}",
            crate::random_hex::<8>().unwrap()
        )));
        let program = program(&root.0);
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        let fifo = std::ffi::CString::new(format!("{}.pipe", program.display())).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        std::fs::write(&program, r#"#!/bin/sh
IFS= read -r request || exit 1
[ "${#request}" -le 1024 ] || exit 1
/usr/bin/perl -MPOSIX -e 'POSIX::setsid() >= 0 or die "setsid"; close(STDIN); close(STDOUT); open(my $pid, ">", $ARGV[1]) or die "pid"; print {$pid} "$$\n"; close($pid); open(my $ready, ">", $ARGV[0]) or die "fifo"; print {$ready} "ready\n"; close($ready); POSIX::pause();' "$0.pipe" "$0.pid" &
IFS= read -r ready < "$0.pipe" || exit 1
printf '%s\n' '{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"Internal error"}}'
"#).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let helper_pid = PathBuf::from(format!("{}.pid", program.display()));
        let (time, _) = watch::channel(0);
        let (waits, received) = mpsc::unbounded_channel();
        let clock = Arc::new(DrainClock { time, waits });
        let (stop, stopped) = watch::channel(false);
        let ports = Ports::production();
        let start = RuntimeStart {
            stopped,
            clock: clock.clone(),
            launcher: ports.launcher,
            release: ports.release,
            data: root.0.clone(),
            temporary: crate::account::Temporary::fixture(&root.0),
            tool_sets: mcp::ToolSets::new(0),
        };
        Fixture {
            root,
            start,
            clock,
            waits: received,
            stop,
            helper_pid,
        }
    }
    async fn end_drain(waits: &mut mpsc::UnboundedReceiver<Moment>, end: impl FnOnce(Moment)) {
        let mut answers = 0;
        for _ in 0..4 {
            let at = waits.recv().await.unwrap();
            if at == ANSWER_TIMEOUT {
                answers += 1;
                if answers == 2 {
                    end(at);
                    return;
                }
            }
        }
        panic!("one answer wait, one process-stop grace, and one drain wait cap");
    }

    #[tokio::test]
    async fn a_failed_start_drain_ends_at_the_answer_deadline_with_escaped_stderr_open() {
        let mut fixture = fixture();
        let clock = fixture.clock.clone();
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                Runtime::start(&fixture.start, "1.4.0"),
                end_drain(&mut fixture.waits, |at| {
                    clock.time.send_replace(at);
                }),
            )
            .0
        })
        .await;
        let pid: i32 = std::fs::read_to_string(&fixture.helper_pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            unsafe { libc::getsid(pid) },
            pid,
            "the live helper escaped the stopped process group and still holds stderr"
        );
        assert!(
            matches!(completed, Ok(Err(ref error @ LaunchFailure::Owner(_))) if error.to_string() == "acp2api: upstream ACP error -32603: Internal error"),
            "the failed-start drain must finish at the existing answer deadline even when a setsid descendant keeps stderr open"
        );
        assert_eq!(fixture.clock.now(), ANSWER_TIMEOUT);
    }

    #[tokio::test]
    async fn a_check_drain_ends_at_the_answer_deadline_with_escaped_stderr_open() {
        let mut fixture = fixture();
        let clock = fixture.clock.clone();
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                crate::compatibility_check::run(
                    &fixture.start,
                    program(&fixture.root.0),
                    Some(fixture.root.0.join("gemini"))
                ),
                end_drain(&mut fixture.waits, |at| {
                    clock.time.send_replace(at);
                }),
            )
            .0
        })
        .await;
        let pid: i32 = std::fs::read_to_string(&fixture.helper_pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            unsafe { libc::getsid(pid) },
            pid,
            "the live helper escaped the stopped check process group and still holds stderr"
        );
        assert!(
            matches!(completed, Ok(Err(crate::compatibility_check::Failure::Runtime(ref error @ LaunchFailure::Owner(_)))) if error.to_string() == "acp2api: upstream ACP error -32603: Internal error"),
            "the check drain must finish at the existing answer deadline even when a setsid descendant keeps stderr open"
        );
        assert_eq!(fixture.clock.now(), ANSWER_TIMEOUT);
    }

    #[tokio::test]
    async fn a_failed_start_drain_ends_on_stop_with_escaped_stderr_open() {
        let mut fixture = fixture();
        let stop = fixture.stop.clone();
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                Runtime::start(&fixture.start, "1.4.0"),
                end_drain(&mut fixture.waits, |_| {
                    stop.send_replace(true);
                }),
            )
            .0
        })
        .await;
        let pid: i32 = std::fs::read_to_string(&fixture.helper_pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            unsafe { libc::getsid(pid) },
            pid,
            "the escaped helper still holds stderr when the owner stops acp2api"
        );
        assert!(
            matches!(completed, Ok(Err(ref error @ LaunchFailure::Owner(_))) if error.to_string() == "acp2api: upstream ACP error -32603: Internal error"),
            "a terminal stop must end the drain while the escaped helper keeps stderr open"
        );
        assert_eq!(
            fixture.clock.now(),
            0,
            "stop must end the drain without advancing to the answer deadline"
        );
    }
}
