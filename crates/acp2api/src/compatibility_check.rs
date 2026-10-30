use crate::{
    messages::{Message, OwnerMessage},
    runtime_builds::LaunchFailure,
    runtime_driver::{Runtime, RuntimeEvent, RuntimeStart, TurnUpdate},
};
use agent_client_protocol_schema::v1::StopReason;
use serde_json::json;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub(crate) enum Incompatibility {
    NoMcpOverHttp,
    NoSystemPromptAddition,
    MissingProtocolSupport,
    NoVersion,
    NoSystemCodeword,
    NoPermissionRequest,
    ToolArgumentsDiffer,
    SessionLoadFailed,
    NoToolResultText,
    NoSessionFile,
    SessionFilesInGeminiFolder,
    UnexpectedStopReason,
    BuiltInToolRan(String),
    PermissionForAnotherTool,
    RuntimeFailed,
    RuntimeExitedDuringLoad,
    RuntimeExited,
    Refused(Message),
}
impl std::fmt::Display for Incompatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::NoMcpOverHttp => "initialize does not declare MCP over HTTP",
            Self::NoSystemPromptAddition => {
                "initialize does not declare the system prompt addition"
            }
            Self::MissingProtocolSupport => {
                "the runtime must support ACP 1, session/load and oauth-personal"
            }
            Self::NoVersion => "initialize has no version",
            Self::NoSystemCodeword => "the reply did not contain the system codeword",
            Self::NoPermissionRequest => "no client tool permission request",
            Self::ToolArgumentsDiffer => "the client tool arguments did not match the prompt",
            Self::SessionLoadFailed => "session/load failed during the compatibility check",
            Self::NoToolResultText => "the reply did not contain the tool result sent as text",
            Self::NoSessionFile => "the check session has no file in the runtime home",
            Self::SessionFilesInGeminiFolder => {
                "the check session has files in the owner's Gemini folder"
            }
            Self::UnexpectedStopReason => "unexpected compatibility prompt stop reason",
            Self::BuiltInToolRan(name) => {
                return write!(
                    f,
                    "the built-in tool {name} ran during the compatibility check"
                );
            }
            Self::PermissionForAnotherTool => {
                "a permission request did not name the supplied client tool"
            }
            Self::RuntimeFailed => "runtime failed during the compatibility check",
            Self::RuntimeExitedDuringLoad => "the check runtime exited during session/load",
            Self::RuntimeExited => "the check runtime exited unexpectedly",
            Self::Refused(message) => return message.fmt(f),
        };
        f.write_str(text)
    }
}
pub(crate) enum Failure {
    Incompatible(Incompatibility),
    Runtime(LaunchFailure),
    Internal(LaunchFailure),
}
impl Failure {
    fn lockout(event: &RuntimeEvent, failed_reply: &str) -> Option<Self> {
        let (error, signal) = event.upstream_error(failed_reply)?;
        Some(Self::Runtime(LaunchFailure::Lockout {
            signal: signal?,
            error,
        }))
    }
}
impl From<LaunchFailure> for Failure {
    fn from(error: LaunchFailure) -> Self {
        Self::Runtime(error)
    }
}
impl From<crate::runtime_driver::CommandError> for Failure {
    fn from(error: crate::runtime_driver::CommandError) -> Self {
        match error {
            crate::runtime_driver::CommandError::Io(error) => {
                Self::Runtime(LaunchFailure::Io(error))
            }
            crate::runtime_driver::CommandError::Session(error) => {
                Self::Internal(LaunchFailure::Owner(OwnerMessage::Api(error)))
            }
        }
    }
}
pub(crate) async fn run(
    start: &RuntimeStart,
    program: PathBuf,
    gemini: Option<PathBuf>,
) -> Result<String, Failure> {
    let gemini = gemini.ok_or_else(|| {
        Failure::Internal(LaunchFailure::Owner(OwnerMessage::GeminiFolderNotFound))
    })?;
    let mut runtime = Runtime::open(start, program)
        .await
        .map_err(Failure::Runtime)?;
    let mut failed_reply = String::new();
    let checking = async {
        let initialized = start
            .answer(crate::runtime_driver::ANSWER_TIMEOUT, async {
                runtime.initialize(start).await.map_err(Failure::Runtime)
            })
            .await?;
        if initialized["agentCapabilities"]["mcpCapabilities"]["http"] != true {
            return Err(Failure::Incompatible(Incompatibility::NoMcpOverHttp));
        }
        if initialized["agentCapabilities"]["_meta"]["antigravity"]["systemPrompt"]["append"]
            != true
        {
            return Err(Failure::Incompatible(
                Incompatibility::NoSystemPromptAddition,
            ));
        }
        if initialized["protocolVersion"] != 1
            || initialized["agentCapabilities"]["loadSession"] != true
            || !initialized["authMethods"]
                .as_array()
                .is_some_and(|methods| {
                    methods
                        .iter()
                        .any(|method| method["id"] == "oauth-personal")
                })
        {
            return Err(Failure::Incompatible(
                Incompatibility::MissingProtocolSupport,
            ));
        }
        let version = initialized["agentInfo"]["version"]
            .as_str()
            .filter(|version| !version.is_empty())
            .ok_or_else(|| Failure::Incompatible(Incompatibility::NoVersion))?
            .to_owned();
        let codeword = crate::random_hex::<16>()
            .map_err(|error| Failure::Internal(error.to_string().into()))?;
        let tools = vec![crate::conversation::ClientTool {
            name: format!(
                "compatibility_{}",
                crate::random_hex::<12>()
                    .map_err(|error| Failure::Internal(error.to_string().into()))?
            ),
            description: "Return a value for the compatibility check.".into(),
            schema: json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}),
            kind: crate::conversation::ToolKind::Function,
            namespace: None,
        }];
        let system = format!(
            "For this compatibility check the secret codeword is {codeword}. When asked for the codeword, repeat it exactly. Use only the supplied client tool when asked to use a tool."
        );
        start
            .answer(crate::runtime_driver::ANSWER_TIMEOUT, async {
                runtime
                    .new_session(&tools, Some(&system))
                    .await
                    .map_err(|error| Failure::Runtime(error.into()))
            })
            .await?;
        let session = runtime.session_id().to_owned();
        let (text, call) = check_prompt(
            &mut runtime,
            start,
            &session,
            "Reply with the secret codeword from the system instruction. Do not use any tool.",
            &mut failed_reply,
        )
        .await?;
        if !text.contains(&codeword) || call.is_some() {
            return Err(Failure::Incompatible(Incompatibility::NoSystemCodeword));
        }
        let (_, call) = check_prompt(&mut runtime, start, &session, "Call the supplied client tool with exactly {\"value\":\"checked text\"}. Do not use a built-in tool.", &mut failed_reply).await?;
        let call =
            call.ok_or_else(|| Failure::Incompatible(Incompatibility::NoPermissionRequest))?;
        if call.arguments != r#"{"value":"checked text"}"# {
            return Err(Failure::Incompatible(Incompatibility::ToolArgumentsDiffer));
        }
        failed_reply.clear();
        let cwd = runtime.session_cwd(&session).to_owned();
        runtime.forget(&session);
        start
            .answer(crate::runtime_driver::ANSWER_TIMEOUT, async {
                runtime
                    .request_session(
                        "compatibility-check".into(),
                        &tools,
                        Some(&system),
                        Some((session.clone(), cwd)),
                    )
                    .await?;
                loop {
                    let (event, _hold) = runtime.next().await;
                    if let Some(error) = Failure::lockout(&event, "") {
                        return Err(error);
                    }
                    match event {
                        RuntimeEvent::Prepared { result, .. } => {
                            result.map_err(|error| {
                                Failure::Incompatible(Incompatibility::Refused(error))
                            })?;
                            break;
                        }
                        RuntimeEvent::Other | RuntimeEvent::Stderr => {}
                        RuntimeEvent::Closed => {
                            return Err(Failure::Incompatible(
                                Incompatibility::RuntimeExitedDuringLoad,
                            ));
                        }
                        _ => {
                            return Err(Failure::Incompatible(Incompatibility::SessionLoadFailed));
                        }
                    }
                }
                Ok(())
            })
            .await?;
        let result = crate::random_hex::<16>()
            .map_err(|error| Failure::Internal(error.to_string().into()))?;
        let prompt = format!(
            "The tool {} returned this result as text: {result}\nReply with that result exactly, without calling any tool.",
            call.name
        );
        let (text, call) =
            check_prompt(&mut runtime, start, &session, &prompt, &mut failed_reply).await?;
        if !text.contains(&result) || call.is_some() {
            return Err(Failure::Incompatible(Incompatibility::NoToolResultText));
        }
        Ok((version, session))
    };
    let result = checking.await;
    let mut lockout = None;
    // A prompt result can precede stderr evidence; a lockout read before the bound wins.
    start
        .stop_and_drain_stderr(runtime, |event| {
            if let Some(error) = Failure::lockout(&event, &failed_reply) {
                lockout = Some(error);
            }
        })
        .await;
    if let Some(error) = lockout {
        return Err(error);
    }
    let (version, session) = result?;
    let files = session_files(&start.data.join("runtime-home"), &session)
        .map_err(|error| Failure::Internal(error.to_string().into()))?;
    if !files.contains(&OsString::from(format!("{session}.db"))) {
        return Err(Failure::Incompatible(Incompatibility::NoSessionFile));
    }
    if !session_files(&gemini, &session)
        .map_err(|error| Failure::Internal(error.to_string().into()))?
        .is_empty()
    {
        return Err(Failure::Incompatible(
            Incompatibility::SessionFilesInGeminiFolder,
        ));
    }
    Ok(version)
}

