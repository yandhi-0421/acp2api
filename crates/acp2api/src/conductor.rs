mod records;
use crate::{
    Clock, Hold, HoldKind, Moment,
    conversation::{EffortAsk, Prompt, PromptRequest, Ticket, TurnEvent, TurnRequest, Usage},
    runtime_driver::{Runtime, RuntimeEvent, TurnUpdate},
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
use tokio::sync::{mpsc, oneshot, watch};
#[derive(Clone)]
pub(crate) struct Conductor {
    send: mpsc::Sender<Input>,
    stopping: watch::Sender<bool>,
    settings: watch::Receiver<Arc<crate::store::Settings>>,
    notices: watch::Receiver<Arc<Vec<Notice>>>,
    pub(crate) highest: u8,
}
pub(crate) enum OwnerAction {
    NewLocalApiKey,
    SetPasswordHash(String),
    SetConcurrencyLimit(u8),
    DismissNotice(String),
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
struct BuildRequest {
    builds: Box<crate::runtime_builds::Builds>,
    action: crate::runtime_builds::Action,
    reply: oneshot::Sender<Result<String, String>>,
}
struct Replacement {
    builds: Box<crate::runtime_builds::Builds>,
    reply: oneshot::Sender<Result<String, String>>,
    source: Option<std::path::PathBuf>,
    notice: String,
}
enum Input {
    Builds {
        builds: Box<crate::runtime_builds::Builds>,
        action: crate::runtime_builds::Action,
        reply: oneshot::Sender<Result<String, String>>,
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
    turn: TurnRequest,
    request: crate::journal::Request,
    hold: Hold,
    admission: oneshot::Sender<Result<Ticket, crate::messages::Refusal>>,
}
pub(crate) const QUEUE_CAPACITY: usize = 32;
const UNAVAILABLE: &str = "official runtime is not available";
type Output = mpsc::UnboundedSender<(TurnEvent, Hold)>;
struct Accepted {
    turn: TurnRequest,
    model: crate::catalog::SelectedModel,
    request: crate::journal::Request,
    output: Output,
}
type WorkId = u64;
type WorkTable = BTreeMap<WorkId, Work>;
enum Work {
    AcceptedQueued(Accepted),
    AcceptedWorking(Active),
    Retiring(OutstandingWork),
}
struct OutstandingWork {
    request: crate::journal::Request,
    session: Option<String>,
    completion: Option<crate::journal::SessionUse>,
}
impl Work {
    fn request(&self) -> &crate::journal::Request {
        match self {
            Self::AcceptedQueued(request) => &request.request,
            Self::AcceptedWorking(active) => &active.request,
            Self::Retiring(outstanding) => &outstanding.request,
        }
    }
    fn execution(&self) -> Option<&Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_) | Self::Retiring(_) => None,
        }
    }
    fn execution_mut(&mut self) -> Option<&mut Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_) | Self::Retiring(_) => None,
        }
    }
    fn into_queued(self) -> Option<Accepted> {
        match self {
            Self::AcceptedQueued(request) => Some(request),
            Self::AcceptedWorking(_) | Self::Retiring(_) => None,
        }
    }
    fn into_active(self) -> Option<Active> {
        match self {
            Self::AcceptedWorking(active) => Some(active),
            Self::AcceptedQueued(_) | Self::Retiring(_) => None,
        }
    }
    fn occupies_place(&self) -> bool {
        matches!(self, Self::AcceptedWorking(_) | Self::Retiring(_))
    }
}
#[derive(Clone)]
struct Session {
    tools: Vec<crate::conversation::ClientTool>,
    system: Option<String>,
}
enum Phase {
    Preparing(PromptRequest),
    Prompt,
    Settling {
        at: Moment,
        can_continue: bool,
        result: Result<(), crate::messages::Message>,
    },
}
struct Active {
    output: Output,
    request: crate::journal::Request,
    session: Option<String>,
    hashes: crate::conversation::RecordHashes,
    tools_hash: crate::conversation::ToolSetHash,
    kind: crate::journal::SessionUse,
    used: u64,
    expires: Moment,
    held: Option<String>,
    text: String,
    thought_dropped: bool,
    input_tokens: u64,
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
    fn release_or_cut_reply(&mut self, next: Option<String>, commands: &mut Vec<Command>) {
        let complete = next.is_none();
        if !complete && self.stop.sequence().is_some() {
            return;
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
            commands.push(Command::Emit(self.output.clone(), TurnEvent::Text(text)));
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
            if let Some(session) = &self.session {
                commands.push(Command::Cancel(session.clone()));
            }
        }
    }
}
enum RuntimeState {
    Serving,
    Retiring,
}
struct Core {
    runtime_state: RuntimeState,
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
    RetireRuntime,
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
    },
    Prompt {
        work: String,
        session: String,
        prompt: PromptRequest,
    },
    Emit(Output, TurnEvent),
    Permission {
        request: crate::runtime_driver::PermissionRequest,
        cancel: bool,
    },
    Forget(String),
}
impl Core {
    fn replacement_ready(&self) -> bool {
        matches!(self.runtime_state, RuntimeState::Retiring)
            && !self.work.values().any(Work::occupies_place)
    }
    fn replaced(&mut self, session: Option<String>, catalog: Option<crate::catalog::Catalog>) {
        self.records.clear();
        self.startup_session = session;
        self.catalog = catalog;
    }

