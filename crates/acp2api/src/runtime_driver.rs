#[cfg(test)]
mod agent_lines;
pub(crate) mod mcp;
use crate::*;
use agent_client_protocol_schema::v1::{
    ContentBlock, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SessionNotification, SessionUpdate,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PermissionId(u64);
pub(crate) struct Runtime {
    input: ProcessWrite,
    lines: mpsc::Receiver<(Line, Hold)>,
    prompt_id: Option<u64>,
    tree: Box<dyn ProcessTree>,
    clock: Arc<dyn Clock>,
    next_id: u64,
    session: String,
    cwd: PathBuf,
    tool_sets: mcp::ToolSets,
    permissions: std::collections::HashMap<PermissionId, Value>,
    next_permission: u64,
}
impl Runtime {
    pub(crate) async fn start(
        ports: &Ports,
        data: &std::path::Path,
        cwd: PathBuf,
        tool_sets: mcp::ToolSets,
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
            cwd: cwd.clone(),
            tool_sets,
            permissions: std::collections::HashMap::new(),
            next_permission: 0,
        };
        runtime.call("initialize", json!({"protocolVersion":1,"clientInfo":{"name":"acp2api","title":"acp2api","version":ports.release.version()},"clientCapabilities":{}})).await?;
        runtime.new_session(&[]).await?;
        Ok(runtime)
    }
    pub(crate) async fn new_session(
        &mut self,
        tools: &[crate::conversation::ClientTool],
    ) -> Result<(), String> {
        let mcp_url = self.tool_sets.insert(tools)?;
        let cwd = self.cwd.join(format!("session-{}", self.next_id));
        std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
        let session = self.call("session/new", json!({"cwd":cwd,"mcpServers":[{"type":"http","name":"client-tools","url":mcp_url,"headers":[]}],"_meta":{"antigravity":{"enabledTools":[],"eagerMcpServers":["client-tools"]}}})).await?;
        self.session = session["sessionId"]
            .as_str()
            .ok_or("missing sessionId")?
            .to_owned();
        Ok(())
    }
    async fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.write(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        Ok(id)
    }
    async fn write(&mut self, value: Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|e| e.to_string())?;
        self.input.flush().await.map_err(|e| e.to_string())
    }
    pub(crate) async fn permission(
        &mut self,
        request: PermissionId,
        cancel: bool,
    ) -> Result<(), String> {
        if cancel {
            self.write(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":self.session}})).await?;
        }
        if let Some(id) = self.permissions.remove(&request) {
            let outcome = RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
            self.write(json!({"jsonrpc":"2.0","id":id,"result":outcome}))
                .await?;
        }
        Ok(())
    }
    /// Answers an agent request whose method acp2api does not handle.
    async fn refuse(&mut self, request: &Value) -> Result<(), String> {
        self.write(json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32601,"message":"Method not found"}}))
            .await
    }
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.send(method, params).await?;
        while let Some((line, _hold)) = self.lines.recv().await {
            let value = match line {
                Line::Stdout(value) => value?,
                Line::Stderr => continue,
                Line::Closed => return Err("runtime stdout closed".into()),
            };
            if value.get("method").is_some() {
                if value.get("id").is_some() {
                    self.refuse(&value).await?;
                }
                continue;
            }
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
        prompt: Vec<crate::conversation::PromptBlock>,
        _model: &str,
    ) -> Result<(), String> {
        let blocks: Vec<agent_client_protocol_schema::v1::ContentBlock> = prompt
            .into_iter()
            .map(|block| match block {
                crate::conversation::PromptBlock::Text(text) => {
                    agent_client_protocol_schema::v1::ContentBlock::Text(
                        agent_client_protocol_schema::v1::TextContent::new(text),
                    )
                }
                crate::conversation::PromptBlock::Image { media_type, base64 } => {
                    agent_client_protocol_schema::v1::ContentBlock::Image(
                        agent_client_protocol_schema::v1::ImageContent::new(base64, media_type),
                    )
                }
            })
            .collect();
        let id = self
            .send(
                "session/prompt",
                json!({"sessionId":self.session,"prompt":blocks}),
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
            Line::Stdout(Ok(value)) => self.decode(value).await,
        };
        (event, hold)
    }
    async fn decode(&mut self, value: Value) -> RuntimeEvent {
        if value["method"] == "session/request_permission" {
            let request: RequestPermissionRequest =
                match serde_json::from_value(value["params"].clone()) {
                    Ok(request) => request,
                    Err(error) => {
                        return RuntimeEvent::Failed(format!(
                            "ACP session/request_permission: {error}"
                        ));
                    }
                };
            self.next_permission += 1;
            let permission = PermissionId(self.next_permission);
            self.permissions.insert(permission, value["id"].clone());
            return RuntimeEvent::Permission {
                request: permission,
                title: if request.session_id.to_string() == self.session {
                    request.tool_call.fields.title.unwrap_or_default()
                } else {
                    String::new()
                },
                arguments: request
                    .tool_call
                    .fields
                    .raw_input
                    .filter(Value::is_object)
                    .map(|v| v.to_string()),
            };
        }
        if value.get("method").is_none() && self.prompt_id.is_some_and(|id| value["id"] == id) {
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
        } else if value.get("method").is_some() && value.get("id").is_some() {
            match self.refuse(&value).await {
                Ok(()) => RuntimeEvent::Other,
                Err(error) => RuntimeEvent::Failed(error),
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
    Permission {
        request: PermissionId,
        title: String,
        arguments: Option<String>,
    },
    PromptEnded,
    Failed(String),
    Stderr,
    Other,
    Closed,
}