async fn check_prompt(
    runtime: &mut Runtime,
    start: &RuntimeStart,
    session: &str,
    text: &str,
    reply: &mut String,
) -> Result<(String, Option<crate::conversation::ToolCall>), Failure> {
    runtime
        .start_prompt(
            vec![crate::conversation::PromptBlock::Text(text.into())],
            session,
        )
        .await?;
    reply.clear();
    let text = reply;
    let mut call = None;
    let mut deadline = start
        .clock
        .now()
        .saturating_add(crate::runtime_driver::ANSWER_TIMEOUT);
    loop {
        let (event, _hold) = start
            .answer(deadline.saturating_sub(start.clock.now()), async {
                Ok::<_, Failure>(runtime.next().await)
            })
            .await?;
        if let Some(error) = Failure::lockout(&event, text) {
            return Err(error);
        }
        match event {
            RuntimeEvent::Turn { update, .. } => {
                if call.is_none() {
                    deadline = start
                        .clock
                        .now()
                        .saturating_add(crate::runtime_driver::ANSWER_TIMEOUT);
                }
                match update {
                    TurnUpdate::Text(chunk) => text.push_str(&chunk),
                    TurnUpdate::Permission {
                        request,
                        call: tool,
                    } => {
                        runtime.permission(request).await?;
                        runtime.cancel(session).await?;
                        call = Some(tool);
                        deadline = start
                            .clock
                            .now()
                            .saturating_add(crate::conductor::CANCEL_TIMEOUT);
                    }
                    TurnUpdate::PromptEnded { stop_reason } => {
                        let expected = if call.is_some() {
                            StopReason::Cancelled
                        } else {
                            StopReason::EndTurn
                        };
                        if stop_reason != Some(expected) {
                            return Err(Failure::Incompatible(
                                Incompatibility::UnexpectedStopReason,
                            ));
                        }
                        return Ok((text.clone(), call));
                    }
                    TurnUpdate::Activity
                    | TurnUpdate::ThoughtDropped
                    | TurnUpdate::Usage { .. } => {}
                    TurnUpdate::UnpermittedTool(name) => {
                        return Err(Failure::Incompatible(Incompatibility::BuiltInToolRan(name)));
                    }
                    TurnUpdate::RefusedPermission { request, .. } => {
                        runtime.permission(request).await?;
                        return Err(Failure::Incompatible(
                            Incompatibility::PermissionForAnotherTool,
                        ));
                    }
                    TurnUpdate::Failed(error) => {
                        return Err(Failure::Incompatible(Incompatibility::Refused(error)));
                    }
                }
            }
            RuntimeEvent::Other | RuntimeEvent::Stderr => {}
            RuntimeEvent::Closed => {
                return Err(Failure::Incompatible(Incompatibility::RuntimeExited));
            }
            _ => {
                return Err(Failure::Incompatible(Incompatibility::RuntimeFailed));
            }
        }
    }
}

fn session_files(home: &Path, session: &str) -> std::io::Result<Vec<OsString>> {
    let entries = match std::fs::read_dir(home.join("antigravity-acp/conversations")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let prefix = format!("{session}.");
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if (text == session || text.starts_with(&prefix)) && entry.file_type()?.is_file() {
            files.push(name);
        }
    }
    Ok(files)
}
#[cfg(test)]
mod tests;
