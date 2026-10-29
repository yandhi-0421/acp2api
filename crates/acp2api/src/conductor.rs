mod keep_alive;
mod quota;
mod records;
mod retry;
pub(crate) mod sign_in;
use crate::{
    Clock, Hold, HoldKind, Moment,
    compatibility_check::Failure as CheckFailure,
    conversation::{EffortAsk, Prompt, PromptRequest, Ticket, TurnEvent, TurnRequest, Usage},
    runtime_builds::{BuildIdentity, LaunchFailure, SelectedBuild},
    runtime_driver::{Runtime, RuntimeEvent, TurnUpdate},
};
use agent_client_protocol_schema::v1::StopReason;
use keep_alive::KEEP_ALIVE_INTERVAL;
pub(crate) use keep_alive::NO_UPDATE_LIMIT;
pub(crate) use retry::WAIT_SECONDS;
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::{mpsc, oneshot, watch};
#[derive(Clone)]
pub(crate) struct Conductor {
    view: watch::Receiver<Arc<View>>,
    quota_result: watch::Receiver<Arc<Option<QuotaResult>>>,
    send: mpsc::Sender<Input>,
    builds: crate::runtime_builds::Builds,
    settings: watch::Receiver<Arc<crate::store::Settings>>,
    notices: watch::Receiver<Arc<Vec<Notice>>>,
    pub(crate) highest: u8,
}
pub(crate) struct View {
    pub(crate) state: &'static str,
    pub(crate) queued: usize,
    pub(crate) sign_in_wait: Option<sign_in::Wait>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            state: "Running",
            queued: 0,
            sign_in_wait: None,
        }
    }
}
pub(crate) enum OwnerAction {
    SignIn,
    CancelSignIn,
    RefreshQuota,
    NewLocalApiKey,
    SetPasswordHash(String),
    SetConcurrencyLimit(u8),
    DismissNotice(String),
}
pub(crate) struct QuotaResult {
    pub(crate) text: String,
    pub(crate) time: String,
}
#[derive(Clone)]
pub(crate) struct Notice {
    pub(crate) id: String,
    name: String,
}
impl Notice {
    pub(crate) fn text(&self) -> String {
        format!(
            "The built-in tool {} ran with no permission request. acp2api cancelled the prompt turn and sent an error to the API client.",
            self.name
        )
    }
}
enum Replacement {
    SignIn,
    Build {
        reply: oneshot::Sender<Result<String, LaunchFailure>>,
        source: Option<SelectedBuild>,
        initial: bool,
    },
    Check {
        reply: oneshot::Sender<Result<String, LaunchFailure>>,
        folder: PathBuf,
        identity: BuildIdentity,
    },
}
type CompatibilityCheck = crate::BoxFuture<'static, CompletedCheck>;
struct CompletedCheck {
    identity: BuildIdentity,
    result: Result<String, CheckFailure>,
    reply: oneshot::Sender<Result<String, LaunchFailure>>,
}
enum Input {
    Builds {
        action: crate::runtime_builds::Action,
        reply: oneshot::Sender<Result<String, LaunchFailure>>,
        hold: Hold,
    },
    Owner {
        action: OwnerAction,
        reply: oneshot::Sender<Result<(), String>>,
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
struct PendingRequest {
    mode: ResponseMode,
    turn: TurnRequest,
    request: crate::journal::Request,
    admission: oneshot::Sender<Result<Ticket, crate::messages::Refusal>>,
    connection: mpsc::UnboundedSender<()>,
}
pub(crate) const QUEUE_CAPACITY: usize = 32;
pub(crate) const CANCEL_TIMEOUT: Moment = 30_000;
const MAX_RUNTIME_FAILURES: usize = 3;
const RUNTIME_FAILURE_WINDOW: Moment = 10 * 60 * 1000;
const REPEATED_FAILURES: &str = "the runtime process failed 3 times in 10 minutes";

fn copy_failed_reason(version: &str) -> String {
    format!("the copy of the accepted build {version} failed")
}

const UNAVAILABLE: &str = "official runtime is not available";
const QUOTA_FALLBACK: std::time::Duration = std::time::Duration::from_secs(60 * 60);
const SETTLE_DELAY: Moment = 200;
type Output = mpsc::UnboundedSender<(TurnEvent, Hold)>;
struct Accepted {
    turn: TurnRequest,
    model: crate::catalog::SelectedModel,
    request: crate::journal::Request,
    reply: ResponseChannel,
}
struct ResponseChannel {
    output: Output,
    keep_alive: Option<Moment>,
}
type WorkId = u64;
type WorkTable = BTreeMap<WorkId, Work>;
enum Work {
    CheckQueued(PendingRequest),
    Checking(PendingRequest),
    AcceptedQueued(Accepted),
    AcceptedWorking(Active),
    Retiring(OutstandingWork),
    QuotaQueued {
        query: QuotaQuery,
        session: Option<String>,
    },
    QuotaWorking {
        query: QuotaQuery,
        phase: QuotaPhase,
        text: String,
        result: Result<(), crate::messages::Message>,
    },
}
struct QuotaQuery {
    work: String,
    purpose: QuotaPurpose,
}
impl QuotaQuery {
    fn disconnect(&mut self, commands: &mut Vec<Command>) {
        let response = match &mut self.purpose {
            QuotaPurpose::Reset { response, .. } | QuotaPurpose::Refusal { response, .. } => {
                response
            }
            QuotaPurpose::Refresh(_) => unreachable!(),
        };
        let active = response.take().unwrap();
        commands.push(Command::Complete(crate::journal::Completion {
            request: active.request,
            session: active.session,
            session_use: Some(active.kind),
            outcome: crate::journal::Outcome::Disconnected,
        }));
    }
}
enum QuotaPurpose {
    Refresh(oneshot::Sender<Result<(), String>>),
    Reset {
        wait: quota::Wait,
        model: Option<String>,
        response: Option<Active>,
    },
    Refusal {
        model: String,
        response: Option<Active>,
    },
}
enum QuotaPhase {
    Preparing,
    Prompt(String),
    Settling { session: String, at: Moment },
}
impl QuotaPhase {
    fn session(&self) -> Option<&str> {
        match self {
            Self::Preparing => None,
            Self::Prompt(session) | Self::Settling { session, .. } => Some(session),
        }
    }
}
struct OutstandingWork {
    model: String,
    request: crate::journal::Request,
    session: Option<String>,
    completion: PendingCompletion,
}
enum PendingCompletion {
    Disconnected(Option<crate::journal::SessionUse>),
    AlreadyCompleted,
}
impl Work {
    fn key(&self) -> &str {
        match self {
            Self::CheckQueued(request) | Self::Checking(request) => &request.request.id,
            Self::AcceptedQueued(request) => &request.request.id,
            Self::AcceptedWorking(active) => &active.request.id,
            Self::Retiring(outstanding) => &outstanding.request.id,
            Self::QuotaQueued { query, .. } | Self::QuotaWorking { query, .. } => &query.work,
        }
    }
    fn execution(&self) -> Option<&Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_)
            | Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaQueued { .. }
            | Self::QuotaWorking { .. } => None,
        }
    }
    fn execution_mut(&mut self) -> Option<&mut Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_)
            | Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaQueued { .. }
            | Self::QuotaWorking { .. } => None,
        }
    }
    fn into_queued(self) -> Option<Accepted> {
        match self {
            Self::AcceptedQueued(request) => Some(request),
            Self::AcceptedWorking(_)
            | Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaQueued { .. }
            | Self::QuotaWorking { .. } => None,
        }
    }
    fn into_active(self) -> Option<Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_)
            | Self::CheckQueued(_)
            | Self::Checking(_)
            | Self::Retiring(_)
            | Self::QuotaQueued { .. }
            | Self::QuotaWorking { .. } => None,
        }
    }
    fn is_queued(&self) -> bool {
        matches!(
            self,
            Self::CheckQueued(_) | Self::AcceptedQueued(_) | Self::QuotaQueued { .. }
        )
    }
    fn occupies_place(&self) -> bool {
        matches!(
            self,
            Self::Checking(_)
                | Self::AcceptedWorking(_)
                | Self::Retiring(_)
                | Self::QuotaWorking { .. }
        )
    }
    fn reply(&self) -> Option<&ResponseChannel> {
        match self {
            Self::AcceptedQueued(request) => Some(&request.reply),
            Self::AcceptedWorking(active) => Some(&active.reply),
            Self::QuotaQueued { query, .. } | Self::QuotaWorking { query, .. } => match &query
                .purpose
            {
                QuotaPurpose::Reset { response, .. } | QuotaPurpose::Refusal { response, .. } => {
                    response.as_ref().map(|active| &active.reply)
                }
                QuotaPurpose::Refresh(_) => None,
            },
            Self::CheckQueued(_) | Self::Checking(_) | Self::Retiring(_) => None,
        }
    }
    fn reply_mut(&mut self) -> Option<&mut ResponseChannel> {
        match self {
            Self::AcceptedQueued(request) => Some(&mut request.reply),
            Self::AcceptedWorking(active) => Some(&mut active.reply),
            Self::QuotaQueued { query, .. } | Self::QuotaWorking { query, .. } => match &mut query
                .purpose
            {
                QuotaPurpose::Reset { response, .. } | QuotaPurpose::Refusal { response, .. } => {
                    response.as_mut().map(|active| &mut active.reply)
                }
                QuotaPurpose::Refresh(_) => None,
            },
            Self::CheckQueued(_) | Self::Checking(_) | Self::Retiring(_) => None,
        }
    }
    fn update_deadline(&self) -> Option<Moment> {
        match self {
            Self::AcceptedWorking(Active { phase, .. }) => phase.update_deadline(),
            _ => None,
        }
    }
    fn deadline(&self) -> Option<Moment> {
        self.completion_deadline()
            .into_iter()
            .chain(self.update_deadline())
            .chain(self.reply().and_then(|reply| reply.keep_alive))
            .min()
    }
    fn completion_deadline(&self) -> Option<Moment> {
        match self {
            Self::AcceptedWorking(Active {
                phase: Phase::Settling { at, .. } | Phase::QuotaSettling { at, .. },
                ..
            })
            | Self::QuotaWorking {
                phase: QuotaPhase::Settling { at, .. },
                ..
            } => Some(*at),
            _ => None,
        }
    }
}
#[derive(Clone)]
struct Session {
    tools: Vec<crate::conversation::ClientTool>,
    system: Option<String>,
}
impl Session {
    fn for_turn(turn: &TurnRequest) -> Self {
        Self {
            system: crate::conversation::system_prompt_addition(&turn.conversation.system),
            tools: match turn.tool_choice {
                crate::conversation::ToolChoice::Off => Vec::new(),
                crate::conversation::ToolChoice::Auto => turn.tools.clone(),
            },
        }
    }
}
enum Phase {
    Completing,
    Preparing(PromptRequest),
    Prompt {
        update_deadline: Moment,
    },
    AwaitingQuotaEnd {
        wait: quota::Wait,
        update_deadline: Option<Moment>,
    },
    QuotaSettling {
        at: Moment,
        wait: quota::Wait,
    },
    Settling {
        at: Moment,
        can_continue: bool,
        result: Result<(), Failure>,
    },
}
impl Phase {
    fn prompt(now: Moment) -> Self {
        Self::Prompt {
            update_deadline: now + NO_UPDATE_LIMIT,
        }
    }
    fn update_deadline(&self) -> Option<Moment> {
        match self {
            Self::Prompt { update_deadline } => Some(*update_deadline),
            Self::AwaitingQuotaEnd {
                update_deadline, ..
            } => *update_deadline,
            _ => None,
        }
    }
}
struct Failure {
    message: crate::messages::Message,
    at: Moment,
}
impl Failure {
    fn at(message: crate::messages::Message, at: Moment) -> Self {
        Self { message, at }
    }
}
struct Active {
    reply: ResponseChannel,
    request: crate::journal::Request,
    session: Option<String>,
    hashes: crate::conversation::RecordHashes,
    tools_hash: crate::conversation::ToolSetHash,
    client_tools_hash: crate::conversation::ToolSetHash,
    kind: crate::journal::SessionUse,
    used: u64,
    expires: Moment,
    held: Option<String>,
    text: String,
    thought_dropped: bool,
    input_tokens: u64,
    model: String,
    model_name: String,
    effort: EffortAsk,
    phase: Phase,
    tool_call: Option<crate::conversation::ToolCall>,
    stop: crate::conversation::Stop,
    stops: Vec<String>,
    stop_prefix: String,
}
impl Active {
    fn earliest_stop(&self, text: &str) -> Option<(usize, String)> {
        self.stops
            .iter()
            .filter_map(|stop| text.find(stop).map(|at| (at, stop.clone())))
            .min_by_key(|(at, _)| *at)
    }
    fn release_or_cut_reply(
        &mut self,
        next: Option<String>,
        now: Moment,
        commands: &mut Vec<Command>,
    ) -> bool {
        let complete = next.is_none();
        if !complete && self.stop.sequence().is_some() {
            return false;
        }
        let mut text = std::mem::take(&mut self.stop_prefix);
        if let Some(held) = self.held.take() {
            text.push_str(&held);
        }
        let previous = text.len();
        if let Some(next) = next {
            self.held = Some(next);
        }
        let matched = self.earliest_stop(&text);
        let end = if let Some((at, _)) = &matched {
            *at
        } else if complete {
            text.len()
        } else {
            let partial = self
                .stops
                .iter()
                .flat_map(|stop| {
                    stop.char_indices()
                        .skip(1)
                        .map(move |(end, _)| &stop[..end])
                })
                .filter(|prefix| text.ends_with(prefix))
                .map(str::len)
                .max()
                .unwrap_or(0);
            previous - partial
        };
        let remaining = text.split_off(end);
        if !text.is_empty() {
            self.text.push_str(&text);
            self.reply.emit(TurnEvent::Text(text), now, commands);
        }
        if !complete {
            self.stop_prefix = remaining;
        }
        let matched = matched.or_else(|| {
            let combined = format!("{}{}", self.stop_prefix, self.held.as_deref().unwrap_or(""));
            self.earliest_stop(&combined)
        });
        if let Some((_, stop)) = matched
            && self.stop.sequence().is_none()
        {
            self.stop = crate::conversation::Stop::Sequence(stop);
            return !complete;
        }
        false
    }
}
struct UnavailableReason {
    message: String,
    cause: Option<std::io::ErrorKind>,
}
enum RuntimeState {
    Serving,
    Retiring { sign_in_wait_ended: bool },
    Restarting,
    Unavailable(UnavailableReason),
    Stopping,
}
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
    cancels: BTreeMap<String, Moment>,
    changes: VecDeque<(
        crate::runtime_builds::Action,
        oneshot::Sender<Result<String, LaunchFailure>>,
    )>,
    replacement: Option<Replacement>,
    catalog: Option<crate::catalog::Catalog>,
    work: WorkTable,
    next_work: WorkId,
    records: Vec<records::Record>,
    startup_session: Option<String>,
    use_sequence: u64,
    limit: u8,
    notices: watch::Sender<Arc<Vec<Notice>>>,
}
enum Event {
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
    RefreshQuota(oneshot::Sender<Result<(), String>>),
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
    StartFailed(String, String),
}
enum Command {
    Authenticate,
    Lockout(crate::store::Lockout),
    AdmitChecked {
        id: WorkId,
        session: String,
    },
    RefusePending {
        request: Box<PendingRequest>,
        error: crate::messages::Message,
    },
    State(&'static str, Option<std::io::ErrorKind>),
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
impl Core {
    fn state(&self) -> &'static str {
        if self.lockout.is_some() {
            "Lockout"
        } else if matches!(self.runtime_state, RuntimeState::Unavailable(_)) {
            "Runtime unavailable"
        } else if self.sign_in_required {
            "Sign-in required"
        } else if self.quota_wait.is_some() {
            "Quota wait"
        } else {
            "Running"
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
    async fn replace(
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
                    let retained = self.retained_sessions();
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
                    &copy_failed_reason(&selected.version),
                    error.io_kind(),
                    runtime,
                    read_open,
                    journal,
                    builds,
                )
                .await;
                // Startup must keep the management panel available after a failed copy.
                let _ = reply.send(Ok(String::new()));
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
        let configured = publish.borrow().agy_acp_server_path.clone();
        let result = builds
            .discover(configured)
            .await
            .map(|()| String::new())
            .map_err(Into::into);
        let _ = reply.send(result);
        None
    }
    fn retained_sessions(&self) -> Vec<(String, PathBuf)> {
        self.records
            .iter()
            .filter_map(|record| match &record.location {
                records::Location::Unloaded(cwd) => Some((record.session.clone(), cwd.clone())),
                records::Location::Loaded => None,
            })
            .collect()
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
    async fn finish_check(
        &mut self,
        check: CompletedCheck,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
        publish: &watch::Sender<Arc<crate::store::Settings>>,
    ) {
        let result = builds.finish_check(&check.identity, check.result);
        self.launch(runtime, read_open, &builds.start.clock, journal, builds)
            .await;
        let configured = publish.borrow().agy_acp_server_path.clone();
        let discovered = builds.discover(configured).await;
        let _ = check
            .reply
            .send(discovered.map_err(LaunchFailure::Runtime).and(result));
    }
    async fn launch(
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
                self.make_unavailable(REPEATED_FAILURES, None, runtime, read_open, journal, builds)
                    .await;
                return;
            }
            let retained = self.cleanup_before_start.then(|| self.retained_sessions());
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
                Err(LaunchFailure::Runtime(_) | LaunchFailure::Io(_)) => {
                    self.apply(Event::RuntimeFailure, runtime, read_open, clock, journal)
                        .await;
                }
                Err(LaunchFailure::Stopped) => {
                    self.apply(Event::Stop, runtime, read_open, clock, journal)
                        .await;
                    return;
                }
                Err(error @ (LaunchFailure::NoExecutable | LaunchFailure::NotAccepted)) => {
                    self.make_unavailable(
                        &error.to_string(),
                        None,
                        runtime,
                        read_open,
                        journal,
                        builds,
                    )
                    .await;
                    return;
                }
            }
        }
    }
    async fn make_unavailable(
        &mut self,
        reason: &str,
        cause: Option<std::io::ErrorKind>,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
    ) {
        self.apply(
            Event::RuntimeUnavailable(UnavailableReason {
                message: builds.unavailable(reason),
                cause,
            }),
            runtime,
            read_open,
            &builds.start.clock,
            journal,
        )
        .await;
    }
    async fn restart(
        &mut self,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
        builds: &crate::runtime_builds::Builds,
    ) {
        self.stop_runtime(runtime, read_open).await;
        self.launch(runtime, read_open, clock, journal, builds)
            .await;
    }
    async fn stop_runtime(&mut self, runtime: &mut Option<Runtime>, read_open: &mut bool) {
        *read_open = false;
        if let Some(old) = runtime.take() {
            for record in &mut self.records {
                if matches!(record.location, records::Location::Loaded) {
                    record.location =
                        records::Location::Unloaded(old.session_cwd(&record.session).to_path_buf());
                }
            }
            old.stop().await;
        }
        self.replaced(None, None);
    }
    fn replacement_ready(&self) -> bool {
        self.lockout.is_none()
            && matches!(
                self.runtime_state,
                RuntimeState::Retiring { .. }
                    | RuntimeState::Restarting
                    | RuntimeState::Unavailable(_)
            )
            && !self.work.values().any(Work::occupies_place)
    }
    fn replaced(&mut self, session: Option<String>, catalog: Option<crate::catalog::Catalog>) {
        self.startup_session = session;
        self.catalog = catalog;
    }

