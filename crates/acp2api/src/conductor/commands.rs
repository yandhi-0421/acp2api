use super::{
    BuildReply, Conductor, Core, Event, Output, PendingRequest, RuntimeState, Session, State, Work,
    WorkId, lockout,
};
use crate::{
    Clock, HoldKind, Moment,
    conversation::{PromptRequest, TurnEvent},
    messages::OwnerMessage,
    runtime_builds::LaunchFailure,
    runtime_driver::{Runtime, RuntimeEvent},
};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};
use tokio::sync::oneshot;

pub(super) enum Command {
    ModelsRead {
        reply: oneshot::Sender<Result<(), OwnerMessage>>,
        result: Result<(), OwnerMessage>,
    },
    Authenticate,
    Lockout(crate::store::Lockout),
    RefuseBuildChange {
        reply: BuildReply,
        error: crate::messages::Message,
    },
    AdmitChecked {
        id: WorkId,
        session: String,
    },
    RefusePending {
        request: Box<PendingRequest>,
        error: crate::messages::Message,
    },
    State(State, Option<std::io::ErrorKind>),
    Usage {
        work: String,
        session: String,
    },
    UpstreamLog {
        session: String,
        error: crate::messages::HttpError,
        late: bool,
    },
    Complete(crate::journal::Completion),
    Safety(crate::journal::SafetyReport),
    Diagnostic(crate::journal::Request, crate::journal::Diagnostic),
    Cancel(String),
    Prepare {
        work: String,
        settings: Session,
        load: Option<(String, PathBuf)>,
    },
    Prompt {
        work: String,
        session: String,
        prompt: PromptRequest,
    },
    Emit(Output, TurnEvent),
    Permission(crate::runtime_driver::PermissionRequest),
    Forget(String),
}

impl Conductor {
    pub(super) async fn commands(
        core: &mut Core,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
        commands: &mut VecDeque<Command>,
    ) {
        while let Some(command) = commands.pop_front() {
            match command {
                Command::ModelsRead { reply, result } => {
                    let _ = reply.send(result);
                }
                Command::RefuseBuildChange { reply, error } => {
                    let _ = reply.send(Err(LaunchFailure::Owner(OwnerMessage::Api(error))));
                }
                Command::Lockout(evidence) => {
                    lockout::persist(&core.state_path, evidence, journal, clock.as_ref());
                    core.stop_runtime(runtime, read_open).await;
                    core.lifecycle.lockout_stopped();
                    core.publish_view();
                }
                Command::Authenticate => {
                    if let Err(error) = runtime.as_mut().unwrap().authenticate().await {
                        commands.extend(core.command_failed(
                            None,
                            error,
                            clock.now(),
                            clock.wall(),
                        ));
                    }
                }
                Command::AdmitChecked { id, session } => {
                    let Work::Checking(request) = core.work.remove(&id).unwrap() else {
                        unreachable!()
                    };
                    let checked = core.check(&request.turn, clock.now());
                    let accepted = core
                        .accept(request, checked, journal, clock.as_ref())
                        .map(Box::new);
                    commands.extend(core.handle(
                        Event::Checked {
                            id,
                            accepted,
                            session,
                        },
                        clock.now(),
                        clock.wall(),
                    ));
                }
                Command::RefusePending { request, error } => {
                    let _ = request.admission.send(Err(crate::messages::Refusal {
                        message: error,
                        hold: clock.hold(HoldKind::TurnEvent),
                    }));
                }
                Command::State(state, cause) => {
                    journal.state(state.name(), cause, clock.as_ref());
                }
                Command::Usage { work, session } => {
                    let result = match readable(runtime, *read_open) {
                        Ok(runtime) => {
                            runtime
                                .start_prompt(
                                    vec![crate::conversation::PromptBlock::Text("/usage".into())],
                                    &session,
                                )
                                .await
                        }
                        Err(error) => Err(error),
                    };
                    if let Err(error) = result {
                        commands.extend(core.command_failed(
                            Some(work),
                            error,
                            clock.now(),
                            clock.wall(),
                        ));
                    }
                }
                Command::UpstreamLog {
                    session,
                    error,
                    late,
                } => journal.upstream(&session, &error, late, clock.as_ref()),
                Command::Cancel(session) => {
                    if let Some(runtime) = runtime.as_mut()
                        && let Err(error) = runtime.cancel(&session).await
                    {
                        commands.extend(core.command_failed(
                            None,
                            error,
                            clock.now(),
                            clock.wall(),
                        ));
                    }
                }
                Command::Safety(report) => journal.safety(report, clock.as_ref()),
                Command::Diagnostic(request, diagnostic) => {
                    journal.diagnostic(&request, diagnostic, clock.as_ref())
                }
                Command::Complete(completion) => journal.complete(completion, clock.as_ref()),
                Command::Prepare {
                    work,
                    settings,
                    load,
                } => {
                    let result = match readable(runtime, *read_open) {
                        Ok(runtime) => {
                            runtime
                                .request_session(
                                    work.clone(),
                                    &settings.tools,
                                    settings.system.as_deref(),
                                    load,
                                )
                                .await
                        }
                        Err(error) => Err(error),
                    };
                    if let Err(error) = result {
                        commands.extend(core.command_failed(
                            Some(work),
                            error,
                            clock.now(),
                            clock.wall(),
                        ));
                    }
                }
                Command::Prompt {
                    work,
                    session,
                    prompt,
                } => {
                    let result = match readable(runtime, *read_open) {
                        Ok(runtime) => runtime.start_configured_prompt(prompt, &session).await,
                        Err(error) => Err(error),
                    };
                    if let Err(error) = result {
                        commands.extend(core.command_failed(
                            Some(work),
                            error,
                            clock.now(),
                            clock.wall(),
                        ));
                    }
                }
                Command::Permission(request) => {
                    if let Some(runtime) = runtime.as_mut()
                        && let Err(error) = runtime.permission(request).await
                    {
                        commands.extend(core.command_failed(
                            None,
                            error,
                            clock.now(),
                            clock.wall(),
                        ));
                    }
                }
                Command::Forget(session) => {
                    if let Some(runtime) = runtime.as_mut() {
                        runtime.forget(&session);
                    }
                }
                Command::Emit(output, event) => {
                    let _ = output.send((event, clock.hold(HoldKind::TurnEvent)));
                }
            }
        }
    }
}

fn readable(
    runtime: &mut Option<Runtime>,
    read_open: bool,
) -> Result<&mut Runtime, crate::runtime_driver::CommandError> {
    runtime
        .as_mut()
        .filter(|_| read_open)
        .ok_or(crate::runtime_driver::CommandError::Session(
            crate::messages::Message::RuntimeNotAvailable,
        ))
}

impl Core {
    pub(super) fn command_failed(
        &mut self,
        work: Option<String>,
        error: crate::runtime_driver::CommandError,
        now: Moment,
        wall: std::time::SystemTime,
    ) -> Vec<Command> {
        if self.lockout.is_some() || matches!(self.lifecycle.state(), RuntimeState::Restarting) {
            return Vec::new();
        }
        let event = match error {
            crate::runtime_driver::CommandError::Io(_)
                if matches!(self.lifecycle.state(), RuntimeState::Stopping) =>
            {
                Event::Stop
            }
            crate::runtime_driver::CommandError::Io(_) => Event::RuntimeFailure,
            crate::runtime_driver::CommandError::Session(error) => match work {
                Some(work) => Event::StartFailed(work, error),
                None => Event::Runtime(RuntimeEvent::Failed(error.to_string())),
            },
        };
        self.handle(event, now, wall)
    }
}
