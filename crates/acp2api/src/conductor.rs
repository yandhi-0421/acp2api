mod admission;
mod commands;
mod keep_alive;
mod lockout;
mod quota;
mod records;
mod request;
mod retry;
mod runtime_lifecycle;
pub(crate) mod sign_in;
mod work;
#[cfg(test)]
use crate::runtime_builds::SelectedBuild;
use crate::{
    Clock, Hold, HoldKind, Moment,
    conversation::{Ticket, TurnEvent, TurnRequest},
    messages::OwnerMessage,
    runtime_builds::LaunchFailure,
    runtime_driver::{Runtime, RuntimeEvent, TurnUpdate},
};
use agent_client_protocol_schema::v1::StopReason;
use commands::Command;
pub(crate) use keep_alive::NO_UPDATE_LIMIT;
use quota::{QuotaPhase, QuotaPurpose, QuotaQuery};
use records::OutstandingWork;
use request::{Active, Failure};
pub(crate) use retry::WAIT_SECONDS;
use runtime_lifecycle::{CompatibilityCheck, Replacement, RuntimeState, UnavailableReason};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{mpsc, oneshot, watch};
use work::{Accepted, PendingRequest, ResponseChannel, Session, Work, WorkId, WorkTable};
#[derive(Clone)]
pub(crate) struct Conductor {
    journal: crate::journal::Journal,
    view: watch::Receiver<Arc<View>>,
    quota_result: watch::Receiver<Arc<Option<QuotaResult>>>,
    send: mpsc::Sender<Input>,
    builds: crate::runtime_builds::Builds,
    settings: watch::Receiver<Arc<crate::store::Settings>>,
    notices: watch::Receiver<Arc<Vec<Notice>>>,
    pub(crate) highest: u8,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum State {
    Running,
    QuotaWait,
    SignInRequired,
    RuntimeUnavailable,
    Lockout,
}
impl State {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::QuotaWait => "Quota wait",
            Self::SignInRequired => "Sign-in required",
            Self::RuntimeUnavailable => "Runtime unavailable",
            Self::Lockout => "Lockout",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RuntimeProcess {
    Starting,
    Running,
    Stopping,
    Stopped,
}
pub(crate) struct View {
    pub(crate) model_names: Vec<String>,
    pub(crate) context_sizes: BTreeMap<String, u64>,
    pub(crate) state: State,
    pub(crate) runtime_process: RuntimeProcess,
    pub(crate) places: usize,
    pub(crate) queued: usize,
    pub(crate) sign_in_wait: Option<sign_in::Wait>,
    pub(crate) lockout_evidence: Option<String>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            model_names: Vec::new(),
            context_sizes: BTreeMap::new(),
            state: State::Running,
            runtime_process: RuntimeProcess::Starting,
            places: 0,
            queued: 0,
            sign_in_wait: None,
            lockout_evidence: None,
        }
    }
}
pub(crate) enum OwnerAction {
    SignIn,
    CancelSignIn,
    RefreshQuota,
    ReadModelNames,
    NewLocalApiKey,
    SetPasswordHash(String),
    SetConcurrencyLimit(u8),
    DismissNotice(String),
}
pub(crate) struct QuotaResult {
    pub(crate) text: Result<String, crate::messages::Message>,
    pub(crate) time: String,
}
#[derive(Clone)]
pub(crate) struct Notice {
    pub(crate) id: String,
    pub(crate) name: String,
}
enum Input {
    Builds {
        action: crate::runtime_builds::Action,
        reply: BuildReply,
        hold: Hold,
    },
    Owner {
        action: OwnerAction,
        reply: oneshot::Sender<Result<(), OwnerMessage>>,
        hold: Hold,
    },
    Turn(Box<TurnWork>),
    Models {
        reply: oneshot::Sender<Result<Vec<crate::catalog::ModelEntry>, crate::messages::Refusal>>,
        hold: Hold,
    },
}
struct TurnWork {
    pending: PendingRequest,
    hold: Hold,
}
#[derive(Clone, Copy)]
pub(crate) enum ResponseMode {
    Body,
    Stream,
}
pub(crate) const QUEUE_CAPACITY: usize = 32;
pub(crate) const CANCEL_TIMEOUT: Moment = 30_000;
const MAX_RUNTIME_FAILURES: usize = 3;

