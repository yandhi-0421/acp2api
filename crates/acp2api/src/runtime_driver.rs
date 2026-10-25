#[cfg(test)]
mod agent_lines;
pub(crate) mod mcp;
#[cfg(test)]
mod rpc_request_ids;
mod safety;
use crate::*;
use agent_client_protocol_schema::v1::{
    ContentBlock, ImageContent, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionNotification, SessionUpdate, StopReason, TextContent,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
pub(crate) struct PermissionRequest {
    id: Value,
    session: String,
}
#[derive(Clone)]
pub(crate) struct RuntimeStart {
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) stopped: tokio::sync::watch::Receiver<bool>,
    pub(crate) launcher: Arc<dyn ProcessLauncher>,
    pub(crate) release: Arc<dyn ReleaseFacts>,
    pub(crate) data: PathBuf,
    pub(crate) temporary: crate::account::Temporary,
    pub(crate) tool_sets: mcp::ToolSets,
}
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
    tool_calls: safety::ToolCalls,
    config: std::collections::HashMap<(String, String), String>,
    configuring: std::collections::HashMap<u64, ConfiguringPrompt>,
    stderr_session: Option<String>,
}
struct ConfiguringPrompt {
    session: String,
    option: String,
    value: String,
    continuation: Option<(crate::conversation::PromptRequest, Vec<&'static str>)>,
}
impl Runtime {
    pub(crate) async fn start(
        start: &RuntimeStart,
        expected_version: &str,
    ) -> Result<(Self, crate::catalog::Catalog), String> {
        let data = &start.data;
        let cwd = start.temporary.sessions();
        let tool_sets = start.tool_sets.clone();
        std::fs::create_dir_all(data.join("runtime-home")).map_err(|e| e.to_string())?;
        let child = crate::startup::launch(
            &start.temporary,
            start.launcher.as_ref(),
            start.clock.clone(),
            LaunchSpec {
                program: program(data),
                args: Vec::new(),
                env_set: vec![(
                    "GEMINI_HOME".into(),
                    data.join("runtime-home").into_os_string(),
                )],
                cwd: cwd.clone(),
                own_temp: None,
            },
        )
        .await?;
        let (tx, lines) = mpsc::channel(128);
        let (stdout_tx, clock) = (tx.clone(), start.clock.clone());
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
        let clock = start.clock.clone();
        tokio::spawn(async move {
            let mut stderr = BufReader::new(child.stderr);
            while let Some(line) = read_line(&mut stderr).await {
                let hold = clock.hold(HoldKind::RuntimeLine);
                if tx.send((Line::Stderr(line), hold)).await.is_err() {
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
            clock: start.clock.clone(),
            next_id: 0,
            session: String::new(),
            cwd: cwd.clone(),
            tool_sets,
            tools: std::collections::HashMap::new(),
            tool_calls: safety::ToolCalls::default(),
            config: std::collections::HashMap::new(),
            configuring: std::collections::HashMap::new(),
            stderr_session: None,
        };
        let client = json!({
            "name": "acp2api",
            "title": "acp2api",
            "version": start.release.version()
        });
        let mut stopped = start.stopped.clone();
        let initialization =
            async {
                let initialized = runtime.call(
                "initialize",
                json!({"protocolVersion": 1, "clientInfo": client, "clientCapabilities": {}}),
            ).await?;
                if initialized["agentInfo"]["version"].as_str() != Some(expected_version) {
                    return Err("initialize version differs from the accepted build".into());
                }
                runtime.new_session(&[], None).await
            };
        let result = tokio::select! {
            result = initialization => result,
            Ok(()) = async { stopped.wait_for(|stopped| *stopped).await.map(|_| ()) } => Err("acp2api is exiting".into()),
        };
        let catalog = match result {
            Ok(catalog) => catalog,
            Err(error) => {
                runtime.stop().await;
                return Err(error);
            }
        };
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
        let session_id = session["sessionId"]
            .as_str()
            .ok_or("missing sessionId")?
            .to_owned();
        self.remember_config(&session_id, &session);
        self.tools.insert(session_id.clone(), tools);
        self.session = session_id;
        Ok(catalog)
    }
    fn remember_config(&mut self, session: &str, result: &Value) {
        for option in result["configOptions"].as_array().into_iter().flatten() {
            if let (Some(id), Some(value)) =
                (option["id"].as_str(), option["currentValue"].as_str())
            {
                self.config
                    .insert((session.into(), id.into()), value.into());
            }
        }
    }
    fn session_parameters(
        &self,
        tools: &[crate::conversation::ClientTool],
        system: Option<&str>,
    ) -> Result<(Value, SessionTools), String> {
        let mcp_url = self.tool_sets.insert(tools)?;
        let cwd = self.cwd.join(format!(
            "session-{}",
            crate::random_hex::<16>().map_err(|error| error.to_string())?
        ));
        std::fs::create_dir(&cwd).map_err(|e| e.to_string())?;
        let mut params = json!({
            "cwd": cwd,
            "mcpServers": [{
                "type": "http",
                "name": mcp::SERVER_NAME,
                "url": mcp_url,
                "headers": []
            }],
            "_meta": {"antigravity": {
                "enabledTools": [],
                "eagerMcpServers": [mcp::SERVER_NAME]
            }}
        });
        if let Some(system) = system {
            params["_meta"]["antigravity"]["systemPrompt"] = json!({"append":system});
        }
        let tools = SessionTools {
            cwd,
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
    pub(crate) fn forget(&mut self, session: &str) {
        self.tool_calls.clear(session);
        self.config.retain(|(id, _), _| id != session);
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
        request: PermissionRequest,
        cancel: bool,
    ) -> Result<(), String> {
        if cancel {
            self.cancel(&request.session).await?;
        }
        let outcome = RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
        self.write(json!({"jsonrpc":"2.0","id":request.id,"result":outcome}))
            .await
    }
    pub(crate) async fn cancel(&mut self, session: &str) -> Result<(), String> {
        for pending in self
            .configuring
            .values_mut()
            .filter(|pending| pending.session == session)
        {
            pending.continuation = None;
        }
        self.write(
            json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session}}),
        )
        .await
    }
    async fn refuse(&mut self, request: &Value) -> Result<(), String> {
        self.write(json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32601,"message":"Method not found"}}))
            .await
    }
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.send(method, params).await?;
        while let Some((line, _hold)) = self.lines.recv().await {
            let value = match line {
                Line::Stdout(value) => value?,
                Line::Stderr(_) => continue,
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
                    .ok_or_else(|| acp_error(&value["error"]).to_string());
            }
        }
        Err("runtime stdout closed".into())
    }
    pub(crate) async fn start_configured_prompt(
        &mut self,
        prompt: crate::conversation::PromptRequest,
        session: &str,
    ) -> Result<(), String> {
        self.configure_prompt(prompt, session, Vec::new()).await
    }
    async fn configure_prompt(
        &mut self,
        prompt: crate::conversation::PromptRequest,
        session: &str,
        mut sent: Vec<&'static str>,
    ) -> Result<(), String> {
        let desired = [
            ("model", Some(prompt.model.as_str())),
            ("thinking_level", prompt.level.map(|level| level.as_str())),
        ];
        let changed = desired
            .into_iter()
            .filter_map(|(id, value)| value.map(|value| (id, value)))
            .find(|(id, value)| {
                self.config
                    .get(&(session.into(), (*id).into()))
                    .map(String::as_str)
                    != Some(*value)
            });
        if let Some((option, value)) = changed {
            if sent.contains(&option) {
                return Err(format!(
                    "acp2api: the official runtime did not apply the {option} value {value}, so acp2api ended the prompt turn."
                ));
            }
            sent.push(option);
            let option = option.to_owned();
            let value = value.to_owned();
            let id = self
                .send(
                    "session/set_config_option",
                    json!({"sessionId":session,"configId":option,"value":value}),
                )
                .await?;
            self.configuring.insert(
                id,
                ConfiguringPrompt {
                    session: session.into(),
                    option,
                    value,
                    continuation: Some((prompt, sent)),
                },
            );
            Ok(())
        } else {
            self.start_prompt(prompt.blocks, session).await
        }
    }
    pub(crate) async fn start_prompt(
        &mut self,
        prompt: Vec<crate::conversation::PromptBlock>,
        session: &str,
    ) -> Result<(), String> {
        self.tool_calls.clear(session);
        let blocks: Vec<ContentBlock> = prompt
            .into_iter()
            .map(|block| match block {
                crate::conversation::PromptBlock::Text(text) => {
                    ContentBlock::Text(TextContent::new(text))
                }
                crate::conversation::PromptBlock::Image { media_type, base64 } => {
                    ContentBlock::Image(ImageContent::new(base64, media_type))
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
            Line::Stderr(line) => self.stderr(&String::from_utf8_lossy(&line)),
            Line::Stdout(Err(error)) => RuntimeEvent::Failed(error),
            Line::Stdout(Ok(value)) => self.decode(value).await,
        };
        (event, hold)
    }
    fn stderr(&mut self, line: &str) -> RuntimeEvent {
        if let Some((_, session)) =
            line.split_once("Antigravity SDK error processing prompt for session ")
        {
            self.stderr_session = Some(session.trim().to_owned());
        } else if let Some((_, error)) = line.split_once("Error ")
            && let Some((code, rest)) = error.split_once(", Message: ")
            && let Ok(code) = code.parse::<u16>()
            && let Some((message, rest)) = rest.split_once(", Status: ")
            && let Some((status, _)) = rest.split_once(", Details:")
            && let Some(session) = self.stderr_session.take()
        {
            return RuntimeEvent::UpstreamError {
                session,
                error: crate::messages::HttpError {
                    code,
                    status: status.into(),
                    message: message.into(),
                },
            };
        }
        RuntimeEvent::Stderr
    }
    async fn decode(&mut self, value: Value) -> RuntimeEvent {
        if value.get("method").is_none()
            && let Some(id) = value["id"].as_u64()
        {
            if let Some(pending) = self.configuring.remove(&id) {
                let session = pending.session;
                if let Some(error) = value.get("error") {
                    return RuntimeEvent::Turn {
                        session,
                        update: TurnUpdate::Failed(acp_error(error)),
                    };
                }
                self.config
                    .insert((session.clone(), pending.option), pending.value);
                self.remember_config(&session, &value["result"]);
                let Some((prompt, sent)) = pending.continuation else {
                    return RuntimeEvent::Retired { session };
                };
                return match self.configure_prompt(prompt, &session, sent).await {
                    Ok(()) => RuntimeEvent::Other,
                    Err(error) => RuntimeEvent::Turn {
                        session,
                        update: TurnUpdate::Failed(crate::messages::Message::Upstream(error)),
                    },
                };
            }
            if let Some((work, tools)) = self.creating.remove(&id) {
                let result = self.created_session(&value);
                match &result {
                    Ok((session, _)) => {
                        self.tools.insert(session.clone(), tools);
                        self.remember_config(session, &value["result"]);
                    }
                    Err(_) => {
                        self.tool_sets.remove(&tools.mcp_url);
                        if value.get("error").is_some() {
                            let _ = std::fs::remove_dir(&tools.cwd);
                        }
                    }
                }
                return RuntimeEvent::Prepared { work, result };
            }
            if let Some(session) = self.prompts.remove(&id) {
                let update = match value.get("error") {
                    Some(error) => TurnUpdate::Failed(acp_error(error)),
                    None => TurnUpdate::PromptEnded {
                        stop_reason: serde_json::from_value(value["result"]["stopReason"].clone())
                            .ok(),
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
            let permission = PermissionRequest {
                id: value["id"].clone(),
                session: session.clone(),
            };
            let update = match self.client_tool_call(&session, &request) {
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
                SessionUpdate::AgentThoughtChunk(_) => Some(TurnUpdate::ThoughtDropped),
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
    fn created_session(&self, value: &Value) -> Result<(String, crate::catalog::Catalog), String> {
        let response = value
            .get("result")
            .ok_or_else(|| acp_error(&value["error"]).to_string())?;
        let session = response["sessionId"]
            .as_str()
            .ok_or("missing sessionId")?
            .to_owned();
        let catalog = crate::catalog::Catalog::decode(response, self.clock.wall())?;
        Ok((session, catalog))
    }
    fn client_tool_call(
        &self,
        session: &str,
        request: &RequestPermissionRequest,
    ) -> Option<crate::conversation::ToolCall> {
        let title = request.tool_call.fields.title.as_deref()?;
        let name = title.strip_prefix("Run ")?.strip_suffix('?')?;
        let tools = &self.tools.get(session)?.tools;
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
            crate::conversation::ToolKind::Custom => input.get("input")?.as_str()?.to_owned(),
        };
        Some(crate::conversation::ToolCall {
            name: tool.name.clone(),
            kind: tool.kind,
            namespace: tool.namespace.clone(),
            arguments,
        })
    }
    pub(crate) async fn stop(mut self) {
        let _ = self.input.shutdown().await;
        drop(self.input);
        self.tree.stop(self.clock).await;
    }
}

pub(crate) fn program(data: &std::path::Path) -> std::path::PathBuf {
    data.join(crate::runtime_builds::COPY)
        .join(crate::runtime_builds::SERVER)
}
async fn read_line(input: &mut BufReader<ProcessRead>) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    match input.read_until(b'\n', &mut line).await {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}
fn acp_error(value: &Value) -> crate::messages::Message {
    match serde_json::from_value(value.clone()) {
        Ok(error) => crate::messages::Message::UpstreamAcp(error),
        Err(error) => {
            crate::messages::Message::Upstream(format!("acp2api: invalid ACP error: {error}"))
        }
    }
}
struct SessionTools {
    cwd: PathBuf,
    mcp_url: String,
    tools: Vec<crate::conversation::ClientTool>,
}
enum Line {
    Stdout(Result<Value, String>),
    Stderr(Vec<u8>),
    Closed,
}
pub(crate) enum RuntimeEvent {
    UpstreamError {
        session: String,
        error: crate::messages::HttpError,
    },
    Prepared {
        work: String,
        result: Result<(String, crate::catalog::Catalog), String>,
    },
    Retired {
        session: String,
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
    ThoughtDropped,
    Usage(u64),
    Permission {
        request: PermissionRequest,
        call: crate::conversation::ToolCall,
    },
    RefusedPermission {
        request: PermissionRequest,
        name: String,
    },
    UnpermittedTool(String),
    PromptEnded {
        stop_reason: Option<StopReason>,
    },
    Failed(crate::messages::Message),
}
