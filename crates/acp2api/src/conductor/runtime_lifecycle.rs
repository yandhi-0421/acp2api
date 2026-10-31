use super::{BuildReply, Core, Event, MAX_RUNTIME_FAILURES, RuntimeProcess, Work};
use crate::{
    Clock, HoldKind,
    compatibility_check::Failure as CheckFailure,
    runtime_builds::{BuildIdentity, LaunchFailure, SelectedBuild},
    runtime_driver::Runtime,
};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::watch;

pub(super) enum Replacement {
    SignIn,
    Build {
        reply: BuildReply,
        source: Option<SelectedBuild>,
        initial: bool,
    },
    Check {
        reply: BuildReply,
        folder: PathBuf,
        identity: BuildIdentity,
    },
}

pub(super) type CompatibilityCheck = crate::BoxFuture<'static, CompletedCheck>;

pub(super) struct CompletedCheck {
    identity: BuildIdentity,
    result: Result<String, CheckFailure>,
    pub(super) reply: BuildReply,
}

pub(super) struct UnavailableReason {
    pub(super) message: crate::messages::Unavailable,
    pub(super) cause: Option<std::io::ErrorKind>,
}

pub(super) enum RuntimeState {
    Stopped,
    Serving,
    Retiring { sign_in_wait_ended: bool },
    Restarting,
    Unavailable(UnavailableReason),
    Stopping,
}

impl RuntimeState {
    pub(super) fn runtime_process(&self) -> RuntimeProcess {
        match self {
            Self::Serving => RuntimeProcess::Running,
            Self::Retiring { .. } => RuntimeProcess::Stopping,
            Self::Restarting => RuntimeProcess::Starting,
            Self::Stopped | Self::Unavailable(_) | Self::Stopping => RuntimeProcess::Stopped,
        }
    }
}

impl Core {
    pub(super) async fn replace(
        &mut self,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
        publish: &watch::Sender<Arc<crate::store::Settings>>,
    ) -> Option<CompatibilityCheck> {
        let (reply, source, initial) = match self.replacement.take().unwrap() {
            Replacement::SignIn => {
                self.restart(runtime, read_open, clock, journal, builds)
                    .await;
                return None;
            }
            Replacement::Build {
                reply,
                source,
                initial,
            } => (reply, source, initial),
            Replacement::Check {
                reply,
                folder,
                identity,
            } => match builds.check_copy(&folder, &identity) {
                Ok(program) => {
                    self.runtime_state = RuntimeState::Retiring {
                        sign_in_wait_ended: false,
                    };
                    self.stop_runtime(runtime, read_open).await;
                    let builds = builds.clone();
                    let retained = self.sessions.retained();
                    return Some(Box::pin(async move {
                        let result = builds.check(program, retained).await;
                        CompletedCheck {
                            identity,
                            result,
                            reply,
                        }
                    }));
                }
                Err(error) => {
                    self.resume_after_copy_error(runtime, read_open, clock, journal)
                        .await;
                    let _ = reply.send(Err(error));
                    return None;
                }
            },
        };
        if let Some(selected) = source
            && let Err(error) = builds.copy(selected.folder)
        {
            if initial {
                self.make_unavailable(
                    crate::messages::RuntimeFault::CopyFailed {
                        version: selected.version.clone(),
                    },
                    error.io_kind(),
                    runtime,
                    read_open,
                    journal,
                    builds,
                )
                .await;
                // Startup must keep the management panel available after a failed copy.
                let _ = reply.send(Ok(None));
            } else {
                self.resume_after_copy_error(runtime, read_open, clock, journal)
                    .await;
                let _ = reply.send(Err(error));
            }
            return None;
        }
        self.restart(runtime, read_open, clock, journal, builds)
            .await;
        if matches!(self.runtime_state, RuntimeState::Stopping) {
            let _ = reply.send(Err(LaunchFailure::Stopped));
            return None;
        }
        if !initial && self.lockout.is_some() {
            let _ = reply.send(Err(self.lockout_failure(clock.now())));
            return None;
        }
        let configured = publish.borrow().agy_acp_server_path.clone();
        let result = builds
            .discover(configured)
            .await
            .map(|()| None)
            .map_err(Into::into);
        let _ = reply.send(result);
        None
    }

    async fn resume_after_copy_error(
        &mut self,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
    ) {
        if matches!(self.runtime_state, RuntimeState::Retiring { .. }) && runtime.is_some() {
            self.apply(Event::ResumeRuntime, runtime, read_open, clock, journal)
                .await;
        }
    }

