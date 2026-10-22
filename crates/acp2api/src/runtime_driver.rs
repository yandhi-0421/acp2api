#[cfg(test)]
mod agent_lines;
use crate::*;
use agent_client_protocol_schema::v1::{ContentBlock, SessionNotification, SessionUpdate};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
pub(crate) struct Runtime {
    input: ProcessWrite,
    lines: mpsc::Receiver<(Line, Hold)>,
    prompt_id: Option<u64>,
    tree: Box<dyn ProcessTree>,
    clock: Arc<dyn Clock>,
    next_id: u64,
    session: String,
}
impl Runtime {
    pub(crate) async fn start(
        ports: &Ports,
        data: &std::path::Path,
        cwd: PathBuf,
        mcp_url: &str,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(data.join("runtime-home")).map_err(|e| e.to_string())?;
        let program = data.join("official-runtime").join(if cfg!(windows) {
            "agy_acp_server.exe"
        } else {
            "agy_acp_server.par"
        });
        std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
        let child = ports
            .launcher
            .launch(LaunchSpec {
                program,
                args: Vec::new(),
                env_set: vec![(
                    "GEMINI_HOME".into(),
                    data.join("runtime-home").into_os_string(),
                )],
                cwd: cwd.clone(),
                own_temp: None,
            })
            .map_err(|e| format!("runtime launch: {e:?}"))?;
        let (tx, lines) = mpsc::channel(128);
        let (stdout_tx, clock) = (tx.clone(), ports.clock.clone());
        tokio::spawn(async move {
            let mut stdout = BufReader::new(child.stdout);
            while let Some(line) = read_line(&mut stdout).await {
                let hold = clock.hold(HoldKind::RuntimeLine);
                let line = serde_json::from_slice(&line).map_err(|e| format!("ACP JSON: {e}"));
                if stdout_tx.send((Line::Stdout(line), hold)).await.is_err() {
                    return;
                }
            }
            let _ = stdout_tx
                .send((Line::Closed, clock.hold(HoldKind::TurnEvent)))
                .await;
        });
        let clock = ports.clock.clone();
        tokio::spawn(async move {
            let mut stderr = BufReader::new(child.stderr);
            while read_line(&mut stderr).await.is_some() {
                let hold = clock.hold(HoldKind::RuntimeLine);
                if tx.send((Line::Stderr, hold)).await.is_err() {
                    return;
                }
            }
        });
        let mut runtime = Self {
            input: child.stdin,
            lines,
            prompt_id: None,
            tree: child.tree,
            clock: ports.clock.clone(),
            next_id: 0,
            session: String::new(),
        };
        runtime.call("initialize", json!({"protocolVersion":1,"clientInfo":{"name":"acp2api","title":"acp2api","version":ports.release.version()},"clientCapabilities":{}})).await?;
        let session = runtime.call("session/new", json!({"cwd": cwd,"mcpServers":[{"type":"http","name":"client-tools","url":mcp_url,"headers":[]}],"_meta":{"antigravity":{"enabledTools":[],"eagerMcpServers":["client-tools"]}}})).await?;
        runtime.session = session["sessionId"]
            .as_str()
            .ok_or("missing sessionId")?
            .to_owned();
        Ok(runtime)
    }
    async fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        self.next_id += 1;
        let id = self.next_id;
        let mut bytes =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|e| e.to_string())?;
        self.input.flush().await.map_err(|e| e.to_string())?;
        Ok(id)
    }
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.send(method, params).await?;
        while let Some((line, _hold)) = self.lines.recv().await {
            let value = match line {
                Line::Stdout(value) => value?,
                Line::Stderr => continue,
                Line::Closed => return Err("runtime stdout closed".into()),
            };
            if value["id"] == id {
                return value
                    .get("result")
                    .cloned()
                    .ok_or_else(|| value["error"].to_string());
            }
        }
        Err("runtime stdout closed".into())
    }
    pub(crate) async fn start_prompt(
        &mut self,
        prompt: String,
        _model: &str,
    ) -> Result<(), String> {
        let id = self
            .send(
                "session/prompt",
                json!({"sessionId":self.session,"prompt":[{"type":"text","text":prompt}]}),
            )
            .await?;
        self.prompt_id = Some(id);
        Ok(())
    }
    pub(crate) async fn next(&mut self) -> (RuntimeEvent, Hold) {
        let Some((line, hold)) = self.lines.recv().await else {
            return (RuntimeEvent::Closed, self.clock.hold(HoldKind::TurnEvent));
        };
        let event = match line {
            Line::Closed => RuntimeEvent::Closed,
            Line::Stderr => RuntimeEvent::Stderr,
            Line::Stdout(Err(error)) => RuntimeEvent::Failed(error),
            Line::Stdout(Ok(value)) => self.decode(value),
        };
        (event, hold)
    }
    fn decode(&mut self, value: Value) -> RuntimeEvent {
        if self.prompt_id.is_some_and(|id| value["id"] == id) {
            self.prompt_id = None;
            if let Some(error) = value.get("error") {
                return RuntimeEvent::Failed(error.to_string());
            }
            return RuntimeEvent::PromptEnded;
        }
        if value["method"] == "session/update" {
            let notification: SessionNotification =
                match serde_json::from_value(value["params"].clone()) {
                    Ok(notification) => notification,
                    Err(_) => {
                        let kind = &value["params"]["update"]["sessionUpdate"];
                        eprintln!("acp2api: skipped an ACP session/update of kind {kind}");
                        return RuntimeEvent::Other;
                    }
                };
            if notification.session_id.to_string() != self.session {
                return RuntimeEvent::Other;
            }
            match notification.update {
                SessionUpdate::AgentMessageChunk(chunk) => {
                    if let ContentBlock::Text(content) = chunk.content {
                        RuntimeEvent::Text(content.text)
                    } else {
                        RuntimeEvent::Other
                    }
                }
                SessionUpdate::UsageUpdate(usage) => RuntimeEvent::Usage(usage.used),
                _ => RuntimeEvent::Other,
            }
        } else {
            RuntimeEvent::Other
        }
    }
    pub(crate) async fn stop(mut self) {
        let _ = self.input.shutdown().await;
        drop(self.input);
        self.tree.stop(self.clock).await;
    }
}

/// The next line in bytes, so that one non-UTF-8 line does not end the stream.
/// A read error ends the stream like its end.
async fn read_line(input: &mut BufReader<ProcessRead>) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    match input.read_until(b'\n', &mut line).await {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}
enum Line {
    Stdout(Result<Value, String>),
    Stderr,
    Closed,
}
pub(crate) enum RuntimeEvent {
    Text(String),
    Usage(u64),
    PromptEnded,
    Failed(String),
    Stderr,
    Other,
    Closed,
}