const QUOTA_FALLBACK: std::time::Duration = std::time::Duration::from_secs(60 * 60);
const SETTLE_DELAY: Moment = 200;
type Output = mpsc::UnboundedSender<(TurnEvent, Hold)>;
type BuildReply = oneshot::Sender<Result<Option<OwnerMessage>, LaunchFailure>>;
struct Core {
    lockout: Option<crate::store::Lockout>,
    sign_in_required: bool,
    sign_in_wait: Option<sign_in::Wait>,
    panel_address: String,
    state_path: PathBuf,
    view: watch::Sender<Arc<View>>,
    quota_result: watch::Sender<Arc<Option<QuotaResult>>>,
    last_refresh: Option<Moment>,
    quota_wait: Option<quota::Wait>,
    retry_waits: Vec<retry::Wait>,
    next_quota: quota::TimerId,
    runtime_state: RuntimeState,
    failures: VecDeque<Moment>,
    cleanup_before_start: bool,
    changes: VecDeque<(crate::runtime_builds::Action, BuildReply)>,
    replacement: Option<Replacement>,
    catalog: Option<crate::catalog::Catalog>,
    model_names: Vec<String>,
    context_sizes: BTreeMap<String, u64>,
    work: WorkTable,
    next_work: WorkId,
    sessions: records::AcpSessions,
    limit: u8,
    notices: watch::Sender<Arc<Vec<Notice>>>,
}
enum Event {
    Lockout {
        signal: &'static str,
        error: crate::messages::Message,
        signalling_work: Option<WorkId>,
    },
    CheckPending(Box<PendingRequest>),
    Checked {
        id: WorkId,
        accepted: Option<Box<Accepted>>,
        session: String,
    },
    RuntimeReady {
        sign_in_required: bool,
    },
    StartSignIn,
    EndSignInWait,
    RefreshQuota(oneshot::Sender<Result<(), OwnerMessage>>),
    ReadModelNames(oneshot::Sender<Result<(), OwnerMessage>>),
    RetireRuntime,
    RuntimeFailure,
    Stop,
    RuntimeUnavailable(UnavailableReason),
    ResumeRuntime,
    Accepted(Box<Accepted>),
    Runtime(RuntimeEvent),
    Deadline,
    Disconnected(String),
    Limit(u8),
    DismissNotice(String),
    StartFailed(String, crate::messages::Message),
}
impl Core {
    fn state(&self) -> State {
        if self.lockout.is_some() {
            State::Lockout
        } else if matches!(self.runtime_state, RuntimeState::Unavailable(_)) {
            State::RuntimeUnavailable
        } else if self.sign_in_required {
            State::SignInRequired
        } else if self.quota_wait.is_some() {
            State::QuotaWait
        } else {
            State::Running
        }
    }
    async fn apply(
        &mut self,
        event: Event,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
    ) {
        let mut commands = self.handle(event, clock.now(), clock.wall()).into();
        Conductor::commands(self, runtime, read_open, clock, journal, &mut commands).await;
    }

    fn by_session(&self, session: &str) -> Option<WorkId> {
        self.work.iter().find_map(|(id, work)| {
            let current = match work {
                Work::CheckQueued(_)
                | Work::Checking(_)
                | Work::AcceptedQueued(_)
                | Work::QuotaQueued { .. }
                | Work::ModelsQueued(_)
                | Work::ModelsReading(_) => None,
                Work::AcceptedWorking(active) => active.session(),
                Work::Retiring(outstanding) => outstanding.session(),
                Work::QuotaWorking { phase, .. } => phase.session(),
            };
            (current == Some(session)).then_some(*id)
        })
    }
    fn by_request(&self, request: &str) -> Option<WorkId> {
        self.work
            .iter()
            .find_map(|(id, work)| (work.key() == request).then_some(*id))
    }