    pub(super) async fn finish_check(
        &mut self,
        check: CompletedCheck,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
        publish: &watch::Sender<Arc<crate::store::Settings>>,
    ) {
        let result = match builds.finish_check(&check.identity, check.result) {
            Err(LaunchFailure::Lockout { signal, error }) => {
                self.apply_lockout(
                    signal,
                    error,
                    runtime,
                    read_open,
                    &builds.start.clock,
                    journal,
                )
                .await;
                let _ = check
                    .reply
                    .send(Err(self.lockout_failure(builds.start.clock.now())));
                return;
            }
            result => result,
        };
        self.launch(runtime, read_open, &builds.start.clock, journal, builds)
            .await;
        if self.lockout.is_some() {
            let _ = check
                .reply
                .send(Err(self.lockout_failure(builds.start.clock.now())));
            return;
        }
        let configured = publish.borrow().agy_acp_server_path.clone();
        let discovered = builds.discover(configured).await;
        let _ = check.reply.send(
            discovered
                .map_err(LaunchFailure::Runtime)
                .and(result.map(Some)),
        );
    }

    async fn apply_lockout(
        &mut self,
        signal: &'static str,
        error: crate::messages::Message,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
    ) {
        self.apply(
            Event::Lockout {
                signal,
                error,
                signalling_work: None,
            },
            runtime,
            read_open,
            clock,
            journal,
        )
        .await;
    }

    pub(super) async fn launch(
        &mut self,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
    ) {
        *read_open = false;
        loop {
            if matches!(self.runtime_state, RuntimeState::Restarting)
                && self.failures.len() >= MAX_RUNTIME_FAILURES
            {
                self.make_unavailable(
                    crate::messages::RuntimeFault::RepeatedFailures,
                    None,
                    runtime,
                    read_open,
                    journal,
                    builds,
                )
                .await;
                return;
            }
            let retained = self.cleanup_before_start.then(|| self.sessions.retained());
            self.cleanup_before_start = true;
            match builds.launch(retained).await {
                Ok((started, catalog)) => {
                    let sign_in_required = catalog.is_none();
                    let session = catalog.as_ref().map(|_| started.session_id().into());
                    self.replaced(session, catalog);
                    *runtime = Some(started);
                    *read_open = true;
                    self.apply(
                        Event::RuntimeReady { sign_in_required },
                        runtime,
                        read_open,
                        clock,
                        journal,
                    )
                    .await;
                    return;
                }
                Err(LaunchFailure::Runtime(_) | LaunchFailure::Io(_) | LaunchFailure::Owner(_)) => {
                    self.apply(Event::RuntimeFailure, runtime, read_open, clock, journal)
                        .await;
                }
                Err(LaunchFailure::Lockout { signal, error }) => {
                    self.apply_lockout(signal, error, runtime, read_open, clock, journal)
                        .await;
                    return;
                }
                Err(LaunchFailure::Stopped) => {
                    self.apply(Event::Stop, runtime, read_open, clock, journal)
                        .await;
                    return;
                }
                Err(error @ (LaunchFailure::NoExecutable | LaunchFailure::NotAccepted)) => {
                    let fault = if matches!(error, LaunchFailure::NoExecutable) {
                        crate::messages::RuntimeFault::NoExecutable
                    } else {
                        crate::messages::RuntimeFault::NotAccepted
                    };
                    self.make_unavailable(fault, None, runtime, read_open, journal, builds)
                        .await;
                    return;
                }
            }
        }
    }

    async fn make_unavailable(
        &mut self,
        fault: crate::messages::RuntimeFault,
        cause: Option<std::io::ErrorKind>,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
    ) {
        self.apply(
            Event::RuntimeUnavailable(UnavailableReason {
                message: builds.unavailable(fault),
                cause,
            }),
            runtime,
            read_open,
            &builds.start.clock,
            journal,
        )
        .await;
    }

    pub(super) async fn restart(
        &mut self,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
    ) {
        let restarting = async {
            self.stop_runtime(runtime, read_open).await;
            self.launch(runtime, read_open, clock, journal, builds)
                .await;
        };
        tokio::pin!(restarting);
        std::future::poll_fn(|cx| {
            // Keep stop-to-launch work visible, releasing it whenever the restart waits.
            let _processing = clock.hold(HoldKind::TurnEvent);
            restarting.as_mut().poll(cx)
        })
        .await;
    }

    pub(super) async fn stop_runtime(
        &mut self,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
    ) {
        *read_open = false;
        if let Some(old) = runtime.take() {
            self.sessions
                .unload(|session| old.session_cwd(session).to_path_buf());
            old.stop().await;
        }
        self.replaced(None, None);
    }

    pub(super) fn replacement_ready(&self) -> bool {
        self.lockout.is_none()
            && matches!(
                self.runtime_state,
                RuntimeState::Retiring { .. }
                    | RuntimeState::Restarting
                    | RuntimeState::Unavailable(_)
            )
            && !self.work.values().any(Work::occupies_place)
    }

    pub(super) fn replaced(
        &mut self,
        session: Option<String>,
        catalog: Option<crate::catalog::Catalog>,
    ) {
        self.sessions.replaced(session);
        self.catalog = catalog;
    }
}
