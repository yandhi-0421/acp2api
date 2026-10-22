#[cfg(test)]
mod agent_lines;
pub(crate) mod mcp;
mod safety;
mod session_folders;
pub(crate) use session_folders::root as session_root;
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
    creating: std::collections::HashMap<u64, (String, SessionTools)>,
    tree: Box<dyn ProcessTree>,
    clock: Arc<dyn Clock>,
    next_id: u64,
    session: String,
    cwd: PathBuf,
    tool_sets: mcp::ToolSets,
    tools: std::collections::HashMap<String, SessionTools>,
    permissions: std::collections::HashMap<PermissionId, (Value, String)>,
    next_permission: u64,
    tool_calls: safety::ToolCalls,
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
            tool_calls: safety::ToolCalls::default(),
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
        let (params, tools) = self.session_parameters(tools, system)?;
        let session = self.call("session/new", params).await?;
        let catalog = crate::catalog::Catalog::decode(&session, self.clock.wall())?;
        self.session = session["sessionId"]
            .as_str()
            .ok_or("missing sessionId")?
            .to_owned();
        self.tools.insert(self.session.clone(), tools);
        Ok(catalog)
    }
    fn session_parameters(
        &self,
        tools: &[crate::conversation::ClientTool],
        system: Option<&str>,
    ) -> Result<(Value, SessionTools), String> {
        let mcp_url = self.tool_sets.insert(tools)?;
        let cwd = self.cwd.join(format!("session-{}", self.next_id));
        std::fs::create_dir(&cwd).map_err(|e| e.to_string())?;
        let mut params = json!({"cwd":cwd,"mcpServers":[{"type":"http","name":"client-tools","url":mcp_url,"headers":[]}],"_meta":{"antigravity":{"enabledTools":[],"eagerMcpServers":["client-tools"]}}});
        if let Some(system) = system {
            params["_meta"]["antigravity"]["systemPrompt"] = json!({"append":system});
        }
        let tools = SessionTools {
            mcp_url,
            tools: tools.to_vec(),
        };
        Ok((params, tools))
    }
    pub(crate) async fn request_session(
        &mut self,
        work: String,
        tools: &[crate::conversation::ClientTool],
        system: Option<&str>,
    ) -> Result<(), String> {
        let (params, tools) = self.session_parameters(tools, system)?;
        let id = self.send("session/new", params).await?;
        self.creating.insert(id, (work, tools));
        Ok(())
    }
    /// Drops the client tools of a session that acp2api does not use again.
    pub(crate) fn forget(&mut self, session: &str) {
        if let Some(tools) = self.tools.remove(session) {
            self.tool_sets.remove(&tools.mcp_url);
        }
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
                self.cancel(&session).await?;
            }
            let outcome = RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
            self.write(json!({"jsonrpc":"2.0","id":id,"result":outcome}))
                .await?;
        }
        Ok(())
    }
    pub(crate) async fn cancel(&mut self, session: &str) -> Result<(), String> {
        self.write(
            json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session}}),
        )
        .await
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
        session: &str,
    ) -> Result<(), String> {
        self.tool_calls.begin_prompt(session);
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
            Line::Stdout(Ok(value)) => self.decode(value).await,
        };
        (event, hold)
    }
    async fn decode(&mut self, value: Value) -> RuntimeEvent {
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
                    Ok((session, catalog))
                })();
                match &result {
                    Ok((session, _)) => {
                        self.tools.insert(session.clone(), tools);
                    }
                    Err(_) => self.tool_sets.remove(&tools.mcp_url),
                }
                return RuntimeEvent::Prepared { work, result };
            }
            if let Some(session) = self.prompts.remove(&id) {
                let update = match value.get("error") {
                    Some(error) => TurnUpdate::Failed(error.to_string()),
                    None => TurnUpdate::PromptEnded {
                        end_turn: value["result"]["stopReason"] == "end_turn",
                    },
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
            self.tool_calls
                .permission(&session, &request.tool_call.tool_call_id.to_string());
            self.next_permission += 1;
            let permission = PermissionId(self.next_permission);
            self.permissions
                .insert(permission, (value["id"].clone(), session.clone()));
            let call = (|| {
                let title = request.tool_call.fields.title.as_deref()?;
                let name = title.strip_prefix("Run ")?.strip_suffix('?')?;
                let tools = &self.tools.get(&session)?.tools;
                let tool = mcp::tool_names(tools)
                    .into_iter()
                    .zip(tools)
                    .find_map(|(mcp_name, tool)| (mcp_name == name).then_some(tool))?;
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
            let update = match call {
                Some(call) => TurnUpdate::Permission {
                    request: permission,
                    call,
                },
                None => TurnUpdate::RefusedPermission {
                    request: permission,
                    name: safety::tool_name(
                        request.tool_call.fields.name.as_deref(),
                        request.tool_call.fields.title.as_deref(),
                    ),
                },
            };
            return RuntimeEvent::Turn { session, update };
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
            let session = notification.session_id.to_string();
            if !self.tools.contains_key(&session) {
                return RuntimeEvent::Other;
            }
            let update = match notification.update {
                SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                    ContentBlock::Text(content) => Some(TurnUpdate::Text(content.text)),
                    _ => None,
                },
                SessionUpdate::UsageUpdate(usage) => Some(TurnUpdate::Usage(usage.used)),
                SessionUpdate::ToolCall(call) => self
                    .tool_calls
                    .update(
                        &session,
                        &call.tool_call_id.to_string(),
                        call.name.as_deref(),
                        Some(&call.title),
                        Some(call.status),
                    )
                    .map(TurnUpdate::UnpermittedTool),
                SessionUpdate::ToolCallUpdate(call) => self
                    .tool_calls
                    .update(
                        &session,
                        &call.tool_call_id.to_string(),
                        call.fields.name.as_deref(),
                        call.fields.title.as_deref(),
                        call.fields.status,
                    )
                    .map(TurnUpdate::UnpermittedTool),
                _ => None,
            };
            match update {
                Some(update) => RuntimeEvent::Turn { session, update },
                None => RuntimeEvent::Other,
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
struct SessionTools {
    mcp_url: String,
    tools: Vec<crate::conversation::ClientTool>,
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
        call: crate::conversation::ToolCall,
    },
    RefusedPermission {
        request: PermissionId,
        name: String,
    },
    UnpermittedTool(String),
    PromptEnded {
        end_turn: bool,
    },
    Failed(String),
}
