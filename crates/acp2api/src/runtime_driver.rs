pub(crate) mod mcp;
mod safety;
pub(crate) use safety::ToolName;
#[cfg(test)]
mod rpc_request_ids;
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
    prompts: std::collections::HashMap<u64, String>,
    creating: std::collections::HashMap<u64, (String, Vec<crate::conversation::ClientTool>)>,
    tree: Box<dyn ProcessTree>,
    clock: Arc<dyn Clock>,
    next_id: u64,
    session: String,
    cwd: PathBuf,
    tool_sets: mcp::ToolSets,
    tools: std::collections::HashMap<String, Vec<crate::conversation::ClientTool>>,
    permissions: std::collections::HashMap<PermissionId, (Value, String)>,
    next_permission: u64,
}
impl Runtime {
    pub(crate) async fn start(
        ports: &Ports,
        data: &std::path::Path,
        cwd: PathBuf,
        tool_sets: mcp::ToolSets,
    ) -> Result<(Self, crate::catalog::Catalog), String> {
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
        let clock = ports.clock.clone();
        tokio::spawn(async move {
            let mut stdout = BufReader::new(child.stdout).lines();
            let mut stderr = BufReader::new(child.stderr).lines();
            let mut stdout_open = true;
            let mut stderr_open = true;
            while stdout_open || stderr_open {
                let (is_stdout, result) = tokio::select! {
                    line=stdout.next_line(),if stdout_open=>(true,line),
                    line=stderr.next_line(),if stderr_open=>(false,line),
                };
                let (line, hold) = match result {
                    Ok(Some(line)) => {
                        let hold = clock.hold(HoldKind::RuntimeLine);
                        let line = if is_stdout {
                            Line::Stdout(
                                serde_json::from_str(&line).map_err(|e| format!("ACP JSON: {e}")),
                            )
                        } else {
                            Line::Stderr
                        };
                        (line, hold)
                    }
                    Ok(None) => {
                        if is_stdout {
                            stdout_open = false;
                        } else {
                            stderr_open = false;
                            continue;
                        }
                        (Line::Closed, clock.hold(HoldKind::TurnEvent))
                    }
                    Err(error) => {
                        if is_stdout {
                            stdout_open = false;
                        } else {
                            stderr_open = false;
                        }
                        (
                            Line::Stdout(Err(error.to_string())),
                            clock.hold(HoldKind::TurnEvent),
                        )
                    }
                };
                if tx.send((line, hold)).await.is_err() {
                    break;
                }
            }
        });
        let mut runtime = Self {
            input: child.stdin,
            lines,
            prompts: std::collections::HashMap::new(),
            creating: std::collections::HashMap::new(),
            tree: child.tree,
            clock: ports.clock.clone(),
            next_id: 0,
            session: String::new(),
            cwd: cwd.clone(),
            tool_sets,
            tools: std::collections::HashMap::new(),
            permissions: std::collections::HashMap::new(),
            next_permission: 0,
        };
        runtime.call("initialize", json!({"protocolVersion":1,"clientInfo":{"name":"acp2api","title":"acp2api","version":ports.release.version()},"clientCapabilities":{}})).await?;
        let catalog = runtime.new_session(&[], None).await?;
        Ok((runtime, catalog))
    }
    pub(crate) async fn new_session(
        &mut self,
        tools: &[crate::conversation::ClientTool],
        system: Option<&str>,
    ) -> Result<crate::catalog::Catalog, String> {
        let params = self.session_parameters(tools, system)?;
        let session = self.call("session/new", params).await?;
        let catalog = crate::catalog::Catalog::decode(&session, self.clock.wall())?;
        self.session = session["sessionId"]
            .as_str()
            .ok_or("missing sessionId")?
            .to_owned();
        self.tools.insert(self.session.clone(), tools.to_vec());
        Ok(catalog)
    }
    fn session_parameters(
        &self,
        tools: &[crate::conversation::ClientTool],
        system: Option<&str>,
    ) -> Result<Value, String> {
        let mcp_url = self.tool_sets.insert(tools)?;
        let cwd = self.cwd.join(format!("session-{}", self.next_id));
        std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
        let mut params = json!({"cwd":cwd,"mcpServers":[{"type":"http","name":"client-tools","url":mcp_url,"headers":[]}],"_meta":{"antigravity":{"enabledTools":[],"eagerMcpServers":["client-tools"]}}});
        if let Some(system) = system {
            params["_meta"]["antigravity"]["systemPrompt"] = json!({"append":system});
        }
        Ok(params)
    }
    pub(crate) async fn request_session(
        &mut self,
        work: String,
        tools: &[crate::conversation::ClientTool],
        system: Option<&str>,
    ) -> Result<(), String> {
        let params = self.session_parameters(tools, system)?;
        let id = self.send("session/new", params).await?;
        self.creating.insert(id, (work, tools.to_vec()));
        Ok(())
    }
    async fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.write(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        Ok(id)
    }
    pub(crate) fn session_id(&self) -> &str {
        &self.session
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
        if let Some((id, session)) = self.permissions.remove(&request) {
            if cancel {
                self.write(json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session}})).await?;
            }
            let outcome = RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
            self.write(json!({"jsonrpc":"2.0","id":id,"result":outcome}))
                .await?;
        }
        Ok(())
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
        prompt: Vec<crate::conversation::PromptBlock>,
        _model: &str,
        session: &str,
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
                json!({"sessionId":session,"prompt":blocks}),
            )
            .await?;
        self.prompts.insert(id, session.into());
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
        if value.get("method").is_none()
            && let Some(id) = value["id"].as_u64()
        {
            if let Some((work, tools)) = self.creating.remove(&id) {
                let result = (|| {
                    let response = value
                        .get("result")
                        .ok_or_else(|| value["error"].to_string())?;
                    let session = response["sessionId"]
                        .as_str()
                        .ok_or("missing sessionId")?
                        .to_owned();
                    let catalog = crate::catalog::Catalog::decode(response, self.clock.wall())?;
                    self.tools.insert(session.clone(), tools);
                    Ok((session, catalog))
                })();
                return RuntimeEvent::Prepared { work, result };
            }
            if let Some(session) = self.prompts.remove(&id) {
                let update = match value.get("error") {
                    Some(error) => TurnUpdate::Failed(error.to_string()),
                    None => TurnUpdate::PromptEnded,
                };
                return RuntimeEvent::Turn { session, update };
            }
        }
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
            let session = request.session_id.to_string();
            self.next_permission += 1;
            let permission = PermissionId(self.next_permission);
            self.permissions
                .insert(permission, (value["id"].clone(), session.clone()));
            let name = ToolName::from_fields(
                request.tool_call.fields.name.as_deref(),
                request.tool_call.fields.title.as_deref(),
            );
            let call = (|| {
                let title = request.tool_call.fields.title.as_deref()?;
                let name = title.strip_prefix("Run ")?.strip_suffix('?')?;
                let tool = self
                    .tools
                    .get(&session)?
                    .iter()
                    .find(|tool| tool.mcp_name() == name)?;
                let input = request
                    .tool_call
                    .fields
                    .raw_input
                    .as_ref()
                    .filter(|value| value.is_object())?;
                let arguments = match tool.kind {
                    crate::conversation::ToolKind::Function => input.to_string(),
                    crate::conversation::ToolKind::Custom => {
                        input.get("input")?.as_str()?.to_owned()
                    }
                };
                Some(crate::conversation::ToolCall {
                    name: tool.name.clone(),
                    kind: tool.kind,
                    namespace: tool.namespace.clone(),
                    arguments,
                })
            })();
            return RuntimeEvent::Turn {
                session,
                update: if call.is_some() {
                    TurnUpdate::Permission {
                        request: permission,
                        call,
                    }
                } else {
                    TurnUpdate::RefusedPermission {
                        request: permission,
                        name,
                    }
                },
            };
        }
        if value["method"] == "session/update" {
            let notification: SessionNotification =
                match serde_json::from_value(value["params"].clone()) {
                    Ok(notification) => notification,
                    Err(error) => {
                        return RuntimeEvent::Failed(format!("ACP session/update: {error}"));
                    }
                };
            let session = notification.session_id.to_string();
            if !self.tools.contains_key(&session) {
                return RuntimeEvent::Other;
            }
            let update = match notification.update {
                SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                    ContentBlock::Text(content) => TurnUpdate::Text(content.text),
                    _ => return RuntimeEvent::Other,
                },
                SessionUpdate::UsageUpdate(usage) => TurnUpdate::Usage(usage.used),
                _ => return RuntimeEvent::Other,
            };
            RuntimeEvent::Turn { session, update }
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

enum Line {
    Stdout(Result<Value, String>),
    Stderr,
    Closed,
}
pub(crate) enum RuntimeEvent {
    Prepared {
        work: String,
        result: Result<(String, crate::catalog::Catalog), String>,
    },
    Turn {
        session: String,
        update: TurnUpdate,
    },
    Failed(String),
    Stderr,
    Other,
    Closed,
}
pub(crate) enum TurnUpdate {
    Text(String),
    Usage(u64),
    Permission {
        request: PermissionId,
        call: Option<crate::conversation::ToolCall>,
    },
    RefusedPermission {
        request: PermissionId,
        name: ToolName,
    },
    PromptEnded,
    Failed(String),
}
