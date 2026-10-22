use super::*;
use crate::conversation::{ClientTool, PromptBlock, ToolKind};
use std::{sync::Mutex, time::Duration};
use tokio::io::{DuplexStream, ReadHalf, WriteHalf};

// Only the official-runtime side is fake. Each run has a 10 s deadline,
// 32 lines per direction, 4 KiB per line, and a 64 KiB pipe. No log is kept.
struct Agent {
    input: BufReader<ReadHalf<DuplexStream>>,
    output: WriteHalf<DuplexStream>,
    received: usize,
    sent: usize,
}
impl Agent {
    async fn receive(&mut self, method: &str) -> Value {
        self.received += 1;
        assert!(self.received <= 32, "fake ACP receive cap");
        let mut line = String::new();
        self.input.read_line(&mut line).await.unwrap();
        assert!(line.len() <= 4096, "fake ACP line cap");
        let value: Value = serde_json::from_str(&line).unwrap();
        if !method.is_empty() {
            assert_eq!(value["method"], method);
        }
        value
    }
    async fn send(&mut self, value: Value) {
        self.sent += 1;
        assert!(self.sent <= 32, "fake ACP send cap");
        let line = format!("{value}\n");
        assert!(line.len() <= 4096, "fake ACP line cap");
        self.output.write_all(line.as_bytes()).await.unwrap();
        self.output.flush().await.unwrap();
    }
    async fn result(&mut self, id: &Value, result: Value) {
        self.send(json!({"jsonrpc":"2.0","id":id,"result":result}))
            .await;
    }
}
struct AgentLauncher {
    program: PathBuf,
    child: Mutex<Option<Launched>>,
}
impl ProcessLauncher for AgentLauncher {
    fn launch(&self, spec: LaunchSpec) -> Result<Launched, LaunchError> {
        if spec.program != self.program {
            return Err(LaunchError::Refused {
                program: spec.program,
            });
        }
        Ok(self
            .child
            .lock()
            .unwrap()
            .take()
            .expect("one fake ACP process"))
    }
}
struct AgentTree;
impl ProcessTree for AgentTree {
    fn pid(&self) -> u32 {
        0
    }
    fn exited(&mut self) -> BoxFuture<'_, ExitStatus> {
        Box::pin(async { ExitStatus::default() })
    }
    fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).unwrap();
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        Self(
            Environment::from_process()
                .temp
                .join(format!("acp2api-rpc-request-ids-{suffix}")),
        )
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn session_result(session: &str) -> Value {
    json!({"sessionId":session,"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"test-model","options":[{"value":"test-model","name":"Test Model"}]}]})
}
async fn start(root: &Root) -> (Runtime, Agent) {
    let (product, agent) = tokio::io::duplex(64 * 1024);
    let (stdout, stdin) = tokio::io::split(product);
    let (input, output) = tokio::io::split(agent);
    let mut agent = Agent {
        input: BufReader::new(input),
        output,
        received: 0,
        sent: 0,
    };
    let mut ports = Ports::production();
    ports.launcher = Arc::new(AgentLauncher {
        program: root.0.join("official-runtime").join(if cfg!(windows) {
            "agy_acp_server.exe"
        } else {
            "agy_acp_server.par"
        }),
        child: Mutex::new(Some(Launched {
            stdin: Box::pin(stdin),
            stdout: Box::pin(stdout),
            stderr: Box::pin(tokio::io::empty()),
            tree: Box::new(AgentTree),
        })),
    });
    let (started, ()) = tokio::join!(
        Runtime::start(&ports, &root.0, root.0.join("cwd"), mcp::ToolSets::new(0)),
        async {
            let initialize = agent.receive("initialize").await;
            agent
                .result(&initialize["id"], json!({"protocolVersion":1}))
                .await;
            let session = agent.receive("session/new").await;
            agent.result(&session["id"], session_result("alpha")).await;
        }
    );
    (started.unwrap().0, agent)
}
async fn ended(runtime: &mut Runtime, expected: &str) {
    let (event, _hold) = runtime.next().await;
    assert!(
        matches!(event, RuntimeEvent::Turn {session, update:TurnUpdate::PromptEnded { .. }} if session == expected),
        "the real outgoing prompt response must still complete its own session"
    );
}
async fn exercise(creation_pending: bool) {
    let root = Root::new();
    let (mut runtime, mut agent) = start(&root).await;
    let tool = ClientTool {
        name: "beta_lookup".into(),
        description: "A tool only in beta".into(),
        schema: "{\"type\":\"object\"}".into(),
        kind: ToolKind::Function,
        namespace: None,
    };
    runtime
        .request_session("work-beta".into(), &[tool], None)
        .await
        .unwrap();
    let beta = agent.receive("session/new").await;
    agent.result(&beta["id"], session_result("beta")).await;
    let (event, _hold) = runtime.next().await;
    assert!(
        matches!(event, RuntimeEvent::Prepared {work,result:Ok((session,_))} if work == "work-beta" && session == "beta")
    );
    let mut prompts = Vec::new();
    for session in ["alpha", "beta"] {
        runtime
            .start_prompt(
                vec![PromptBlock::Text(session.into())],
                "test-model",
                session,
            )
            .await
            .unwrap();
        let prompt = agent.receive("session/prompt").await;
        assert_eq!(prompt["params"]["sessionId"], session);
        prompts.push(prompt);
    }
    let incoming_id = if creation_pending {
        runtime
            .request_session("work-gamma".into(), &[], None)
            .await
            .unwrap();
        agent.receive("session/new").await["id"].clone()
    } else {
        prompts[0]["id"].clone()
    };
    agent.send(json!({"jsonrpc":"2.0","id":incoming_id,"method":"session/request_permission","params":{"sessionId":"beta","toolCall":{"toolCallId":"beta-call","title":"Run beta_lookup?","rawInput":{"item":"beta-only"}},"options":[]}})).await;
    let (event, _hold) = runtime.next().await;
    let RuntimeEvent::Turn {
        session,
        update: TurnUpdate::Permission { request, call },
    } = event
    else {
        panic!(
            "incoming permission must remain a permission while an outgoing RPC with the same numeric ID is pending"
        );
    };
    assert_eq!(session, "beta", "permission uses its own ACP session");
    assert_eq!(call.name, "beta_lookup");
    assert_eq!(call.arguments, "{\"item\":\"beta-only\"}");
    runtime.permission(request, true).await.unwrap();
    assert_eq!(
        agent.receive("session/cancel").await["params"]["sessionId"],
        "beta",
        "cancel at permission must not cancel the colliding outgoing session"
    );
    assert_eq!(
        agent.receive("").await,
        json!({"jsonrpc":"2.0","id":incoming_id,"result":{"outcome":{"outcome":"cancelled"}}}),
        "permission response echoes the incoming ID without changing outgoing correlation"
    );
    agent
        .result(&prompts[1]["id"], json!({"stopReason":"cancelled"}))
        .await;
    ended(&mut runtime, "beta").await;
    if creation_pending {
        agent.result(&incoming_id, session_result("gamma")).await;
        let (event, _hold) = runtime.next().await;
        assert!(
            matches!(event, RuntimeEvent::Prepared {work,result:Ok((session,_))} if work == "work-gamma" && session == "gamma"),
            "the real outgoing session response must still complete its pending work"
        );
        runtime
            .start_prompt(
                vec![PromptBlock::Text("gamma".into())],
                "test-model",
                "gamma",
            )
            .await
            .unwrap();
        let gamma = agent.receive("session/prompt").await;
        assert_eq!(gamma["params"]["sessionId"], "gamma");
        agent
            .result(&gamma["id"], json!({"stopReason":"end_turn"}))
            .await;
        ended(&mut runtime, "gamma").await;
    }
    agent
        .result(&prompts[0]["id"], json!({"stopReason":"end_turn"}))
        .await;
    ended(&mut runtime, "alpha").await;
    println!(
        "PASS: permission id={incoming_id}, session=beta, response echoes incoming id; pending {} completes on its later real response; concurrent alpha and beta stay separate",
        if creation_pending {
            "session/new"
        } else {
            "session/prompt"
        }
    );
    runtime.stop().await;
}
#[tokio::test]
async fn permission_id_can_equal_another_sessions_pending_prompt_id() {
    tokio::time::timeout(Duration::from_secs(10), exercise(false))
        .await
        .expect("RPC routing probe cap: 10 seconds");
}
#[tokio::test]
async fn permission_id_can_equal_a_pending_session_creation_id() {
    tokio::time::timeout(Duration::from_secs(10), exercise(true))
        .await
        .expect("RPC routing probe cap: 10 seconds");
}