    fn by_session(&self, session: &str) -> Option<WorkId> {
        self.work.iter().find_map(|(id, work)| {
            let current = match work {
                Work::CheckQueued(_)
                | Work::Checking(_)
                | Work::AcceptedQueued(_)
                | Work::QuotaQueued { .. } => None,
                Work::AcceptedWorking(active) => active.session.as_deref(),
                Work::Retiring(outstanding) => outstanding.session.as_deref(),
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
            .chain(self.records.iter().map(|record| record.expires))
            .chain(self.cancels.values().copied())
            .chain(self.quota_wait.as_ref().map(|wait| wait.deadline))
            .chain(self.sign_in_wait.as_ref().map(|wait| wait.deadline))
            .chain(self.retry_waits.iter().map(|wait| wait.deadline))
            .min()
    }
    fn refusal(&self, now: Moment) -> Option<crate::messages::Message> {
        if let Some(evidence) = &self.lockout {
            return Some(evidence.message(&self.panel_address));
        }
        if let RuntimeState::Unavailable(reason) = &self.runtime_state {
            return Some(crate::messages::Message::RuntimeUnavailable(
                reason.message.clone(),
            ));
        }
        if self.sign_in_required {
            return Some(self.sign_in_message());
        }
        self.quota_wait
            .as_ref()
            .filter(|wait| wait.deadline > now)
            .map(|wait| wait.message(now))
    }
    fn start_quota(
        &mut self,
        until: std::time::SystemTime,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) -> quota::Wait {
        let started = self
            .quota_wait
            .as_ref()
            .is_none_or(|wait| wait.deadline <= now);
        self.next_quota.0 += 1;
        let wait = quota::Wait::new(until, now, wall, self.next_quota);
        self.quota_wait = Some(wait.clone());
        if started {
            let message = self.refusal(now).unwrap_or_else(|| wait.message(now));
            self.refuse_queued(
                crate::journal::Outcome::Refused("quota_wait"),
                message,
                commands,
            );
        }
        wait
    }
    fn check(
        &self,
        turn: &TurnRequest,
        now: Moment,
    ) -> Result<crate::catalog::SelectedModel, crate::messages::Refusal> {
        match self.refusal(now) {
            Some(message) => Err(message.into()),
            None => self.check_model(turn).and_then(|model| {
                retry::refusal(&self.retry_waits, turn, now)
                    .map_or(Ok(model), |message| Err(message.into()))
            }),
        }
    }
    fn check_model(
        &self,
        turn: &TurnRequest,
    ) -> Result<crate::catalog::SelectedModel, crate::messages::Refusal> {
        self.catalog.as_ref().map_or_else(
            || {
                Ok(crate::catalog::SelectedModel {
                    model: turn.model.clone(),
                    level: None,
                    level_from_name: false,
                })
            },
            |catalog| catalog.resolve(&turn.model, &turn.effort),
        )
    }
    fn model_entries(
        &self,
        now: Moment,
    ) -> Result<Vec<crate::catalog::ModelEntry>, crate::messages::Refusal> {
        if let Some(message) = self.refusal(now) {
            return Err(message.into());
        }
        Ok(self
            .catalog
            .as_ref()
            .map(crate::catalog::Catalog::entries)
            .unwrap_or_default())
    }
    fn owner_action(
        &mut self,
        action: OwnerAction,
        reply: oneshot::Sender<Result<(), String>>,
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
                    let _ = reply.send(Err(sign_in::CANNOT_START.into()));
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
                let _ = reply.send(Err(error));
                Vec::new()
            }
        }
    }
    fn queue_full(&self) -> bool {
        self.work.values().filter(|work| work.is_queued()).count() == QUEUE_CAPACITY
    }
    fn quota_signal(
        &mut self,
        error: &crate::messages::Message,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) -> Option<(quota::Wait, bool)> {
        let reset = quota::reset_time(quota::error_text(error)?, wall);
        let known = reset.is_some();
        let wait = self.start_quota(reset.unwrap_or(wall + QUOTA_FALLBACK), now, wall, commands);
        Some((wait, known))
    }
    fn admit(
        &self,
        work: TurnWork,
        journal: &crate::journal::Journal,
        clock: &dyn Clock,
    ) -> Option<(Event, Hold)> {
        let TurnWork { pending, hold } = work;
        if self.sign_in_required
            && self.lockout.is_none()
            && !matches!(self.runtime_state, RuntimeState::Unavailable(_))
        {
            if !self.queue_full() {
                return Some((Event::CheckPending(Box::new(pending)), hold));
            }
            self.accept(
                pending,
                Err(crate::messages::Message::QueueFull.into()),
                journal,
                clock,
            );
            return None;
        }
        let mut checked = self.check(&pending.turn, clock.now());
        if self.queue_full() {
            checked = checked.and(Err(crate::messages::Message::QueueFull.into()));
        }
        self.accept(pending, checked, journal, clock)
            .map(|accepted| (Event::Accepted(Box::new(accepted)), hold))
    }
    fn accept(
        &self,
        pending: PendingRequest,
        checked: Result<crate::catalog::SelectedModel, crate::messages::Refusal>,
        journal: &crate::journal::Journal,
        clock: &dyn Clock,
    ) -> Option<Accepted> {
        let (output, ticket) = mpsc::unbounded_channel();
        let (model, admitted) = match checked {
            Ok(model) => {
                if self.catalog.is_some()
                    && !model.level_from_name
                    && let EffortAsk::Unknown(value) = &pending.turn.effort
                {
                    journal.diagnostic(
                        &pending.request,
                        crate::journal::Diagnostic::UnknownEffort {
                            value: value.clone(),
                        },
                        clock,
                    );
                }
                let ticket = Ticket {
                    events: ticket,
                    hold: clock.hold(HoldKind::TurnEvent),
                };
                (Some(model), pending.admission.send(Ok(ticket)))
            }
            Err(mut error) => {
                error.hold = clock.hold(HoldKind::TurnEvent);
                (None, pending.admission.send(Err(error)))
            }
        };
        if admitted.is_err() {
            journal.complete(
                crate::journal::Completion {
                    request: pending.request,
                    session: None,
                    session_use: None,
                    outcome: crate::journal::Outcome::Disconnected,
                },
                clock,
            );
            return None;
        }
        let model = model?;
        Some(Accepted {
            turn: pending.turn,
            model,
            request: pending.request,
            reply: ResponseChannel {
                output,
                keep_alive: matches!(pending.mode, ResponseMode::Stream)
                    .then(|| clock.now() + KEEP_ALIVE_INTERVAL),
            },
        })
    }
    fn handle(
        &mut self,
        mut event: Event,
        now: Moment,
        wall: std::time::SystemTime,
    ) -> Vec<Command> {
        if matches!(event, Event::Deadline) && self.cancels.values().any(|at| *at <= now) {
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
        let mut upstream_error = None;
        let mut failed_reply_signal = None;
        let error = match &mut event {
            Event::Runtime(
                RuntimeEvent::Authenticated(Err(error))
                | RuntimeEvent::Prepared {
                    result: Err(error), ..
                }
                | RuntimeEvent::Turn {
                    update: TurnUpdate::Failed(error),
                    ..
                },
            ) => Some(error),
            Event::Runtime(RuntimeEvent::UpstreamError { session, error }) => {
                if let Some(active) = self
                    .by_session(session)
                    .and_then(|id| self.work[&id].execution())
                {
                    let reply = format!(
                        "{}{}{}",
                        active.text,
                        active.stop_prefix,
                        active.held.as_deref().unwrap_or("")
                    );
                    if reply.contains("Terms of Service") {
                        failed_reply_signal = Some("Terms of Service");
                    }
                }
                Some(upstream_error.insert(crate::messages::Message::UpstreamHttp(error.clone())))
            }
            _ => None,
        };
        if let Some(error) = error {
            if let Some(signal) = error.lockout_signal().or(failed_reply_signal)
                && self.lockout.is_none()
            {
                let evidence = crate::store::Lockout {
                    since: crate::journal::timestamp(wall).expect("clock after the Unix epoch"),
                    signal: signal.into(),
                    original_error_text: crate::journal::redact_error(&error.to_string()),
                };
                let message = evidence.message(&self.panel_address);
                let first_message = crate::messages::Message::Lockout(format!(
                    "{message} Original error: {}",
                    evidence.original_error_text
                ));
                let trigger = match &event {
                    Event::Runtime(RuntimeEvent::Prepared { work, .. }) => self.by_request(work),
                    Event::Runtime(
                        RuntimeEvent::Turn { session, .. }
                        | RuntimeEvent::UpstreamError { session, .. },
                    ) => self.by_session(session),
                    _ => None,
                };
                self.lockout = Some(evidence.clone());
                self.sign_in_wait = None;
                self.cancels.clear();
                for work in self.work.values() {
                    if let Work::QuotaWorking {
                        phase: QuotaPhase::Prompt(session),
                        ..
                    } = work
                    {
                        commands.push(Command::Cancel(session.clone()));
                    }
                    if let Some(active) = work.execution()
                        && active.phase.update_deadline().is_some()
                        && let Some(session) = &active.session
                    {
                        commands.push(Command::Cancel(session.clone()));
                    }
                }
                commands.push(Command::Lockout(evidence));
                commands.push(Command::State(self.state(), None));
                self.refuse_queued(
                    crate::journal::Outcome::Refused("lockout"),
                    message.clone(),
                    &mut commands,
                );
                while let Some(id) = self.work.keys().next().copied() {
                    let error = if trigger == Some(id) {
                        first_message.clone()
                    } else {
                        message.clone()
                    };
                    self.finish(id, Err(Failure::at(error, now)), now, &mut commands);
                }
                self.view.send_replace(Arc::new(View {
                    state: self.state(),
                    queued: 0,
                    sign_in_wait: None,
                }));
                return commands;
            }
            *error = self.sign_in_error(error.clone(), now, &mut commands);
        }
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
            self.cancels.remove(session);
        }
        let quota_work = work_id.filter(|id| matches!(self.work[id], Work::QuotaWorking { .. }));
        match (quota_work, event) {
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
            (None, Event::RefreshQuota(reply)) => {
                let error = if let Some(evidence) = &self.lockout {
                    Some(evidence.message(&self.panel_address).to_string())
                } else if let RuntimeState::Unavailable(reason) = &self.runtime_state {
                    Some(reason.message.clone())
                } else if matches!(self.runtime_state, RuntimeState::Stopping) {
                    Some(crate::messages::Message::Stopped.to_string())
                } else if self.sign_in_required {
                    Some(self.sign_in_message().to_string())
                } else if self
                    .last_refresh
                    .is_some_and(|at| now.saturating_sub(at) < 60_000)
                {
                    Some("Wait 60 s between quota refreshes.".into())
                } else if self.queue_full() {
                    Some(crate::messages::Message::QueueFull.to_string())
                } else {
                    None
                };
                if let Some(error) = error {
                    let _ = reply.send(Err(error));
                } else {
                    self.last_refresh = Some(now);
                    self.enqueue_quota(QuotaPurpose::Refresh(reply), None);
                }
            }
            (None, Event::Stop) => {
                self.sign_in_wait = None;
                self.runtime_state = RuntimeState::Stopping;
                self.cancels.clear();
                self.refuse_queued(
                    crate::journal::Outcome::Refused("stopped"),
                    crate::messages::Message::Stopped,
                    &mut commands,
                );
                let ids: Vec<_> = self.work.keys().copied().collect();
                for id in ids {
                    if let Some(active) = self.work[&id].execution()
                        && matches!(active.phase, Phase::Prompt { .. })
                        && let Some(session) = &active.session
                    {
                        commands.push(Command::Cancel(session.clone()));
                    }
                    self.finish(
                        id,
                        Err(Failure::at(crate::messages::Message::Stopped, now)),
                        now,
                        &mut commands,
                    );
                }
            }
            (None, Event::RuntimeUnavailable(reason)) => {
                self.sign_in_wait = None;
                if matches!(self.replacement, Some(Replacement::SignIn)) {
                    self.replacement = None;
                }
                let error = crate::messages::Message::RuntimeUnavailable(reason.message.clone());
                self.runtime_state = RuntimeState::Unavailable(reason);
                self.refuse_queued(
                    crate::journal::Outcome::Refused("runtime_unavailable"),
                    error.clone(),
                    &mut commands,
                );
                let queries: Vec<_> = self
                    .work
                    .iter()
                    .filter_map(|(id, work)| {
                        matches!(work, Work::QuotaQueued { .. }).then_some(*id)
                    })
                    .collect();
                for id in queries {
                    self.finish(id, Err(Failure::at(error.clone(), now)), now, &mut commands);
                }
            }
            (None, Event::RetireRuntime) => {
                let sign_in_wait_ended = self.sign_in_wait.take().is_some()
                    || matches!(
                        self.runtime_state,
                        RuntimeState::Retiring {
                            sign_in_wait_ended: true
                        }
                    );
                if !matches!(self.runtime_state, RuntimeState::Unavailable(_)) {
                    self.runtime_state = RuntimeState::Retiring { sign_in_wait_ended };
                }
            }
            (None, Event::ResumeRuntime) => {
                if matches!(
                    self.runtime_state,
                    RuntimeState::Retiring {
                        sign_in_wait_ended: true
                    }
                ) {
                    self.replacement.get_or_insert(Replacement::SignIn);
                } else {
                    self.runtime_state = RuntimeState::Serving;
                }
            }
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
                let id = self.by_request(&request_id).unwrap();
                if let Work::QuotaQueued { query, .. } | Work::QuotaWorking { query, .. } =
                    self.work.get_mut(&id).unwrap()
                {
                    query.disconnect(&mut commands);
                } else {
                    match self.work.remove(&id).unwrap() {
                        Work::CheckQueued(request) => {
                            commands.push(Command::Complete(crate::journal::Completion {
                                request: request.request,
                                session: None,
                                session_use: None,
                                outcome: crate::journal::Outcome::Disconnected,
                            }))
                        }
                        Work::Checking(request) => {
                            self.work.insert(
                                id,
                                Work::Retiring(OutstandingWork {
                                    model: request.turn.model,
                                    request: request.request,
                                    session: None,
                                    completion: PendingCompletion::Disconnected(None),
                                }),
                            );
                        }
                        Work::AcceptedQueued(request) => Self::drop_queued(request, &mut commands),
                        Work::AcceptedWorking(active) => {
                            self.keep_quota_wait_reset_query(&active);
                            let ended = matches!(
                                active.phase,
                                Phase::Settling { .. } | Phase::QuotaSettling { .. }
                            );
                            if active.phase.update_deadline().is_some() {
                                self.cancel(
                                    active.session.as_ref().unwrap().clone(),
                                    now,
                                    &mut commands,
                                );
                            }
                            self.work.insert(
                                id,
                                Work::Retiring(OutstandingWork {
                                    model: active.model,
                                    request: active.request,
                                    session: active.session,
                                    completion: PendingCompletion::Disconnected(Some(active.kind)),
                                }),
                            );
                            if ended {
                                self.finish(id, Ok(()), now, &mut commands);
                            }
                        }
                        Work::Retiring(_)
                        | Work::QuotaQueued { .. }
                        | Work::QuotaWorking { .. } => unreachable!(),
                    }
                }
            }
            (None, Event::StartFailed(_, error)) => {
                if let Some(id) = work_id {
                    self.finish(
                        id,
                        Err(Failure::at(crate::messages::Message::Upstream(error), now)),
                        now,
                        &mut commands,
                    );
                }
            }
            (None, Event::Runtime(RuntimeEvent::Prepared { work, result })) => {
                if let Some(id) = work_id {
                    match result {
                        Ok((session, catalog)) => {
                            self.catalog = Some(catalog);
                            self.sign_in_required = false;
                            match self.work.get_mut(&id).unwrap() {
                                Work::Checking(_) => {
                                    commands.push(Command::AdmitChecked { id, session });
                                }
                                Work::Retiring(outstanding) => {
                                    outstanding.session = Some(session);
                                    self.finish(id, Ok(()), now, &mut commands);
                                }
                                entry => {
                                    let active = entry.execution_mut().unwrap();
                                    active.session = Some(session.clone());
                                    if let Phase::AwaitingQuotaEnd { wait, .. } = &active.phase {
                                        active.phase = Phase::QuotaSettling {
                                            at: now,
                                            wait: wait.clone(),
                                        };
                                    } else if let Phase::Preparing(prompt) =
                                        std::mem::replace(&mut active.phase, Phase::prompt(now))
                                    {
                                        if let Some(message) = self.refusal(now) {
                                            self.finish(
                                                id,
                                                Err(Failure::at(message, now)),
                                                now,
                                                &mut commands,
                                            );
                                        } else {
                                            commands.push(Command::Prompt {
                                                work,
                                                session,
                                                prompt,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            if let Some((wait, known)) =
                                self.quota_signal(&error, now, wall, &mut commands)
                            {
                                let pending_model = match &self.work[&id] {
                                    Work::Checking(request) => Some(request.turn.model.clone()),
                                    Work::Retiring(outstanding) => Some(outstanding.model.clone()),
                                    _ => None,
                                };
                                if known || pending_model.is_some() {
                                    self.finish(
                                        id,
                                        Err(Failure::at(
                                            self.refusal(now).unwrap_or_else(|| wait.message(now)),
                                            now,
                                        )),
                                        now,
                                        &mut commands,
                                    );
                                    if !known {
                                        self.enqueue_reset_query(wait, pending_model);
                                    }
                                } else {
                                    let mut active =
                                        self.work.remove(&id).unwrap().into_active().unwrap();
                                    active.phase = Phase::Settling {
                                        at: now,
                                        can_continue: false,
                                        result: Ok(()),
                                    };
                                    self.enqueue_quota(
                                        QuotaPurpose::Reset {
                                            wait,
                                            model: Some(active.model.clone()),
                                            response: Some(active),
                                        },
                                        None,
                                    );
                                }
                            } else {
                                self.finish(id, Err(Failure::at(error, now)), now, &mut commands);
                            }
                        }
                    }
                } else if let Ok((session, _)) = result {
                    commands.push(Command::Forget(session));
                }
            }
            (None, Event::Runtime(RuntimeEvent::Retired { session })) => {
                self.cancels.remove(&session);
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
                let id = work_id;
                let message = upstream_error.unwrap();
                if let Some((wait, known)) = self.quota_signal(&message, now, wall, &mut commands) {
                    if let Some(id) = id {
                        if let Some(active) = self.work.get_mut(&id).unwrap().execution_mut() {
                            if known {
                                self.end_request(
                                    id,
                                    Failure::at(wait.message(now), now),
                                    now,
                                    &mut commands,
                                );
                            } else {
                                active.phase = match active.phase {
                                    Phase::Settling { at, .. }
                                    | Phase::QuotaSettling { at, .. } => {
                                        Phase::QuotaSettling { at, wait }
                                    }
                                    _ => Phase::AwaitingQuotaEnd {
                                        wait,
                                        update_deadline: active.phase.update_deadline(),
                                    },
                                };
                            }
                        } else if !known {
                            let Work::Retiring(outstanding) = &self.work[&id] else {
                                unreachable!()
                            };
                            let model = Some(outstanding.model.clone());
                            self.enqueue_reset_query(wait, model);
                        }
                    } else {
                        self.records.retain(|record| record.session != session);
                        if self.startup_session.as_ref() == Some(&session) {
                            self.startup_session = None;
                        }
                        commands.push(Command::Forget(session));
                        if !known {
                            self.enqueue_reset_query(wait, None);
                        }
                    }
                } else if let Some(id) = id {
                    if !matches!(self.work[&id], Work::Retiring(_)) {
                        self.fail_request(id, session, message, now, &mut commands);
                    }
                } else {
                    self.records.retain(|record| record.session != session);
                    if self.startup_session.as_ref() == Some(&session) {
                        self.startup_session = None;
                    }
                    commands.push(Command::Forget(session));
                }
            }
            (None, Event::Runtime(RuntimeEvent::Failed(error))) => {
                self.forget_idle(&mut commands);
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
            (None, Event::Runtime(RuntimeEvent::Closed))
                if matches!(self.runtime_state, RuntimeState::Restarting) => {}
            (None, Event::Runtime(RuntimeEvent::Closed) | Event::RuntimeFailure) => {
                self.sign_in_wait = None;
                self.runtime_state = RuntimeState::Restarting;
                self.cancels.clear();
                self.failures
                    .retain(|at| now.saturating_sub(*at) <= RUNTIME_FAILURE_WINDOW);
                self.failures.push_back(now);
                self.startup_session = None;
                while let Some(id) = self
                    .work
                    .iter()
                    .find_map(|(id, work)| work.occupies_place().then_some(*id))
                {
                    self.finish(
                        id,
                        Err(Failure::at(crate::messages::Message::RuntimeFailed, now)),
                        now,
                        &mut commands,
                    );
                }
                self.catalog = None;
            }
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
        self.records.retain(|record| {
            if record.expires <= now {
                commands.push(Command::Forget(record.session.clone()));
                false
            } else {
                true
            }
        });
        let mut places = self
            .work
            .values()
            .filter(|work| work.occupies_place())
            .count();
        while self.lockout.is_none()
            && matches!(self.runtime_state, RuntimeState::Serving)
            && places < usize::from(self.limit)
        {
            let Some(id) = self.work.iter().find_map(|(id, work)| match work {
                Work::CheckQueued(_) => Some(*id),
                Work::QuotaQueued { .. } if !self.sign_in_required => Some(*id),
                Work::AcceptedQueued(_) if !self.sign_in_required && self.quota_wait.is_none() => {
                    Some(*id)
                }
                _ => None,
            }) else {
                break;
            };
            let request = match self.work.remove(&id).unwrap() {
                Work::CheckQueued(request) => {
                    commands.push(Command::Prepare {
                        work: request.request.id.clone(),
                        settings: Session::for_turn(&request.turn),
                        load: None,
                    });
                    self.work.insert(id, Work::Checking(request));
                    places += 1;
                    continue;
                }
                Work::QuotaQueued { query, session } => {
                    if let QuotaPurpose::Reset { wait, .. } = &query.purpose
                        && !self.can_query_quota_reset(wait)
                    {
                        let error = self.quota_wait.as_ref().unwrap().message(now);
                        self.complete_quota(
                            query,
                            session,
                            Err(Failure::at(error, now)),
                            now,
                            &mut commands,
                        );
                        continue;
                    }
                    let phase =
                        if let Some(session) = session.or_else(|| self.startup_session.take()) {
                            commands.push(Command::Usage {
                                work: query.work.clone(),
                                session: session.clone(),
                            });
                            QuotaPhase::Prompt(session)
                        } else {
                            commands.push(Command::Prepare {
                                work: query.work.clone(),
                                settings: Session {
                                    tools: Vec::new(),
                                    system: None,
                                },
                                load: None,
                            });
                            QuotaPhase::Preparing
                        };
                    self.work.insert(
                        id,
                        Work::QuotaWorking {
                            query,
                            phase,
                            text: String::new(),
                            result: Ok(()),
                        },
                    );
                    places += 1;
                    continue;
                }
                work => work.into_queued().unwrap(),
            };
            if self.start_request(id, request, None, now, &mut commands) {
                places += 1;
            }
        }
        self.emit_keep_alives(now, &mut commands);
        if self.state() != previous {
            let cause = match &self.runtime_state {
                RuntimeState::Unavailable(reason) => reason.cause,
                _ => None,
            };
            commands.insert(0, Command::State(self.state(), cause));
        }
        self.view.send_replace(Arc::new(View {
            sign_in_wait: self.sign_in_wait.clone(),
            state: self.state(),
            queued: self.work.values().filter(|work| work.is_queued()).count(),
        }));
        commands
    }
    fn start_request(
        &mut self,
        id: WorkId,
        request: Accepted,
        mut checked_session: Option<String>,
        now: Moment,
        commands: &mut Vec<Command>,
    ) -> bool {
        if request.reply.output.is_closed() {
            commands.extend(checked_session.map(Command::Forget));
            Self::drop_queued(request, commands);
            return false;
        }
        if let Some(message) = retry::refusal(&self.retry_waits, &request.turn, now) {
            commands.extend(checked_session.map(Command::Forget));
            Self::refuse_request(
                request,
                crate::journal::Outcome::Refused("retry_wait"),
                message,
                commands,
            );
            return false;
        }
        let client_tools_hash = crate::conversation::tool_set_hash(&request.turn.tools);
        let settings = Session::for_turn(&request.turn);
        let hashes = crate::conversation::record_hashes(&request.turn.conversation);
        let tools_hash = crate::conversation::tool_set_hash(&settings.tools);
        let items = request.turn.conversation.items();
        let matched = records::select(&self.records, &hashes, tools_hash, &items);
        let prompt = match matched {
            Some((_, new_items)) => Prompt::Continue { new_items },
            None if request.turn.conversation.earlier.is_empty() => Prompt::New {
                message: &request.turn.conversation.last,
            },
            None => Prompt::Rebuild {
                conversation_items: &items,
            },
        };
        let (kind, prompt) = crate::conversation::prompt(prompt, &items);
        let prompt = PromptRequest {
            blocks: prompt,
            model: request.model.model,
            level: request.model.level,
        };
        let model = prompt.model.clone();
        let ready = match matched {
            Some((index, _)) => {
                let record = self.records.remove(index);
                Some((record.session, record.location))
            }
            None if checked_session.is_some() => checked_session
                .take()
                .map(|session| (session, records::Location::Loaded)),
            None if settings.system.is_none() && settings.tools.is_empty() => self
                .startup_session
                .take()
                .map(|id| (id, records::Location::Loaded)),
            None => None,
        };
        if matched.is_some() {
            commands.extend(checked_session.map(Command::Forget));
        }
        self.use_sequence += 1;
        let work = request.request.id.clone();
        let (session, phase) = match ready {
            Some((id, records::Location::Loaded)) => {
                commands.push(Command::Prompt {
                    work: work.clone(),
                    session: id.clone(),
                    prompt,
                });
                (Some(id), Phase::prompt(now))
            }
            Some((session, records::Location::Unloaded(cwd))) => {
                commands.push(Command::Prepare {
                    work,
                    settings: settings.clone(),
                    load: Some((session.clone(), cwd)),
                });
                (Some(session), Phase::Preparing(prompt))
            }
            None => {
                commands.push(Command::Prepare {
                    work,
                    settings: settings.clone(),
                    load: None,
                });
                (None, Phase::Preparing(prompt))
            }
        };
        self.work.insert(
            id,
            Work::AcceptedWorking(Active {
                reply: request.reply,
                request: request.request,
                session,
                hashes,
                tools_hash,
                client_tools_hash,
                kind,
                used: self.use_sequence,
                expires: now.saturating_add(24 * 60 * 60 * 1000),
                held: None,
                text: String::new(),
                thought_dropped: false,
                input_tokens: request.turn.input_tokens,
                model,
                model_name: request.turn.model,
                effort: request.turn.effort,
                phase,
                tool_call: None,
                stop: crate::conversation::Stop::EndTurn,
                stops: request.turn.stop,
                stop_prefix: String::new(),
            }),
        );
        true
    }
    fn quota_event(
        &mut self,
        id: WorkId,
        event: Event,
        upstream_error: Option<crate::messages::Message>,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        match event {
            Event::StartFailed(_, error) => self.finish_quota(
                id,
                Err(crate::messages::Message::Upstream(error)),
                now,
                wall,
                commands,
            ),
            Event::Runtime(RuntimeEvent::Prepared { work, result }) => match result {
                Ok((session, catalog)) => {
                    self.catalog = Some(catalog);
                    let Work::QuotaWorking { phase, .. } = self.work.get_mut(&id).unwrap() else {
                        unreachable!()
                    };
                    *phase = QuotaPhase::Prompt(session.clone());
                    commands.push(Command::Usage { work, session });
                }
                Err(error) => self.finish_quota(id, Err(error), now, wall, commands),
            },
            Event::Runtime(RuntimeEvent::Turn { session, update }) => {
                self.update_quota(id, session, update, now, commands)
            }
            Event::Runtime(RuntimeEvent::UpstreamError { .. }) => {
                let message = upstream_error.unwrap();
                let _ = self.quota_signal(&message, now, wall, commands);
                let Work::QuotaWorking { result, .. } = self.work.get_mut(&id).unwrap() else {
                    unreachable!()
                };
                *result = Err(message);
            }
            Event::Deadline => self.finish_quota(id, Ok(()), now, wall, commands),
            _ => unreachable!(),
        }
    }
    fn enqueue_reset_query(&mut self, wait: quota::Wait, model: Option<String>) {
        self.enqueue_quota(
            QuotaPurpose::Reset {
                wait,
                model,
                response: None,
            },
            None,
        );
    }
    fn enqueue_quota(&mut self, purpose: QuotaPurpose, session: Option<String>) {
        let id = self.next_work;
        self.next_work += 1;
        let query = QuotaQuery {
            work: format!("quota-{id}"),
            purpose,
        };
        self.work.insert(id, Work::QuotaQueued { query, session });
    }
    fn update_quota(
        &mut self,
        id: WorkId,
        session: String,
        update: TurnUpdate,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        let Work::QuotaWorking {
            phase,
            text,
            result,
            ..
        } = self.work.get_mut(&id).unwrap()
        else {
            unreachable!()
        };
        match update {
            TurnUpdate::Text(chunk) => text.push_str(&chunk),
            TurnUpdate::PromptEnded { .. } => {
                *phase = QuotaPhase::Settling {
                    session,
                    at: now.saturating_add(SETTLE_DELAY),
                }
            }
            TurnUpdate::Failed(error) => {
                *result = Err(error);
                *phase = QuotaPhase::Settling {
                    session,
                    at: now.saturating_add(SETTLE_DELAY),
                };
            }
            TurnUpdate::Permission { request, .. }
            | TurnUpdate::RefusedPermission { request, .. } => {
                commands.push(Command::Permission(request))
            }
            TurnUpdate::UnpermittedTool(name) => {
                *result = Err(crate::messages::Message::BuiltInToolRan(name));
                self.cancel(session, now, commands);
            }
            TurnUpdate::Activity | TurnUpdate::Usage(_) | TurnUpdate::ThoughtDropped => {}
        }
    }
    fn finish_quota(
        &mut self,
        id: WorkId,
        result: Result<(), crate::messages::Message>,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        let Work::QuotaWorking {
            mut query,
            phase,
            text,
            result: recorded,
        } = self.work.remove(&id).unwrap()
        else {
            unreachable!()
        };
        let result = result.and(recorded);
        self.quota_result.send_replace(Arc::new(Some(QuotaResult {
            text: match &result {
                Ok(()) => text.clone(),
                Err(error) => error.to_string(),
            },
            time: crate::journal::timestamp(wall).expect("quota result time in UTC"),
        })));
        let end = match &mut query.purpose {
            QuotaPurpose::Refresh(_) => result,
            QuotaPurpose::Reset { wait, model, .. } => {
                if result.is_ok()
                    && let Some(until) = model
                        .as_deref()
                        .and_then(|model| quota::usage_reset(&text, model, wall))
                {
                    *wait = quota::Wait::new(until, now, wall, wait.timer);
                    if self.is_current_quota_wait(wait) {
                        self.quota_wait = Some(wait.clone());
                    }
                }
                Err(self.refusal(now).unwrap_or_else(|| wait.message(now)))
            }
            QuotaPurpose::Refusal { model, .. } => {
                if result.is_ok() && quota::has_quota(&text, model) {
                    Ok(())
                } else {
                    let until = if result.is_ok() {
                        quota::usage_reset(&text, model, wall)
                    } else {
                        None
                    };
                    let wait = self.start_quota(
                        until.unwrap_or(wall + QUOTA_FALLBACK),
                        now,
                        wall,
                        commands,
                    );
                    Err(self.refusal(now).unwrap_or_else(|| wait.message(now)))
                }
            }
        };
        self.complete_quota(
            query,
            phase.session().map(str::to_owned),
            end.map_err(|message| Failure::at(message, now)),
            now,
            commands,
        );
    }
    fn complete_quota(
        &mut self,
        query: QuotaQuery,
        session: Option<String>,
        result: Result<(), Failure>,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        let response = match query.purpose {
            QuotaPurpose::Refresh(reply) => {
                let _ = reply.send(result.map_err(|failure| failure.message.to_string()));
                commands.extend(session.map(Command::Forget));
                return;
            }
            QuotaPurpose::Reset { response, .. } | QuotaPurpose::Refusal { response, .. } => {
                response
            }
        };
        if let Some(active) = response {
            let forget = self.complete_request(active, result, now, commands);
            commands.extend(
                session
                    .filter(|session| Some(session) != forget.as_ref())
                    .map(Command::Forget),
            );
            commands.extend(forget.map(Command::Forget));
        } else {
            commands.extend(session.map(Command::Forget));
        }
    }
    fn update(
        &mut self,
        id: WorkId,
        session: String,
        update: TurnUpdate,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        if matches!(self.work[&id], Work::Retiring(_)) {
            match update {
                TurnUpdate::PromptEnded { .. } | TurnUpdate::Failed(_) => {
                    self.finish(id, Ok(()), now, commands);
                }
                TurnUpdate::Permission { request, .. }
                | TurnUpdate::RefusedPermission { request, .. } => {
                    commands.push(Command::Permission(request));
                }
                TurnUpdate::Activity
                | TurnUpdate::Text(_)
                | TurnUpdate::Usage(_)
                | TurnUpdate::ThoughtDropped
                | TurnUpdate::UnpermittedTool(_) => {}
            }
            return;
        }
        if let TurnUpdate::Failed(error) = &update
            && let Some((wait, known)) = self.quota_signal(error, now, wall, commands)
        {
            if known {
                self.finish(id, Err(Failure::at(wait.message(now), now)), now, commands);
            } else {
                self.work
                    .get_mut(&id)
                    .unwrap()
                    .execution_mut()
                    .unwrap()
                    .phase = Phase::QuotaSettling {
                    at: now.saturating_add(SETTLE_DELAY),
                    wait,
                };
            }
            return;
        }
        let active = self.work.get_mut(&id).unwrap().execution_mut().unwrap();
        match &mut active.phase {
            Phase::Prompt { .. } => active.phase = Phase::prompt(now),
            Phase::AwaitingQuotaEnd {
                update_deadline: Some(update_deadline),
                ..
            } => {
                *update_deadline = now + NO_UPDATE_LIMIT;
            }
            _ => {}
        }
        if matches!(
            self.work[&id].execution().unwrap().phase,
            Phase::AwaitingQuotaEnd { .. } | Phase::QuotaSettling { .. }
        ) {
            let active = self.work.get_mut(&id).unwrap().execution_mut().unwrap();
            match update {
                TurnUpdate::PromptEnded { .. } | TurnUpdate::Failed(_) => {
                    if let Phase::AwaitingQuotaEnd { wait, .. } = &active.phase {
                        active.phase = Phase::QuotaSettling {
                            at: now.saturating_add(SETTLE_DELAY),
                            wait: wait.clone(),
                        };
                    }
                }
                TurnUpdate::Permission { request, .. }
                | TurnUpdate::RefusedPermission { request, .. } => {
                    commands.push(Command::Permission(request))
                }
                _ => {}
            }
            return;
        }
        let active = self.work.get_mut(&id).unwrap().execution_mut().unwrap();
        match update {
            TurnUpdate::Activity => {}
            TurnUpdate::Text(text) => {
                if active.tool_call.is_none()
                    && active.release_or_cut_reply(Some(text), now, commands)
                {
                    let session = active.session.as_ref().unwrap().clone();
                    self.cancel(session, now, commands);
                }
            }
            TurnUpdate::Usage(used) => active.input_tokens = used,
            TurnUpdate::ThoughtDropped => {
                if !active.thought_dropped {
                    active.thought_dropped = true;
                    commands.push(Command::Diagnostic(
                        active.request.clone(),
                        crate::journal::Diagnostic::DroppedThoughtText,
                    ));
                }
            }
            TurnUpdate::PromptEnded { stop_reason } => {
                if stop_reason == Some(StopReason::Refusal) {
                    active.stop = crate::conversation::Stop::Refusal;
                }
                if stop_reason == Some(StopReason::MaxTokens) && active.stop.sequence().is_none() {
                    active.stop = crate::conversation::Stop::MaxTokens;
                }
                active.phase = Phase::Settling {
                    at: now.saturating_add(SETTLE_DELAY),
                    result: if stop_reason == Some(StopReason::MaxTurnRequests) {
                        Err(Failure::at(crate::messages::Message::MaxTurnRequests, now))
                    } else {
                        Ok(())
                    },
                    can_continue: stop_reason == Some(StopReason::EndTurn)
                        || stop_reason == Some(StopReason::MaxTokens)
                        || stop_reason == Some(StopReason::Cancelled) && active.tool_call.is_some(),
                };
            }
            TurnUpdate::Failed(error) => {
                active.phase = Phase::Settling {
                    at: now.saturating_add(SETTLE_DELAY),
                    can_continue: false,
                    result: Err(Failure::at(error, now)),
                }
            }
            TurnUpdate::UnpermittedTool(name) => {
                let notice = Notice {
                    id: active.request.id.clone(),
                    name: name.clone(),
                };
                self.notices
                    .send_modify(|notices| Arc::make_mut(notices).push(notice));
                commands.push(Command::Safety(crate::journal::SafetyReport {
                    request: active.request.clone(),
                    session: session.clone(),
                    kind: crate::journal::SafetyKind::BuiltInToolRan,
                    tool: name.clone(),
                }));
                self.fail_request(
                    id,
                    session,
                    crate::messages::Message::BuiltInToolRan(name),
                    now,
                    commands,
                );
            }
            TurnUpdate::RefusedPermission { request, name } => {
                commands.push(Command::Safety(crate::journal::SafetyReport {
                    request: active.request.clone(),
                    session,
                    kind: crate::journal::SafetyKind::PermissionRefused,
                    tool: name,
                }));
                commands.push(Command::Permission(request));
            }
            TurnUpdate::Permission { request, call } => {
                let cancel =
                    matches!(active.phase, Phase::Prompt { .. }) && active.tool_call.is_none();
                if cancel {
                    active.tool_call = Some(call);
                    self.cancel(session, now, commands);
                }
                commands.push(Command::Permission(request));
            }
        }
    }
    fn forget_idle(&mut self, commands: &mut Vec<Command>) {
        let idle = self.records.drain(..).map(|record| record.session);
        commands.extend(idle.chain(self.startup_session.take()).map(Command::Forget));
    }
    fn refuse_queued(
        &mut self,
        outcome: crate::journal::Outcome,
        message: crate::messages::Message,
        commands: &mut Vec<Command>,
    ) {
        let queued: Vec<_> = self
            .work
            .iter()
            .filter_map(|(id, work)| {
                matches!(work, Work::AcceptedQueued(_) | Work::CheckQueued(_)).then_some(*id)
            })
            .collect();
        for id in queued {
            let request = match self.work.remove(&id).unwrap() {
                Work::CheckQueued(request) => {
                    commands.push(Command::RefusePending {
                        request: Box::new(request),
                        error: message.clone(),
                    });
                    continue;
                }
                work => work.into_queued().unwrap(),
            };
            Self::refuse_request(request, outcome, message.clone(), commands);
        }
    }
    fn refuse_request(
        request: Accepted,
        outcome: crate::journal::Outcome,
        message: crate::messages::Message,
        commands: &mut Vec<Command>,
    ) {
        commands.push(Command::Complete(crate::journal::Completion {
            request: request.request,
            session: None,
            session_use: None,
            outcome,
        }));
        commands.push(Command::Emit(
            request.reply.output,
            TurnEvent::End(Err(message)),
        ));
    }
    fn drop_queued(request: Accepted, commands: &mut Vec<Command>) {
        commands.push(Command::Complete(crate::journal::Completion {
            request: request.request,
            session: None,
            session_use: None,
            outcome: crate::journal::Outcome::Disconnected,
        }));
    }
    fn command_failed(
        &mut self,
        work: Option<String>,
        error: crate::runtime_driver::CommandError,
        now: Moment,
        wall: std::time::SystemTime,
    ) -> Vec<Command> {
        if self.lockout.is_some() || matches!(self.runtime_state, RuntimeState::Restarting) {
            return Vec::new();
        }
        let event = match error {
            crate::runtime_driver::CommandError::Io(_)
                if matches!(self.runtime_state, RuntimeState::Stopping) =>
            {
                Event::Stop
            }
            crate::runtime_driver::CommandError::Io(_) => Event::RuntimeFailure,
            crate::runtime_driver::CommandError::Session(error) => match work {
                Some(work) => Event::StartFailed(work, error),
                None => Event::Runtime(RuntimeEvent::Failed(error)),
            },
        };
        self.handle(event, now, wall)
    }
    fn cancel(&mut self, session: String, now: Moment, commands: &mut Vec<Command>) {
        self.cancels
            .entry(session.clone())
            .or_insert_with(|| now.saturating_add(CANCEL_TIMEOUT));
        commands.push(Command::Cancel(session));
    }
    fn fail_request(
        &mut self,
        id: WorkId,
        session: String,
        error: crate::messages::Message,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        if self.work[&id].update_deadline().is_some() {
            self.cancel(session, now, commands);
        }
        self.end_request(id, Failure::at(error, now), now, commands);
    }
    fn is_current_quota_wait(&self, wait: &quota::Wait) -> bool {
        self.quota_wait
            .as_ref()
            .is_some_and(|current| current.timer == wait.timer)
    }
    fn can_query_quota_reset(&self, wait: &quota::Wait) -> bool {
        self.quota_wait.is_none() || self.is_current_quota_wait(wait)
    }
    /// A request that leaves a quota wait without its usage check hands the reset query on.
    fn keep_quota_wait_reset_query(&mut self, active: &Active) {
        if let Phase::AwaitingQuotaEnd { wait, .. } | Phase::QuotaSettling { wait, .. } =
            &active.phase
            && self.is_current_quota_wait(wait)
        {
            self.enqueue_reset_query(wait.clone(), Some(active.model.clone()));
        }
    }
    fn end_request(
        &mut self,
        id: WorkId,
        error: Failure,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        let active = self.work.remove(&id).unwrap().into_active().unwrap();
        let ended = matches!(
            active.phase,
            Phase::Settling { .. } | Phase::QuotaSettling { .. }
        );
        let outstanding = OutstandingWork {
            model: active.model.clone(),
            request: active.request.clone(),
            session: active.session.clone(),
            completion: PendingCompletion::AlreadyCompleted,
        };
        let forget = self.complete_request(active, Err(error), now, commands);
        if ended {
            commands.extend(forget.map(Command::Forget));
        } else {
            self.work.insert(id, Work::Retiring(outstanding));
        }
    }
    fn finish(
        &mut self,
        id: WorkId,
        result: Result<(), Failure>,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        match self.work.remove(&id).unwrap() {
            Work::CheckQueued(request) | Work::Checking(request) => {
                commands.push(Command::RefusePending {
                    request: Box::new(request),
                    error: result.err().unwrap().message,
                });
            }
            Work::QuotaQueued { query, session } => {
                self.complete_quota(query, session, result, now, commands)
            }
            Work::QuotaWorking { query, phase, .. } => self.complete_quota(
                query,
                phase.session().map(str::to_owned),
                result,
                now,
                commands,
            ),
            Work::Retiring(outstanding) => {
                if let Some(session) = &outstanding.session {
                    commands.push(Command::Forget(session.clone()));
                }
                if let PendingCompletion::Disconnected(kind) = outstanding.completion {
                    commands.push(Command::Complete(crate::journal::Completion {
                        request: outstanding.request,
                        session: outstanding.session,
                        session_use: kind,
                        outcome: crate::journal::Outcome::Disconnected,
                    }));
                }
            }
            work => {
                let mut active = work.into_active().unwrap();
                let result = match std::mem::replace(&mut active.phase, Phase::Completing) {
                    Phase::QuotaSettling { wait, .. } if result.is_ok() => {
                        if self.can_query_quota_reset(&wait) {
                            let session = active.session.clone();
                            active.phase = Phase::Settling {
                                at: 0,
                                can_continue: false,
                                result: Ok(()),
                            };
                            self.enqueue_quota(
                                QuotaPurpose::Reset {
                                    wait,
                                    model: Some(active.model.clone()),
                                    response: Some(active),
                                },
                                session,
                            );
                            return;
                        }
                        Err(Failure::at(
                            self.refusal(now).unwrap_or_else(|| wait.message(now)),
                            now,
                        ))
                    }
                    phase => {
                        active.phase = phase;
                        result
                    }
                };
                if result.is_ok() && matches!(active.stop, crate::conversation::Stop::Refusal) {
                    let model = active.model.clone();
                    let session = active.session.clone();
                    self.enqueue_quota(
                        QuotaPurpose::Refusal {
                            model,
                            response: Some(active),
                        },
                        session,
                    );
                    return;
                }
                let forget = self.complete_request(active, result, now, commands);
                commands.extend(forget.map(Command::Forget));
            }
        }
    }
    fn complete_request(
        &mut self,
        mut active: Active,
        result: Result<(), Failure>,
        now: Moment,
        commands: &mut Vec<Command>,
    ) -> Option<String> {
        self.keep_quota_wait_reset_query(&active);
        let (can_continue, result) = match std::mem::replace(&mut active.phase, Phase::Completing) {
            Phase::Settling {
                can_continue,
                result: pending,
                ..
            } => (can_continue, result.and(pending)),
            Phase::Completing
            | Phase::Preparing(_)
            | Phase::Prompt { .. }
            | Phase::AwaitingQuotaEnd { .. }
            | Phase::QuotaSettling { .. } => (false, result),
        };
        let tool_hash = active
            .tool_call
            .as_ref()
            .map(|call| crate::conversation::tool_call_hash(&call.name, &call.arguments));
        if let Err(failure) = &result
            && let Some(wait) = retry::Wait::after(&active, failure, self.catalog.as_ref())
        {
            self.retry_waits.push(wait);
        }
        if result.as_ref().is_err_and(|failure| {
            !matches!(
                failure.message,
                crate::messages::Message::MaxTurnRequests | crate::messages::Message::NoUpdate
            )
        }) {
            active.held = None;
        }
        active.release_or_cut_reply(None, now, commands);
        let can_continue = can_continue && active.stop.sequence().is_none();
        let end = result.map_err(|failure| failure.message).map(|()| {
            let mut output = active.text.clone();
            if let Some(call) = active.tool_call.take() {
                output.push_str(&call.arguments);
                commands.push(Command::Emit(
                    active.reply.output.clone(),
                    TurnEvent::ToolCall(call),
                ));
            }
            Usage {
                input_tokens: active.input_tokens,
                output_tokens: crate::conversation::estimate_tokens(&output),
            }
        });
        let mut forget = None;
        if let Some(session) = &active.session {
            if end.is_ok() && can_continue {
                if tool_hash.is_none() || !active.text.is_empty() {
                    active
                        .hashes
                        .messages
                        .push(crate::conversation::reply_hash(&active.text));
                }
                if let Some(hash) = tool_hash {
                    active.hashes.messages.push(hash);
                }
                self.records.push(records::Record {
                    location: records::Location::Loaded,
                    session: session.clone(),
                    hashes: active.hashes,
                    tools: active.tools_hash,
                    used: active.used,
                    expires: active.expires,
                });
            } else {
                forget = Some(session.clone());
            }
        }
        let outcome = match &end {
            Ok(usage) => crate::journal::Outcome::Success {
                input: usage.input_tokens,
                output: usage.output_tokens,
            },
            Err(crate::messages::Message::BuiltInToolRan(_)) => {
                crate::journal::Outcome::BuiltInToolRan
            }
            Err(crate::messages::Message::MaxTurnRequests) => {
                crate::journal::Outcome::MaxTurnRequests
            }
            Err(_) => crate::journal::Outcome::Failed,
        };
        commands.push(Command::Complete(crate::journal::Completion {
            request: active.request,
            session: active.session,
            session_use: Some(active.kind),
            outcome,
        }));
        if end.is_ok() {
            commands.push(Command::Emit(
                active.reply.output.clone(),
                TurnEvent::Stop(active.stop),
            ));
        }
        commands.push(Command::Emit(active.reply.output, TurnEvent::End(end)));
        forget
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
        let lockout = match crate::store::StateFile::read(&state_path) {
            Ok(state) => {
                if !state_path.exists()
                    && let Err(error) = state.save(&state_path)
                {
                    journal.state_file_failure("create", &error, clock.as_ref());
                }
                state.lockout
            }
            Err(error) => Some(crate::store::Lockout {
                since: crate::journal::timestamp(clock.wall()).expect("clock after the Unix epoch"),
                signal: "unreadable or damaged state file".into(),
                original_error_text: crate::journal::redact_error(&error),
            }),
        };
        let panel_address = format!("http://127.0.0.1:{}/", settings.port);
        let (view, view_watch) = watch::channel(Arc::new(View::default()));
        let (publish, settings_watch) = watch::channel(Arc::new(settings));
        let (notices, notices_watch) = watch::channel(Arc::new(Vec::new()));
        let (quota_result, quota_watch) = watch::channel(Arc::new(None));
        let task = tokio::spawn(async move {
            let mut runtime: Option<Runtime> = None;
            let mut core = Core {
                quota_result,
                last_refresh: None,
                quota_wait: None,
                retry_waits: Vec::new(),
                next_quota: quota::TimerId::default(),
                runtime_state: RuntimeState::Serving,
                lockout,
                sign_in_required: false,
                sign_in_wait: None,
                panel_address,
                state_path,
                view,
                failures: VecDeque::new(),
                cleanup_before_start: false,
                cancels: BTreeMap::new(),
                changes: VecDeque::new(),
                replacement: None,
                startup_session: None,
                catalog: None,
                records: Vec::new(),
                use_sequence: 0,
                limit,
                work: BTreeMap::new(),
                next_work: 0,
                notices,
            };
            let mut read_open = false;
            core.view.send_replace(Arc::new(View {
                state: core.state(),
                queued: 0,
                sign_in_wait: None,
            }));
            journal.state(core.state(), None, clock.as_ref());
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
                                let _ = reply.send(Err(error.into()));
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
                                        let _ = reply.send(Ok(String::new()));
                                    }
                                    crate::runtime_builds::Selection::Notice(notice) => {
                                        let _ = reply.send(Ok(notice.into()));
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
                                if let Some(evidence) = &core.lockout {
                                    let result = if matches!(action, crate::runtime_builds::Action::Initial) {
                                        Ok(String::new())
                                    } else {
                                        Err(LaunchFailure::Runtime(evidence.message(&core.panel_address).to_string()))
                                    };
                                    let _ = reply.send(result);
                                } else {
                                    core.changes.push_back((action, reply));
                                }
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
                        if matches!(event, RuntimeEvent::Closed) {
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
        (
            Self {
                view: view_watch,
                quota_result: quota_watch,
                send,
                builds,
                settings: settings_watch,
                notices: notices_watch,
                highest,
            },
            task,
        )
    }
    async fn commands(
        core: &mut Core,
        runtime: &mut Option<Runtime>,
        read_open: &mut bool,
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
        commands: &mut VecDeque<Command>,
    ) {
        while let Some(command) = commands.pop_front() {
            match command {
                Command::Lockout(mut evidence) => {
                    evidence.original_error_text =
                        crate::journal::redact_error(&evidence.original_error_text);
                    let result =
                        crate::store::StateFile::read(&core.state_path).and_then(|mut state| {
                            state.lockout = Some(evidence);
                            state.save(&core.state_path)
                        });
                    if let Err(error) = result {
                        eprintln!(
                            "acp2api: cannot save lockout: {}",
                            crate::journal::redact_error(&error)
                        );
                    }
                    core.stop_runtime(runtime, read_open).await;
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
                Command::State(state, cause) => journal.state(state, cause, clock.as_ref()),
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
                        Err(error) => Err(crate::runtime_driver::CommandError::Session(error)),
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
                        Err(error) => Err(crate::runtime_driver::CommandError::Session(error)),
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
                        Err(error) => Err(crate::runtime_driver::CommandError::Session(error)),
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
    pub(crate) fn builds_summary(&self) -> Result<crate::runtime_builds::Summary, String> {
        self.builds.summary()
    }
    pub(crate) fn notices(&self) -> Arc<Vec<Notice>> {
        self.notices.borrow().clone()
    }
    pub(crate) fn quota_result(&self) -> Arc<Option<QuotaResult>> {
        self.quota_result.borrow().clone()
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
    ) -> Result<String, LaunchFailure> {
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
    pub(crate) async fn owner(&self, action: OwnerAction, hold: Hold) -> Result<(), String> {
        let (reply, result) = oneshot::channel();
        self.send
            .send(Input::Owner {
                action,
                reply,
                hold,
            })
            .await
            .map_err(|_| "conductor stopped")?;
        result.await.map_err(|_| "conductor stopped")?
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
fn readable(runtime: &mut Option<Runtime>, read_open: bool) -> Result<&mut Runtime, String> {
    runtime
        .as_mut()
        .filter(|_| read_open)
        .ok_or_else(|| UNAVAILABLE.into())
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
mod tests {
    use super::*;
    impl Core {
        fn fixture(limit: u8) -> Self {
            Self {
                quota_result: watch::channel(Arc::new(None)).0,
                last_refresh: None,
                quota_wait: None,
                retry_waits: Vec::new(),
                next_quota: quota::TimerId::default(),
                runtime_state: RuntimeState::Serving,
                lockout: None,
                sign_in_required: false,
                sign_in_wait: None,
                panel_address: "http://127.0.0.1:8329/".into(),
                state_path: PathBuf::new(),
                view: watch::channel(Arc::new(View::default())).0,
                failures: VecDeque::new(),
                cleanup_before_start: false,
                cancels: BTreeMap::new(),
                changes: VecDeque::new(),
                replacement: None,
                catalog: None,
                work: BTreeMap::new(),
                next_work: 0,
                records: Vec::new(),
                startup_session: None,
                use_sequence: 0,
                limit,
                notices: watch::channel(Arc::new(Vec::new())).0,
            }
        }
    }

    fn quota_error_without_reset_time() -> crate::messages::HttpError {
        crate::messages::HttpError {
            code: Some(429),
            status: "RESOURCE_EXHAUSTED".into(),
            message: "Quota exhausted; no reset time.".into(),
        }
    }

    fn prepare_quota_queries(core: &mut Core, commands: &[Command], now: Moment) -> Vec<Command> {
        let mut prepared = Vec::new();
        for command in commands {
            if let Command::Prepare { work, .. } = command {
                let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
                let catalog =
                    crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
                prepared.extend(core.handle(
                    Event::Runtime(RuntimeEvent::Prepared {
                        work: work.clone(),
                        result: Ok(("quota-query".into(), catalog)),
                    }),
                    now,
                    std::time::UNIX_EPOCH,
                ));
            }
        }
        prepared
    }

    fn error_after_reply(
        reply: &str,
        error: crate::messages::HttpError,
    ) -> (Core, Vec<Command>, PathBuf) {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-lockout-error-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal =
            crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.state_path = folder.join(crate::store::STATE_FILE);
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Text(reply.into()),
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error,
            }),
            1,
            std::time::UNIX_EPOCH,
        );
        (core, commands, folder)
    }

    #[test]
    fn a_503_with_order_14030_in_the_reply_does_not_start_lockout() {
        let (core, _, folder) = error_after_reply(
            "order 14030",
            crate::messages::HttpError {
                code: Some(503),
                status: "UNAVAILABLE".into(),
                message: "Service unavailable".into(),
            },
        );
        let state = core.state();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            state, "Running",
            "FR-034: a number in reply text is not an HTTP status"
        );
    }

    #[test]
    fn a_429_with_7f403a_in_the_error_starts_quota_wait() {
        let (core, _, folder) = error_after_reply(
            "",
            crate::messages::HttpError {
                code: Some(429),
                status: "RESOURCE_EXHAUSTED".into(),
                message: "request 7f403a; resets 1970-01-01T00:02:00Z".into(),
            },
        );
        let state = core.state();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            state, "Quota wait",
            "FR-034: an identifier in the error is not an HTTP status"
        );
    }

    #[tokio::test]
    async fn a_failed_reply_classifies_terms_without_copying_private_text() {
        let (mut core, commands, folder) = error_after_reply(
            "Terms of Service: PRIVATE-REPLY my small secret",
            crate::messages::HttpError {
                code: Some(400),
                status: "INVALID_ARGUMENT".into(),
                message: "Account name%40example.com failed".into(),
            },
        );
        let first = commands
            .iter()
            .find_map(|command| match command {
                Command::Emit(_, TurnEvent::End(Err(error)))
                    if error.to_string().contains(" Original error: ") =>
                {
                    Some(error.to_string())
                }
                _ => None,
            })
            .expect("the signalling response has the Google error");
        let journal =
            crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        Conductor::commands(
            &mut core,
            &mut None,
            &mut false,
            &clock,
            &journal,
            &mut commands.into(),
        )
        .await;
        let state_text = std::fs::read_to_string(&core.state_path).unwrap();
        let state: serde_json::Value = serde_json::from_str(&state_text).unwrap();
        let original = state["lockout"]["original_error_text"]
            .as_str()
            .unwrap()
            .to_owned();
        let response_original = first.split_once(" Original error: ").unwrap().1.to_owned();
        std::fs::remove_dir_all(folder).unwrap();
        let expected =
            "acp2api: upstream error 400 INVALID_ARGUMENT: Account <REDACTED> failed".to_owned();
        assert_eq!(
            (original, response_original),
            (expected.clone(), expected),
            "FR-035/FR-056: evidence and the first response contain only the redacted error channel"
        );
        assert!(!state_text.contains("PRIVATE-REPLY"));
    }

    #[test]
    fn a_known_reset_replaces_a_quota_wait_without_another_reset_query() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-replaced-quota-wait-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: quota_error_without_reset_time(),
            }),
            60_000,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_000),
        );
        assert_eq!(core.quota_wait.as_ref().unwrap().deadline, 3_660_000);
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: "resets 1970-01-01T00:02:00Z".into(),
                },
            }),
            60_001,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_001),
        );
        assert_eq!(core.quota_wait.as_ref().unwrap().deadline, 120_000);
        assert_eq!(
            ended
                .iter()
                .filter(|command| matches!(
                    command,
                    Command::Emit(
                        _,
                        TurnEvent::End(Err(crate::messages::Message::QuotaWait { .. }))
                    )
                ))
                .count(),
            1
        );
        assert_eq!(
            core.work
                .values()
                .filter(|work| matches!(work, Work::QuotaQueued { .. } | Work::QuotaWorking { .. }))
                .count(),
            0,
            "FR-036/FR-052 send no reset query for a replaced quota wait"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_request_settling_after_its_quota_wait_was_replaced_ends_with_the_current_quota_wait() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-settled-replaced-quota-wait-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for (error, until, retry_after, deadline, queries) in [
            (
                quota_error_without_reset_time(),
                "1970-01-01T01:01:00Z",
                3600,
                3_660_001,
                1,
            ),
            (
                crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: "resets 1970-01-01T00:02:00Z".into(),
                },
                "1970-01-01T00:02:00Z",
                60,
                120_000,
                0,
            ),
        ] {
            let (mut core, second, _tickets) = prompting_with_waiter(&journal);
            core.handle(Event::Limit(2), 0, std::time::UNIX_EPOCH);
            let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
            let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
            core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work: second,
                    result: Ok(("other".into(), catalog)),
                }),
                0,
                std::time::UNIX_EPOCH,
            );
            core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "running".into(),
                    update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                        quota_error_without_reset_time(),
                    )),
                }),
                60_000,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_000),
            );
            core.handle(
                Event::Runtime(RuntimeEvent::UpstreamError {
                    session: "other".into(),
                    error,
                }),
                60_001,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_001),
            );
            let ended = core.handle(
                Event::Deadline,
                60_200,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_200),
            );
            assert_eq!(
                ended
                    .iter()
                    .filter(|command| matches!(
                        command,
                        Command::Usage { .. } | Command::Prepare { .. }
                    ))
                    .count(),
                0,
                "FR-036/FR-052 send no reset query for the replaced wait"
            );
            assert_eq!(
                ended.iter().filter(|command| matches!(command,
                    Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::QuotaWait { until: reset, retry_after: seconds })))
                        if reset == until && *seconds == retry_after
                )).count(),
                1,
                "the running request ends once with the current quota wait, without waiting for /usage"
            );
            let refusal = core.model_entries(60_200).err().unwrap();
            assert_eq!(crate::dialect::CHAT.refusal(&refusal).status(), 429);
            assert_eq!(core.quota_wait.as_ref().unwrap().deadline, deadline);
            core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "other".into(),
                    update: TurnUpdate::PromptEnded {
                        stop_reason: Some(StopReason::EndTurn),
                    },
                }),
                60_201,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_201),
            );
            let current = core.handle(
                Event::Deadline,
                60_401,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_401),
            );
            assert_eq!(
                current
                    .iter()
                    .filter(|command| matches!(command,
                        Command::Usage { session, .. } if session == "other"
                    ))
                    .count(),
                queries,
                "only the current quota wait with no reset time needs a query"
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_request_queries_its_reset_time_after_the_replacing_quota_wait_expires() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-expired-replacing-quota-wait-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        for (session, error, now) in [
            ("running", quota_error_without_reset_time(), 60_000),
            (
                "other",
                crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: "resets 1970-01-01T00:01:01Z".into(),
                },
                60_001,
            ),
        ] {
            core.handle(
                Event::Runtime(RuntimeEvent::UpstreamError {
                    session: session.into(),
                    error,
                }),
                now,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(now),
            );
        }
        core.handle(
            Event::Deadline,
            61_000,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_000),
        );
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::EndTurn),
                },
            }),
            61_001,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_001),
        );
        let querying = core.handle(
            Event::Deadline,
            61_201,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_201),
        );
        assert_eq!(
            querying
                .iter()
                .filter(|command| matches!(command,
                    Command::Usage { session, .. } if session == "running"
                ))
                .count(),
            1,
            "FR-036 still queries the missing reset time when no quota wait remains"
        );
        assert!(
            !querying
                .iter()
                .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(_)))),
            "the response waits for its reset query"
        );
        for update in [
            TurnUpdate::Text("### Quota Limits\n- **Model M**\n  *(Models within this group: M)*\n  - **Five Hour Limit Remaining:** 0% remaining (resets 1970-01-01T00:02:00Z)".into()),
            TurnUpdate::PromptEnded { stop_reason: Some(StopReason::EndTurn) },
        ] {
            core.handle(
                Event::Runtime(RuntimeEvent::Turn { session: "running".into(), update }),
                61_202,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_202),
            );
        }
        let ended = core.handle(
            Event::Deadline,
            61_402,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_402),
        );
        assert_eq!(ended.iter().filter(|command| matches!(command,
            Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::QuotaWait { until, retry_after: 59 })))
                if until == "1970-01-01T00:02:00Z"
        )).count(), 1, "the running request ends with the queried reset time");
        assert_eq!(core.view.borrow().state, "Running");
        assert!(
            core.model_entries(61_402).is_ok(),
            "the query does not restart an expired quota wait"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_queued_reset_query_runs_only_without_another_current_quota_wait() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-queued-replaced-quota-wait-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for (release, queries, responses, state) in
            [(60_300, 0, 1, "Quota wait"), (120_001, 1, 0, "Running")]
        {
            let (mut core, second, _tickets) = prompting_with_waiter(&journal);
            core.handle(Event::Limit(2), 0, std::time::UNIX_EPOCH);
            let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
            let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
            core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work: second,
                    result: Ok(("other".into(), catalog)),
                }),
                0,
                std::time::UNIX_EPOCH,
            );
            core.handle(Event::Limit(1), 1, std::time::UNIX_EPOCH);
            core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "running".into(),
                    update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                        quota_error_without_reset_time(),
                    )),
                }),
                60_000,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_000),
            );
            let queued = core.handle(
                Event::Deadline,
                60_200,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_200),
            );
            assert!(
                !queued.iter().any(|command| matches!(
                    command,
                    Command::Usage { .. } | Command::Emit(_, TurnEvent::End(_))
                )),
                "the reset query and its response wait for a place"
            );
            core.handle(
                Event::Runtime(RuntimeEvent::UpstreamError {
                    session: "other".into(),
                    error: crate::messages::HttpError {
                        code: Some(429),
                        status: "RESOURCE_EXHAUSTED".into(),
                        message: "resets 1970-01-01T00:02:00Z".into(),
                    },
                }),
                60_201,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_201),
            );
            let dispatched = core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "other".into(),
                    update: TurnUpdate::PromptEnded {
                        stop_reason: Some(StopReason::EndTurn),
                    },
                }),
                release,
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(release),
            );
            assert_eq!(
                dispatched
                    .iter()
                    .filter(|command| matches!(command,
                        Command::Usage { session, .. } if session == "running"
                    ))
                    .count(),
                queries,
                "FR-036/FR-052 skip the queued reset query only while another quota wait is current"
            );
            assert_eq!(dispatched.iter().filter(|command| matches!(command,
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::QuotaWait { until, retry_after: 60 })))
                    if until == "1970-01-01T00:02:00Z"
            )).count(), responses, "a skipped query ends its response with the current quota wait");
            assert_eq!(core.view.borrow().state, state);
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_client_disconnect_cancels_a_prompt_awaiting_quota_end() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-disconnect-quota-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let request = core.work[&0].key().to_owned();
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: quota_error_without_reset_time(),
            }),
            60_000,
            std::time::UNIX_EPOCH,
        );
        let disconnected = core.handle(Event::Disconnected(request), 60_001, std::time::UNIX_EPOCH);
        assert_eq!(
            disconnected
                .iter()
                .filter(|command| matches!(command,
                    Command::Cancel(session) if session == "running"
                ))
                .count(),
            1,
            "FR-031 cancels the prompt when the API client disconnects during the quota wait"
        );
        assert!(
            !disconnected.iter().any(|command| matches!(
                command,
                Command::Prepare { .. }
                    | Command::Usage { .. }
                    | Command::Forget(_)
                    | Command::Emit(_, _)
            )),
            "the disconnected prompt keeps its place until it ends and sends no response"
        );
        assert_eq!(core.work.values().filter(|work| matches!(work,
            Work::QuotaQueued { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, .. }
                if model == "m" && wait.deadline == 3_660_000
        )).count(), 1, "FR-036/FR-052 preserve exactly one reset query after the disconnect");
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::Cancelled),
                },
            }),
            60_002,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            ended
                .iter()
                .filter(|command| matches!(command,
                    Command::Forget(session) if session == "running"
                ))
                .count(),
            1,
            "FR-031 clears the ACP session after its prompt ends"
        );
        assert_eq!(
            ended
                .iter()
                .filter(|command| matches!(
                    command,
                    Command::Complete(crate::journal::Completion {
                        outcome: crate::journal::Outcome::Disconnected,
                        ..
                    })
                ))
                .count(),
            1
        );
        assert!(
            !ended
                .iter()
                .any(|command| matches!(command, Command::Emit(_, _)))
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_client_disconnect_after_the_quota_error_ends_the_turn_keeps_the_reset_query() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-disconnect-ended-quota-turn-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let request = core.work[&0].key().to_owned();
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                    quota_error_without_reset_time(),
                )),
            }),
            60_000,
            std::time::UNIX_EPOCH,
        );
        let disconnected = core.handle(Event::Disconnected(request), 60_001, std::time::UNIX_EPOCH);
        assert_eq!(
            disconnected
                .iter()
                .filter_map(|command| match command {
                    Command::Cancel(_) => Some("cancel"),
                    Command::Forget(session) if session == "running" => Some("forget"),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            ["forget"],
            "sends no cancel because the prompt turn has ended, then forgets the ACP session"
        );
        assert_eq!(core.work.values().filter(|work| matches!(work,
            Work::QuotaWorking { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, phase: QuotaPhase::Preparing, .. }
                if model == "m" && wait.deadline == 3_660_000
        )).count(), 1, "FR-036/FR-052 preserve exactly one reset query after the disconnect");
        assert_eq!(
            disconnected
                .iter()
                .filter(|command| matches!(
                    command,
                    Command::Complete(crate::journal::Completion {
                        outcome: crate::journal::Outcome::Disconnected,
                        ..
                    })
                ))
                .count(),
            1
        );
        assert!(
            !disconnected
                .iter()
                .any(|command| matches!(command, Command::Emit(_, _)))
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_non_quota_error_after_the_quota_error_ends_the_turn_keeps_the_reset_query() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-quota-settling-error-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                    quota_error_without_reset_time(),
                )),
            }),
            60_000,
            std::time::UNIX_EPOCH,
        );
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(503),
                    status: "UNAVAILABLE".into(),
                    message: "Temporary failure".into(),
                },
            }),
            60_001,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(ended.iter().filter(|command| matches!(command,
            Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::UpstreamHttp(error))))
                if error.code == Some(503) && error.message == "Temporary failure"
        )).count(), 1, "the non-quota error still ends the API request once");
        assert_eq!(core.work.values().filter(|work| matches!(work,
            Work::QuotaWorking { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, phase: QuotaPhase::Preparing, .. }
                if model == "m" && wait.deadline == 3_660_000
        )).count(), 1, "FR-036/FR-052 preserve exactly one reset query after the error");
        assert!(
            !ended
                .iter()
                .any(|command| matches!(command, Command::Cancel(_))),
            "sends no cancel because the prompt turn has ended"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_runtime_failure_while_awaiting_quota_end_keeps_the_reset_query() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-quota-runtime-failure-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: quota_error_without_reset_time(),
            }),
            60_000,
            std::time::UNIX_EPOCH,
        );
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Failed("runtime failed".into())),
            60_001,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            ended
                .iter()
                .filter(|command| matches!(command,
                    Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::Upstream(error))))
                        if error == "runtime failed"
                ))
                .count(),
            1,
            "the runtime failure still ends the API request once"
        );
        assert_eq!(core.work.values().filter(|work| matches!(work,
            Work::QuotaWorking { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, phase: QuotaPhase::Preparing, .. }
                if model == "m" && wait.deadline == 3_660_000
        )).count(), 1, "FR-036/FR-052 preserve exactly one reset query after the runtime failure");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_non_quota_error_cancels_a_prompt_awaiting_quota_end() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-quota-error-cancel-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: quota_error_without_reset_time(),
            }),
            60_000,
            std::time::UNIX_EPOCH,
        );
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(503),
                    status: "UNAVAILABLE".into(),
                    message: "Temporary failure".into(),
                },
            }),
            60_001,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            commands
                .iter()
                .filter(
                    |command| matches!(command, Command::Cancel(session) if session == "running")
                )
                .count(),
            1,
            "a non-quota error cancels the prompt while it awaits the quota end"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_no_update_cancel_keeps_the_pending_quota_reset_query() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-idle-quota-reset-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: quota_error_without_reset_time(),
            }),
            60_000,
            std::time::UNIX_EPOCH,
        );
        let timer = core.quota_wait.as_ref().unwrap().timer;
        let cancelled = core.handle(Event::Deadline, 300_000, std::time::UNIX_EPOCH);
        assert!(
            cancelled
                .iter()
                .any(|command| matches!(command, Command::Cancel(session) if session == "running"))
        );
        assert_eq!(core.work.values().filter(|work| matches!(work,
            Work::QuotaQueued { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, session: None }
            if model == "m" && wait.deadline == 3_660_000
        )).count(), 1, "FR-036/FR-052 keep exactly one reset query after the FR-051 cancel");
        assert!(
            !cancelled
                .iter()
                .any(|command| matches!(command, Command::Prepare { .. } | Command::Usage { .. })),
            "the cancelled prompt still occupies its place"
        );
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::Cancelled),
                },
            }),
            300_001,
            std::time::UNIX_EPOCH,
        );
        let prepared = prepare_quota_queries(&mut core, &ended, 300_001);
        assert_eq!(prepared.iter().filter(|command| matches!(command, Command::Usage { session, .. } if session == "quota-query")).count(), 1, "the preserved reset query reaches /usage");
        assert_eq!(
            cancelled
                .iter()
                .filter(|command| matches!(
                    command,
                    Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::NoUpdate)))
                ))
                .count(),
            1,
            "the API request ends once with M-19"
        );
        assert!(
            !ended
                .iter()
                .chain(&prepared)
                .any(|command| matches!(command, Command::Emit(_, _) | Command::Complete(_))),
            "the reset query holds no second API response"
        );
        let mut query_end = Vec::new();
        for update in [
            TurnUpdate::Text("### Quota Limits\n- **Model M**\n  *(Models within this group: M)*\n  - **Five Hour Limit Remaining:** 0% remaining (resets 1970-01-01T00:10:00Z)".into()),
            TurnUpdate::PromptEnded { stop_reason: Some(StopReason::EndTurn) },
        ] {
            query_end.extend(core.handle(Event::Runtime(RuntimeEvent::Turn {
                session: "quota-query".into(), update,
            }), 300_002, std::time::UNIX_EPOCH + std::time::Duration::from_millis(300_002)));
        }
        query_end.extend(core.handle(
            Event::Deadline,
            300_202,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(300_202),
        ));
        let wait = core.quota_wait.as_ref().unwrap();
        assert!(
            wait.timer == timer,
            "the reset result belongs to the original quota wait"
        );
        assert_eq!(
            wait.deadline, 600_000,
            "the reset query replaces the 60-minute fallback"
        );
        assert!(
            !query_end.iter().any(|command| matches!(
                command,
                Command::Emit(_, _) | Command::Complete(_) | Command::Usage { .. }
            )),
            "the reset result neither repeats /usage nor completes the API request again"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_prompt_after_a_quota_signal_keeps_its_no_update_deadline() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-quota-idle-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for with_update in [false, true] {
            let (mut core, _, _tickets) = prompting_with_waiter(&journal);
            core.handle(
                Event::Runtime(RuntimeEvent::UpstreamError {
                    session: "running".into(),
                    error: crate::messages::HttpError {
                        code: Some(429),
                        status: "RESOURCE_EXHAUSTED".into(),
                        message: "Quota exhausted".into(),
                    },
                }),
                60_000,
                std::time::UNIX_EPOCH,
            );
            if with_update {
                core.handle(
                    Event::Runtime(RuntimeEvent::Turn {
                        session: "running".into(),
                        update: TurnUpdate::Usage(1000),
                    }),
                    240_000,
                    std::time::UNIX_EPOCH,
                );
            }
            let deadline = if with_update { 540_000 } else { 300_000 };
            let early = core.handle(Event::Deadline, deadline - 1, std::time::UNIX_EPOCH);
            assert!(
                !early
                    .iter()
                    .any(|command| matches!(command, Command::Cancel(_)))
            );
            assert_eq!(
                core.next_deadline(),
                Some(deadline),
                "the running prompt still schedules its FR-051 deadline"
            );
            let due = core.handle(Event::Deadline, deadline, std::time::UNIX_EPOCH);
            assert!(
                due.iter().any(
                    |command| matches!(command, Command::Cancel(session) if session == "running")
                ),
                "FR-051 cancels a silent prompt after a quota signal"
            );
            assert!(
                due.iter().any(|command| matches!(
                    command,
                    Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::NoUpdate)))
                )),
                "FR-051 ends with M-19 rather than a quota response"
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn an_idle_prompt_failure_starts_the_existing_retry_wait() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-idle-retry-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"m","messages":[{"role":"user","content":"First"}]}"#)
            .ok()
            .unwrap()
            .turn;
        core.handle(Event::Deadline, 300_000, std::time::UNIX_EPOCH);
        let actual = core.check(&turn, 305_000).err().map(|error| {
            let seconds = match error.row() {
                crate::messages::Row::Temporary503(seconds) => Some(seconds),
                _ => None,
            };
            (seconds, error.message.to_string())
        });
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(actual, Some((Some(25), "acp2api: a request of this conversation failed less than 30 s ago. Send the request again in 25 s. The error was: the official runtime sent no update for 5 minutes, so acp2api cancelled the prompt turn.".into())), "FR-029: M-19 uses the existing 503-row retry wait");
    }

    #[test]
    fn runtime_unavailable_ends_each_kind_of_queued_quota_query() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-queued-quota-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let mut observed = Vec::new();
        for kind in ["reset", "refusal", "refresh"] {
            let (mut core, _, _tickets) = prompting_with_waiter(&journal);
            let active = core.work.remove(&0).unwrap().into_active().unwrap();
            let (reply, mut result) = oneshot::channel();
            let purpose = match kind {
                "reset" => QuotaPurpose::Reset {
                    wait: quota::Wait::new(
                        std::time::UNIX_EPOCH + std::time::Duration::from_secs(3600),
                        0,
                        std::time::UNIX_EPOCH,
                        quota::TimerId(1),
                    ),
                    model: Some(active.model.clone()),
                    response: Some(active),
                },
                "refusal" => QuotaPurpose::Refusal {
                    model: active.model.clone(),
                    response: Some(active),
                },
                _ => QuotaPurpose::Refresh(reply),
            };
            core.enqueue_quota(purpose, None);
            let commands = core.handle(
                Event::RuntimeUnavailable(UnavailableReason {
                    message: "runtime missing".into(),
                    cause: None,
                }),
                0,
                std::time::UNIX_EPOCH,
            );
            let answers = commands.iter().filter(|command| matches!(command, Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::RuntimeUnavailable(text)))) if text == "runtime missing")).count();
            observed.push((
                answers,
                core.work
                    .values()
                    .any(|work| matches!(work, Work::QuotaQueued { .. })),
                kind != "refresh" || matches!(result.try_recv(), Ok(Err(_))),
            ));
        }
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            observed,
            [(2, false, true), (2, false, true), (1, false, true)],
            "queued Reset/Refusal replies get M-14 and the queued owner action gets an error"
        );
    }
    #[test]
    fn model_lists_use_the_quota_row_until_the_deadline_without_waiting_for_its_timer() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-quota-models-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let status = |result: Result<Vec<crate::catalog::ModelEntry>, crate::messages::Refusal>| {
            match result {
                Ok(_) => 200,
                Err(error) => crate::dialect::CHAT.refusal(&error).status().as_u16(),
            }
        };
        let normal = status(core.model_entries(0));
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: "resets 1970-01-01T00:02:00Z".into(),
                },
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        let results = [
            normal,
            status(core.model_entries(1)),
            status(core.model_entries(120_000)),
        ];
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            results,
            [200, 429, 200],
            "FR-012/FR-036 GET /v1/models uses the state's row, including the exact quota deadline"
        );
    }
    #[test]
    fn a_quota_error_after_disconnect_still_queries_the_reset_time() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-retiring-quota-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let request = core.work[&0].key().to_owned();
        core.handle(Event::Disconnected(request), 0, std::time::UNIX_EPOCH);
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: quota_error_without_reset_time(),
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::Cancelled),
                },
            }),
            1,
            std::time::UNIX_EPOCH,
        );
        let prepared = prepare_quota_queries(&mut core, &commands, 1);
        let queries = prepared
            .iter()
            .filter(|command| matches!(command, Command::Usage { .. }))
            .count();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            queries, 1,
            "FR-036/FR-052 a lost API reply does not discard a global quota signal or its reset query"
        );
    }
    #[test]
    fn quota_expiry_on_an_owner_event_keeps_runtime_unavailable_and_reports_state_changes() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-quota-state-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let mut commands = core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: "resets 1970-01-01T00:02:00Z".into(),
                },
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        commands.extend(core.handle(
            Event::RuntimeUnavailable(UnavailableReason {
                message: "unavailable".into(),
                cause: None,
            }),
            1,
            std::time::UNIX_EPOCH,
        ));
        commands.extend(core.handle(Event::Limit(2), 120_000, std::time::UNIX_EPOCH));
        assert!(matches!(
            core.refusal(120_000),
            Some(crate::messages::Message::RuntimeUnavailable(_))
        ));
        commands.extend(core.handle(Event::ResumeRuntime, 120_000, std::time::UNIX_EPOCH));
        let states: Vec<_> = commands
            .into_iter()
            .filter_map(|command| match command {
                Command::State(state, _) => Some(state),
                _ => None,
            })
            .collect();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            states,
            ["Quota wait", "Runtime unavailable", "Running"],
            "FR-002/FR-056 each condition keeps its own end rule, including expiry during an owner action"
        );
    }
    #[test]
    fn acp_quota_error_refuses_the_current_and_queued_requests() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-acp-quota-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let mut commands = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Failed(crate::messages::Message::UpstreamAcp(
                    crate::messages::AcpError {
                        data: None,
                        code: 429,
                        message: "RESOURCE_EXHAUSTED; resets 1970-01-01T00:02:00Z".into(),
                    },
                )),
            }),
            1,
            std::time::UNIX_EPOCH,
        );
        commands.extend(core.handle(Event::Deadline, 201, std::time::UNIX_EPOCH));
        let statuses: Vec<_> = commands
            .into_iter()
            .filter_map(|command| match command {
                Command::Emit(_, TurnEvent::End(Err(message))) => Some(
                    crate::dialect::MESSAGES
                        .refusal(&message.into())
                        .status()
                        .as_u16(),
                ),
                _ => None,
            })
            .collect();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            statuses,
            [429, 429],
            "FR-033/FR-036 an ACP quota error has the same state row as a stderr quota error"
        );
    }
    #[test]
    fn preparation_rechecks_quota_after_runtime_recovery() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-quota-preparation-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let mut results = Vec::new();
        for quota in [false, true] {
            let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
            core.handle(Event::Limit(2), 0, std::time::UNIX_EPOCH);
            if quota {
                core.handle(
                    Event::Runtime(RuntimeEvent::UpstreamError {
                        session: "running".into(),
                        error: crate::messages::HttpError {
                            code: Some(429),
                            status: "RESOURCE_EXHAUSTED".into(),
                            message: "resets 1970-01-01T00:02:00Z".into(),
                        },
                    }),
                    1,
                    std::time::UNIX_EPOCH,
                );
                core.handle(
                    Event::RuntimeUnavailable(UnavailableReason {
                        message: "runtime unavailable".into(),
                        cause: None,
                    }),
                    1,
                    std::time::UNIX_EPOCH,
                );
                core.handle(Event::ResumeRuntime, 1, std::time::UNIX_EPOCH);
            }
            let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
            let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
            let commands = core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work: queued,
                    result: Ok(("prepared".into(), catalog)),
                }),
                2,
                std::time::UNIX_EPOCH,
            );
            results.push(
                if commands
                    .iter()
                    .any(|command| matches!(command, Command::Prompt { .. }))
                {
                    "prompt"
                } else if commands.iter().any(|command| {
                    matches!(
                        command,
                        Command::Emit(
                            _,
                            TurnEvent::End(Err(crate::messages::Message::QuotaWait { .. }))
                        )
                    )
                }) {
                    "quota"
                } else {
                    "no result"
                },
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            results,
            ["prompt", "quota"],
            "FR-036 runtime recovery clears only runtime unavailable, and preparation must check the remaining condition before prompt dispatch"
        );
    }
    fn answer_quota(core: &mut Core, mut commands: Vec<Command>, now: Moment) -> Vec<Command> {
        let sessions: Vec<_> = commands
            .iter()
            .filter_map(|command| match command {
                Command::Usage { session, .. } => Some(session.clone()),
                _ => None,
            })
            .collect();
        for session in sessions {
            commands.extend(core.handle(Event::Runtime(RuntimeEvent::Turn { session:session.clone(), update:TurnUpdate::Text("### Quota Limits\n- **Gemini Models**\n  *(Models within this group: Gemini Flash, Gemini Pro)*\n  - **Weekly Limit Remaining:** 75% remaining (resets Oct 24, 2026, 12:34 UTC)".into()) }), now + 1, std::time::UNIX_EPOCH));
            commands.extend(core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session,
                    update: TurnUpdate::PromptEnded {
                        stop_reason: Some(StopReason::EndTurn),
                    },
                }),
                now + 1,
                std::time::UNIX_EPOCH,
            ));
            commands.extend(core.handle(Event::Deadline, now + 201, std::time::UNIX_EPOCH));
        }
        commands
    }
    struct Facts(Vec<crate::ListedBuild>);
    impl crate::ReleaseFacts for Facts {
        fn version(&self) -> &str {
            "test"
        }
        fn platform(&self) -> &str {
            if cfg!(windows) {
                "windows-x64"
            } else {
                "macos-arm64"
            }
        }
        fn listed_builds(&self) -> &[crate::ListedBuild] {
            &self.0
        }
        fn highest_concurrency_limit(&self) -> u8 {
            1
        }
    }
    fn prompting_with_waiter(
        journal: &crate::journal::Journal,
    ) -> (
        Core,
        String,
        [mpsc::UnboundedReceiver<(TurnEvent, Hold)>; 2],
    ) {
        let mut core = Core {
            startup_session: Some("running".into()),
            ..Core::fixture(1)
        };
        let mut tickets = Vec::new();
        let mut queued = String::new();
        for text in ["First", "Queued"] {
            let body = serde_json::json!({"model":"m","messages":[{"role":"user","content":text}]});
            let turn = crate::dialect::CHAT
                .parse(&serde_json::to_vec(&body).unwrap())
                .ok()
                .unwrap()
                .turn;
            let request = journal.begin(crate::journal::Api::Chat, 0);
            queued = request.id.clone();
            let (output, ticket) = mpsc::unbounded_channel();
            tickets.push(ticket);
            core.handle(
                Event::Accepted(Box::new(Accepted {
                    turn,
                    model: crate::catalog::SelectedModel {
                        model: "m".into(),
                        level: None,
                        level_from_name: false,
                    },
                    request,
                    reply: ResponseChannel {
                        output,
                        keep_alive: None,
                    },
                })),
                0,
                std::time::UNIX_EPOCH,
            );
        }
        (core, queued, tickets.try_into().ok().unwrap())
    }

    #[tokio::test]
    async fn no_update_releases_the_last_chunk_before_the_error() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-no-update-chunk-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for stream in [false, true] {
            let (mut core, _, [events, _queued]) = prompting_with_waiter(&journal);
            let form = crate::dialect::CHAT
                .parse(
                    &serde_json::to_vec(&serde_json::json!({
                        "model":"m", "messages":[{"role":"user","content":"First"}], "stream":stream
                    }))
                    .unwrap(),
                )
                .ok()
                .unwrap()
                .form;
            let mut commands = core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "running".into(),
                    update: TurnUpdate::Text("Last reply chunk".into()),
                }),
                0,
                std::time::UNIX_EPOCH,
            );
            commands.extend(core.handle(Event::Deadline, 300_000, std::time::UNIX_EPOCH));
            let emitted: Vec<_> = commands
                .iter()
                .filter_map(|command| match command {
                    Command::Emit(_, TurnEvent::Text(text)) => Some(text.as_str()),
                    Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::NoUpdate))) => {
                        Some("M-19")
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                emitted,
                ["Last reply chunk", "M-19"],
                "FR-051: stream={stream}"
            );
            let clock = crate::Ports::production().clock;
            for command in commands {
                if let Command::Emit(output, event) = command {
                    let _ = output.send((event, clock.hold(HoldKind::TurnEvent)));
                }
            }
            let response = crate::dialect::CHAT
                .respond(
                    form,
                    Ticket {
                        events,
                        hold: clock.hold(HoldKind::TurnEvent),
                    },
                    clock,
                )
                .await;
            assert_eq!(response.status().as_u16(), if stream { 200 } else { 503 });
            let body = axum::body::to_bytes(response.into_body(), 65_536)
                .await
                .unwrap();
            let body = String::from_utf8(body.to_vec()).unwrap();
            if stream {
                assert!(body.find("Last reply chunk").unwrap() < body.find("no_update").unwrap());
            } else {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"]["code"],
                    "no_update"
                );
            }
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[tokio::test]
    async fn max_turn_requests_releases_the_last_chunk_before_the_error() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-max-turn-chunk-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for stream in [false, true] {
            let (mut core, _, [events, _queued]) = prompting_with_waiter(&journal);
            let form = crate::dialect::CHAT
                .parse(
                    &serde_json::to_vec(&serde_json::json!({
                        "model":"m", "messages":[{"role":"user","content":"First"}], "stream":stream
                    }))
                    .unwrap(),
                )
                .ok()
                .unwrap()
                .form;
            let mut commands = Vec::new();
            for update in [
                TurnUpdate::Text("Last reply chunk".into()),
                TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::MaxTurnRequests),
                },
            ] {
                commands.extend(core.handle(
                    Event::Runtime(RuntimeEvent::Turn {
                        session: "running".into(),
                        update,
                    }),
                    0,
                    std::time::UNIX_EPOCH,
                ));
            }
            commands.extend(core.handle(Event::Deadline, 200, std::time::UNIX_EPOCH));
            let emitted: Vec<_> = commands
                .iter()
                .filter_map(|command| match command {
                    Command::Emit(_, TurnEvent::Text(text)) => Some(text.as_str()),
                    Command::Emit(
                        _,
                        TurnEvent::End(Err(crate::messages::Message::MaxTurnRequests)),
                    ) => Some("M-21"),
                    _ => None,
                })
                .collect();
            assert_eq!(
                emitted,
                ["Last reply chunk", "M-21"],
                "FR-028: stream={stream}"
            );
            let clock = crate::Ports::production().clock;
            for command in commands {
                if let Command::Emit(output, event) = command {
                    let _ = output.send((event, clock.hold(HoldKind::TurnEvent)));
                }
            }
            let response = crate::dialect::CHAT
                .respond(
                    form,
                    Ticket {
                        events,
                        hold: clock.hold(HoldKind::TurnEvent),
                    },
                    clock,
                )
                .await;
            assert_eq!(response.status().as_u16(), if stream { 200 } else { 502 });
            let body = axum::body::to_bytes(response.into_body(), 65_536)
                .await
                .unwrap();
            let body = String::from_utf8(body.to_vec()).unwrap();
            if stream {
                assert!(
                    body.find("Last reply chunk").unwrap()
                        < body.find("max_turn_requests").unwrap()
                );
            } else {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"]["code"],
                    "max_turn_requests"
                );
            }
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn runtime_failure_keeps_retry_model_resolution_after_clearing_the_catalog() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-retry-catalog-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for catalog_at_admission in [false, true] {
            let (mut core, _, _tickets) = prompting_with_waiter(&journal);
            core.work.clear();
            core.startup_session = None;
            let metadata = serde_json::json!({
                "configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}],
                "models":{"availableModels":[{"modelId":"m-low"},{"modelId":"m-high"}]}
            });
            core.catalog = catalog_at_admission.then(|| {
                crate::catalog::Catalog::decode(&metadata, std::time::UNIX_EPOCH).unwrap()
            });
            let turn = |model: &str, effort: &str| {
                crate::dialect::CHAT
                    .parse(
                        &serde_json::to_vec(
                            &serde_json::json!({"model":model,"reasoning_effort":effort,
                    "messages":[{"role":"user","content":"First"}]}),
                        )
                        .unwrap(),
                    )
                    .ok()
                    .unwrap()
                    .turn
            };
            let clock = crate::Ports::production().clock;
            let request = journal.begin(crate::journal::Api::Chat, 0);
            let work = request.id.clone();
            let (admission, mut admitted) = oneshot::channel();
            let (accepted, _) = core
                .admit(
                    TurnWork {
                        pending: PendingRequest {
                            mode: ResponseMode::Body,
                            turn: turn("m", "high"),
                            request,
                            admission,
                            connection: mpsc::unbounded_channel().0,
                        },
                        hold: clock.hold(HoldKind::TurnEvent),
                    },
                    &journal,
                    clock.as_ref(),
                )
                .unwrap();
            let _seed = admitted.try_recv().unwrap().ok().unwrap();
            core.handle(accepted, 0, std::time::UNIX_EPOCH);
            core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work,
                    result: Ok((
                        "failed".into(),
                        crate::catalog::Catalog::decode(&metadata, std::time::UNIX_EPOCH).unwrap(),
                    )),
                }),
                0,
                std::time::UNIX_EPOCH,
            );
            assert!(
                core.catalog.is_some(),
                "catalog loads before the M-20 failure"
            );
            core.handle(Event::RuntimeFailure, 0, std::time::UNIX_EPOCH);
            assert!(core.catalog.is_none(), "M-20 clears the live catalog");
            let actual: Vec<_> = [("m", "high"), ("m-high", "low"), ("m", "low")]
                .into_iter()
                .map(|(model, effort)| {
                    let (admission, mut answer) = oneshot::channel();
                    let _ = core.admit(
                        TurnWork {
                            pending: PendingRequest {
                                mode: ResponseMode::Body,
                                turn: turn(model, effort),
                                request: journal.begin(crate::journal::Api::Chat, 0),
                                admission,
                                connection: mpsc::unbounded_channel().0,
                            },
                            hold: clock.hold(HoldKind::TurnEvent),
                        },
                        &journal,
                        clock.as_ref(),
                    );
                    match answer.try_recv().unwrap() {
                        Err(error)
                            if matches!(
                                error.message,
                                crate::messages::Message::RetryWait { .. }
                            ) =>
                        {
                            "retry_wait"
                        }
                        Err(_) => "other error",
                        Ok(_) => "accepted",
                    }
                })
                .collect();
            assert_eq!(
                actual,
                ["retry_wait", "retry_wait", "accepted"],
                "FR-029 resolves the same model ID without the live catalog; catalog at admission: {catalog_at_admission}"
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn retry_wait_keeps_the_client_tool_set_when_tool_calls_are_disabled() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-retry-tools-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _) = prompting_with_waiter(&journal);
        core.work.clear();
        let mut tickets = Vec::new();
        let mut request = |tool: &str| {
            let body = serde_json::json!({
                "model":"m", "tool_choice":"none",
                "tools":[{"type":"function","function":{"name":tool,"parameters":{}}}],
                "messages":[{"role":"user","content":"Read a file"}]
            });
            let turn = crate::dialect::CHAT
                .parse(&serde_json::to_vec(&body).unwrap())
                .ok()
                .unwrap()
                .turn;
            let (output, ticket) = mpsc::unbounded_channel();
            tickets.push(ticket);
            Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: "m".into(),
                    level: None,
                    level_from_name: false,
                },
                request: journal.begin(crate::journal::Api::Chat, 0),
                reply: ResponseChannel {
                    output,
                    keep_alive: None,
                },
            }
        };
        let seed = request("read_file");
        let seed_id = seed.request.id.clone();
        core.handle(Event::Accepted(Box::new(seed)), 0, std::time::UNIX_EPOCH);
        core.handle(
            Event::StartFailed(seed_id, "acp2api: session creation failed".into()),
            0,
            std::time::UNIX_EPOCH,
        );
        let same = core.handle(
            Event::Accepted(Box::new(request("read_file"))),
            5000,
            std::time::UNIX_EPOCH,
        );
        let changed = core.handle(
            Event::Accepted(Box::new(request("list_files"))),
            5000,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            (
                same.iter()
                    .filter(|command| matches!(
                        command,
                        Command::Emit(
                            _,
                            TurnEvent::End(Err(crate::messages::Message::RetryWait { .. }))
                        )
                    ))
                    .count(),
                changed
                    .iter()
                    .filter(|command| matches!(command, Command::Prepare { .. }))
                    .count(),
            ),
            (1, 1),
            "FR-029 keeps the request's client tool set in the retry match"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    async fn failed_checks(timeout: bool, restart_fails: bool) {
        use crate::{BoxFuture, Launched, ProcessLauncher, ProcessTree, ReleaseFacts};
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        struct Tree;
        impl ProcessTree for Tree {
            fn pid(&self) -> u32 {
                std::process::id()
            }
            fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
                Box::pin(std::future::pending())
            }
            fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
                Box::pin(async {})
            }
        }
        struct CheckClock(Arc<AtomicBool>, Arc<dyn Clock>);
        impl Clock for CheckClock {
            fn now(&self) -> Moment {
                self.1.now()
            }
            fn wall(&self) -> std::time::SystemTime {
                self.1.wall()
            }
            fn sleep_until(&self, _: Moment) -> BoxFuture<'static, Hold> {
                if self.0.load(Ordering::SeqCst) {
                    Box::pin(async { Hold::empty() })
                } else {
                    Box::pin(std::future::pending())
                }
            }
            fn hold(&self, _: HoldKind) -> Hold {
                Hold::empty()
            }
        }
        struct Output(std::io::Cursor<Vec<u8>>);
        impl tokio::io::AsyncRead for Output {
            fn poll_read(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                if self.0.position() < self.0.get_ref().len() as u64 {
                    std::pin::Pin::new(&mut self.0).poll_read(cx, buf)
                } else {
                    std::task::Poll::Pending
                }
            }
        }
        struct Launcher {
            checks: AtomicUsize,
            starts: AtomicUsize,
            checking: Arc<AtomicBool>,
            timeout: bool,
            restart_fails: bool,
        }
        impl ProcessLauncher for Launcher {
            fn launch(&self, spec: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
                let check = spec
                    .program
                    .parent()
                    .unwrap()
                    .ends_with("official-runtime-check");
                self.checking.store(check, Ordering::SeqCst);
                let stdout: crate::ProcessRead = if check {
                    assert!(
                        self.checks.fetch_add(1, Ordering::SeqCst) < 3,
                        "three candidate starts cap"
                    );
                    if self.timeout {
                        Box::pin(Output(std::io::Cursor::new(Vec::new())))
                    } else {
                        Box::pin(tokio::io::empty())
                    }
                } else {
                    let start = self.starts.fetch_add(1, Ordering::SeqCst);
                    assert!(start < 4, "one start and three restoration attempts cap");
                    if self.restart_fails && start > 0 {
                        return Err(crate::LaunchError::Io(std::io::Error::other(
                            "accepted build restart failed",
                        )));
                    }
                    let replies = [
                        serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}}),
                        serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"restored","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
                    ];
                    Box::pin(Output(std::io::Cursor::new(
                        replies
                            .iter()
                            .map(|reply| format!("{reply}\n"))
                            .collect::<String>()
                            .into_bytes(),
                    )))
                };
                Ok(Launched {
                    stdin: Box::pin(tokio::io::sink()),
                    stdout,
                    stderr: Box::pin(tokio::io::empty()),
                    tree: Box::new(Tree),
                })
            }
        }
        struct Facts(crate::ListedBuild);
        impl ReleaseFacts for Facts {
            fn version(&self) -> &str {
                "test"
            }
            fn platform(&self) -> &str {
                &self.0.platform
            }
            fn listed_builds(&self) -> &[crate::ListedBuild] {
                std::slice::from_ref(&self.0)
            }
            fn highest_concurrency_limit(&self) -> u8 {
                1
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            use sha2::{Digest, Sha256};
            let root = std::env::temp_dir().join(format!("acp2api-check-failures-{}", crate::random_hex::<12>().unwrap()));
            let source = root.join("exe");
            let accepted = root.join("official-runtime");
            for (folder, bytes) in [(&source, b"candidate".as_slice()), (&accepted, b"accepted".as_slice())] {
                std::fs::create_dir_all(folder).unwrap();
                for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(folder.join(name), bytes).unwrap(); }
            }
            let checking = Arc::new(AtomicBool::new(false));
            let launcher = Arc::new(Launcher { checks:AtomicUsize::new(0),starts:AtomicUsize::new(0),checking:checking.clone(),timeout,restart_fails });
            let mut ports = crate::Ports::production();
            ports.clock = Arc::new(CheckClock(checking, ports.clock));
            let hash = crate::hex(&Sha256::digest(b"accepted"));
            ports.release = Arc::new(Facts(crate::ListedBuild { version:"1.4.0".into(),platform:ports.release.platform().into(),agy_acp_server_sha256:hash.clone(),localharness_external_sha256:hash }));
            ports.launcher = launcher.clone();
            let builds = crate::runtime_builds::Builds::new(&ports, crate::Environment {
                account_root:root.join("account"), temp:root.join("temp"), home:Some(root.join("home")), local_app_data:None, path:vec![], exe_dir:source
            }, root.clone(), crate::account::Temporary::fixture(&root), crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
            let configuration = root.join("config.toml");
            let settings = crate::store::read(&configuration, 1).unwrap().settings;
            let journal = crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = Conductor::start(builds, ports.clock, journal, settings, configuration, 1);
            conductor.builds(crate::runtime_builds::Action::Initial, Hold::empty()).await.unwrap();
            assert_eq!(serde_json::to_value(conductor.model_names(Hold::empty()).await.ok().unwrap()).unwrap()[0]["id"], "m");
            let identity = conductor.builds_summary().unwrap().found[0].identity.clone();
            for _ in 0..if restart_fails { 1 } else { 3 } {
                let notice = conductor.builds(crate::runtime_builds::Action::Check(identity.clone()), Hold::empty()).await.unwrap();
                assert!(notice.contains("compatibility check failed"));
                let models = conductor.model_names(Hold::empty()).await;
                if restart_fails {
                    let message = models.err().unwrap().message.to_string();
                    assert!(message.contains(REPEATED_FAILURES), "the three failed restorations must count: {message}");
                } else {
                    assert_eq!(serde_json::to_value(models.unwrap_or_else(|error| panic!("a failed check must restore the accepted version: {}", error.message))).unwrap()[0]["id"], "m", "a failed check must restore the earlier accepted version without using its failure budget");
                }
            }
            assert_eq!(launcher.starts.load(Ordering::SeqCst), 4);
            drop(conductor);
            task.await.unwrap();
            std::fs::remove_dir_all(root).unwrap();
        }).await.expect("failure check cap: 5 seconds, three candidates, four serving starts, two replies under 1 KiB per serving start, no prompts or logs");
    }
    #[tokio::test]
    async fn crashed_checks_restore_the_accepted_version_without_consuming_its_failure_limit() {
        failed_checks(false, false).await;
    }
    #[tokio::test]
    async fn timed_out_checks_restore_the_accepted_version_without_consuming_its_failure_limit() {
        failed_checks(true, false).await;
    }
    #[tokio::test]
    async fn failures_to_restore_the_accepted_version_count_normally() {
        failed_checks(false, true).await;
    }
    #[tokio::test]
    async fn a_check_queues_requests_when_no_accepted_runtime_existed() {
        use crate::{BoxFuture, Launched, ProcessLauncher, ProcessTree};
        use std::sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        };
        struct Tree(Arc<AtomicBool>);
        impl ProcessTree for Tree {
            fn pid(&self) -> u32 {
                std::process::id()
            }
            fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
                Box::pin(std::future::pending())
            }
            fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
                Box::pin(async move {
                    self.0.store(true, Ordering::SeqCst);
                })
            }
        }
        struct Launcher(Mutex<Option<Launched>>, mpsc::UnboundedSender<()>);
        impl ProcessLauncher for Launcher {
            fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
                self.1.send(()).unwrap();
                Ok(self
                    .0
                    .lock()
                    .unwrap()
                    .take()
                    .expect("one check process cap"))
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-check-unavailable-{}",
                crate::random_hex::<12>().unwrap()
            ));
            let source = root.join("exe");
            std::fs::create_dir_all(&source).unwrap();
            for name in [
                crate::runtime_builds::SERVER,
                crate::runtime_builds::HARNESS,
            ] {
                std::fs::write(source.join(name), b"unlisted candidate").unwrap();
            }
            let (stdout, _peer) = tokio::io::duplex(8192);
            let stopped = Arc::new(AtomicBool::new(false));
            let (launched, mut launch) = mpsc::unbounded_channel();
            let mut ports = crate::Ports::production();
            ports.launcher = Arc::new(Launcher(
                Mutex::new(Some(Launched {
                    stdin: Box::pin(tokio::io::sink()),
                    stdout: Box::pin(stdout),
                    stderr: Box::pin(tokio::io::empty()),
                    tree: Box::new(Tree(stopped.clone())),
                })),
                launched,
            ));
            let (stop, stopping) = watch::channel(false);
            let builds = crate::runtime_builds::Builds::new(
                &ports,
                crate::Environment {
                    account_root: root.join("account"),
                    temp: root.join("temp"),
                    home: Some(root.join("home")),
                    local_app_data: None,
                    path: vec![],
                    exe_dir: source,
                },
                root.clone(),
                crate::account::Temporary::fixture(&root),
                crate::runtime_driver::mcp::ToolSets::new(0),
                stopping,
                0,
            );
            let configuration = root.join("config.toml");
            let settings = crate::store::read(&configuration, 1).unwrap().settings;
            let journal =
                crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
            let request = journal.begin(crate::journal::Api::Chat, ports.clock.now());
            let (conductor, task) = Conductor::start(
                builds,
                ports.clock.clone(),
                journal,
                settings,
                configuration,
                1,
            );
            conductor
                .builds(crate::runtime_builds::Action::Initial, Hold::empty())
                .await
                .unwrap();
            assert!(conductor.model_names(Hold::empty()).await.is_err());
            let identity = conductor.builds_summary().unwrap().found[0]
                .identity
                .clone();
            let owner = conductor.clone();
            let check = tokio::spawn(async move {
                owner
                    .builds(
                        crate::runtime_builds::Action::Check(identity),
                        Hold::empty(),
                    )
                    .await
            });
            launch.recv().await.unwrap();
            let turn = crate::dialect::CHAT
                .parse(br#"{"model":"m","messages":[{"role":"user","content":"queued"}]}"#)
                .ok()
                .unwrap()
                .turn;
            let admission = conductor
                .admit(turn, Hold::empty(), request, ResponseMode::Body)
                .await;
            assert!(
                admission.is_ok(),
                "a request during a check must queue even when no earlier accepted runtime existed"
            );
            let mut ticket = admission.ok().unwrap();
            assert!(
                matches!(
                    ticket.events.try_recv(),
                    Err(mpsc::error::TryRecvError::Empty)
                ),
                "a check sends no API prompt"
            );
            stop.send(true).unwrap();
            task.await.unwrap();
            assert!(matches!(check.await.unwrap(), Err(LaunchFailure::Stopped)));
            assert!(
                stopped.load(Ordering::SeqCst),
                "stop must end the check process"
            );
            std::fs::remove_dir_all(root).unwrap();
        })
        .await
        .expect(
            "check cap: 5 seconds, one process, one initialize write, 8 KiB input, one API request",
        );
    }
    #[tokio::test]
    async fn a_planned_build_change_replaces_the_actor_runtime_after_the_running_turn() {
        use crate::{
            LaunchError, LaunchSpec, Launched, ListedBuild, ProcessLauncher, ProcessTree,
            ReleaseFacts,
        };
        use std::sync::Mutex;
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
        fn record(trace: &Mutex<Vec<String>>, event: String) {
            let mut trace = trace.lock().unwrap();
            assert!(trace.len() < 32, "fake trace cap: 32 events");
            trace.push(event);
        }
        struct Tree(&'static str, Arc<Mutex<Vec<String>>>);
        impl ProcessTree for Tree {
            fn pid(&self) -> u32 {
                std::process::id()
            }
            fn exited(&mut self) -> crate::BoxFuture<'_, std::process::ExitStatus> {
                Box::pin(std::future::pending())
            }
            fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> crate::BoxFuture<'static, ()> {
                Box::pin(async move {
                    record(&self.1, format!("stop:{}", self.0));
                })
            }
        }
        struct Launcher(
            Mutex<VecDeque<(&'static str, Launched)>>,
            Arc<Mutex<Vec<String>>>,
        );
        impl ProcessLauncher for Launcher {
            fn launch(&self, _: LaunchSpec) -> Result<Launched, LaunchError> {
                let (label, child) = self
                    .0
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("two launches only");
                record(&self.1, format!("launch:{label}"));
                Ok(child)
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            use sha2::{Digest, Sha256};
            let root = std::env::temp_dir().join(format!("acp2api-build-replacement-{}", crate::random_hex::<12>().unwrap()));
            let temporary = crate::account::Temporary::fixture(&root);
            let trace = Arc::new(Mutex::new(Vec::new()));
            let release_a = Arc::new(tokio::sync::Notify::new());
            let prompting_a = Arc::new(tokio::sync::Notify::new());
            let mut children = VecDeque::new();
            let mut agents = Vec::new();
            let mut facts = Vec::new();
            for (label, version, folder) in [("old", "1.4.0", root.join("official-runtime")), ("new", "1.5.0", root.join("new-build"))] {
                std::fs::create_dir_all(&folder).unwrap();
                for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(folder.join(name), label.as_bytes()).unwrap(); }
                let hash = crate::hex(&Sha256::digest(label.as_bytes()));
                facts.push(ListedBuild { version: version.into(), platform: Facts(Vec::new()).platform().into(), agy_acp_server_sha256: hash.clone(), localharness_external_sha256: hash });
                let (product, agent) = tokio::io::duplex(65536);
                let (stdout, stdin) = tokio::io::split(product);
                children.push_back((label, Launched { stdin: Box::pin(stdin), stdout: Box::pin(stdout), stderr: Box::pin(tokio::io::empty()), tree: Box::new(Tree(label, trace.clone())) }));
                let trace = trace.clone();
                let release_a = release_a.clone();
                let prompting_a = prompting_a.clone();
                agents.push(tokio::spawn(async move {
                    let (input, mut output) = tokio::io::split(agent);
                    let mut input = tokio::io::BufReader::new(input);
                    let mut sessions = 0;
                    for _ in 0..8 {
                        let mut bytes = Vec::new();
                        let read = (&mut input).take(65537).read_until(b'\n', &mut bytes).await.unwrap();
                        if read == 0 { return; }
                        assert!(bytes.len() <= 65536, "fake ACP line cap: 64 KiB");
                        let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                        let method = request["method"].as_str().unwrap();
                        let options = serde_json::json!([{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]);
                        let result = match method {
                            "initialize" => serde_json::json!({"protocolVersion":1,"agentInfo":{"version":version}}),
                            "session/new" => {
                                sessions += 1;
                                record(&trace, format!("prepare:{label}:{sessions}"));
                                serde_json::json!({"sessionId":format!("{label}-{sessions}"),"configOptions":options})
                            }
                            "session/set_config_option" => serde_json::json!({"configOptions":options}),
                            "session/prompt" => {
                                let session = request["params"]["sessionId"].as_str().unwrap();
                                assert!(session.starts_with(label), "no ACP session from the old process may reach the new process");
                                record(&trace, format!("prompt:{label}"));
                                if label == "old" { prompting_a.notify_one(); release_a.notified().await; }
                                record(&trace, format!("reply:{label}"));
                                serde_json::json!({"stopReason":"end_turn"})
                            }
                            _ => panic!("unexpected ACP method: {method}"),
                        };
                        let reply = serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result});
                        if output.write_all(format!("{reply}\n").as_bytes()).await.is_err() { return; }
                    }
                    panic!("fake ACP request cap: 8");
                }));
            }
            let mut ports = crate::Ports::production();
            ports.release = Arc::new(Facts(facts));
            ports.launcher = Arc::new(Launcher(Mutex::new(children), trace.clone()));
            let environment = crate::Environment { account_root: root.join("account"), temp: root.join("temp"), home: None, local_app_data: None, path: vec![], exe_dir: root.join("new-build") };
            let builds = crate::runtime_builds::Builds::new(&ports, environment, root.clone(), temporary, crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
            let config = root.join("config.toml");
            let settings = crate::store::read(&config, 1).unwrap().settings;
            let choice = crate::runtime_builds::identity(&ports.release.listed_builds()[1]);
            let journal = crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = Conductor::start(builds, ports.clock.clone(), journal.clone(), settings, config, 1);
            conductor.builds(crate::runtime_builds::Action::Initial, Hold::empty()).await.unwrap();
            let parse = |system: &str| crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"system","content":system},{"role":"user","content":"request"}]})).unwrap()).ok().unwrap().turn;
            let mut a = conductor.admit(parse("A"), Hold::empty(), journal.begin(crate::journal::Api::Chat, 0), ResponseMode::Body).await.ok().unwrap();
            prompting_a.notified().await;
            let (reply, owner) = oneshot::channel();
            conductor.send.send(Input::Builds { action: crate::runtime_builds::Action::Build(choice.to_string()), reply, hold: Hold::empty() }).await.unwrap();
            let mut b = conductor.admit(parse("B"), Hold::empty(), journal.begin(crate::journal::Api::Chat, 0), ResponseMode::Body).await.ok().unwrap();
            let waiting = trace.lock().unwrap().clone();
            let deferred = !waiting.iter().any(|event| event == "stop:old" || event == "launch:new");
            if !deferred {
                task.abort(); for agent in agents { agent.abort(); }
                std::fs::remove_dir_all(&root).unwrap();
                assert!(deferred, "the actor replaced the runtime before A ended: {waiting:?}");
                return;
            }
            release_a.notify_one();
            let mut normal = Vec::new();
            for ticket in [&mut a, &mut b] {
                while let Some((event, _hold)) = ticket.events.recv().await {
                    if let TurnEvent::End(result) = event { normal.push(result.is_ok()); break; }
                }
            }
            owner.await.unwrap().unwrap();
            drop(conductor);
            task.await.unwrap();
            for agent in agents { agent.await.unwrap(); }
            let actual = trace.lock().unwrap().clone();
            let index = |event: &str| actual.iter().position(|entry| entry == event).unwrap();
            let ordered = index("reply:old") < index("stop:old") && index("stop:old") < index("launch:new") && index("launch:new") < index("prepare:new:2") && index("prepare:new:2") < index("prompt:new");
            std::fs::remove_dir_all(root).unwrap();
            assert!(normal == [true, true] && ordered, "A and B must end normally around the planned replacement: {actual:?}");
        }).await.expect("planned build replacement cap: 5 seconds; two fake agents, 8 requests and 64 KiB per line each, 32 trace events");
    }

    #[test]
    fn disconnect_during_session_load_does_not_fail_another_prompt_after_30_seconds() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-load-disconnect-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, loading, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Text("Hi".into()),
            }),
            1,
            std::time::UNIX_EPOCH,
        );
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(agent_client_protocol_schema::v1::StopReason::EndTurn),
                },
            }),
            2,
            std::time::UNIX_EPOCH,
        );
        core.handle(Event::Deadline, 202, std::time::UNIX_EPOCH);
        core.records[0].location = records::Location::Unloaded(folder.join("saved-cwd"));
        let id = core.by_request(&loading).unwrap();
        core.work.get_mut(&id).and_then(|work| match work {
            Work::AcceptedQueued(request) => Some(request),
            _ => None,
        }).unwrap().turn = crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"user","content":"First"},{"role":"assistant","content":"Hi"},{"role":"user","content":"Next"}]})).unwrap()).ok().unwrap().turn;
        core.replaced(Some("other".into()), None);
        core.limit = 2;
        let preparing = core.handle(Event::ResumeRuntime, 203, std::time::UNIX_EPOCH);
        assert!(preparing.iter().any(|command| matches!(command, Command::Prepare { load: Some((session, _)), .. } if session == "running")));
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"m","messages":[{"role":"user","content":"Independent"}]}"#)
            .ok()
            .unwrap()
            .turn;
        let (output, _ticket) = mpsc::unbounded_channel();
        core.handle(
            Event::Accepted(Box::new(Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: "m".into(),
                    level: None,
                    level_from_name: false,
                },
                request: journal.begin(crate::journal::Api::Chat, 203),
                reply: ResponseChannel {
                    output,
                    keep_alive: None,
                },
            })),
            203,
            std::time::UNIX_EPOCH,
        );
        core.handle(
            Event::Disconnected(loading.clone()),
            204,
            std::time::UNIX_EPOCH,
        );
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: loading,
                result: Ok(("running".into(), catalog)),
            }),
            205,
            std::time::UNIX_EPOCH,
        );
        let mut commands = core.handle(Event::Deadline, 30_205, std::time::UNIX_EPOCH);
        commands.extend(core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "other".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(agent_client_protocol_schema::v1::StopReason::EndTurn),
                },
            }),
            30_206,
            std::time::UNIX_EPOCH,
        ));
        commands.extend(core.handle(Event::Deadline, 30_406, std::time::UNIX_EPOCH));
        let ends: Vec<_> = commands
            .into_iter()
            .filter_map(|command| match command {
                Command::Emit(_, TurnEvent::End(result)) => Some(
                    result
                        .map(|_| "success".into())
                        .unwrap_or_else(|error| error.to_string()),
                ),
                _ => None,
            })
            .collect();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            ends,
            ["success"],
            "FR-041 a disconnected session/load has no running prompt to time out"
        );
    }

    #[tokio::test]
    async fn a_broken_stdin_counts_once_for_a_command_batch_and_later_closed() {
        use crate::{BoxFuture, Launched, ProcessLauncher, ProcessTree};
        use tokio::io::AsyncWriteExt;
        struct Tree;
        impl ProcessTree for Tree {
            fn pid(&self) -> u32 {
                std::process::id()
            }
            fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
                Box::pin(std::future::pending())
            }
            fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
                Box::pin(async {})
            }
        }
        struct Launcher(std::sync::Mutex<Option<Launched>>);
        impl ProcessLauncher for Launcher {
            fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
                Ok(self.0.lock().unwrap().take().expect("one launch cap"))
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let folder = std::env::temp_dir().join(format!("acp2api-cancel-pipe-{}", crate::random_hex::<12>().unwrap()));
            let clock = crate::Ports::production().clock;
            let mut builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
            let (stdin, agent_input) = tokio::io::duplex(65536);
            let (stdout, mut agent_output) = tokio::io::duplex(65536);
            let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
            let replies = [serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}}), serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"running","configOptions":options["configOptions"]}})];
            for reply in replies { let line = format!("{reply}\n"); assert!(line.len() <= 65536); agent_output.write_all(line.as_bytes()).await.unwrap(); }
            let permission = serde_json::json!({"jsonrpc":"2.0","id":"permission","method":"session/request_permission","params":{"sessionId":"running","toolCall":{"toolCallId":"call","title":"Run view_file?"},"options":[]}});
            agent_output.write_all(format!("{permission}\n").as_bytes()).await.unwrap();
            builds.start.launcher = Arc::new(Launcher(std::sync::Mutex::new(Some(Launched { stdin:Box::pin(stdin), stdout:Box::pin(stdout), stderr:Box::pin(tokio::io::empty()), tree:Box::new(Tree) }))));
            let (mut runtime, _) = Runtime::start(&builds.start, "1.4.0").await.unwrap();
            let (RuntimeEvent::Turn { update:TurnUpdate::RefusedPermission { request, .. }, .. }, _hold) = runtime.next().await else { panic!("the fake must send its permission request"); };
            let mut runtime = Some(runtime);
            let mut read_open = true;
            let journal = crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
            let (mut core, _, [ _a, mut b ]) = prompting_with_waiter(&journal);
            let active = core.work[&0].execution().unwrap();
            core.records.push(records::Record { location:records::Location::Unloaded(folder.join("saved-cwd")), session:"saved".into(), hashes:crate::conversation::RecordHashes { system:active.hashes.system, messages:active.hashes.messages.clone() }, tools:active.tools_hash, used:0, expires:86_400_000 });
            core.startup_session = Some("sibling".into());
            core.handle(Event::Limit(2), 0, std::time::UNIX_EPOCH);
            drop(agent_input);
            let mut batch = Vec::new();
            core.cancel("running".into(), 0, &mut batch);
            batch.push(Command::Permission(request));
            let mut batch: VecDeque<_> = batch.into();
            Conductor::commands(&mut core, &mut runtime, &mut read_open, &clock, &journal, &mut batch).await;
            assert_eq!(core.failures.len(), 1, "two writes to one closed stdin are one runtime failure");
            let closed = core.handle(Event::Runtime(RuntimeEvent::Closed), 1, std::time::UNIX_EPOCH);
            let deadline = core.handle(Event::Deadline, 30_000, std::time::UNIX_EPOCH);
            assert_eq!(core.failures.len(), 1, "Closed and the old deadline must not count the failed runtime again");
            assert!(closed.is_empty() && deadline.is_empty(), "the failed runtime must emit no second M-20");
            let mut message = None;
            while let Ok((event, _hold)) = b.try_recv() { if let TurnEvent::End(Err(error)) = event { message = Some(error.to_string()); } }
            let retained: Vec<_> = core.records.iter().map(|record| record.session.as_str()).collect();
            let actual = (message, retained == ["saved"]);
            runtime.take().unwrap().stop().await;
            std::fs::remove_dir_all(folder).unwrap();
            assert_eq!(actual, (Some("acp2api: the runtime process failed, so acp2api ended the prompt turn.".into()), true), "a closed stdin is a runtime failure before stdout Closed arrives");
        }).await.expect("broken pipe cap: five seconds, one fake launch, three messages of at most 64 KiB");
    }

    #[tokio::test]
    async fn a_failed_initial_copy_succeeds_with_a_prd_unavailable_reason() {
        use crate::{ListedBuild, ProcessLauncher, ReleaseFacts};
        use sha2::{Digest, Sha256};
        struct NoLaunch;
        impl ProcessLauncher for NoLaunch {
            fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
                panic!("a failed first copy must launch no process")
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let folder = std::env::temp_dir().join(format!("acp2api-initial-copy-{}", crate::random_hex::<12>().unwrap()));
            let clock = crate::Ports::production().clock;
            let mut builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
            builds.start.launcher = Arc::new(NoLaunch);
            let hash = crate::hex(&Sha256::digest(b"accepted source"));
            builds.start.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:Facts(Vec::new()).platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
            let source = folder.join("source");
            std::fs::create_dir_all(&source).unwrap();
            for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(source.join(name), b"accepted source").unwrap(); }
            let config = folder.join("config.toml");
            let mut settings = crate::store::read(&config, 1).unwrap().settings;
            settings.agy_acp_server_path = Some(source.join(crate::runtime_builds::SERVER));
            let crate::runtime_builds::Selection::Start(selected) = builds.select(crate::runtime_builds::Action::Initial, &mut settings, &config).await.unwrap() else { panic!("the accepted source must be selected"); };
            std::fs::remove_file(source.join(crate::runtime_builds::HARNESS)).unwrap();
            std::fs::create_dir(source.join(crate::runtime_builds::HARNESS)).unwrap();
            let journal = crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
            let mut core = Core::fixture(1);
            core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
            let (reply, result) = oneshot::channel();
            core.replacement = Some(Replacement::Build { reply, source:selected, initial:true });
            let settings = watch::channel(Arc::new(settings)).0;
            let mut runtime = None;
            let mut read_open = false;
            core.replace(&mut runtime, &mut read_open, &clock, &journal, &builds, &settings).await;
            let reason = match core.runtime_state { RuntimeState::Unavailable(reason) => Some(reason.message), _ => None };
            let actual = (result.await.unwrap().map_err(|error| error.to_string()), reason, runtime.is_none() && !read_open);
            std::fs::remove_dir_all(folder).unwrap();
            assert_eq!(actual, (Ok(String::new()), Some("acp2api: the official runtime is unavailable: the copy of the accepted build 1.4.0 failed. Accepted versions: 1.4.0. See http://127.0.0.1:0/.".into()), true), "a failed automatic copy must let startup keep the management panel available with FR-040 M-14");
        }).await.expect("initial-copy test cap: five seconds, one copy attempt, zero process launches");
    }

    #[tokio::test]
    async fn a_failed_owner_copy_preserves_the_existing_unavailable_reason() {
        use crate::{ListedBuild, ProcessLauncher, ReleaseFacts};
        use sha2::{Digest, Sha256};
        struct NoLaunch;
        impl ProcessLauncher for NoLaunch {
            fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
                panic!("a copy failure must launch no process")
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let folder = std::env::temp_dir().join(format!("acp2api-unavailable-copy-{}", crate::random_hex::<12>().unwrap()));
            let source = folder.join("source");
            let mut ports = crate::Ports::production();
            let hash = crate::hex(&Sha256::digest(b"accepted source"));
            ports.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:Facts(Vec::new()).platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
            ports.launcher = Arc::new(NoLaunch);
            let builds = crate::runtime_builds::Builds::new(&ports, crate::Environment { account_root:folder.join("account"), temp:folder.join("temp"), home:None, local_app_data:None, path:vec![], exe_dir:source.clone() }, folder.clone(), crate::account::Temporary::fixture(&folder), crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
            let config = folder.join("config.toml");
            let settings = crate::store::read(&config, 1).unwrap().settings;
            let journal = crate::journal::Journal::open(&folder.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = Conductor::start(builds.clone(), ports.clock.clone(), journal, settings, config, 1);
            conductor.builds(crate::runtime_builds::Action::Initial, Hold::empty()).await.unwrap();
            let before = conductor.model_names(Hold::empty()).await.err().map(|error| error.message.to_string());
            std::fs::create_dir_all(&source).unwrap();
            for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(source.join(name), b"accepted source").unwrap(); }
            builds.discover(None).await.unwrap();
            let choice = crate::runtime_builds::identity(&ports.release.listed_builds()[0]);
            std::fs::remove_file(source.join(crate::runtime_builds::HARNESS)).unwrap();
            std::fs::create_dir(source.join(crate::runtime_builds::HARNESS)).unwrap();
            let copy_error = std::fs::copy(source.join(crate::runtime_builds::HARNESS), folder.join("probe")).unwrap_err().to_string();
            let owner = conductor.builds(crate::runtime_builds::Action::Build(choice.to_string()), Hold::empty()).await.map_err(|error| error.to_string());
            let after = conductor.model_names(Hold::empty()).await.err().map(|error| error.message.to_string());
            drop(conductor);
            task.await.unwrap();
            std::fs::remove_dir_all(folder).unwrap();
            let expected = "acp2api: the official runtime is unavailable: acp2api found no executable of the official runtime. Accepted versions: 1.4.0. See http://127.0.0.1:0/.".to_string();
            assert_eq!((owner, before, after), (Err(copy_error), Some(expected.clone()), Some(expected)), "an owner's failed copy must preserve the earlier FR-040 state and M-14 reason");
        }).await.expect("unavailable-copy test cap: five seconds, four conductor requests, zero process launches");
    }

    #[tokio::test]
    async fn a_failed_owner_copy_keeps_the_running_runtime_and_counts_no_failure() {
        use crate::{BoxFuture, Launched, ListedBuild, ProcessLauncher, ProcessTree, ReleaseFacts};
        use sha2::{Digest, Sha256};
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use tokio::io::AsyncWriteExt;
        struct Tree(Arc<AtomicBool>);
        impl ProcessTree for Tree {
            fn pid(&self) -> u32 {
                std::process::id()
            }
            fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
                Box::pin(std::future::pending())
            }
            fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
                Box::pin(async move {
                    self.0.store(true, Ordering::SeqCst);
                })
            }
        }
        struct Launcher(std::sync::Mutex<Option<Launched>>, AtomicUsize);
        impl ProcessLauncher for Launcher {
            fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
                assert!(self.1.fetch_add(1, Ordering::SeqCst) < 4, "launch cap: 4");
                self.0.lock().unwrap().take().ok_or_else(|| {
                    crate::LaunchError::Io(std::io::Error::other("unexpected replacement start"))
                })
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let folder = std::env::temp_dir().join(format!("acp2api-owner-copy-{}", crate::random_hex::<12>().unwrap()));
            let mut ports = crate::Ports::production();
            let copy = folder.join(crate::runtime_builds::COPY);
            std::fs::create_dir_all(&copy).unwrap();
            for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(copy.join(name), b"accepted").unwrap(); }
            let hash = crate::hex(&Sha256::digest(b"accepted"));
            ports.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:Facts(Vec::new()).platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
            let (stdin, _agent_input) = tokio::io::duplex(65536);
            let (stdout, mut agent_output) = tokio::io::duplex(65536);
            let replies = [serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}}), serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"running","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}})];
            for reply in replies { let line = format!("{reply}\n"); assert!(line.len() <= 65536); agent_output.write_all(line.as_bytes()).await.unwrap(); }
            let stopped = Arc::new(AtomicBool::new(false));
            ports.launcher = Arc::new(Launcher(std::sync::Mutex::new(Some(Launched { stdin:Box::pin(stdin), stdout:Box::pin(stdout), stderr:Box::pin(tokio::io::empty()), tree:Box::new(Tree(stopped.clone())) })), AtomicUsize::new(0)));
            let temporary = crate::account::Temporary::fixture(&folder);
            let builds = crate::runtime_builds::Builds::new(&ports, crate::Environment { account_root:folder.join("account"), temp:folder.join("temp"), home:None, local_app_data:None, path:vec![], exe_dir:folder.join("exe") }, folder.clone(), temporary, crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
            let (running, _) = Runtime::start(&builds.start, "1.4.0").await.unwrap();
            let mut runtime = Some(running);
            let mut read_open = true;
            let source = folder.join("new-build");
            std::fs::create_dir_all(source.join(crate::runtime_builds::HARNESS)).unwrap();
            std::fs::write(source.join(crate::runtime_builds::SERVER), b"accepted").unwrap();
            let copy_error = std::fs::copy(source.join(crate::runtime_builds::HARNESS), folder.join("probe")).unwrap_err().to_string();
            let journal = crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
            let (build_reply, build_result) = oneshot::channel();
            let (check_reply, check_result) = oneshot::channel();
            for (replacement, result) in [
                (Replacement::Build { reply:build_reply, source:Some(SelectedBuild { folder:source.clone(), version:"1.4.0".into() }), initial:false }, build_result),
                (Replacement::Check { reply:check_reply, folder:source, identity:crate::runtime_builds::identity(&ports.release.listed_builds()[0]) }, check_result),
            ] {
                let (mut core, _, _tickets) = prompting_with_waiter(&journal);
                core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
                core.handle(Event::Runtime(RuntimeEvent::Turn { session:"running".into(), update:TurnUpdate::PromptEnded { stop_reason:Some(agent_client_protocol_schema::v1::StopReason::EndTurn) } }), 1, std::time::UNIX_EPOCH);
                core.handle(Event::Deadline, 201, std::time::UNIX_EPOCH);
                core.cleanup_before_start = true;
                core.replacement = Some(replacement);
                let config = folder.join("config.toml");
                let settings = watch::channel(Arc::new(crate::store::read(&config, 1).unwrap().settings)).0;
                core.replace(&mut runtime, &mut read_open, &ports.clock, &journal, &builds, &settings).await;
                let actual = (result.await.unwrap().map_err(|error| error.to_string()), core.failures.len(), stopped.load(Ordering::SeqCst), runtime.is_some() && read_open, matches!(core.runtime_state, RuntimeState::Serving));
                assert_eq!(actual, (Err(copy_error.clone()), 0, false, true, true), "owner copy errors must return to the owner while the existing runtime stays available");
            }
            if let Some(runtime) = runtime { runtime.stop().await; }
            std::fs::remove_dir_all(folder).unwrap();
        }).await.expect("owner copy cap: five seconds, four launch attempts, two replies of at most 64 KiB");
    }

    #[tokio::test]
    async fn a_cleanup_error_during_replacement_counts_failures_and_ends_the_queue() {
        use crate::{ListedBuild, ProcessLauncher, ReleaseFacts};
        use sha2::{Digest, Sha256};
        struct NoLaunch;
        impl ProcessLauncher for NoLaunch {
            fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
                panic!("cleanup failure must start no process")
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let folder = std::env::temp_dir().join(format!("acp2api-cleanup-replacement-{}", crate::random_hex::<12>().unwrap()));
            let mut ports = crate::Ports::production();
            let copy = folder.join(crate::runtime_builds::COPY);
            std::fs::create_dir_all(&copy).unwrap();
            for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(copy.join(name), b"accepted").unwrap(); }
            let hash = crate::hex(&Sha256::digest(b"accepted"));
            ports.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:Facts(Vec::new()).platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
            ports.launcher = Arc::new(NoLaunch);
            let temporary = crate::account::Temporary::fixture(&folder);
            let cwd = temporary.sessions().join("kept");
            crate::account::private_folder(&cwd).unwrap();
            std::fs::write(temporary.sessions().join("not-a-folder"), b"cleanup obstruction").unwrap();
            let builds = crate::runtime_builds::Builds::new(&ports, crate::Environment { account_root:folder.join("account"), temp:folder.join("temp"), home:None, local_app_data:None, path:vec![], exe_dir:folder.join("exe") }, folder.clone(), temporary, crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
            let journal = crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
            let (mut core, _, [_a, mut queued]) = prompting_with_waiter(&journal);
            core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
            core.handle(Event::Runtime(RuntimeEvent::Turn { session:"running".into(), update:TurnUpdate::PromptEnded { stop_reason:Some(agent_client_protocol_schema::v1::StopReason::EndTurn) } }), 1, std::time::UNIX_EPOCH);
            core.handle(Event::Deadline, 201, std::time::UNIX_EPOCH);
            core.records[0].location = records::Location::Unloaded(cwd);
            core.cleanup_before_start = true;
            let (reply, result) = oneshot::channel();
            core.replacement = Some(Replacement::Build { reply, source:None, initial:false });
            let config = folder.join("config.toml");
            let settings = watch::channel(Arc::new(crate::store::read(&config, 1).unwrap().settings)).0;
            let mut runtime = None; let mut read_open = false;
            core.replace(&mut runtime, &mut read_open, &ports.clock, &journal, &builds, &settings).await;
            let owner = result.await.unwrap();
            let mut unavailable = false;
            while let Ok((event, _hold)) = queued.try_recv() {
                if let TurnEvent::End(Err(crate::messages::Message::RuntimeUnavailable(message))) = event { unavailable = message.contains("the runtime process failed 3 times in 10 minutes"); }
            }
            let actual = (owner.is_ok(), core.failures.len(), unavailable);
            std::fs::remove_dir_all(folder).unwrap();
            assert_eq!(actual, (true, 3, true), "a verified copy whose preparation fails must not leave a serving conductor without a runtime");
        }).await.expect("cleanup failure cap: five seconds, one local obstruction, no process launch or HTTP request");
    }

    #[test]
    fn a_broken_pipe_during_exit_keeps_the_conductor_stopping() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-exit-pipe-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::Stop, 1, std::time::UNIX_EPOCH);
        core.command_failed(
            None,
            crate::runtime_driver::CommandError::Io(std::io::ErrorKind::BrokenPipe.into()),
            2,
            std::time::UNIX_EPOCH,
        );
        let exit_without_restart =
            matches!(core.runtime_state, RuntimeState::Stopping) && core.failures.is_empty();
        std::fs::remove_dir_all(folder).unwrap();
        assert!(
            exit_without_restart,
            "FR-045 an exit cancel that finds a closed pipe must not start a replacement"
        );
    }

    #[tokio::test]
    async fn closed_build_channels_report_stopped() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let folder = std::env::temp_dir().join(format!(
                "acp2api-closed-build-channel-{}",
                crate::random_hex::<12>().unwrap()
            ));
            let clock = crate::Ports::production().clock;
            for close_before_send in [true, false] {
                let (send, mut recv) = mpsc::channel(1);
                if close_before_send {
                    recv.close();
                }
                let receiver = tokio::spawn(async move {
                    if let Some(Input::Builds { reply, .. }) = recv.recv().await {
                        drop(reply);
                    }
                });
                let settings = crate::store::read(&folder.join("config.toml"), 1)
                    .unwrap()
                    .settings;
                let conductor = Conductor {
                    view: watch::channel(Arc::new(View::default())).1,
                    quota_result: watch::channel(Arc::new(None)).1,
                    send,
                    builds: crate::runtime_builds::Builds::fixture(&folder, clock.clone()),
                    settings: watch::channel(Arc::new(settings)).1,
                    notices: watch::channel(Arc::new(Vec::new())).1,
                    highest: 1,
                };
                let result = conductor
                    .builds(crate::runtime_builds::Action::Search, Hold::empty())
                    .await;
                receiver.await.unwrap();
                assert!(
                    matches!(result, Err(LaunchFailure::Stopped)),
                    "a closed conductor channel must report Stopped: {result:?}"
                );
            }
            std::fs::remove_dir_all(folder).unwrap();
        })
        .await
        .expect("closed-channel test cap: five seconds, two owner requests, no process launch");
    }

    #[test]
    fn a_session_prepared_after_runtime_failure_is_forgotten() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-late-prepared-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
        let work = core.work[&0].key().to_owned();
        core.handle(
            Event::Runtime(RuntimeEvent::Failed("broken pipe".into())),
            1,
            std::time::UNIX_EPOCH,
        );
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work,
                result: Ok(("late-session".into(), catalog)),
            }),
            2,
            std::time::UNIX_EPOCH,
        );
        std::fs::remove_dir_all(folder).unwrap();
        assert!(matches!(&commands[..], [Command::Forget(session)] if session == "late-session"));
    }

    #[test]
    fn a_failed_replacement_ends_queued_requests_with_runtime_unavailable() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-core-build-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, tickets) = prompting_with_waiter(&journal);
        core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
        let message = "acp2api: the official runtime is unavailable: runtime launch failed. Accepted versions: 1.4.0. See http://127.0.0.1:8329/.";
        let commands = core.handle(
            Event::RuntimeUnavailable(UnavailableReason {
                message: message.into(),
                cause: None,
            }),
            2,
            std::time::UNIX_EPOCH,
        );
        let ends: Vec<_> = commands
            .iter()
            .filter_map(|command| match command {
                Command::Emit(
                    _,
                    TurnEvent::End(Err(crate::messages::Message::RuntimeUnavailable(text))),
                ) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            ends,
            [message],
            "queued requests must receive M-14 when a planned replacement fails"
        );
        std::fs::remove_dir_all(folder).unwrap();
        drop(tickets);
    }

    #[test]
    fn a_build_change_finishes_a_before_preparing_b_on_the_new_runtime() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-core-build-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, queued, tickets) = prompting_with_waiter(&journal);
        let id = core.by_request(&queued).unwrap();
        let mut b = core.work.remove(&id).unwrap().into_queued().unwrap();
        b.turn = crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({
            "model":"m", "messages":[{"role":"system","content":"B's instructions"},{"role":"user","content":"B"}]
        })).unwrap()).ok().unwrap().turn;
        let changed = core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
        let arrival = core.handle(Event::Accepted(Box::new(b)), 2, std::time::UNIX_EPOCH);
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(agent_client_protocol_schema::v1::StopReason::EndTurn),
                },
            }),
            3,
            std::time::UNIX_EPOCH,
        );
        let completed = core.handle(Event::Deadline, 203, std::time::UNIX_EPOCH);
        let no_dispatch = changed
            .iter()
            .chain(&arrival)
            .chain(&ended)
            .chain(&completed)
            .all(|command| {
                !matches!(
                    command,
                    Command::Prepare { .. } | Command::Prompt { .. } | Command::Cancel(_)
                )
            });
        let ended_normally = completed
            .iter()
            .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Ok(_)))));
        assert!(
            no_dispatch && ended_normally && core.replacement_ready(),
            "B must wait for A to end normally before runtime replacement"
        );
        core.replaced(Some("new-runtime-startup".into()), None);
        let dispatched = core.handle(Event::ResumeRuntime, 204, std::time::UNIX_EPOCH);
        assert!(
            core.records.len() == 1
                && dispatched.iter().any(
                    |command| matches!(command, Command::Prepare {work,..} if work == &queued)
                ),
            "ADR-0003 retains A's record while the unrelated B gets a new ACP session"
        );
        std::fs::remove_dir_all(folder).unwrap();
        drop(tickets);
    }

    #[test]
    fn an_unpermitted_tool_keeps_its_place_until_the_prompt_ends() {
        let folder =
            std::env::temp_dir().join(format!("acp2api-tool-retirement-{}", std::process::id()));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, queued, tickets) = prompting_with_waiter(&journal);
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::UnpermittedTool("run_command".into()),
            }),
            1,
            std::time::UNIX_EPOCH,
        );
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::Cancel(session) if session == "running"))
        );
        assert!(commands.iter().any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::BuiltInToolRan(name)))) if name == "run_command")));
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Prepare { .. } | Command::Prompt { .. })),
            "FR-049 the cancelled prompt still occupies the only place"
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Forget(_))),
            "the runtime must retain the outstanding prompt until its result"
        );
        drop(commands);
        assert!(
            tickets[0].is_closed(),
            "the ended request retains no reply route"
        );
        let late = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Text("Late text".into()),
            }),
            2,
            std::time::UNIX_EPOCH,
        );
        assert!(late.is_empty(), "no output follows the request's error");
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded { stop_reason: None },
            }),
            3,
            std::time::UNIX_EPOCH,
        );
        assert!(
            ended
                .iter()
                .any(|command| matches!(command, Command::Prepare { work, .. } if work == &queued)),
            "FR-049 the prompt result releases its place for the queued request"
        );
        assert!(
            !ended
                .iter()
                .any(|command| matches!(command, Command::Emit(_, _) | Command::Complete(_))),
            "retirement sends no second end or request record"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn an_upstream_error_keeps_its_place_until_the_prompt_fails() {
        let folder =
            std::env::temp_dir().join(format!("acp2api-http-retirement-{}", std::process::id()));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, queued, tickets) = prompting_with_waiter(&journal);
        let error = || {
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(503),
                    status: "UNAVAILABLE".into(),
                    message: "HTTP failure".into(),
                },
            })
        };
        let commands = core.handle(error(), 1, std::time::UNIX_EPOCH);
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Prepare { .. } | Command::Prompt { .. })),
            "FR-049 an upstream error does not end the outstanding prompt"
        );
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::Cancel(session) if session == "running"))
        );
        assert!(commands.iter().any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::UpstreamHttp(error)))) if error.code == Some(503) && error.message == "HTTP failure")));
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Forget(_)))
        );
        drop(commands);
        assert!(
            tickets[0].is_closed(),
            "the ended request retains no reply route"
        );
        let repeated = core.handle(error(), 2, std::time::UNIX_EPOCH);
        assert!(
            matches!(&repeated[..], [Command::UpstreamLog { .. }]),
            "a repeated error keeps the occupied entry and sends no second end"
        );
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Failed(crate::messages::Message::Upstream(
                    "RPC failure".into(),
                )),
            }),
            3,
            std::time::UNIX_EPOCH,
        );
        assert!(
            ended
                .iter()
                .any(|command| matches!(command, Command::Prepare { work, .. } if work == &queued)),
            "FR-049 the failed prompt releases its place for the queued request"
        );
        assert!(
            !ended
                .iter()
                .any(|command| matches!(command, Command::Emit(_, _) | Command::Complete(_))),
            "retirement sends no second end or request record"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_retired_configuration_call_dispatches_the_next_request() {
        let folder =
            std::env::temp_dir().join(format!("acp2api-config-retirement-{}", std::process::id()));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
        let request = core.work[&core.by_session("running").unwrap()]
            .key()
            .to_owned();
        let disconnected = core.handle(Event::Disconnected(request), 1, std::time::UNIX_EPOCH);
        assert!(
            !disconnected
                .iter()
                .any(|command| matches!(command, Command::Prepare { .. }))
        );
        let retired = core.handle(
            Event::Runtime(RuntimeEvent::Retired {
                session: "running".into(),
            }),
            2,
            std::time::UNIX_EPOCH,
        );
        assert!(
            retired
                .iter()
                .any(|command| matches!(command, Command::Prepare { work, .. } if work == &queued)),
            "FR-049 the retired ACP call releases its place for the queued request"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn disconnect_during_session_creation_retires_without_a_prompt() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-disconnected-prepare-{}",
            std::process::id()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#)
            .ok()
            .unwrap()
            .turn;
        let request = journal.begin(crate::journal::Api::Chat, 0);
        let work = request.id.clone();
        let (output, ticket) = mpsc::unbounded_channel();
        let mut core = Core::fixture(1);
        let commands = core.handle(
            Event::Accepted(Box::new(Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: "m".into(),
                    level: None,
                    level_from_name: false,
                },
                request,
                reply: ResponseChannel {
                    output,
                    keep_alive: None,
                },
            })),
            0,
            std::time::UNIX_EPOCH,
        );
        assert!(matches!(&commands[..], [Command::Prepare { .. }]));
        drop(ticket);
        core.handle(Event::Disconnected(work.clone()), 1, std::time::UNIX_EPOCH);
        assert_eq!(
            core.work.len(),
            1,
            "an outstanding ACP call retains its place"
        );
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"m","messages":[{"role":"user","content":"Queued"}]}"#)
            .ok()
            .unwrap()
            .turn;
        let queued = journal.begin(crate::journal::Api::Chat, 1);
        let queued_id = queued.id.clone();
        let (output, _queued_ticket) = mpsc::unbounded_channel();
        assert!(
            core.handle(
                Event::Accepted(Box::new(Accepted {
                    turn,
                    model: crate::catalog::SelectedModel {
                        model: "m".into(),
                        level: None,
                        level_from_name: false
                    },
                    request: queued,
                    reply: ResponseChannel {
                        output,
                        keep_alive: None
                    },
                })),
                1,
                std::time::UNIX_EPOCH
            )
            .is_empty(),
            "the second request waits for the outstanding ACP call"
        );
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: work.clone(),
                result: Ok(("disconnected-session".into(), catalog)),
            }),
            2,
            std::time::UNIX_EPOCH,
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Prompt { .. })),
            "FR-031 sends no prompt for a disconnected request"
        );
        assert!(commands.iter().any(|command| matches!(command, Command::Forget(session) if session == "disconnected-session")), "FR-031 clears the created ACP session");
        assert!(
            commands.iter().any(
                |command| matches!(command, Command::Prepare { work, .. } if work == &queued_id)
            ),
            "the completed ACP call dispatches the next queued request"
        );
        assert!(
            core.by_request(&work).is_none(),
            "the completed ACP call releases its entry"
        );
        assert!(
            core.records.is_empty(),
            "the disconnected session cannot continue"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_prompt_rpc_error_waits_for_the_http_error_lines() {
        let folder =
            std::env::temp_dir().join(format!("acp2api-error-wait-{}", std::process::id()));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for with_http in [false, true] {
            let turn = crate::dialect::CHAT
                .parse(br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#)
                .ok()
                .unwrap()
                .turn;
            let (output, _ticket) = mpsc::unbounded_channel();
            let mut core = Core {
                startup_session: Some("session".into()),
                ..Core::fixture(1)
            };
            core.handle(
                Event::Accepted(Box::new(Accepted {
                    turn,
                    model: crate::catalog::SelectedModel {
                        model: "m".into(),
                        level: None,
                        level_from_name: false,
                    },
                    request: journal.begin(crate::journal::Api::Chat, 0),
                    reply: ResponseChannel {
                        output,
                        keep_alive: None,
                    },
                })),
                0,
                std::time::UNIX_EPOCH,
            );
            let ended = core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "session".into(),
                    update: TurnUpdate::Failed(crate::messages::Message::UpstreamAcp(
                        crate::messages::AcpError {
                            data: None,
                            code: -32099,
                            message: "RPC failure".into(),
                        },
                    )),
                }),
                0,
                std::time::UNIX_EPOCH,
            );
            assert!(
                !ended
                    .iter()
                    .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(_)))),
                "FR-028 waits for error lines after a prompt RPC failure"
            );
            assert!(
                core.handle(Event::Deadline, 199, std::time::UNIX_EPOCH)
                    .is_empty()
            );
            let ended = if with_http {
                core.handle(
                    Event::Runtime(RuntimeEvent::UpstreamError {
                        session: "session".into(),
                        error: crate::messages::HttpError {
                            code: Some(503),
                            status: "UNAVAILABLE".into(),
                            message: "HTTP failure".into(),
                        },
                    }),
                    200,
                    std::time::UNIX_EPOCH,
                )
            } else {
                core.handle(Event::Deadline, 200, std::time::UNIX_EPOCH)
            };
            let message = ended
                .into_iter()
                .find_map(|command| match command {
                    Command::Emit(_, TurnEvent::End(Err(error))) => Some(error.to_string()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                message,
                if with_http {
                    "acp2api: upstream error 503 UNAVAILABLE: HTTP failure"
                } else {
                    "acp2api: upstream ACP error -32099: RPC failure"
                }
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_tool_call_does_not_make_a_refused_turn_reusable() {
        let folder =
            std::env::temp_dir().join(format!("acp2api-tool-refusal-{}", std::process::id()));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let mut selected = Vec::new();
        for stop_reason in [
            Some(StopReason::Refusal),
            Some(StopReason::EndTurn),
            Some(StopReason::Cancelled),
            None,
        ] {
            let turn = crate::dialect::CHAT
                .parse(br#"{"model":"gemini-3.8-flash","tools":[{"type":"function","function":{"name":"read","parameters":{}}}],"messages":[{"role":"user","content":"First"}]}"#)
                .ok()
                .unwrap()
                .turn;
            let (output, _ticket) = mpsc::unbounded_channel();
            let mut core = Core {
                work: BTreeMap::from([(
                    0,
                    Work::AcceptedWorking(Active {
                        reply: ResponseChannel {
                            output: output.clone(),
                            keep_alive: None,
                        },
                        request: journal.begin(crate::journal::Api::Chat, 0),
                        session: Some("existing".into()),
                        hashes: crate::conversation::record_hashes(&turn.conversation),
                        tools_hash: crate::conversation::tool_set_hash(&turn.tools),
                        client_tools_hash: crate::conversation::tool_set_hash(&turn.tools),
                        kind: crate::journal::SessionUse::New,
                        used: 1,
                        expires: 100_000,
                        held: Some("Before".into()),
                        text: String::new(),
                        thought_dropped: false,
                        input_tokens: turn.input_tokens,
                        model: "gemini-3.8-flash".into(),
                        model_name: "gemini-3.8-flash".into(),
                        effort: EffortAsk::Absent,
                        phase: Phase::Prompt {
                            update_deadline: NO_UPDATE_LIMIT,
                        },
                        tool_call: Some(crate::conversation::ToolCall {
                            name: "read".into(),
                            arguments: "{}".into(),
                            kind: crate::conversation::ToolKind::Function,
                            namespace: None,
                        }),
                        stop: crate::conversation::Stop::EndTurn,
                        stops: Vec::new(),
                        stop_prefix: String::new(),
                    }),
                )]),
                next_work: 1,
                use_sequence: 1,
                ..Core::fixture(1)
            };
            core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "existing".into(),
                    update: TurnUpdate::PromptEnded { stop_reason },
                }),
                0,
                std::time::UNIX_EPOCH,
            );
            let commands = core.handle(Event::Deadline, 200, std::time::UNIX_EPOCH);
            answer_quota(&mut core, commands, 200);
            let next = crate::dialect::CHAT.parse(br#"{"model":"gemini-3.8-flash","tools":[{"type":"function","function":{"name":"read","parameters":{}}}],"messages":[
                {"role":"user","content":"First"},
                {"role":"assistant","content":"Before","tool_calls":[{"id":"call-id","function":{"name":"read","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"call-id","content":"Done"},
                {"role":"user","content":"Next"}
            ]}"#).ok().unwrap().turn;
            selected.extend(
                core.handle(
                    Event::Accepted(Box::new(Accepted {
                        model: crate::catalog::SelectedModel {
                            model: "gemini-3.8-flash".into(),
                            level: None,
                            level_from_name: false,
                        },
                        turn: next,
                        request: journal.begin(crate::journal::Api::Chat, 200),
                        reply: ResponseChannel {
                            output,
                            keep_alive: None,
                        },
                    })),
                    200,
                    std::time::UNIX_EPOCH,
                )
                .into_iter()
                .filter_map(|command| match command {
                    Command::Prepare { .. } => Some("rebuild".to_owned()),
                    Command::Prompt { session, .. } => Some(session),
                    _ => None,
                }),
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            selected,
            ["rebuild", "existing", "existing", "rebuild"],
            "ADR-0003 excludes a refused turn even when it returned a tool call"
        );
    }
    fn start_without_runtime(
        clock: &Arc<dyn Clock>,
        journal: &crate::journal::Journal,
        folder: &std::path::Path,
    ) -> (Conductor, tokio::task::JoinHandle<()>) {
        let settings = crate::store::Settings {
            port: 8329,
            concurrency_limit: 1,
            local_api_key: String::new(),
            management_password_hash: String::new(),
            agy_acp_server_path: None,
        };
        let configuration = folder.join("config.toml");
        Conductor::start(
            crate::runtime_builds::Builds::fixture(folder, clock.clone()),
            clock.clone(),
            journal.clone(),
            settings,
            configuration,
            3,
        )
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn a_start_without_state_survives_an_unwritable_data_folder() {
        use std::os::unix::fs::PermissionsExt;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let folder = std::env::temp_dir().join(format!(
                "acp2api-state-create-failure-{}",
                crate::random_hex::<12>().unwrap()
            ));
            std::fs::create_dir_all(&folder).unwrap();
            let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
            let journal =
                crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
            let builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
            let configuration = folder.join("config.toml");
            let settings = crate::store::read(&configuration, 3).unwrap().settings;
            let state_path = folder.join(crate::store::STATE_FILE);
            assert!(!state_path.exists());
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
            let (conductor, task) =
                Conductor::start(builds, clock.clone(), journal, settings, configuration, 3);
            let result = conductor
                .builds(crate::runtime_builds::Action::Initial, Hold::empty())
                .await;
            let state = conductor.view().state;
            let file_exists = state_path.exists();
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
            drop(conductor);
            task.await.unwrap();
            let logged = std::fs::read_dir(folder.join("logs"))
                .unwrap()
                .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
                .collect::<String>();
            std::fs::remove_dir_all(folder).unwrap();
            assert!(
                result.is_ok(),
                "the start keeps serving after a state-file write failure: {result:?}"
            );
            assert_eq!(
                state, "Runtime unavailable",
                "the missing executable, rather than lockout, stops runtime work"
            );
            assert!(!file_exists);
            assert!(
                logged.contains("state_file_write_failed"),
                "the journal reports the failed state-file creation"
            );
        })
        .await
        .expect("state-file startup test cap: five seconds, no runtime launches");
    }

    #[test]
    fn models_are_empty_after_a_runtime_closes() {
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        let mut core = Core {
            catalog: Some(catalog),
            ..Core::fixture(1)
        };
        assert_eq!(core.model_entries(0).ok().unwrap().len(), 1);
        let missing = crate::dialect::CHAT
            .parse(br#"{"model":"missing","messages":[{"role":"user","content":"Hi"}]}"#)
            .ok()
            .unwrap()
            .turn;
        assert!(core.check_model(&missing).is_err());
        core.handle(
            Event::Runtime(RuntimeEvent::Closed),
            0,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            core.model_entries(0).ok().unwrap().len(),
            0,
            "GET /v1/models"
        );
        assert!(core.check_model(&missing).is_ok());
    }
    #[test]
    fn a_session_that_leaves_for_good_is_forgotten() {
        let folder = std::env::temp_dir().join(format!("acp2api-forget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let mut core = Core::fixture(1);
        let mut outputs = Vec::new();
        let mut forgotten = Vec::new();
        let mut turn = |core: &mut Core, system: &str, session: &str, ends: TurnUpdate| {
            let body = format!(
                r#"{{"model":"gemini-3.8-flash","messages":[{{"role":"system","content":"{system}"}},{{"role":"user","content":"Hi"}}]}}"#
            );
            let turn = crate::dialect::CHAT
                .parse(body.as_bytes())
                .ok()
                .unwrap()
                .turn;
            let request = journal.begin(crate::journal::Api::Chat, 0);
            let work = request.id.clone();
            let (output, ticket) = mpsc::unbounded_channel();
            outputs.push(ticket);
            let mut commands = core.handle(
                Event::Accepted(Box::new(Accepted {
                    model: crate::catalog::SelectedModel {
                        model: "gemini-3.8-flash".into(),
                        level: None,
                        level_from_name: false,
                    },
                    turn,
                    request,
                    reply: ResponseChannel {
                        output,
                        keep_alive: None,
                    },
                })),
                0,
                std::time::UNIX_EPOCH,
            );
            let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"gemini-3.8-flash","options":[{"value":"gemini-3.8-flash","name":"M"}]}]});
            let catalog = crate::catalog::Catalog::decode(&options, clock.wall()).unwrap();
            let prepared = Ok((session.to_owned(), catalog));
            commands.extend(core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work,
                    result: prepared,
                }),
                0,
                std::time::UNIX_EPOCH,
            ));
            let session = session.to_owned();
            commands.extend(core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session,
                    update: ends,
                }),
                0,
                std::time::UNIX_EPOCH,
            ));
            commands.extend(core.handle(Event::Deadline, 1000, std::time::UNIX_EPOCH));
            commands = answer_quota(core, commands, 1000);
            forgotten.extend(commands.into_iter().filter_map(|command| match command {
                Command::Forget(session) => Some(session),
                _ => None,
            }));
        };
        let ended = |end_turn| TurnUpdate::PromptEnded {
            stop_reason: Some(if end_turn {
                agent_client_protocol_schema::v1::StopReason::EndTurn
            } else {
                agent_client_protocol_schema::v1::StopReason::Refusal
            }),
        };
        turn(
            &mut core,
            "A",
            "s1",
            TurnUpdate::Failed(crate::messages::Message::Upstream("x".into())),
        );
        turn(&mut core, "B", "s2", ended(false));
        turn(&mut core, "C", "s3", ended(true));
        turn(&mut core, "D", "s4", ended(true));
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(
            forgotten,
            ["s1", "s2"],
            "a failed session and a session with no record; records stay above the limit"
        );
    }
    #[test]
    fn an_idle_session_is_forgotten_by_its_24_hour_wake_up() {
        let body = br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#;
        let turn = crate::dialect::CHAT.parse(body).ok().unwrap().turn;
        let mut core = Core {
            records: vec![records::Record {
                location: records::Location::Loaded,
                session: "idle".into(),
                hashes: crate::conversation::record_hashes(&turn.conversation),
                tools: crate::conversation::tool_set_hash(&[]),
                used: 1,
                expires: 24 * 60 * 60 * 1000,
            }],
            use_sequence: 1,
            ..Core::fixture(1)
        };
        let mut forgotten = Vec::new();
        if let Some(at) = core.next_deadline()
            && at <= 24 * 60 * 60 * 1000
        {
            for command in core.handle(Event::Deadline, at, std::time::UNIX_EPOCH) {
                if let Command::Forget(session) = command {
                    forgotten.push((at, session));
                }
            }
        }
        assert_eq!(
            forgotten,
            [(86_400_000, "idle".to_owned())],
            "an idle session is forgotten at 24 hours with only its scheduled wake-up"
        );
    }
    #[test]
    fn a_failed_session_creation_still_logs_its_session_use() {
        let folder = std::env::temp_dir().join(format!("acp2api-use-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let mut core = Core::fixture(1);
        let body = br#"{"model":"m","messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"},{"role":"user","content":"Again"}]}"#;
        let turn = crate::dialect::CHAT.parse(body).ok().unwrap().turn;
        let request = journal.begin(crate::journal::Api::Chat, 0);
        let work = request.id.clone();
        let (output, _ticket) = mpsc::unbounded_channel();
        let accepted = Event::Accepted(Box::new(Accepted {
            model: crate::catalog::SelectedModel {
                model: "m".into(),
                level: None,
                level_from_name: false,
            },
            turn,
            request,
            reply: ResponseChannel {
                output,
                keep_alive: None,
            },
        }));
        assert!(matches!(
            &core.handle(accepted, 0, std::time::UNIX_EPOCH)[..],
            [Command::Prepare { .. }]
        ));
        let failed = Event::Runtime(RuntimeEvent::Prepared {
            work,
            result: Err(crate::messages::Message::Upstream(
                "session/new failed".into(),
            )),
        });
        for command in core.handle(failed, 0, std::time::UNIX_EPOCH) {
            if let Command::Complete(completion) = command {
                journal.complete(completion, clock.as_ref());
            }
        }
        let lines: String = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        std::fs::remove_dir_all(&folder).unwrap();
        let line: serde_json::Value = serde_json::from_str(lines.trim()).unwrap();
        assert_eq!(line["result"], "failed");
        assert_eq!(line["session"], serde_json::Value::Null);
        assert_eq!(line["session_use"], "rebuild", "FR-056 and FR-021");
    }
    #[tokio::test]
    async fn without_a_runtime_a_request_gets_no_model_name_error() {
        let folder =
            std::env::temp_dir().join(format!("acp2api-no-runtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
        let (conductor, _task) = start_without_runtime(&clock, &journal, &folder);
        let body = br#"{"model":"gemini-3.8-flash","reasoning_effort":"extreme","messages":[{"role":"user","content":"Hi"}]}"#;
        let turn = crate::dialect::CHAT.parse(body).ok().unwrap().turn;
        let request = journal.begin(crate::journal::Api::Chat, clock.now());
        let admitted = conductor
            .admit(
                turn,
                clock.hold(HoldKind::HttpRequest),
                request,
                ResponseMode::Body,
            )
            .await;
        let end = match admitted {
            Ok(Ticket { mut events, hold }) => {
                drop(hold);
                loop {
                    if let (TurnEvent::End(end), _) = events.recv().await.unwrap() {
                        break end.err().map(|message| message.to_string());
                    }
                }
            }
            Err(refusal) => Some(refusal.message.to_string()),
        };
        let listed = conductor
            .model_names(clock.hold(HoldKind::HttpRequest))
            .await
            .ok()
            .map(|entries| entries.len());
        let lines: String = std::fs::read_dir(folder.join("logs"))
            .unwrap()
            .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect();
        std::fs::remove_dir_all(&folder).unwrap();
        let unavailable = Some("official runtime is not available".to_owned());
        assert_eq!(end, unavailable, "answer to a request");
        assert_eq!(listed, Some(0), "GET /v1/models lists no model");
        assert!(
            lines.contains(r#""result":"failed""#),
            "request log control: {lines}"
        );
        assert!(
            !lines.contains(r#""event":"unknown_effort""#),
            "61ee383 emits no effort diagnostic without a catalog: {lines}"
        );
    }
    #[tokio::test]
    async fn a_client_gone_before_admission_still_has_its_request_line() {
        let folder = std::env::temp_dir().join(format!("acp2api-conductor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
        let (conductor, _task) = start_without_runtime(&clock, &journal, &folder);
        let body = br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#;
        let turn = || crate::dialect::CHAT.parse(body).ok().unwrap().turn;
        let request = || journal.begin(crate::journal::Api::Chat, clock.now());
        let (admission, reply) = oneshot::channel();
        drop(reply);
        conductor
            .send
            .send(Input::Turn(Box::new(TurnWork {
                pending: PendingRequest {
                    mode: ResponseMode::Body,
                    turn: turn(),
                    request: request(),
                    admission,
                    connection: mpsc::unbounded_channel().0,
                },
                hold: clock.hold(HoldKind::HttpRequest),
            })))
            .await
            .ok()
            .unwrap();
        let next = conductor.admit(
            turn(),
            clock.hold(HoldKind::HttpRequest),
            request(),
            ResponseMode::Body,
        );
        assert!(next.await.is_ok(), "the conductor serves the next request");
        let lines: String = std::fs::read_dir(folder.join("logs"))
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(
            lines.contains(r#""result":"disconnected""#),
            "request line of the gone client: {lines}"
        );
    }
    #[test]
    fn a_disconnected_sign_in_check_keeps_its_place_until_its_acp_reply() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-check-disconnect-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.work.clear();
        core.sign_in_required = true;
        core.limit = 1;
        let mut replies = Vec::new();
        let mut leases = Vec::new();
        let mut ids = Vec::new();
        for text in ["A", "B"] {
            let body = serde_json::json!({"model":"m","messages":[{"role":"user","content":text}]})
                .to_string();
            let turn = crate::dialect::CHAT
                .parse(body.as_bytes())
                .ok()
                .unwrap()
                .turn;
            let request = journal.begin(crate::journal::Api::Chat, 0);
            ids.push(request.id.clone());
            let (admission, reply) = oneshot::channel();
            replies.push(reply);
            let (connection, lease) = mpsc::unbounded_channel();
            leases.push(lease);
            core.handle(
                Event::CheckPending(Box::new(PendingRequest {
                    mode: ResponseMode::Body,
                    turn,
                    request,
                    admission,
                    connection,
                })),
                0,
                std::time::UNIX_EPOCH,
            );
        }
        let commands = core.handle(
            Event::Disconnected(ids[0].clone()),
            0,
            std::time::UNIX_EPOCH,
        );
        assert!(
            matches!(
                core.work[&core.by_request(&ids[1]).unwrap()],
                Work::CheckQueued(_)
            ),
            "FR-049 the next check waits for the disconnected call reply"
        );
        assert!(
            core.by_request(&ids[0])
                .is_some_and(|id| matches!(core.work[&id], Work::Retiring(_))),
            "the disconnected check retains its occupied entry"
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Prepare { work, .. } if work == &ids[1])),
            "FR-049 disconnect does not release outstanding ACP work"
        );
        let error = crate::messages::Message::UpstreamAcp(
            serde_json::from_value(
                serde_json::json!({"code":-32000,"message":"Authentication required"}),
            )
            .unwrap(),
        );
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: ids[0].clone(),
                result: Err(error),
            }),
            1,
            std::time::UNIX_EPOCH,
        );
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::Prepare { work, .. } if work == &ids[1])),
            "FR-049 ACP completion releases the place"
        );
        assert!(commands.iter().any(|command| matches!(command, Command::Complete(done) if done.request.id == ids[0] && done.session_use.is_none())), "FR-031 the pending request retains its journal completion");
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_successful_sign_in_ends_only_its_own_condition() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-conditions-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.work.clear();
        core.start_quota(
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(120),
            0,
            std::time::UNIX_EPOCH,
            &mut Vec::new(),
        );
        core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
        core.handle(
            Event::RuntimeReady {
                sign_in_required: false,
            },
            30_000,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            core.state(),
            "Quota wait",
            "FR-038 a successful check clears sign-in required during the owner's wait"
        );
        assert_eq!(core.sign_in_wait.as_ref().unwrap().deadline, 600_000);
        core.handle(
            Event::Runtime(RuntimeEvent::Authenticated(Ok(()))),
            30_000,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            core.state(),
            "Quota wait",
            "FR-038 sign-in preserves the unexpired quota wait"
        );
        assert_eq!(core.next_deadline(), Some(120_000));
        core.handle(Event::StartSignIn, 40_000, std::time::UNIX_EPOCH);
        core.handle(
            Event::RuntimeUnavailable(UnavailableReason {
                message: "runtime missing".into(),
                cause: None,
            }),
            40_000,
            std::time::UNIX_EPOCH,
        );
        core.handle(
            Event::Runtime(RuntimeEvent::Authenticated(Ok(()))),
            40_000,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            core.state(),
            "Runtime unavailable",
            "sign-in cannot clear runtime unavailability"
        );
        assert_eq!(core.quota_wait.as_ref().unwrap().deadline, 120_000);
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_sign_in_required_replacement_refuses_the_queue_before_it_can_dispatch() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-replacement-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::RuntimeFailure, 0, std::time::UNIX_EPOCH);
        let commands = core.handle(
            Event::RuntimeReady {
                sign_in_required: true,
            },
            1,
            std::time::UNIX_EPOCH,
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Prepare { .. } | Command::Prompt { .. })),
            "FR-038 a signed-out runtime starts no queued request"
        );
        assert!(commands.iter().any(|command| matches!(command, Command::Complete(done) if done.request.id == queued && matches!(done.outcome, crate::journal::Outcome::Refused("sign_in_required")))), "FR-037 the queued request gets the condition that started");
        assert_eq!(core.state(), "Sign-in required");
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn retirement_and_unavailability_end_the_sign_in_wait_in_either_order() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-retirement-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for unavailable in [false, true] {
            for wait_ends_first in [false, true] {
                for deadline in [false, true] {
                    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
                    core.work.clear();
                    core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
                    let end = |core: &mut Core| {
                        if deadline {
                            core.handle(Event::Deadline, 600_000, std::time::UNIX_EPOCH);
                        } else if core.sign_in_wait.is_some() {
                            core.handle(Event::EndSignInWait, 1, std::time::UNIX_EPOCH);
                        }
                    };
                    if wait_ends_first {
                        end(&mut core);
                    }
                    if unavailable {
                        core.handle(
                            Event::RuntimeUnavailable(UnavailableReason {
                                message: "runtime missing".into(),
                                cause: None,
                            }),
                            1,
                            std::time::UNIX_EPOCH,
                        );
                    } else {
                        core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
                        core.replacement = Some(Replacement::Build {
                            reply: oneshot::channel().0,
                            source: None,
                            initial: false,
                        });
                    }
                    if !wait_ends_first {
                        end(&mut core);
                    }
                    assert!(
                        core.sign_in_wait.is_none(),
                        "retirement or unavailability ends the sign-in wait"
                    );
                    if unavailable {
                        assert!(
                            matches!(core.runtime_state, RuntimeState::Unavailable(_)),
                            "FR-040 a wait cannot restart an unavailable runtime"
                        );
                        assert!(
                            core.replacement.is_none(),
                            "FR-040 no automatic replacement remains"
                        );
                    } else {
                        assert!(
                            matches!(core.runtime_state, RuntimeState::Retiring { .. }),
                            "a wait cannot overwrite the build retirement"
                        );
                        assert!(
                            core.replacement_ready(),
                            "the existing build replacement can proceed"
                        );
                        assert!(matches!(core.replacement, Some(Replacement::Build { .. })));
                    }
                }
            }
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_sign_in_replacement_waits_for_running_prompt_turns() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-active-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
        let commands = core.handle(Event::EndSignInWait, 1, std::time::UNIX_EPOCH);
        assert!(
            core.replacement.is_some(),
            "FR-039 sign-in uses the ordinary replacement path"
        );
        assert!(
            !core.replacement_ready(),
            "FR-037 the running prompt retains its runtime"
        );
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, Command::Cancel(_) | Command::Emit(_, TurnEvent::End(_))))
        );
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded { stop_reason: None },
            }),
            2,
            std::time::UNIX_EPOCH,
        );
        let commands = core.handle(Event::Deadline, 202, std::time::UNIX_EPOCH);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, Command::Emit(_, TurnEvent::End(Ok(_))))),
            "FR-037 the running prompt ends normally"
        );
        assert!(
            core.replacement_ready(),
            "FR-039 the replacement proceeds after the prompt ends"
        );
        assert!(
            core.failures.is_empty(),
            "FR-039 planned replacement is not a runtime failure"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn aborting_retirement_replaces_a_runtime_with_an_ended_sign_in_wait() {
        let mut core = Core::fixture(1);
        core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
        core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
        assert!(
            core.sign_in_wait.is_none(),
            "retirement ends the owner's wait"
        );
        core.handle(Event::ResumeRuntime, 2, std::time::UNIX_EPOCH);
        assert!(
            matches!(core.replacement, Some(Replacement::SignIn)),
            "FR-039 an aborted retirement must still stop the runtime's authenticate call"
        );
        assert!(
            core.replacement_ready(),
            "the planned replacement can proceed"
        );
        assert!(
            core.failures.is_empty(),
            "FR-039 replacement is not a runtime failure"
        );
    }
    #[test]
    fn an_elapsed_quota_reset_completes_a_pending_sign_in_check() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-elapsed-quota-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let wall = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
        let journal = crate::journal::Journal::open(&folder, wall).unwrap();
        for (reset, retry_after) in [
            ("1970-01-01T00:00:00Z", 3600),
            ("1970-01-01T00:00:01Z", 0),
            ("1970-01-01T00:00:01.000500Z", 0),
        ] {
            let mut core = Core::fixture(2);
            core.sign_in_required = true;
            let mut checks = Vec::new();
            for _ in 0..2 {
                let turn = crate::dialect::CHAT
                    .parse(br#"{"model":"m","messages":[{"role":"user","content":"Check"}]}"#)
                    .ok()
                    .unwrap()
                    .turn;
                let request = journal.begin(crate::journal::Api::Chat, 0);
                checks.push(request.id.clone());
                let (admission, _reply) = oneshot::channel();
                let (connection, _lease) = mpsc::unbounded_channel();
                core.handle(
                    Event::CheckPending(Box::new(PendingRequest {
                        mode: ResponseMode::Body,
                        turn,
                        request,
                        admission,
                        connection,
                    })),
                    0,
                    wall,
                );
            }
            let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
            let catalog = crate::catalog::Catalog::decode(&options, wall).unwrap();
            core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work: checks[0].clone(),
                    result: Ok(("checked".into(), catalog)),
                }),
                1,
                wall,
            );
            assert!(
                !core.sign_in_required,
                "the other check cleared sign-in required"
            );
            let error = crate::messages::AcpError {
                code: 429,
                message: format!("RESOURCE_EXHAUSTED; resets {reset}"),
                data: None,
            };
            let commands = core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work: checks[1].clone(),
                    result: Err(crate::messages::Message::UpstreamAcp(error)),
                }),
                1,
                wall,
            );
            assert!(
                commands.iter().any(|command| matches!(
                    command,
                    Command::RefusePending {
                        error: crate::messages::Message::QuotaWait { retry_after: actual, .. },
                        ..
                    } if *actual == retry_after
                )),
                "FR-036 reset {reset} completes the failed request with 429 (past reset uses the fallback)"
            );
            assert!(
                core.by_request(&checks[1]).is_none(),
                "the failed check releases its place"
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_quota_signal_without_a_reset_in_a_sign_in_check_uses_the_fallback() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-quota-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for disconnected in [false, true] {
            let (mut core, _, _tickets) = prompting_with_waiter(&journal);
            core.work.clear();
            core.sign_in_required = true;
            let turn = crate::dialect::CHAT
                .parse(br#"{"model":"m","messages":[{"role":"user","content":"Check"}]}"#)
                .ok()
                .unwrap()
                .turn;
            let request = journal.begin(crate::journal::Api::Chat, 0);
            let work = request.id.clone();
            let (admission, _reply) = oneshot::channel();
            let (connection, _lease) = mpsc::unbounded_channel();
            core.handle(
                Event::CheckPending(Box::new(PendingRequest {
                    mode: ResponseMode::Body,
                    turn,
                    request,
                    admission,
                    connection,
                })),
                0,
                std::time::UNIX_EPOCH,
            );
            if disconnected {
                core.handle(Event::Disconnected(work.clone()), 0, std::time::UNIX_EPOCH);
            }
            let commands = core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work: work.clone(),
                    result: Err(crate::messages::Message::UpstreamAcp(
                        serde_json::from_value(
                            serde_json::json!({"code":429,"message":"RESOURCE_EXHAUSTED"}),
                        )
                        .unwrap(),
                    )),
                }),
                0,
                std::time::UNIX_EPOCH,
            );
            assert_eq!(
                core.quota_wait.as_ref().unwrap().deadline,
                3_600_000,
                "FR-036 no reset time uses the 60-minute fallback"
            );
            assert!(
                core.work
                    .values()
                    .any(|w| matches!(w, Work::QuotaQueued { .. })),
                "FR-036 the reset query uses the shared queue"
            );
            assert!(
                core.by_request(&work).is_none(),
                "the completed check releases its place"
            );
            if disconnected {
                assert!(commands.iter().any(|c| matches!(c, Command::Complete(done) if done.request.id == work && matches!(done.outcome, crate::journal::Outcome::Disconnected))));
            } else {
                assert!(
                    commands.iter().any(|c| matches!(
                        c,
                        Command::RefusePending {
                            error: crate::messages::Message::SignInRequired(_),
                            ..
                        }
                    )),
                    "FR-037 the higher-priority sign-in condition selects the error"
                );
            }
        }
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn starting_a_sign_in_wait_keeps_the_normal_state_and_queue() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-normal-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
        let commands = core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
        assert_eq!(
            core.state(),
            "Running",
            "FR-039 the wait alone does not require sign-in"
        );
        assert!(
            core.by_request(&queued).is_some(),
            "FR-037 starting a wait does not flush the queue"
        );
        assert!(!commands.iter().any(|c| matches!(
            c,
            Command::Emit(_, TurnEvent::End(_)) | Command::RefusePending { .. }
        )));
        assert!(
            core.refusal(0).is_none(),
            "the wait alone does not refuse admission"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_successful_request_check_allows_admission_during_the_owners_wait() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-success-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.work.clear();
        core.sign_in_required = true;
        core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"m","messages":[{"role":"user","content":"Check"}]}"#)
            .ok()
            .unwrap()
            .turn;
        let request = journal.begin(crate::journal::Api::Chat, 0);
        let work = request.id.clone();
        let (admission, _reply) = oneshot::channel();
        let (connection, _lease) = mpsc::unbounded_channel();
        core.handle(
            Event::CheckPending(Box::new(PendingRequest {
                mode: ResponseMode::Body,
                turn,
                request,
                admission,
                connection,
            })),
            0,
            std::time::UNIX_EPOCH,
        );
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work,
                result: Ok(("checked".into(), catalog)),
            }),
            1,
            std::time::UNIX_EPOCH,
        );
        assert!(
            core.refusal(1).is_none(),
            "FR-038 the successful request check clears sign-in required"
        );
        assert!(
            core.sign_in_wait.is_some(),
            "the owner still has its independent wait"
        );
        assert!(
            commands.iter().any(
                |c| matches!(c, Command::AdmitChecked { session, .. } if session == "checked")
            )
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn the_owners_wait_shows_the_latest_sign_in_address() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-sign-in-address-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.work.clear();
        core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
        for address in [
            "https://accounts.example.com/first",
            "https://accounts.example.com/latest",
        ] {
            core.handle(
                Event::Runtime(RuntimeEvent::SignInAddress(address.into())),
                1,
                std::time::UNIX_EPOCH,
            );
        }
        assert_eq!(
            core.sign_in_wait.as_ref().unwrap().address.as_deref(),
            Some("https://accounts.example.com/latest")
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn a_sign_in_error_from_authenticate_starts_the_required_condition() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-authentication-error-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        for code in [401, 500] {
            let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
            core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
            let error = crate::messages::Message::UpstreamAcp(
                serde_json::from_value(
                    serde_json::json!({"code":code,"message":"Authentication failed"}),
                )
                .unwrap(),
            );
            let commands = core.handle(
                Event::Runtime(RuntimeEvent::Authenticated(Err(error))),
                1,
                std::time::UNIX_EPOCH,
            );
            assert_eq!(
                core.sign_in_required,
                code == 401,
                "FR-038 the authentication error selects sign-in required"
            );
            assert_eq!(
                core.by_request(&queued).is_some(),
                code != 401,
                "FR-037 the queue ends only when a condition starts"
            );
            assert!(
                !commands
                    .iter()
                    .any(|command| matches!(command, Command::Cancel(_))),
                "FR-037 running turns are preserved"
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
    }
}
