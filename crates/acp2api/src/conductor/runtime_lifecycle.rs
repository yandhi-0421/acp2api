use super::{Command, Core, Event, Failure, RuntimeProcess, Work};
use crate::{
    Clock, HoldKind, Moment,
    compatibility_check::Failure as CheckFailure,
    messages::{Message, OwnerMessage},
    runtime_builds::{BuildIdentity, LaunchFailure, SelectedBuild},
    runtime_driver::Runtime,
};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};
use tokio::sync::{oneshot, watch};

pub(super) type BuildReply = oneshot::Sender<Result<Option<OwnerMessage>, LaunchFailure>>;

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

pub(super) enum BuildSelection {
    Retire(Replacement),
    Ready(Replacement),
    StartFailed(LaunchFailure),
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

pub(super) struct Lifecycle {
    state: RuntimeState,
    failures: VecDeque<Moment>,
    cleanup_before_start: bool,
    changes: VecDeque<(crate::runtime_builds::Action, BuildReply)>,
    replacement: Option<Replacement>,
    deferred_start: bool,
}

impl Lifecycle {
    pub(super) fn new(state: RuntimeState) -> Self {
        Self {
            state,
            failures: VecDeque::new(),
            cleanup_before_start: false,
            changes: VecDeque::new(),
            replacement: None,
            deferred_start: false,
        }
    }

    pub(super) fn queue(&mut self, action: crate::runtime_builds::Action, reply: BuildReply) {
        self.changes.push_back((action, reply));
    }

    pub(super) fn state(&self) -> &RuntimeState {
        &self.state
    }

    pub(super) fn defer_start(&mut self) {
        self.deferred_start = true;
    }

    pub(super) fn replacement_pending(&self) -> bool {
        self.replacement.is_some()
    }

    pub(super) fn plan(&mut self, replacement: Replacement) {
        self.replacement = Some(replacement);
    }

    pub(super) fn state_file_reason(&self) -> Option<&crate::messages::Unavailable> {
        match &self.state {
            RuntimeState::Unavailable(reason)
                if matches!(
                    reason.message.fault,
                    crate::messages::RuntimeFault::StateFileNotWritable
                ) =>
            {
                Some(&reason.message)
            }
            _ => None,
        }
    }

    pub(super) fn state_file_made(&mut self) {
        if std::mem::take(&mut self.deferred_start) {
            let (reply, _) = oneshot::channel();
            self.changes
                .push_back((crate::runtime_builds::Action::Initial, reply));
        }
    }

    pub(super) fn lockout_stopped(&mut self) {
        self.state = RuntimeState::Stopped;
    }

    pub(super) fn lockout_ended(&mut self, now: Moment) {
        self.failures
            .retain(|at| now.saturating_sub(*at) <= RUNTIME_FAILURE_WINDOW);
        if std::mem::take(&mut self.deferred_start) {
            // The start of this process has not run, so the end runs it as a start.
            let (reply, _) = oneshot::channel();
            self.changes
                .push_back((crate::runtime_builds::Action::Initial, reply));
            self.state = RuntimeState::Serving;
        } else {
            self.state = RuntimeState::Restarting;
        }
    }

    pub(super) fn refuse_build_changes(
        &mut self,
        message: &crate::messages::Message,
        commands: &mut Vec<Command>,
    ) {
        if let Some(replacement) = self.replacement.take() {
            match replacement {
                Replacement::Build { reply, .. } | Replacement::Check { reply, .. } => {
                    commands.push(Command::RefuseBuildChange {
                        reply,
                        error: message.clone(),
                    });
                }
                Replacement::SignIn => {}
            }
        }
        commands.extend(
            self.changes
                .drain(..)
                .map(|(_, reply)| Command::RefuseBuildChange {
                    reply,
                    error: message.clone(),
                }),
        );
    }
}

#[cfg(test)]
impl Lifecycle {
    pub(super) fn set_state(&mut self, state: RuntimeState) {
        self.state = state;
    }

    pub(super) fn set_cleanup_before_start(&mut self) {
        self.cleanup_before_start = true;
    }

    pub(super) fn failure_count(&self) -> usize {
        self.failures.len()
    }

    pub(super) fn replacement(&self) -> Option<&Replacement> {
        self.replacement.as_ref()
    }