    fn by_session(&self, session: &str) -> Option<WorkId> {
        self.work.iter().find_map(|(id, work)| {
            let current = match work {
                Work::AcceptedQueued(_) => None,
                Work::AcceptedWorking(active) => active.session.as_deref(),
                Work::Retiring(outstanding) => outstanding.session.as_deref(),
            };
            (current == Some(session)).then_some(*id)
        })
    }
    fn by_request(&self, request: &str) -> Option<WorkId> {
        self.work
            .iter()
            .find_map(|(id, work)| (work.request().id == request).then_some(*id))
    }

    fn next_deadline(&self) -> Option<Moment> {
        self.work
            .values()
            .filter_map(Work::execution)
            .filter_map(|active| match active.phase {
                Phase::Settling { at, .. } => Some(at),
                _ => None,
            })
            .chain(self.records.iter().map(|record| record.expires))
            .min()
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
            |catalog| catalog.resolve(turn),
        )
    }
    fn model_entries(&self) -> Vec<crate::catalog::ModelEntry> {
        self.catalog
            .as_ref()
            .map(crate::catalog::Catalog::entries)
            .unwrap_or_default()
    }
    fn owner_action(
        &mut self,
        action: OwnerAction,
        settings: &watch::Sender<Arc<crate::store::Settings>>,
        configuration: &std::path::Path,
        clock: &dyn Clock,
    ) -> Result<Vec<Command>, String> {
        let mut changed = (**settings.borrow()).clone();
        match action {
            OwnerAction::NewLocalApiKey => changed.local_api_key = crate::store::random_key()?,
            OwnerAction::SetPasswordHash(hash) => changed.management_password_hash = hash,
            OwnerAction::SetConcurrencyLimit(limit) => changed.concurrency_limit = limit,
            OwnerAction::DismissNotice(id) => {
                return Ok(self.handle(Event::DismissNotice(id), clock.now()));
            }
        }
        crate::store::save(configuration, &changed)?;
        let limit = changed.concurrency_limit;
        settings.send_replace(Arc::new(changed));
        Ok(self.handle(Event::Limit(limit), clock.now()))
    }
    fn admit(
        &self,
        work: TurnWork,
        journal: &crate::journal::Journal,
        clock: &dyn Clock,
    ) -> Option<(Event, Hold)> {
        let (output, ticket) = mpsc::unbounded_channel();
        let mut checked = self.check_model(&work.turn);
        if self
            .work
            .values()
            .filter(|work| matches!(work, Work::AcceptedQueued(_)))
            .count()
            == QUEUE_CAPACITY
        {
            checked = checked.and(Err(crate::messages::Message::QueueFull.into()));
        }
        let (model, admitted) = match checked {
            Ok(model) => {
                if self.catalog.is_some()
                    && !model.level_from_name
                    && let EffortAsk::Unknown(value) = &work.turn.effort
                {
                    journal.diagnostic(
                        &work.request,
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
                (Some(model), work.admission.send(Ok(ticket)))
            }
            Err(mut error) => {
                error.hold = clock.hold(HoldKind::TurnEvent);
                (None, work.admission.send(Err(error)))
            }
        };
        if admitted.is_err() {
            journal.complete(
                crate::journal::Completion {
                    request: work.request,
                    session: None,
                    session_use: None,
                    outcome: crate::journal::Outcome::Disconnected,
                },
                clock,
            );
            return None;
        }
        let model = model?;
        let accepted = Accepted {
            turn: work.turn,
            model,
            request: work.request,
            output,
        };
        Some((Event::Accepted(Box::new(accepted)), work.hold))
    }
    fn handle(&mut self, event: Event, now: Moment) -> Vec<Command> {
        let mut commands = Vec::new();
        match event {
            Event::RetireRuntime => self.runtime_state = RuntimeState::Retiring,
            Event::ResumeRuntime => self.runtime_state = RuntimeState::Serving,
            Event::Accepted(request) => {
                self.work
                    .insert(self.next_work, Work::AcceptedQueued(*request));
                self.next_work += 1;
            }
            Event::DismissNotice(id) => {
                self.notices.send_if_modified(|notices| {
                    let count = notices.len();
                    Arc::make_mut(notices).retain(|notice| notice.id != id);
                    notices.len() != count
                });
            }
            Event::Limit(limit) => {
                self.limit = limit;
            }
            Event::Disconnected(request_id) => {
                let id = self.by_request(&request_id).unwrap();
                if matches!(self.work[&id], Work::AcceptedQueued(_)) {
                    let request = self.work.remove(&id).unwrap().into_queued().unwrap();
                    Self::drop_queued(request, &mut commands);
                } else {
                    let active = self.work.remove(&id).unwrap().into_active().unwrap();
                    let ended = matches!(active.phase, Phase::Settling { .. });
                    if let Some(session) = &active.session {
                        commands.push(Command::Cancel(session.clone()));
                    }
                    self.work.insert(
                        id,
                        Work::Retiring(OutstandingWork {
                            request: active.request,
                            session: active.session,
                            completion: Some(active.kind),
                        }),
                    );
                    if ended {
                        self.finish(id, Ok(()), &mut commands);
                    }
                }
            }
            Event::StartFailed(work, error) => {
                if let Some(id) = self.by_request(&work) {
                    self.finish(
                        id,
                        Err(crate::messages::Message::Upstream(error)),
                        &mut commands,
                    );
                }
            }
            Event::Runtime(RuntimeEvent::Prepared { work, result }) => {
                if let Some(id) = self.by_request(&work) {
                    match result {
                        Ok((session, catalog)) => {
                            self.catalog = Some(catalog);
                            match self.work.get_mut(&id).unwrap() {
                                Work::Retiring(outstanding) => {
                                    outstanding.session = Some(session);
                                    self.finish(id, Ok(()), &mut commands);
                                }
                                entry => {
                                    let active = entry.execution_mut().unwrap();
                                    active.session = Some(session.clone());
                                    if let Phase::Preparing(prompt) =
                                        std::mem::replace(&mut active.phase, Phase::Prompt)
                                    {
                                        commands.push(Command::Prompt {
                                            work,
                                            session,
                                            prompt,
                                        });
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            self.finish(
                                id,
                                Err(crate::messages::Message::Upstream(error)),
                                &mut commands,
                            );
                        }
                    }
                }
            }
            Event::Runtime(RuntimeEvent::Retired { session }) => {
                if let Some(id) = self.by_session(&session) {
                    self.finish(id, Ok(()), &mut commands);
                }
            }
            Event::Runtime(RuntimeEvent::Turn { session, update }) => {
                if let Some(id) = self.by_session(&session) {
                    self.update(id, session, update, now, &mut commands);
                } else if let TurnUpdate::Permission { request, .. }
                | TurnUpdate::RefusedPermission { request, .. } = update
                {
                    commands.push(Command::Permission {
                        request,
                        cancel: false,
                    });
                }
            }
            Event::Runtime(RuntimeEvent::UpstreamError { session, error }) => {
                let id = self.by_session(&session);
                commands.push(Command::UpstreamLog {
                    session: session.clone(),
                    error: error.clone(),
                    late: id.is_none(),
                });
                if let Some(id) = id {
                    if !matches!(self.work[&id], Work::Retiring(_)) {
                        self.retire(
                            id,
                            session,
                            crate::messages::Message::UpstreamHttp(error),
                            &mut commands,
                        );
                    }
                } else {
                    self.records.retain(|record| record.session != session);
                    if self.startup_session.as_ref() == Some(&session) {
                        self.startup_session = None;
                    }
                    commands.push(Command::Forget(session));
                }
            }
            Event::Runtime(RuntimeEvent::Failed(error)) => {
                self.forget_idle(&mut commands);
                while let Some(id) = self
                    .work
                    .iter()
                    .find_map(|(id, work)| work.occupies_place().then_some(*id))
                {
                    self.finish(
                        id,
                        Err(crate::messages::Message::Upstream(error.clone())),
                        &mut commands,
                    );
                }
            }
            Event::Runtime(RuntimeEvent::Closed) => {
                self.catalog = None;
                self.forget_idle(&mut commands);
                while let Some(id) = self
                    .work
                    .iter()
                    .find_map(|(id, work)| work.occupies_place().then_some(*id))
                {
                    self.finish(
                        id,
                        Err(crate::messages::Message::Upstream(
                            "runtime stdout closed".into(),
                        )),
                        &mut commands,
                    );
                }
            }
            Event::Runtime(RuntimeEvent::Stderr | RuntimeEvent::Other) => {}
            Event::Deadline => {
                let due: Vec<_> = self
                    .work
                    .iter()
                    .filter_map(|(id, work)| {
                        work.execution().and_then(|active| {
                            matches!(active.phase, Phase::Settling { at, .. } if at <= now)
                                .then_some(*id)
                        })
                    })
                    .collect();
                for id in due {
                    self.finish(id, Ok(()), &mut commands);
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
        while matches!(self.runtime_state, RuntimeState::Serving)
            && places < usize::from(self.limit)
        {
            let Some(id) = self
                .work
                .iter()
                .find_map(|(id, work)| matches!(work, Work::AcceptedQueued(_)).then_some(*id))
            else {
                break;
            };
            let request = self.work.remove(&id).unwrap().into_queued().unwrap();
            if request.output.is_closed() {
                Self::drop_queued(request, &mut commands);
                continue;
            }
            let settings = Session {
                system: crate::conversation::system_prompt_addition(
                    &request.turn.conversation.system,
                ),
                tools: match request.turn.tool_choice {
                    crate::conversation::ToolChoice::Off => Vec::new(),
                    crate::conversation::ToolChoice::Auto => request.turn.tools,
                },
            };
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
            let ready = match matched {
                Some((index, _)) => Some(self.records.remove(index).session),
                None if settings.system.is_none() && settings.tools.is_empty() => {
                    self.startup_session.take()
                }
                None => None,
            };
            self.use_sequence += 1;
            let work = request.request.id.clone();
            let (session, phase) = match ready {
                Some(id) => {
                    commands.push(Command::Prompt {
                        work: work.clone(),
                        session: id.clone(),
                        prompt,
                    });
                    (Some(id), Phase::Prompt)
                }
                None => {
                    commands.push(Command::Prepare {
                        work,
                        settings: settings.clone(),
                    });
                    (None, Phase::Preparing(prompt))
                }
            };
            self.work.insert(
                id,
                Work::AcceptedWorking(Active {
                    output: request.output,
                    request: request.request,
                    session,
                    hashes,
                    tools_hash,
                    kind,
                    used: self.use_sequence,
                    expires: now.saturating_add(24 * 60 * 60 * 1000),
                    held: None,
                    text: String::new(),
                    thought_dropped: false,
                    input_tokens: request.turn.input_tokens,
                    phase,
                    tool_call: None,
                    stop: crate::conversation::Stop::EndTurn,
                    stops: request.turn.stop,
                    stop_prefix: String::new(),
                }),
            );
            places += 1;
        }
        commands
    }
    fn update(
        &mut self,
        id: WorkId,
        session: String,
        update: TurnUpdate,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        if matches!(self.work[&id], Work::Retiring(_)) {
            match update {
                TurnUpdate::PromptEnded { .. } | TurnUpdate::Failed(_) => {
                    self.finish(id, Ok(()), commands);
                }
                TurnUpdate::Permission { request, .. }
                | TurnUpdate::RefusedPermission { request, .. } => {
                    commands.push(Command::Permission {
                        request,
                        cancel: false,
                    });
                }
                TurnUpdate::Text(_)
                | TurnUpdate::Usage(_)
                | TurnUpdate::ThoughtDropped
                | TurnUpdate::UnpermittedTool(_) => {}
            }
            return;
        }
        let active = self.work.get_mut(&id).unwrap().execution_mut().unwrap();
        match update {
            TurnUpdate::Text(text) => {
                if active.tool_call.is_none() {
                    active.release_or_cut_reply(Some(text), commands);
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
                use agent_client_protocol_schema::v1::StopReason;
                if stop_reason == Some(StopReason::MaxTokens) && active.stop.sequence().is_none() {
                    active.stop = crate::conversation::Stop::MaxTokens;
                }
                active.phase = Phase::Settling {
                    at: now.saturating_add(200),
                    result: Ok(()),
                    can_continue: stop_reason == Some(StopReason::EndTurn)
                        || stop_reason == Some(StopReason::MaxTokens)
                        || stop_reason == Some(StopReason::Cancelled) && active.tool_call.is_some(),
                };
            }
            TurnUpdate::Failed(error) => {
                active.phase = Phase::Settling {
                    at: now.saturating_add(200),
                    can_continue: false,
                    result: Err(error),
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
                self.retire(
                    id,
                    session,
                    crate::messages::Message::BuiltInToolRan(name),
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
                commands.push(Command::Permission {
                    request,
                    cancel: false,
                });
            }
            TurnUpdate::Permission { request, call } => {
                let cancel = matches!(active.phase, Phase::Prompt) && active.tool_call.is_none();
                if cancel {
                    active.tool_call = Some(call);
                }
                commands.push(Command::Permission { request, cancel });
            }
        }
    }
    fn forget_idle(&mut self, commands: &mut Vec<Command>) {
        let idle = self.records.drain(..).map(|record| record.session);
        commands.extend(idle.chain(self.startup_session.take()).map(Command::Forget));
    }
    fn drop_queued(request: Accepted, commands: &mut Vec<Command>) {
        commands.push(Command::Complete(crate::journal::Completion {
            request: request.request,
            session: None,
            session_use: None,
            outcome: crate::journal::Outcome::Disconnected,
        }));
    }
    fn retire(
        &mut self,
        id: WorkId,
        session: String,
        error: crate::messages::Message,
        commands: &mut Vec<Command>,
    ) {
        let active = self.work.remove(&id).unwrap().into_active().unwrap();
        let ended = matches!(active.phase, Phase::Settling { .. });
        if !ended {
            commands.push(Command::Cancel(session));
        }
        let outstanding = OutstandingWork {
            request: active.request.clone(),
            session: active.session.clone(),
            completion: None,
        };
        let forget = self.complete_request(active, Err(error), commands);
        if ended {
            commands.extend(forget.map(Command::Forget));
        } else {
            self.work.insert(id, Work::Retiring(outstanding));
        }
    }
    fn finish(
        &mut self,
        id: WorkId,
        result: Result<(), crate::messages::Message>,
        commands: &mut Vec<Command>,
    ) {
        match self.work.remove(&id).unwrap() {
            Work::Retiring(outstanding) => {
                if let Some(session) = &outstanding.session {
                    commands.push(Command::Forget(session.clone()));
                }
                if let Some(kind) = outstanding.completion {
                    commands.push(Command::Complete(crate::journal::Completion {
                        request: outstanding.request,
                        session: outstanding.session,
                        session_use: Some(kind),
                        outcome: crate::journal::Outcome::Disconnected,
                    }));
                }
            }
            work => {
                let active = work.into_active().unwrap();
                let forget = self.complete_request(active, result, commands);
                commands.extend(forget.map(Command::Forget));
            }
        }
    }
    fn complete_request(
        &mut self,
        mut active: Active,
        result: Result<(), crate::messages::Message>,
        commands: &mut Vec<Command>,
    ) -> Option<String> {
        let (can_continue, result) = match std::mem::replace(&mut active.phase, Phase::Prompt) {
            Phase::Settling {
                can_continue,
                result: pending,
                ..
            } => (can_continue, result.and(pending)),
            Phase::Preparing(_) | Phase::Prompt => (false, result),
        };
        let tool_hash = active
            .tool_call
            .as_ref()
            .map(|call| crate::conversation::tool_call_hash(&call.name, &call.arguments));
        if result.is_err() {
            active.held = None;
        }
        active.release_or_cut_reply(None, commands);
        let can_continue = can_continue && active.stop.sequence().is_none();
        let end = result.map(|()| {
            let mut output = active.text.clone();
            if let Some(call) = active.tool_call.take() {
                output.push_str(&call.arguments);
                commands.push(Command::Emit(
                    active.output.clone(),
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
                active.output.clone(),
                TurnEvent::Stop(active.stop),
            ));
        }
        commands.push(Command::Emit(active.output, TurnEvent::End(end)));
        forget
    }
}
impl Conductor {
    pub(crate) fn start(
        started: Option<(Runtime, crate::catalog::Catalog)>,
        clock: Arc<dyn Clock>,
        journal: crate::journal::Journal,
        settings: crate::store::Settings,
        configuration: std::path::PathBuf,
        highest: u8,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (send, mut recv) = mpsc::channel::<Input>(32);
        let (stopping, mut stopped) = watch::channel(false);
        let runtime_stop = stopping.clone();
        let limit = settings.concurrency_limit;
        let (publish, settings_watch) = watch::channel(Arc::new(settings));
        let (notices, notices_watch) = watch::channel(Arc::new(Vec::new()));
        let task = tokio::spawn(async move {
            let (mut runtime, catalog) = started.unzip();
            let mut core = Core {
                runtime_state: RuntimeState::Serving,
                startup_session: runtime.as_ref().map(|runtime| runtime.session_id().into()),
                catalog,
                records: Vec::new(),
                use_sequence: 0,
                limit,
                work: BTreeMap::new(),
                next_work: 0,
                notices,
            };
            let mut read_open = runtime.is_some();
            let mut unavailable = None::<String>;
            let mut changes = VecDeque::<BuildRequest>::new();
            let mut replacement = None::<Replacement>;
            loop {
                if *stopped.borrow() {
                    break;
                }
                if core.replacement_ready()
                    && let Some(change) = replacement.take()
                {
                    if let Some(old) = runtime.take() {
                        old.stop().await;
                    }
                    core.replaced(None, None);
                    match change.builds.launch(change.source).await {
                        Ok((started, catalog)) => {
                            core.replaced(Some(started.session_id().into()), Some(catalog));
                            runtime = Some(started);
                            unavailable = None;
                        }
                        Err(_) => unavailable = Some(change.builds.unavailable()),
                    }
                    read_open = runtime.is_some();
                    let result = change
                        .builds
                        .after_start(&publish.borrow())
                        .map(|()| change.notice);
                    let _ = change.reply.send(result);
                }
                if replacement.is_none() {
                    while let Some(request) = changes.pop_front() {
                        let mut changed = (**publish.borrow()).clone();
                        match request
                            .builds
                            .select(request.action, &mut changed, &configuration)
                        {
                            Err(error) => {
                                let _ = request.reply.send(Err(error));
                            }
                            Ok((selection, notice)) => {
                                publish.send_replace(Arc::new(changed));
                                match selection {
                                    crate::runtime_builds::Selection::Keep => {
                                        let result = request
                                            .builds
                                            .after_start(&publish.borrow())
                                            .map(|()| notice);
                                        let _ = request.reply.send(result);
                                    }
                                    crate::runtime_builds::Selection::Start(source) => {
                                        let mut commands =
                                            core.handle(Event::RetireRuntime, clock.now()).into();
                                        Self::commands(
                                            &mut core,
                                            &mut runtime,
                                            &mut read_open,
                                            &clock,
                                            &journal,
                                            &mut commands,
                                        )
                                        .await;
                                        replacement = Some(Replacement {
                                            builds: request.builds,
                                            reply: request.reply,
                                            source,
                                            notice,
                                        });
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if replacement.is_some() && core.replacement_ready() {
                        continue;
                    }
                    if replacement.is_none() && matches!(core.runtime_state, RuntimeState::Retiring)
                    {
                        let mut commands = core.handle(Event::ResumeRuntime, clock.now()).into();
                        Self::commands(
                            &mut core,
                            &mut runtime,
                            &mut read_open,
                            &clock,
                            &journal,
                            &mut commands,
                        )
                        .await;
                    }
                }
                let connected: Vec<_> = core
                    .work
                    .values()
                    .filter_map(|work| match work {
                        Work::AcceptedWorking(active) => {
                            Some((active.request.id.clone(), active.output.clone()))
                        }
                        Work::AcceptedQueued(request) => {
                            Some((request.request.id.clone(), request.output.clone()))
                        }
                        Work::Retiring(_) => None,
                    })
                    .collect();
                let timer = core.next_deadline().map(|at| clock.sleep_until(at));
                let (event, hold) = tokio::select! {
                    biased;
                    Ok(()) = async { stopped.wait_for(|stopped| *stopped).await.map(|_| ()) } => break,
                    work = first_closed(&connected), if !connected.is_empty() => {
                        (Event::Disconnected(work), clock.hold(HoldKind::TurnEvent))
                    }
                    work = recv.recv() => {
                        let Some(work) = work else {
                            break;
                        };
                        match work {
                            Input::Builds { mut builds, action, reply, hold } => {
                                builds.start.stopped = runtime_stop.subscribe();
                                changes.push_back(BuildRequest { builds, action, reply });
                                drop(hold);
                                continue;
                            }
                            Input::Models { reply, hold } => {
                                let result = match &unavailable {
                                    Some(message) => {
                                        let mut error: crate::messages::Refusal = crate::messages::Message::Unavailable(message.clone()).into();
                                        error.hold = clock.hold(HoldKind::TurnEvent);
                                        Err(error)
                                    }
                                    None => Ok(core.model_entries()),
                                };
                                let _ = reply.send(result);
                                drop(hold);
                                continue;
                            }
                            Input::Owner { action, reply, hold } => {
                                let result = core.owner_action(
                                    action,
                                    &publish,
                                    &configuration,
                                    clock.as_ref(),
                                );
                                let (result, mut commands) = match result {
                                    Ok(commands) => (Ok(()), commands.into()),
                                    Err(error) => (Err(error), VecDeque::new()),
                                };
                                let _ = reply.send(result);
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
                                if let Some(message) = &unavailable {
                                    let mut error: crate::messages::Refusal = crate::messages::Message::Unavailable(message.clone()).into();
                                    error.hold = clock.hold(HoldKind::TurnEvent);
                                    if work.admission.send(Err(error)).is_err() {
                                        journal.complete(crate::journal::Completion { request: work.request, session: None, session_use: None, outcome: crate::journal::Outcome::Disconnected }, clock.as_ref());
                                    }
                                    continue;
                                }
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
                let mut commands: VecDeque<_> = core.handle(event, clock.now()).into();
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
            }
            if let Some(runtime) = runtime {
                runtime.stop().await;
            }
        });
        (
            Self {
                send,
                stopping,
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
                Command::UpstreamLog {
                    session,
                    error,
                    late,
                } => journal.upstream(&session, &error, late, clock.as_ref()),
                Command::Cancel(session) => {
                    if let Some(runtime) = runtime.as_mut()
                        && let Err(error) = runtime.cancel(&session).await
                    {
                        commands.extend(
                            core.handle(Event::Runtime(RuntimeEvent::Failed(error)), clock.now()),
                        );
                    }
                }
                Command::Safety(report) => journal.safety(report, clock.as_ref()),
                Command::Diagnostic(request, diagnostic) => {
                    journal.diagnostic(&request, diagnostic, clock.as_ref())
                }
                Command::Complete(completion) => journal.complete(completion, clock.as_ref()),
                Command::Prepare { work, settings } => {
                    let result = match readable(runtime, *read_open) {
                        Ok(runtime) => {
                            runtime
                                .request_session(
                                    work.clone(),
                                    &settings.tools,
                                    settings.system.as_deref(),
                                )
                                .await
                        }
                        Err(error) => Err(error),
                    };
                    if let Err(error) = result {
                        commands.extend(core.handle(Event::StartFailed(work, error), clock.now()));
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
                        commands.extend(core.handle(Event::StartFailed(work, error), clock.now()));
                    }
                }
                Command::Permission { request, cancel } => {
                    if let Some(runtime) = runtime.as_mut()
                        && let Err(error) = runtime.permission(request, cancel).await
                    {
                        commands.extend(
                            core.handle(Event::Runtime(RuntimeEvent::Failed(error)), clock.now()),
                        );
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
    pub(crate) fn stop(&self) {
        self.stopping.send_replace(true);
    }
    pub(crate) fn notices(&self) -> Arc<Vec<Notice>> {
        self.notices.borrow().clone()
    }
    pub(crate) fn settings(&self) -> Arc<crate::store::Settings> {
        self.settings.borrow().clone()
    }
    pub(crate) async fn builds(
        &self,
        builds: crate::runtime_builds::Builds,
        action: crate::runtime_builds::Action,
        hold: Hold,
    ) -> Result<String, String> {
        let (reply, result) = oneshot::channel();
        let request = async {
            self.send
                .send(Input::Builds {
                    builds: Box::new(builds),
                    action,
                    reply,
                    hold,
                })
                .await
                .map_err(|_| "conductor stopped".to_owned())?;
            result.await.map_err(|_| "conductor stopped".to_owned())?
        }
        .await;
        if *self.stopping.borrow() {
            Ok(String::new())
        } else {
            request
        }
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
    ) -> Result<Ticket, crate::messages::Refusal> {
        let (admission, reply) = oneshot::channel();
        self.send
            .send(Input::Turn(Box::new(TurnWork {
                turn,
                request,
                hold,
                admission,
            })))
            .await
            .map_err(stopped)?;
        reply.await.map_err(stopped)?
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
async fn first_closed(outputs: &[(String, Output)]) -> String {
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
    crate::messages::Message::Upstream("conductor stopped".into()).into()
}
#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    fn prompting_with_waiter(
        journal: &crate::journal::Journal,
    ) -> (
        Core,
        String,
        [mpsc::UnboundedReceiver<(TurnEvent, Hold)>; 2],
    ) {
        let mut core = Core {
            runtime_state: RuntimeState::Serving,
            catalog: None,
            work: BTreeMap::new(),
            next_work: 0,
            records: Vec::new(),
            startup_session: Some("running".into()),
            use_sequence: 0,
            limit: 1,
            notices: watch::channel(Arc::new(Vec::new())).0,
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
                    output,
                })),
                0,
            );
        }
        (core, queued, tickets.try_into().ok().unwrap())
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
        struct Facts(Vec<ListedBuild>);
        impl ReleaseFacts for Facts {
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
            fn listed_builds(&self) -> &[ListedBuild] {
                &self.0
            }
            fn highest_concurrency_limit(&self) -> u8 {
                1
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
                for name in crate::runtime_builds::names() { std::fs::write(folder.join(name), label.as_bytes()).unwrap(); }
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
            let builds = crate::runtime_builds::Builds::new(&ports, environment, root.clone(), temporary, crate::runtime_driver::mcp::ToolSets::new(0), 0);
            let (runtime, catalog) = builds.launch(None).await.unwrap();
            let config = root.join("config.toml");
            let settings = crate::store::read(&config, 1).unwrap().settings;
            builds.after_start(&settings).unwrap();
            let journal = crate::journal::Journal::prepare().unwrap();
            journal.open_writer(&root.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = Conductor::start(Some((runtime, catalog)), ports.clock.clone(), journal.clone(), settings, config, 1);
            let parse = |system: &str| crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"system","content":system},{"role":"user","content":"request"}]})).unwrap()).ok().unwrap().turn;
            let mut a = conductor.admit(parse("A"), Hold::empty(), journal.begin(crate::journal::Api::Chat, 0)).await.ok().unwrap();
            prompting_a.notified().await;
            let (reply, owner) = oneshot::channel();
            conductor.send.send(Input::Builds { builds: Box::new(builds), action: crate::runtime_builds::Action::Build("1.5.0".into()), reply, hold: Hold::empty() }).await.unwrap();
            let mut b = conductor.admit(parse("B"), Hold::empty(), journal.begin(crate::journal::Api::Chat, 0)).await.ok().unwrap();
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
    fn a_build_change_finishes_a_before_preparing_b_on_the_new_runtime() {
        let journal = crate::journal::Journal::prepare().unwrap();
        let (mut core, queued, tickets) = prompting_with_waiter(&journal);
        let id = core.by_request(&queued).unwrap();
        let mut b = core.work.remove(&id).unwrap().into_queued().unwrap();
        b.turn = crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({
            "model":"m", "messages":[{"role":"system","content":"B's instructions"},{"role":"user","content":"B"}]
        })).unwrap()).ok().unwrap().turn;
        let changed = core.handle(Event::RetireRuntime, 1);
        let arrival = core.handle(Event::Accepted(Box::new(b)), 2);
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(agent_client_protocol_schema::v1::StopReason::EndTurn),
                },
            }),
            3,
        );
        let completed = core.handle(Event::Deadline, 203);
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
        let dispatched = core.handle(Event::ResumeRuntime, 204);
        assert!(
            core.records.is_empty()
                && dispatched.iter().any(
                    |command| matches!(command, Command::Prepare {work,..} if work == &queued)
                ),
            "after replacement B is prepared with no ACP record from the old process"
        );
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
        );
        assert!(late.is_empty(), "no output follows the request's error");
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded { stop_reason: None },
            }),
            3,
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
                    code: 503,
                    status: "UNAVAILABLE".into(),
                    message: "HTTP failure".into(),
                },
            })
        };
        let commands = core.handle(error(), 1);
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
        assert!(commands.iter().any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::UpstreamHttp(error)))) if error.code == 503 && error.message == "HTTP failure")));
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
        let repeated = core.handle(error(), 2);
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
            .request()
            .id
            .clone();
        let disconnected = core.handle(Event::Disconnected(request), 1);
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
        let mut core = Core {
            runtime_state: RuntimeState::Serving,
            catalog: None,
            work: BTreeMap::new(),
            next_work: 0,
            records: Vec::new(),
            startup_session: None,
            use_sequence: 0,
            limit: 1,
            notices: watch::channel(Arc::new(Vec::new())).0,
        };
        let commands = core.handle(
            Event::Accepted(Box::new(Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: "m".into(),
                    level: None,
                    level_from_name: false,
                },
                request,
                output,
            })),
            0,
        );
        assert!(matches!(&commands[..], [Command::Prepare { .. }]));
        drop(ticket);
        core.handle(Event::Disconnected(work.clone()), 1);
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
                    output,
                })),
                1
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
                runtime_state: RuntimeState::Serving,
                catalog: None,
                work: BTreeMap::new(),
                next_work: 0,
                records: Vec::new(),
                startup_session: Some("session".into()),
                use_sequence: 0,
                limit: 1,
                notices: watch::channel(Arc::new(Vec::new())).0,
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
                    output,
                })),
                0,
            );
            let ended = core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "session".into(),
                    update: TurnUpdate::Failed(crate::messages::Message::UpstreamAcp(
                        crate::messages::AcpError {
                            code: -32099,
                            message: "RPC failure".into(),
                        },
                    )),
                }),
                0,
            );
            assert!(
                !ended
                    .iter()
                    .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(_)))),
                "FR-028 waits for error lines after a prompt RPC failure"
            );
            assert!(core.handle(Event::Deadline, 199).is_empty());
            let ended = if with_http {
                core.handle(
                    Event::Runtime(RuntimeEvent::UpstreamError {
                        session: "session".into(),
                        error: crate::messages::HttpError {
                            code: 503,
                            status: "UNAVAILABLE".into(),
                            message: "HTTP failure".into(),
                        },
                    }),
                    200,
                )
            } else {
                core.handle(Event::Deadline, 200)
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
        use agent_client_protocol_schema::v1::StopReason;
        for stop_reason in [
            Some(StopReason::Refusal),
            Some(StopReason::EndTurn),
            Some(StopReason::Cancelled),
            None,
        ] {
            let turn = crate::dialect::CHAT
                .parse(br#"{"model":"m","tools":[{"type":"function","function":{"name":"read","parameters":{}}}],"messages":[{"role":"user","content":"First"}]}"#)
                .ok()
                .unwrap()
                .turn;
            let (output, _ticket) = mpsc::unbounded_channel();
            let mut core = Core {
                runtime_state: RuntimeState::Serving,
                catalog: None,
                work: BTreeMap::from([(
                    0,
                    Work::AcceptedWorking(Active {
                        output: output.clone(),
                        request: journal.begin(crate::journal::Api::Chat, 0),
                        session: Some("existing".into()),
                        hashes: crate::conversation::record_hashes(&turn.conversation),
                        tools_hash: crate::conversation::tool_set_hash(&turn.tools),
                        kind: crate::journal::SessionUse::New,
                        used: 1,
                        expires: 100_000,
                        held: Some("Before".into()),
                        text: String::new(),
                        thought_dropped: false,
                        input_tokens: turn.input_tokens,
                        phase: Phase::Prompt,
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
                records: Vec::new(),
                startup_session: None,
                use_sequence: 1,
                limit: 1,
                notices: watch::channel(Arc::new(Vec::new())).0,
            };
            core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "existing".into(),
                    update: TurnUpdate::PromptEnded { stop_reason },
                }),
                0,
            );
            core.handle(Event::Deadline, 200);
            let next = crate::dialect::CHAT.parse(br#"{"model":"m","tools":[{"type":"function","function":{"name":"read","parameters":{}}}],"messages":[
                {"role":"user","content":"First"},
                {"role":"assistant","content":"Before","tool_calls":[{"id":"call-id","function":{"name":"read","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"call-id","content":"Done"},
                {"role":"user","content":"Next"}
            ]}"#).ok().unwrap().turn;
            selected.extend(
                core.handle(
                    Event::Accepted(Box::new(Accepted {
                        model: crate::catalog::SelectedModel {
                            model: "m".into(),
                            level: None,
                            level_from_name: false,
                        },
                        turn: next,
                        request: journal.begin(crate::journal::Api::Chat, 200),
                        output,
                    })),
                    200,
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
            None,
            clock.clone(),
            journal.clone(),
            settings,
            configuration,
            3,
        )
    }
    #[test]
    fn models_are_empty_after_a_runtime_closes() {
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        let mut core = Core {
            runtime_state: RuntimeState::Serving,
            catalog: Some(catalog),
            work: BTreeMap::new(),
            next_work: 0,
            records: Vec::new(),
            startup_session: None,
            use_sequence: 0,
            limit: 1,
            notices: watch::channel(Arc::new(Vec::new())).0,
        };
        assert_eq!(core.model_entries().len(), 1);
        let missing = crate::dialect::CHAT
            .parse(br#"{"model":"missing","messages":[{"role":"user","content":"Hi"}]}"#)
            .ok()
            .unwrap()
            .turn;
        assert!(core.check_model(&missing).is_err());
        core.handle(Event::Runtime(RuntimeEvent::Closed), 0);
        assert_eq!(core.model_entries().len(), 0, "GET /v1/models");
        assert!(core.check_model(&missing).is_ok());
    }
    #[test]
    fn a_session_that_leaves_for_good_is_forgotten() {
        let folder = std::env::temp_dir().join(format!("acp2api-forget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let mut core = Core {
            runtime_state: RuntimeState::Serving,
            catalog: None,
            work: BTreeMap::new(),
            next_work: 0,
            records: Vec::new(),
            startup_session: None,
            use_sequence: 0,
            limit: 1,
            notices: watch::channel(Arc::new(Vec::new())).0,
        };
        let mut outputs = Vec::new();
        let mut forgotten = Vec::new();
        let mut turn = |core: &mut Core, system: &str, session: &str, ends: TurnUpdate| {
            let body = format!(
                r#"{{"model":"m","messages":[{{"role":"system","content":"{system}"}},{{"role":"user","content":"Hi"}}]}}"#
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
                        model: "m".into(),
                        level: None,
                        level_from_name: false,
                    },
                    turn,
                    request,
                    output,
                })),
                0,
            );
            let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
            let catalog = crate::catalog::Catalog::decode(&options, clock.wall()).unwrap();
            let prepared = Ok((session.to_owned(), catalog));
            commands.extend(core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work,
                    result: prepared,
                }),
                0,
            ));
            let session = session.to_owned();
            commands.extend(core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session,
                    update: ends,
                }),
                0,
            ));
            commands.extend(core.handle(Event::Deadline, 1000));
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
            runtime_state: RuntimeState::Serving,
            catalog: None,
            work: BTreeMap::new(),
            next_work: 0,
            records: vec![records::Record {
                session: "idle".into(),
                hashes: crate::conversation::record_hashes(&turn.conversation),
                tools: crate::conversation::tool_set_hash(&[]),
                used: 1,
                expires: 24 * 60 * 60 * 1000,
            }],
            startup_session: None,
            use_sequence: 1,
            limit: 1,
            notices: watch::channel(Arc::new(Vec::new())).0,
        };
        let mut forgotten = Vec::new();
        if let Some(at) = core.next_deadline()
            && at <= 24 * 60 * 60 * 1000
        {
            for command in core.handle(Event::Deadline, at) {
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
        let mut core = Core {
            runtime_state: RuntimeState::Serving,
            catalog: None,
            work: BTreeMap::new(),
            next_work: 0,
            records: Vec::new(),
            startup_session: None,
            use_sequence: 0,
            limit: 1,
            notices: watch::channel(Arc::new(Vec::new())).0,
        };
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
            output,
        }));
        assert!(matches!(
            &core.handle(accepted, 0)[..],
            [Command::Prepare { .. }]
        ));
        let failed = Event::Runtime(RuntimeEvent::Prepared {
            work,
            result: Err("session/new failed".into()),
        });
        for command in core.handle(failed, 0) {
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
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let (conductor, _task) = start_without_runtime(&clock, &journal, &folder);
        let body = br#"{"model":"gemini-3.8-flash","reasoning_effort":"extreme","messages":[{"role":"user","content":"Hi"}]}"#;
        let turn = crate::dialect::CHAT.parse(body).ok().unwrap().turn;
        let request = journal.begin(crate::journal::Api::Chat, clock.now());
        let admitted = conductor
            .admit(turn, clock.hold(HoldKind::HttpRequest), request)
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
        let lines: String = std::fs::read_dir(&folder)
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
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let (conductor, _task) = start_without_runtime(&clock, &journal, &folder);
        let body = br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#;
        let turn = || crate::dialect::CHAT.parse(body).ok().unwrap().turn;
        let request = || journal.begin(crate::journal::Api::Chat, clock.now());
        let (admission, reply) = oneshot::channel();
        drop(reply);
        conductor
            .send
            .send(Input::Turn(Box::new(TurnWork {
                turn: turn(),
                request: request(),
                hold: clock.hold(HoldKind::HttpRequest),
                admission,
            })))
            .await
            .ok()
            .unwrap();
        let next = conductor.admit(turn(), clock.hold(HoldKind::HttpRequest), request());
        assert!(next.await.is_ok(), "the conductor serves the next request");
        let lines: String = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(
            lines.contains(r#""result":"disconnected""#),
            "request line of the gone client: {lines}"
        );
    }
}