    fn next_deadline(&self) -> Option<Moment> {
        self.work
            .values()
            .filter_map(Work::deadline)
            .chain(self.sessions.next_deadline())
            .chain(self.quota_wait.as_ref().map(|wait| wait.deadline))
            .chain(self.sign_in_wait.as_ref().map(|wait| wait.deadline))
            .chain(self.retry_waits.iter().map(|wait| wait.deadline))
            .min()
    }
    fn owner_action(
        &mut self,
        action: OwnerAction,
        reply: oneshot::Sender<Result<(), OwnerMessage>>,
        settings: &watch::Sender<Arc<crate::store::Settings>>,
        configuration: &std::path::Path,
        clock: &dyn Clock,
    ) -> Vec<Command> {
        let mut changed = (**settings.borrow()).clone();
        let change = match action {
            OwnerAction::SignIn => {
                if self.lockout.is_some()
                    || self.sign_in_wait.is_some()
                    || !matches!(self.runtime_state, RuntimeState::Serving)
                {
                    let _ = reply.send(Err(OwnerMessage::SignInCannotStart));
                    return Vec::new();
                }
                let commands = self.handle(Event::StartSignIn, clock.now(), clock.wall());
                let _ = reply.send(Ok(()));
                return commands;
            }
            OwnerAction::CancelSignIn => {
                let commands = if self.sign_in_wait.is_some() {
                    self.handle(Event::EndSignInWait, clock.now(), clock.wall())
                } else {
                    Vec::new()
                };
                let _ = reply.send(Ok(()));
                return commands;
            }
            OwnerAction::RefreshQuota => {
                return self.handle(Event::RefreshQuota(reply), clock.now(), clock.wall());
            }
            OwnerAction::ReadModelNames => {
                return self.handle(Event::ReadModelNames(reply), clock.now(), clock.wall());
            }
            OwnerAction::DismissNotice(id) => {
                let commands = self.handle(Event::DismissNotice(id), clock.now(), clock.wall());
                let _ = reply.send(Ok(()));
                return commands;
            }
            OwnerAction::NewLocalApiKey => {
                crate::store::random_key().map(|key| changed.local_api_key = key)
            }
            OwnerAction::SetPasswordHash(hash) => {
                changed.management_password_hash = hash;
                Ok(())
            }
            OwnerAction::SetConcurrencyLimit(limit) => {
                changed.concurrency_limit = limit;
                Ok(())
            }
        };
        match change.and_then(|()| crate::store::save(configuration, &changed)) {
            Ok(()) => {
                let limit = changed.concurrency_limit;
                settings.send_replace(Arc::new(changed));
                let commands = self.handle(Event::Limit(limit), clock.now(), clock.wall());
                let _ = reply.send(Ok(()));
                commands
            }
            Err(error) => {
                let _ = reply.send(Err(OwnerMessage::Diagnostic(error)));
                Vec::new()
            }
        }
    }
    fn handle(
        &mut self,
        mut event: Event,
        now: Moment,
        wall: std::time::SystemTime,
    ) -> Vec<Command> {
        if matches!(event, Event::Deadline) && self.sessions.cancellation_due(now) {
            return self.handle(Event::RuntimeFailure, now, wall);
        }
        if matches!(event, Event::Deadline)
            && self
                .sign_in_wait
                .as_ref()
                .is_some_and(|wait| wait.deadline <= now)
        {
            return self.handle(Event::EndSignInWait, now, wall);
        }
        let previous = self.state();
        let mut commands = Vec::new();
        let upstream_error = self.upstream_error(&mut event, now, &mut commands);
        self.retry_waits.retain(|wait| wait.deadline > now);
        if self
            .quota_wait
            .as_ref()
            .is_some_and(|wait| wait.deadline <= now)
        {
            self.quota_wait = None;
        }
        let work_id = match &event {
            Event::StartFailed(work, _) | Event::Runtime(RuntimeEvent::Prepared { work, .. }) => {
                self.by_request(work)
            }
            Event::Runtime(
                RuntimeEvent::Turn { session, .. } | RuntimeEvent::UpstreamError { session, .. },
            ) => self.by_session(session),
            _ => None,
        };
        if let Event::Runtime(RuntimeEvent::UpstreamError { session, error }) = &event {
            commands.push(Command::UpstreamLog {
                session: session.clone(),
                error: error.clone(),
                late: work_id.is_none(),
            });
        }
        if let Event::Runtime(RuntimeEvent::Turn {
            session,
            update: TurnUpdate::PromptEnded { .. } | TurnUpdate::Failed(_),
        }) = &event
        {
            self.sessions.cancel_ended(session);
        }
        let quota_work = work_id.filter(|id| matches!(self.work[id], Work::QuotaWorking { .. }));
        match (quota_work, event) {
            (
                None,
                Event::Lockout {
                    signal,
                    error,
                    signalling_work,
                },
            ) => {
                self.start_lockout(signal, error, signalling_work, now, wall, &mut commands);
            }
            (None, Event::CheckPending(request)) => {
                self.work
                    .insert(self.next_work, Work::CheckQueued(*request));
                self.next_work += 1;
            }
            (
                None,
                Event::Checked {
                    id,
                    accepted,
                    session,
                },
            ) => {
                if let Some(request) = accepted {
                    self.start_request(id, *request, Some(session), now, &mut commands);
                } else {
                    commands.push(Command::Forget(session));
                }
            }
            (None, Event::StartSignIn) => {
                self.sign_in_wait = Some(sign_in::Wait::new(now));
                commands.push(Command::Authenticate);
            }
            (None, Event::EndSignInWait) => {
                self.sign_in_wait = None;
                if matches!(self.runtime_state, RuntimeState::Serving) {
                    self.runtime_state = RuntimeState::Retiring {
                        sign_in_wait_ended: true,
                    };
                    self.replacement.get_or_insert(Replacement::SignIn);
                }
            }
            (None, Event::Runtime(RuntimeEvent::SignInAddress(address))) => {
                if let Some(wait) = &mut self.sign_in_wait {
                    wait.address = Some(address);
                }
            }
            (None, Event::Runtime(RuntimeEvent::Authenticated(result))) => {
                self.sign_in_wait = None;
                if result.is_ok() {
                    self.sign_in_required = false;
                }
            }
            (None, Event::RuntimeReady { sign_in_required }) => {
                self.runtime_state = RuntimeState::Serving;
                if sign_in_required {
                    self.require_sign_in(&mut commands);
                } else {
                    self.sign_in_required = false;
                }
            }
            (Some(id), event) => {
                self.quota_event(id, event, upstream_error, now, wall, &mut commands)
            }
            (None, Event::RefreshQuota(reply)) => self.refresh_quota(reply, now),
            (None, Event::ReadModelNames(reply)) => self.read_model_names(reply),
            (None, Event::Stop) => self.stopping(now, &mut commands),
            (None, Event::RuntimeUnavailable(reason)) => {
                self.runtime_unavailable(reason, now, &mut commands)
            }
            (None, Event::RetireRuntime) => self.retire_runtime(),
            (None, Event::ResumeRuntime) => self.resume_runtime(),
            (None, Event::Accepted(request)) => {
                self.work
                    .insert(self.next_work, Work::AcceptedQueued(*request));
                self.next_work += 1;
            }
            (None, Event::DismissNotice(id)) => {
                self.notices.send_if_modified(|notices| {
                    let count = notices.len();
                    Arc::make_mut(notices).retain(|notice| notice.id != id);
                    notices.len() != count
                });
            }
            (None, Event::Limit(limit)) => {
                self.limit = limit;
            }
            (None, Event::Disconnected(request_id)) => {
                self.client_disconnected(&request_id, now, &mut commands)
            }
            (None, Event::StartFailed(_, error)) => {
                if let Some(id) = work_id {
                    self.finish(id, Err(Failure::at(error, now)), now, &mut commands);
                }
            }
            (None, Event::Runtime(RuntimeEvent::Prepared { work, result })) => {
                self.runtime_prepared(work_id, work, result, now, wall, &mut commands);
            }
            (None, Event::Runtime(RuntimeEvent::Retired { session })) => {
                self.sessions.cancel_ended(&session);
                if let Some(id) = self.by_session(&session) {
                    self.finish(id, Ok(()), now, &mut commands);
                }
            }
            (None, Event::Runtime(RuntimeEvent::Turn { session, update })) => {
                if let Some(id) = work_id {
                    self.update(id, session, update, now, wall, &mut commands);
                } else if let TurnUpdate::Permission { request, .. }
                | TurnUpdate::RefusedPermission { request, .. } = update
                {
                    commands.push(Command::Permission(request));
                }
            }
            (None, Event::Runtime(RuntimeEvent::UpstreamError { session, .. })) => {
                self.upstream_error_reported(
                    work_id,
                    session,
                    upstream_error.unwrap(),
                    now,
                    wall,
                    &mut commands,
                );
            }
            (None, Event::Runtime(RuntimeEvent::Failed(error))) => {
                commands.extend(self.sessions.forget_idle());
                while let Some(id) = self
                    .work
                    .iter()
                    .find_map(|(id, work)| work.occupies_place().then_some(*id))
                {
                    self.finish(
                        id,
                        Err(Failure::at(
                            crate::messages::Message::Upstream(error.clone()),
                            now,
                        )),
                        now,
                        &mut commands,
                    );
                }
            }
            (None, Event::Runtime(RuntimeEvent::Closed | RuntimeEvent::WriteFailed(_)))
                if matches!(self.runtime_state, RuntimeState::Restarting) =>
            {
                self.refuse_model_reads(&crate::messages::Message::RuntimeFailed, &mut commands);
            }
            (
                None,
                Event::Runtime(RuntimeEvent::Closed | RuntimeEvent::WriteFailed(_))
                | Event::RuntimeFailure,
            ) => self.runtime_failed(now, &mut commands),
            (None, Event::Runtime(RuntimeEvent::Stderr | RuntimeEvent::Other)) => {}
            (None, Event::Deadline) => {
                self.retire_idle(now, &mut commands);
                let due: Vec<_> = self
                    .work
                    .iter()
                    .filter_map(|(id, work)| {
                        work.completion_deadline()
                            .filter(|at| *at <= now)
                            .map(|_| *id)
                    })
                    .collect();
                for id in due {
                    match &self.work[&id] {
                        Work::QuotaWorking { .. } => {
                            self.quota_event(id, Event::Deadline, None, now, wall, &mut commands)
                        }
                        _ => self.finish(id, Ok(()), now, &mut commands),
                    }
                }
            }
        }
        commands.extend(self.sessions.expire(now));
        self.start_queued_work(now, &mut commands);
        self.emit_keep_alives(now, &mut commands);
        if self.state() != previous {
            let cause = match &self.runtime_state {
                RuntimeState::Unavailable(reason) => reason.cause,
                _ => None,
            };
            commands.insert(0, Command::State(self.state(), cause));
        }
        self.publish_view();
        commands
    }
    fn publish_view(&mut self) {
        if let Some(catalog) = &self.catalog {
            self.model_names = catalog.names().map(|(name, _)| name).collect();
        }
        self.view.send_replace(Arc::new(View {
            model_names: self.model_names.clone(),
            context_sizes: self.context_sizes.clone(),
            sign_in_wait: self.sign_in_wait.clone(),
            lockout_evidence: self
                .lockout
                .as_ref()
                .map(|evidence| evidence.original_error_text.clone()),
            state: self.state(),
            runtime_process: self.runtime_state.runtime_process(),
            places: self
                .work
                .values()
                .filter(|work| work.occupies_place())
                .count(),
            queued: self.work.values().filter(|work| work.is_queued()).count(),
        }));
    }
}
impl Conductor {
    pub(crate) fn start(
        builds: crate::runtime_builds::Builds,
        clock: Arc<dyn Clock>,
        journal: crate::journal::Journal,
        settings: crate::store::Settings,
        configuration: PathBuf,
        highest: u8,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (send, mut recv) = mpsc::channel::<Input>(32);
        let actor_builds = builds.clone();
        let limit = settings.concurrency_limit;
        let state_path = builds.start.data.join(crate::store::STATE_FILE);
        let lockout = lockout::load(&state_path, clock.as_ref(), &journal);
        let panel_address = format!("http://127.0.0.1:{}/", settings.port);
        let (view, view_watch) = watch::channel(Arc::new(View::default()));
        let (publish, settings_watch) = watch::channel(Arc::new(settings));
        let (notices, notices_watch) = watch::channel(Arc::new(Vec::new()));
        let (quota_result, quota_watch) = watch::channel(Arc::new(None));
        let conductor = Self {
            journal: journal.clone(),
            view: view_watch,
            quota_result: quota_watch,
            send,
            builds,
            settings: settings_watch,
            notices: notices_watch,
            highest,
        };
        let task = tokio::spawn(async move {
            let mut runtime: Option<Runtime> = None;
            let mut core = Core {
                quota_result,
                last_refresh: None,
                quota_wait: None,
                retry_waits: Vec::new(),
                next_quota: quota::TimerId::default(),
                runtime_state: if lockout.is_some() {
                    RuntimeState::Stopped
                } else {
                    RuntimeState::Serving
                },
                lockout,
                sign_in_required: false,
                sign_in_wait: None,
                panel_address,
                state_path,
                view,
                failures: VecDeque::new(),
                cleanup_before_start: false,
                changes: VecDeque::new(),
                replacement: None,
                catalog: None,
                model_names: Vec::new(),
                context_sizes: BTreeMap::new(),
                sessions: records::AcpSessions::default(),
                limit,
                work: BTreeMap::new(),
                next_work: 0,
                notices,
            };
            let mut read_open = false;
            core.publish_view();
            journal.state(core.state().name(), None, clock.as_ref());
            let mut checking: Option<CompatibilityCheck> = None;
            let mut stopped = actor_builds.start.stopped.clone();
            loop {
                if core.replacement_ready() && core.replacement.is_some() {
                    checking = core
                        .replace(
                            &mut runtime,
                            &mut read_open,
                            &clock,
                            &journal,
                            &actor_builds,
                            &publish,
                        )
                        .await;
                    if matches!(core.runtime_state, RuntimeState::Stopping) {
                        break;
                    }
                }
                if core.lockout.is_none()
                    && matches!(core.runtime_state, RuntimeState::Restarting)
                    && core.replacement.is_none()
                {
                    core.restart(
                        &mut runtime,
                        &mut read_open,
                        &clock,
                        &journal,
                        &actor_builds,
                    )
                    .await;
                    if matches!(core.runtime_state, RuntimeState::Stopping) {
                        break;
                    }
                }
                if core.replacement.is_none() && checking.is_none() {
                    while let Some((action, reply)) = core.changes.pop_front() {
                        let initial = matches!(action, crate::runtime_builds::Action::Initial);
                        let mut changed = (**publish.borrow()).clone();
                        match actor_builds
                            .select(action, &mut changed, &configuration)
                            .await
                        {
                            Err(error) => {
                                let _ = reply.send(Err(error));
                            }
                            Ok(selection) => {
                                publish.send_replace(Arc::new(changed));
                                match selection {
                                    crate::runtime_builds::Selection::Keep => {
                                        if matches!(
                                            core.runtime_state,
                                            RuntimeState::Unavailable(_)
                                        ) {
                                            core.replacement = Some(Replacement::Build {
                                                reply,
                                                source: None,
                                                initial,
                                            });
                                            break;
                                        }
                                        let _ = reply.send(Ok(None));
                                    }
                                    crate::runtime_builds::Selection::Notice(notice) => {
                                        let _ = reply.send(Ok(Some(notice)));
                                    }
                                    crate::runtime_builds::Selection::Check {
                                        folder,
                                        identity,
                                    } => {
                                        core.apply(
                                            Event::RetireRuntime,
                                            &mut runtime,
                                            &mut read_open,
                                            &clock,
                                            &journal,
                                        )
                                        .await;
                                        core.replacement = Some(Replacement::Check {
                                            reply,
                                            folder,
                                            identity,
                                        });
                                        break;
                                    }
                                    crate::runtime_builds::Selection::Start(source) => {
                                        core.apply(
                                            Event::RetireRuntime,
                                            &mut runtime,
                                            &mut read_open,
                                            &clock,
                                            &journal,
                                        )
                                        .await;
                                        core.replacement = Some(Replacement::Build {
                                            reply,
                                            source,
                                            initial,
                                        });
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if core.replacement.is_some() && core.replacement_ready() {
                        continue;
                    }
                    if core.replacement.is_none()
                        && matches!(core.runtime_state, RuntimeState::Retiring { .. })
                    {
                        core.apply(
                            Event::ResumeRuntime,
                            &mut runtime,
                            &mut read_open,
                            &clock,
                            &journal,
                        )
                        .await;
                    }
                }
                let connected: Vec<_> = core
                    .work
                    .values()
                    .filter_map(|work| {
                        work.reply()
                            .map(|reply| (work.key().to_owned(), reply.output.clone()))
                    })
                    .collect();
                let pending: Vec<_> = core
                    .work
                    .values()
                    .filter_map(|work| match work {
                        Work::CheckQueued(request) | Work::Checking(request) => {
                            Some((request.request.id.clone(), request.connection.clone()))
                        }
                        _ => None,
                    })
                    .collect();
                let timer = core.next_deadline().map(|at| clock.sleep_until(at));
                let (event, hold) = tokio::select! {
                    biased;
                    Ok(()) = async { stopped.wait_for(|stopped| *stopped).await.map(|_| ()) } => {
                        (Event::Stop, clock.hold(HoldKind::TurnEvent))
                    }
                    completed = async { checking.as_mut().unwrap().as_mut().await }, if checking.is_some() => {
                        checking = None;
                        core.finish_check(completed, &mut runtime, &mut read_open, &journal, &actor_builds, &publish).await;
                        continue;
                    }
                    work = first_closed(&connected), if !connected.is_empty() => {
                        (Event::Disconnected(work), clock.hold(HoldKind::TurnEvent))
                    }
                    work = first_closed(&pending), if !pending.is_empty() => {
                        (Event::Disconnected(work), clock.hold(HoldKind::TurnEvent))
                    }
                    work = recv.recv() => {
                        let Some(work) = work else {
                            break;
                        };
                        match work {
                            Input::Builds { action, reply, hold } => {
                                core.build_change(action, reply);
                                drop(hold);
                                continue;
                            }
                            Input::Models { reply, hold } => {
                                let result = core.model_entries(clock.now()).map_err(|mut error| {
                                    error.hold = clock.hold(HoldKind::TurnEvent);
                                    error
                                });
                                let _ = reply.send(result);
                                drop(hold);
                                continue;
                            }
                            Input::Owner { action, reply, hold } => {
                                let mut commands = core.owner_action(action, reply, &publish, &configuration, clock.as_ref()).into();
                                Self::commands(
                                    &mut core,
                                    &mut runtime,
                                    &mut read_open,
                                    &clock,
                                    &journal,
                                    &mut commands,
                                )
                                .await;
                                drop(hold);
                                continue;
                            }
                            Input::Turn(work) => {
                                match core.admit(*work, &journal, clock.as_ref()) {
                                    Some(accepted) => accepted,
                                    None => continue,
                                }
                            }
                        }
                    }
                    (event, hold) = async {
                        runtime.as_mut().expect("readable runtime").next().await
                    }, if read_open => {
                        if matches!(event, RuntimeEvent::Closed | RuntimeEvent::WriteFailed(_)) {
                            read_open = false;
                        }
                        (Event::Runtime(event), hold)
                    }
                    hold = deadline(timer) => (Event::Deadline, hold),
                };
                core.apply(event, &mut runtime, &mut read_open, &clock, &journal)
                    .await;
                drop(hold);
                if matches!(core.runtime_state, RuntimeState::Stopping) {
                    break;
                }
            }
            if let Some(check) = checking {
                let check = check.await;
                let _ = check.reply.send(Err(LaunchFailure::Stopped));
            }
            if let Some(runtime) = runtime {
                runtime.stop().await;
            }
        });
        (conductor, task)
    }
    pub(crate) fn builds_summary(&self) -> Result<crate::runtime_builds::Summary, String> {
        self.builds.summary()
    }
    pub(crate) fn data_folder(&self) -> &std::path::Path {
        self.builds.data_folder()
    }
    pub(crate) fn notices(&self) -> Arc<Vec<Notice>> {
        self.notices.borrow().clone()
    }
    pub(crate) fn quota_result(&self) -> Arc<Option<QuotaResult>> {
        self.quota_result.borrow().clone()
    }
    pub(crate) fn history(&self) -> crate::journal::History {
        self.journal.history()
    }
    pub(crate) fn settings(&self) -> Arc<crate::store::Settings> {
        self.settings.borrow().clone()
    }
    pub(crate) fn view(&self) -> Arc<View> {
        self.view.borrow().clone()
    }
    pub(crate) async fn builds(
        &self,
        action: crate::runtime_builds::Action,
        hold: Hold,
    ) -> Result<Option<OwnerMessage>, LaunchFailure> {
        let (reply, result) = oneshot::channel();
        self.send
            .send(Input::Builds {
                action,
                reply,
                hold,
            })
            .await
            .map_err(|_| LaunchFailure::Stopped)?;
        result.await.map_err(|_| LaunchFailure::Stopped)?
    }
    pub(crate) async fn owner(&self, action: OwnerAction, hold: Hold) -> Result<(), OwnerMessage> {
        let (reply, result) = oneshot::channel();
        self.send
            .send(Input::Owner {
                action,
                reply,
                hold,
            })
            .await
            .map_err(|_| OwnerMessage::Api(crate::messages::Message::Stopped))?;
        result
            .await
            .map_err(|_| OwnerMessage::Api(crate::messages::Message::Stopped))?
    }
    pub(crate) async fn admit(
        &self,
        turn: TurnRequest,
        hold: Hold,
        request: crate::journal::Request,
        mode: ResponseMode,
    ) -> Result<Ticket, crate::messages::Refusal> {
        let (admission, reply) = oneshot::channel();
        let (connection, lease) = mpsc::unbounded_channel();
        self.send
            .send(Input::Turn(Box::new(TurnWork {
                pending: PendingRequest {
                    mode,
                    turn,
                    request,
                    admission,
                    connection,
                },
                hold,
            })))
            .await
            .map_err(stopped)?;
        let result = reply.await.map_err(stopped)?;
        drop(lease);
        result
    }
    pub(crate) async fn model_names(
        &self,
        hold: Hold,
    ) -> Result<Vec<crate::catalog::ModelEntry>, crate::messages::Refusal> {
        let (reply, result) = oneshot::channel();
        self.send
            .send(Input::Models { reply, hold })
            .await
            .map_err(stopped)?;
        result.await.map_err(stopped)?
    }
}
async fn first_closed<T>(outputs: &[(String, mpsc::UnboundedSender<T>)]) -> String {
    let waits = outputs.iter().map(|(id, output)| {
        Box::pin(async move {
            output.closed().await;
            id.clone()
        })
    });
    futures_util::future::select_all(waits).await.0
}
async fn deadline(timer: Option<crate::BoxFuture<'static, Hold>>) -> Hold {
    match timer {
        Some(timer) => timer.await,
        None => std::future::pending().await,
    }
}
fn stopped<T>(_: T) -> crate::messages::Refusal {
    crate::messages::Message::Stopped.into()
}
#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests;
