use crate::{
    runtime_builds::LaunchFailure,
    runtime_driver::{Runtime, RuntimeEvent, RuntimeStart, TurnUpdate},
};
use agent_client_protocol_schema::v1::StopReason;
use serde_json::json;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
pub(crate) enum Failure {
    Incompatible(String),
    Runtime(LaunchFailure),
    Internal(String),
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
                Self::Runtime(LaunchFailure::Runtime(error.to_string()))
            }
            crate::runtime_driver::CommandError::Session(error) => Self::Internal(error),
        }
    }
}
pub(crate) async fn run(
    start: &RuntimeStart,
    program: PathBuf,
    gemini: Option<PathBuf>,
) -> Result<String, Failure> {
    let gemini = gemini
        .ok_or_else(|| Failure::Internal("cannot locate the owner's Gemini folder".into()))?;
    let mut runtime = Runtime::open(start, program)
        .await
        .map_err(Failure::Runtime)?;
    let checking = async {
        let initialized = start
            .answer(crate::runtime_driver::ANSWER_TIMEOUT, async {
                runtime.initialize(start).await.map_err(Failure::Runtime)
            })
            .await?;
        if initialized["agentCapabilities"]["mcpCapabilities"]["http"] != true {
            return Err(Failure::Incompatible(
                "initialize does not declare MCP over HTTP".into(),
            ));
        }
        if initialized["agentCapabilities"]["_meta"]["antigravity"]["systemPrompt"]["append"]
            != true
        {
            return Err(Failure::Incompatible(
                "initialize does not declare the system prompt addition".into(),
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
                "the runtime must support ACP 1, session/load and oauth-personal".into(),
            ));
        }
        let version = initialized["agentInfo"]["version"]
            .as_str()
            .filter(|version| !version.is_empty())
            .ok_or_else(|| Failure::Incompatible("initialize has no version".into()))?
            .to_owned();
        let codeword =
            crate::random_hex::<16>().map_err(|error| Failure::Internal(error.to_string()))?;
        let tools = vec![crate::conversation::ClientTool {
            name: format!(
                "compatibility_{}",
                crate::random_hex::<12>().map_err(|error| Failure::Internal(error.to_string()))?
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
        )
        .await?;
        if !text.contains(&codeword) || call.is_some() {
            return Err(Failure::Incompatible(
                "the reply did not contain the system codeword".into(),
            ));
        }
        let (_, call) = check_prompt(&mut runtime, start, &session, "Call the supplied client tool with exactly {\"value\":\"checked text\"}. Do not use a built-in tool.").await?;
        let call =
            call.ok_or_else(|| Failure::Incompatible("no client tool permission request".into()))?;
        if call.arguments != r#"{"value":"checked text"}"# {
            return Err(Failure::Incompatible(
                "the client tool arguments did not match the prompt".into(),
            ));
        }
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
                    match event {
                        RuntimeEvent::Prepared { result, .. } => {
                            result.map_err(Failure::Incompatible)?;
                            break;
                        }
                        RuntimeEvent::Other | RuntimeEvent::Stderr => {}
                        RuntimeEvent::Closed => {
                            return Err(Failure::Runtime(LaunchFailure::Runtime(
                                "the check runtime exited during session/load".into(),
                            )));
                        }
                        _ => {
                            return Err(Failure::Incompatible(
                                "session/load failed during the compatibility check".into(),
                            ));
                        }
                    }
                }
                Ok(())
            })
            .await?;
        let result =
            crate::random_hex::<16>().map_err(|error| Failure::Internal(error.to_string()))?;
        let prompt = format!(
            "The tool {} returned this result as text: {result}\nReply with that result exactly, without calling any tool.",
            call.name
        );
        let (text, call) = check_prompt(&mut runtime, start, &session, &prompt).await?;
        if !text.contains(&result) || call.is_some() {
            return Err(Failure::Incompatible(
                "the reply did not contain the tool result sent as text".into(),
            ));
        }
        Ok((version, session))
    };
    let result = checking.await;
    runtime.stop().await;
    let (version, session) = result?;
    let files = session_files(&start.data.join("runtime-home"), &session)
        .map_err(|error| Failure::Internal(error.to_string()))?;
    if !files.contains(&OsString::from(format!("{session}.db"))) {
        return Err(Failure::Incompatible(
            "the check session has no file in the runtime home".into(),
        ));
    }
    if !session_files(&gemini, &session)
        .map_err(|error| Failure::Internal(error.to_string()))?
        .is_empty()
    {
        return Err(Failure::Incompatible(
            "the check session has files in the owner's Gemini folder".into(),
        ));
    }
    Ok(version)
}

async fn check_prompt(
    runtime: &mut Runtime,
    start: &RuntimeStart,
    session: &str,
    text: &str,
) -> Result<(String, Option<crate::conversation::ToolCall>), Failure> {
    runtime
        .start_prompt(
            vec![crate::conversation::PromptBlock::Text(text.into())],
            session,
        )
        .await?;
    let mut text = String::new();
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
                                "unexpected compatibility prompt stop reason".into(),
                            ));
                        }
                        return Ok((text, call));
                    }
                    TurnUpdate::ThoughtDropped | TurnUpdate::Usage(_) => {}
                    TurnUpdate::UnpermittedTool(name) => {
                        return Err(Failure::Incompatible(format!(
                            "the built-in tool {name} ran during the compatibility check"
                        )));
                    }
                    TurnUpdate::RefusedPermission { request, .. } => {
                        runtime.permission(request).await?;
                        return Err(Failure::Incompatible(
                            "a permission request did not name the supplied client tool".into(),
                        ));
                    }
                    TurnUpdate::Failed(error) => {
                        return Err(Failure::Incompatible(error.to_string()));
                    }
                }
            }
            RuntimeEvent::Other | RuntimeEvent::Stderr => {}
            RuntimeEvent::Closed => {
                return Err(Failure::Runtime(LaunchFailure::Runtime(
                    "the check runtime exited unexpectedly".into(),
                )));
            }
            _ => {
                return Err(Failure::Incompatible(
                    "runtime failed during the compatibility check".into(),
                ));
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