    pub(super) fn pop_change(&mut self) -> Option<(crate::runtime_builds::Action, BuildReply)> {
        self.changes.pop_front()
    }
}

const MAX_RUNTIME_FAILURES: usize = 3;
pub(super) const RUNTIME_FAILURE_WINDOW: Moment = 10 * 60 * 1000;

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
        let (reply, source, initial) = match self.lifecycle.replacement.take().unwrap() {
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
                    self.lifecycle.state = RuntimeState::Retiring {
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
        if matches!(self.lifecycle.state, RuntimeState::Stopping) {
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
        if matches!(self.lifecycle.state, RuntimeState::Retiring { .. }) && runtime.is_some() {
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
            if matches!(self.lifecycle.state, RuntimeState::Restarting)
                && self.lifecycle.failures.len() >= MAX_RUNTIME_FAILURES
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
            let retained = self
                .lifecycle
                .cleanup_before_start
                .then(|| self.sessions.retained());
            self.lifecycle.cleanup_before_start = true;
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
                self.lifecycle.state,
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

    pub(super) fn stopping(&mut self, now: Moment, commands: &mut Vec<Command>) {
        self.sign_in.end_wait();
        self.lifecycle.state = RuntimeState::Stopping;
        self.end_all_work(
            "stopped",
            |_| crate::messages::Message::Stopped,
            now,
            commands,
        );
    }

    pub(super) fn runtime_unavailable(
        &mut self,
        reason: UnavailableReason,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        self.sign_in.end_wait();
        if matches!(self.lifecycle.replacement, Some(Replacement::SignIn)) {
            self.lifecycle.replacement = None;
        }
        let error = crate::messages::Message::RuntimeUnavailable(reason.message.clone());
        self.lifecycle.state = RuntimeState::Unavailable(reason);
        self.refuse_queued(
            crate::journal::Outcome::Refused("runtime_unavailable"),
            error.clone(),
            commands,
        );
        let queries: Vec<_> = self
            .work
            .iter()
            .filter_map(|(id, work)| matches!(work, Work::QuotaQueued { .. }).then_some(*id))
            .collect();
        for id in queries {
            self.finish(id, Err(Failure::at(error.clone(), now)), now, commands);
        }
    }

    pub(super) fn retire_runtime(&mut self) {
        let sign_in_wait_ended = self.sign_in.end_wait()
            || matches!(
                self.lifecycle.state,
                RuntimeState::Retiring {
                    sign_in_wait_ended: true
                }
            );
        if !matches!(self.lifecycle.state, RuntimeState::Unavailable(_)) {
            self.lifecycle.state = RuntimeState::Retiring { sign_in_wait_ended };
        }
    }

    pub(super) fn resume_runtime(&mut self) {
        if matches!(
            self.lifecycle.state,
            RuntimeState::Retiring {
                sign_in_wait_ended: true
            }
        ) {
            self.lifecycle
                .replacement
                .get_or_insert(Replacement::SignIn);
        } else {
            self.lifecycle.state = RuntimeState::Serving;
        }
    }

    pub(super) fn runtime_failed(&mut self, now: Moment, commands: &mut Vec<Command>) {
        self.refuse_model_reads(&crate::messages::Message::RuntimeFailed, commands);
        self.sign_in.end_wait();
        self.lifecycle.state = RuntimeState::Restarting;
        self.sessions.runtime_failed();
        self.lifecycle
            .failures
            .retain(|at| now.saturating_sub(*at) <= RUNTIME_FAILURE_WINDOW);
        self.lifecycle.failures.push_back(now);
        self.end_occupying_work(crate::messages::Message::RuntimeFailed, now, commands);
        self.catalog = None;
    }

    pub(super) fn build_change(
        &mut self,
        action: crate::runtime_builds::Action,
        reply: BuildReply,
    ) {
        if let Some(evidence) = &self.lockout {
            let result = if matches!(action, crate::runtime_builds::Action::Initial) {
                self.lifecycle.defer_start();
                Ok(None)
            } else {
                Err(LaunchFailure::Owner(OwnerMessage::Api(Message::lockout(
                    evidence.evidence(),
                    &self.panel_address,
                    false,
                ))))
            };
            let _ = reply.send(result);
            return;
        }
        self.lifecycle.queue(action, reply);
    }

    pub(super) async fn next_build_change(
        &mut self,
        builds: &crate::runtime_builds::Builds,
        publish: &watch::Sender<Arc<crate::store::Settings>>,
        configuration: &std::path::Path,
    ) -> Option<BuildSelection> {
        while let Some((action, reply)) = self.lifecycle.changes.pop_front() {
            let initial = matches!(action, crate::runtime_builds::Action::Initial);
            let mut changed = (**publish.borrow()).clone();
            match builds.select(action, &mut changed, configuration).await {
                Err(error) => {
                    // Only the start after a stored lockout or after the state file is made has no reader.
                    if initial && reply.is_closed() {
                        return Some(BuildSelection::StartFailed(error));
                    }
                    let _ = reply.send(Err(error));
                }
                Ok(selection) => {
                    publish.send_replace(Arc::new(changed));
                    match selection {
                        crate::runtime_builds::Selection::Keep => {
                            if matches!(self.lifecycle.state, RuntimeState::Unavailable(_)) {
                                return Some(BuildSelection::Ready(Replacement::Build {
                                    reply,
                                    source: None,
                                    initial,
                                }));
                            }
                            let _ = reply.send(Ok(None));
                        }
                        crate::runtime_builds::Selection::Notice(notice) => {
                            let _ = reply.send(Ok(Some(notice)));
                        }
                        crate::runtime_builds::Selection::Check { folder, identity } => {
                            return Some(BuildSelection::Retire(Replacement::Check {
                                reply,
                                folder,
                                identity,
                            }));
                        }
                        crate::runtime_builds::Selection::Start(source) => {
                            return Some(BuildSelection::Retire(Replacement::Build {
                                reply,
                                source,
                                initial,
                            }));
                        }
                    }
                }
            }
        }
        None
    }

    pub(super) fn sign_in_wait_ended(&mut self) {
        self.sign_in.end_wait();
        if matches!(self.lifecycle.state, RuntimeState::Serving) {
            self.lifecycle.state = RuntimeState::Retiring {
                sign_in_wait_ended: true,
            };
            self.lifecycle
                .replacement
                .get_or_insert(Replacement::SignIn);
        }
    }

    pub(super) fn runtime_ready(&mut self, sign_in_required: bool, commands: &mut Vec<Command>) {
        self.lifecycle.state = RuntimeState::Serving;
        if sign_in_required {
            self.require_sign_in(commands);
        } else {
            self.sign_in.clear_required();
        }
    }

    pub(super) async fn deferred_start_failed(
        &mut self,
        error: LaunchFailure,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
    ) {
        let fault = if matches!(error, LaunchFailure::NoExecutable) {
            crate::messages::RuntimeFault::NoExecutable
        } else {
            crate::messages::RuntimeFault::NotAccepted
        };
        self.make_unavailable(fault, error.io_kind(), runtime, read_open, journal, builds)
            .await;
    }
}
