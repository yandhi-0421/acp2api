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

struct Agent {
    stdout: DuplexStream,
    stderr: DuplexStream,
    stdin: BufReader<DuplexStream>,
}
impl Agent {
    async fn stdout(&mut self, line: Value) {
        self.stdout
            .write_all(format!("{line}\n").as_bytes())
            .await
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
        0
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
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random).unwrap();
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
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
    };
    agent.stdout(result(1, json!({"protocolVersion":1}))).await;
    for line in before_session {
        agent.stdout(line).await;
    }
    agent
        .stdout(result(
            2,
            json!({"sessionId":"s1","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}),
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
        Runtime::start(&ports, &root.0, root.0.join("cwd"), mcp::ToolSets::new(1)),
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
            TurnUpdate::Text(text) => format!("text {text}"),
            TurnUpdate::Usage(used) => format!("usage {used}"),
            TurnUpdate::Permission { call, .. } => format!("permission {call:?}"),
            TurnUpdate::RefusedPermission { name, .. } => format!("permission of {name}"),
            TurnUpdate::UnpermittedTool(name) => format!("unpermitted {name}"),
            TurnUpdate::PromptEnded { .. } => "prompt ended".into(),
            TurnUpdate::Failed(error) => format!("failed {error}"),
        },
        RuntimeEvent::Prepared { .. } => "prepared".into(),
        RuntimeEvent::Failed(error) => format!("failed {error}"),
        RuntimeEvent::Stderr => "stderr".into(),
        RuntimeEvent::Other => "other".into(),
        RuntimeEvent::Closed => "closed".into(),
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

#[tokio::test]
async fn stderr_bytes_do_not_fail_a_turn_and_a_stdout_read_error_closes_the_runtime() {
    let (mut runtime, mut agent, _root) = start(Vec::new()).await;
    agent
        .stderr
        .write_all(b"\xff\xfe code page 936\n")
        .await
        .unwrap();
    agent.stderr.write_all(b"next stderr line\n").await.unwrap();
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
    assert_eq!(next(&mut runtime).await, "closed");
}

#[tokio::test]
async fn an_unparsable_session_update_is_skipped() {
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
    assert_eq!(turn(&mut runtime).await, ["text Hello", "prompt ended"]);
}

#[tokio::test]
async fn an_agent_request_with_an_unknown_method_gets_32601_and_is_no_reply() {
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
}

#[tokio::test]
async fn a_permission_request_is_a_permission_event_and_not_refused() {
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
}
