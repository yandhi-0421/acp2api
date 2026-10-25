mod records;
use crate::{
    Clock, Hold, HoldKind, Moment,
    conversation::{EffortAsk, Prompt, PromptRequest, Ticket, TurnEvent, TurnRequest, Usage},
    runtime_driver::{Runtime, RuntimeEvent, TurnUpdate},
};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{mpsc, oneshot, watch};
#[derive(Clone)]
pub(crate) struct Conductor {
    send: mpsc::Sender<Work>,
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
enum Work {
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
const UNAVAILABLE: &str = "official runtime is not available";
type Output = mpsc::UnboundedSender<(TurnEvent, Hold)>;
struct Accepted {
    turn: TurnRequest,
    model: crate::catalog::SelectedModel,
    request: crate::journal::Request,
    output: Output,
}
#[derive(Clone)]
struct Session {
    tools: Vec<crate::conversation::ClientTool>,
    system: Option<String>,
}
enum Phase {
    Preparing(PromptRequest),
    Prompt,
    Settling { at: Moment, can_continue: bool },
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
    disconnected: bool,
    held: Option<String>,
    text: String,
    thought_dropped: bool,
    input_tokens: u64,
    phase: Phase,
    tool_call: Option<crate::conversation::ToolCall>,
}
struct Core {
    catalog: Option<crate::catalog::Catalog>,
    active: Vec<Active>,
    waiting: VecDeque<Accepted>,
    records: Vec<records::Record>,
    startup_session: Option<String>,
    use_sequence: u64,
    limit: u8,
    notices: watch::Sender<Arc<Vec<Notice>>>,
}
enum Event {
    Accepted(Accepted),
    Runtime(RuntimeEvent),
    Deadline,
    Disconnected(String),
    Limit(u8),
    DismissNotice(String),
    StartFailed(String, String),
}
enum Command {
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
    fn next_deadline(&self) -> Option<Moment> {
        self.active
            .iter()
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
        let (model, admitted) = match self.check_model(&work.turn) {
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
                (Some(model), work.admission.send(Ok(ticket)))
            }
            Err(error) => (None, work.admission.send(Err(error))),
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
        Some((Event::Accepted(accepted), work.hold))
    }
    fn handle(&mut self, event: Event, now: Moment) -> Vec<Command> {
        let mut commands = Vec::new();
        match event {
            Event::Accepted(request) => self.waiting.push_back(request),
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
            Event::Disconnected(work) => {
                if let Some(active) = self
                    .active
                    .iter_mut()
                    .find(|active| active.request.id == work)
                {
                    active.disconnected = true;
                }
            }
            Event::StartFailed(work, error) => {
                if let Some(index) = self
                    .active
                    .iter()
                    .position(|active| active.request.id == work)
                {
                    self.finish(
                        index,
                        Err(crate::messages::Message::Upstream(error)),
                        &mut commands,
                    );
                }
            }
            Event::Runtime(RuntimeEvent::Prepared { work, result }) => {
                if let Some(index) = self
                    .active
                    .iter()
                    .position(|active| active.request.id == work)
                {
                    match result {
                        Ok((session, catalog)) => {
                            self.catalog = Some(catalog);
                            let active = &mut self.active[index];
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
                        Err(error) => self.finish(
                            index,
                            Err(crate::messages::Message::Upstream(error)),
                            &mut commands,
                        ),
                    }
                }
            }
            Event::Runtime(RuntimeEvent::Turn { session, update }) => {
                if let Some(index) = self
                    .active
                    .iter()
                    .position(|active| active.session.as_ref() == Some(&session))
                {
                    self.update(index, session, update, now, &mut commands);
                } else if let TurnUpdate::Permission { request, .. }
                | TurnUpdate::RefusedPermission { request, .. } = update
                {
                    commands.push(Command::Permission {
                        request,
                        cancel: false,
                    });
                }
            }
            Event::Runtime(RuntimeEvent::Failed(error)) => {
                self.forget_idle(&mut commands);
                while !self.active.is_empty() {
                    self.finish(
                        0,
                        Err(crate::messages::Message::Upstream(error.clone())),
                        &mut commands,
                    );
                }
            }
            Event::Runtime(RuntimeEvent::Closed) => {
                self.catalog = None;
                self.forget_idle(&mut commands);
                while !self.active.is_empty() {
                    self.finish(
                        0,
                        Err(crate::messages::Message::Upstream(
                            "runtime stdout closed".into(),
                        )),
                        &mut commands,
                    );
                }
            }
            Event::Runtime(RuntimeEvent::Stderr | RuntimeEvent::Other) => {}
            Event::Deadline => {
                let mut index = 0;
                while index < self.active.len() {
                    if matches!(self.active[index].phase,Phase::Settling { at, .. } if at<=now) {
                        self.finish(index, Ok(()), &mut commands);
                    } else {
                        index += 1;
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
        while self.active.len() < usize::from(self.limit) {
            let Some(request) = self.waiting.pop_front() else {
                break;
            };
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
            self.active.push(Active {
                output: request.output,
                request: request.request,
                session,
                hashes,
                tools_hash,
                kind,
                used: self.use_sequence,
                expires: now.saturating_add(24 * 60 * 60 * 1000),
                disconnected: false,
                held: None,
                text: String::new(),
                thought_dropped: false,
                input_tokens: request.turn.input_tokens,
                phase,
                tool_call: None,
            });
        }
        commands
    }
    fn update(
        &mut self,
        index: usize,
        session: String,
        update: TurnUpdate,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        let active = &mut self.active[index];
        match update {
            TurnUpdate::Text(text) => {
                if active.tool_call.is_none()
                    && let Some(previous) = active.held.replace(text)
                {
                    active.text.push_str(&previous);
                    commands.push(Command::Emit(
                        active.output.clone(),
                        TurnEvent::Text(previous),
                    ));
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
                active.phase = Phase::Settling {
                    at: now.saturating_add(200),
                    can_continue: stop_reason == Some(StopReason::EndTurn)
                        || stop_reason == Some(StopReason::Cancelled) && active.tool_call.is_some(),
                };
            }
            TurnUpdate::Failed(error) => self.finish(
                index,
                Err(crate::messages::Message::Upstream(error)),
                commands,
            ),
            TurnUpdate::UnpermittedTool(name) => {
                let notice = Notice {
                    id: active.request.id.clone(),
                    name: name.clone(),
                };
                self.notices
                    .send_modify(|notices| Arc::make_mut(notices).push(notice));
                commands.push(Command::Cancel(session.clone()));
                commands.push(Command::Safety(crate::journal::SafetyReport {
                    request: active.request.clone(),
                    session,
                    kind: crate::journal::SafetyKind::BuiltInToolRan,
                    tool: name.clone(),
                }));
                self.finish(
                    index,
                    Err(crate::messages::Message::BuiltInToolRan(name)),
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
    fn finish(
        &mut self,
        index: usize,
        result: Result<(), crate::messages::Message>,
        commands: &mut Vec<Command>,
    ) {
        let mut active = self.active.remove(index);
        let can_continue = matches!(
            active.phase,
            Phase::Settling {
                can_continue: true,
                ..
            }
        );
        let tool_hash = active
            .tool_call
            .as_ref()
            .map(|call| crate::conversation::tool_call_hash(&call.name, &call.arguments));
        let end = result.map(|()| {
            if let Some(text) = active.held.take() {
                active.text.push_str(&text);
                commands.push(Command::Emit(active.output.clone(), TurnEvent::Text(text)));
            }
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
        if let Some(session) = &active.session {
            if end.is_ok() && !active.disconnected && can_continue {
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
                commands.push(Command::Forget(session.clone()));
            }
        }
        let outcome = if active.disconnected {
            crate::journal::Outcome::Disconnected
        } else {
            match &end {
                Ok(usage) => crate::journal::Outcome::Success {
                    input: usage.input_tokens,
                    output: usage.output_tokens,
                },
                Err(crate::messages::Message::BuiltInToolRan(_)) => {
                    crate::journal::Outcome::BuiltInToolRan
                }
                Err(_) => crate::journal::Outcome::Failed,
            }
        };
        commands.push(Command::Complete(crate::journal::Completion {
            request: active.request,
            session: active.session,
            session_use: Some(active.kind),
            outcome,
        }));
        commands.push(Command::Emit(active.output, TurnEvent::End(end)));
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
        let (send, mut recv) = mpsc::channel::<Work>(32);
        let limit = settings.concurrency_limit;
        let (publish, settings_watch) = watch::channel(Arc::new(settings));
        let (notices, notices_watch) = watch::channel(Arc::new(Vec::new()));
        let task = tokio::spawn(async move {
            let (mut runtime, catalog) = started.unzip();
            let mut core = Core {
                startup_session: runtime.as_ref().map(|runtime| runtime.session_id().into()),
                catalog,
                records: Vec::new(),
                use_sequence: 0,
                limit,
                active: Vec::new(),
                waiting: VecDeque::new(),
                notices,
            };
            let mut read_open = runtime.is_some();
            loop {
                let connected: Vec<_> = core
                    .active
                    .iter()
                    .filter(|active| !active.disconnected)
                    .map(|active| (active.request.id.clone(), active.output.clone()))
                    .collect();
                let timer = core.next_deadline().map(|at| clock.sleep_until(at));
                let (event, hold) = tokio::select! {
                    biased;
                    work = first_closed(&connected), if !connected.is_empty() => {
                        (Event::Disconnected(work), clock.hold(HoldKind::TurnEvent))
                    }
                    work = recv.recv() => {
                        let Some(work) = work else {
                            break;
                        };
                        match work {
                            Work::Models { reply, hold } => {
                                let _ = reply.send(Ok(core.model_entries()));
                                drop(hold);
                                continue;
                            }
                            Work::Owner { action, reply, hold } => {
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
                            Work::Turn(work) => {
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
    pub(crate) fn notices(&self) -> Arc<Vec<Notice>> {
        self.notices.borrow().clone()
    }
    pub(crate) fn settings(&self) -> Arc<crate::store::Settings> {
        self.settings.borrow().clone()
    }
    pub(crate) async fn owner(&self, action: OwnerAction, hold: Hold) -> Result<(), String> {
        let (reply, result) = oneshot::channel();
        self.send
            .send(Work::Owner {
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
            .send(Work::Turn(Box::new(TurnWork {
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
            .send(Work::Models { reply, hold })
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
                catalog: None,
                active: vec![Active {
                    output: output.clone(),
                    request: journal.begin(crate::journal::Api::Chat, 0),
                    session: Some("existing".into()),
                    hashes: crate::conversation::record_hashes(&turn.conversation),
                    tools_hash: crate::conversation::tool_set_hash(&turn.tools),
                    kind: crate::journal::SessionUse::New,
                    used: 1,
                    expires: 100_000,
                    disconnected: false,
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
                }],
                waiting: VecDeque::new(),
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
                    Event::Accepted(Accepted {
                        model: crate::catalog::SelectedModel {
                            model: "m".into(),
                            level: None,
                            level_from_name: false,
                        },
                        turn: next,
                        request: journal.begin(crate::journal::Api::Chat, 200),
                        output,
                    }),
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
            catalog: Some(catalog),
            active: Vec::new(),
            waiting: VecDeque::new(),
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
            catalog: None,
            active: Vec::new(),
            waiting: VecDeque::new(),
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
                Event::Accepted(Accepted {
                    model: crate::catalog::SelectedModel {
                        model: "m".into(),
                        level: None,
                        level_from_name: false,
                    },
                    turn,
                    request,
                    output,
                }),
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
        turn(&mut core, "A", "s1", TurnUpdate::Failed("x".into()));
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
            catalog: None,
            active: Vec::new(),
            waiting: VecDeque::new(),
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
            catalog: None,
            active: Vec::new(),
            waiting: VecDeque::new(),
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
        let accepted = Event::Accepted(Accepted {
            model: crate::catalog::SelectedModel {
                model: "m".into(),
                level: None,
                level_from_name: false,
            },
            turn,
            request,
            output,
        });
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
            Ok(mut ticket) => loop {
                if let (TurnEvent::End(end), _) = ticket.recv().await.unwrap() {
                    break end.err().map(|message| message.to_string());
                }
            },
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
            .send(Work::Turn(Box::new(TurnWork {
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
