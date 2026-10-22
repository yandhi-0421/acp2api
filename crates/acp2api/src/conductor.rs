use crate::{
    Clock, Hold, HoldKind, Moment,
    conversation::{Ticket, TurnEvent, TurnRequest, Usage},
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
    /// Sets the Argon2id hash of a new management password.
    ChangePassword(String),
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
    request: crate::journal::Request,
    output: Output,
}
#[derive(Clone, PartialEq, Eq)]
struct Session {
    tools: Vec<crate::conversation::ClientTool>,
    system: Option<String>,
}
struct Idle {
    id: String,
    settings: Session,
}
struct PendingPrompt {
    text: Vec<crate::conversation::PromptBlock>,
    model: String,
}
enum Phase {
    Preparing(PendingPrompt),
    Prompt,
    Settling(Moment),
}
struct Active {
    output: Output,
    request: crate::journal::Request,
    session: Option<crate::journal::Session>,
    settings: Session,
    disconnected: bool,
    held: Option<String>,
    text: String,
    input_tokens: u64,
    phase: Phase,
    tool_call: Option<crate::conversation::ToolCall>,
}
struct Core {
    /// `None` while acp2api has no runtime process.
    catalog: Option<crate::catalog::Catalog>,
    active: Vec<Active>,
    waiting: VecDeque<Accepted>,
    idle: Vec<Idle>,
    limit: u8,
    notices: Vec<Notice>,
}
enum Event {
    Accepted(Accepted),
    Runtime(RuntimeEvent),
    Deadline,
    Disconnected(String),
    Limit(u8),
    DismissNotice(String),
    PreparationFailed(String, String),
}
enum Command {
    Complete(crate::journal::Completion),
    Safety(crate::journal::SafetyReport),
    Cancel(String),
    Prepare {
        work: String,
        settings: Session,
    },
    Prompt {
        work: String,
        session: String,
        prompt: PendingPrompt,
    },
    Emit(Output, TurnEvent),
    Permission {
        request: crate::runtime_driver::PermissionId,
        cancel: bool,
    },
    /// The ACP session leaves for good; its client tools go too.
    Forget(String),
}
impl Core {
    fn next_deadline(&self) -> Moment {
        self.active
            .iter()
            .filter_map(|active| match active.phase {
                Phase::Settling(at) => Some(at),
                _ => None,
            })
            .min()
            .unwrap_or(u64::MAX)
    }
    fn handle(&mut self, event: Event, now: Moment) -> Vec<Command> {
        let mut commands = Vec::new();
        match event {
            Event::Accepted(request) => self.waiting.push_back(request),
            Event::DismissNotice(id) => self.notices.retain(|notice| notice.id != id),
            Event::Limit(limit) => {
                self.limit = limit;
                let kept = self.idle.len().min(usize::from(limit));
                self.forget_idle(kept, &mut commands);
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
            Event::PreparationFailed(work, error) => {
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
                            active.session = Some(crate::journal::Session {
                                id: session.clone(),
                                kind: crate::journal::SessionUse::New,
                            });
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
                    .position(|active| active.session.as_ref().is_some_and(|s| s.id == session))
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
                self.forget_idle(0, &mut commands);
                while !self.active.is_empty() {
                    self.finish(
                        0,
                        Err(crate::messages::Message::Upstream(error.clone())),
                        &mut commands,
                    );
                }
            }
            Event::Runtime(RuntimeEvent::Closed) => {
                self.forget_idle(0, &mut commands);
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
                    if matches!(self.active[index].phase,Phase::Settling(at) if at<=now) {
                        self.finish(index, Ok(()), &mut commands);
                    } else {
                        index += 1;
                    }
                }
            }
        }
        while self.active.len() < usize::from(self.limit) {
            let Some(request) = self.waiting.pop_front() else {
                break;
            };
            let prompt = PendingPrompt {
                text: crate::conversation::prompt(&request.turn),
                model: request.turn.model,
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
            let idle = self
                .idle
                .iter()
                .position(|session| session.settings == settings)
                .map(|index| self.idle.remove(index));
            let work = request.request.id.clone();
            let (session, phase) = match idle {
                Some(idle) => {
                    commands.push(Command::Prompt {
                        work: work.clone(),
                        session: idle.id.clone(),
                        prompt,
                    });
                    (
                        Some(crate::journal::Session {
                            id: idle.id,
                            kind: crate::journal::SessionUse::New,
                        }),
                        Phase::Prompt,
                    )
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
                settings,
                disconnected: false,
                held: None,
                text: String::new(),
                input_tokens: 0,
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
            TurnUpdate::PromptEnded => active.phase = Phase::Settling(now.saturating_add(200)),
            TurnUpdate::Failed(error) => self.finish(
                index,
                Err(crate::messages::Message::Upstream(error)),
                commands,
            ),
            TurnUpdate::UnpermittedTool(name) => {
                self.notices.push(Notice {
                    id: active.request.id.clone(),
                    name: name.clone(),
                });
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
    /// Keeps the first `kept` idle sessions and forgets the others.
    fn forget_idle(&mut self, kept: usize, commands: &mut Vec<Command>) {
        commands.extend(self.idle.drain(kept..).map(|idle| Command::Forget(idle.id)));
    }
    fn finish(
        &mut self,
        index: usize,
        result: Result<(), crate::messages::Message>,
        commands: &mut Vec<Command>,
    ) {
        let mut active = self.active.remove(index);
        if let Some(session) = &active.session {
            if result.is_ok() && !active.disconnected {
                self.idle.push(Idle {
                    id: session.id.clone(),
                    settings: active.settings,
                });
                if self.idle.len() > usize::from(self.limit) {
                    let oldest = self.idle.remove(0);
                    commands.push(Command::Forget(oldest.id));
                }
            } else {
                commands.push(Command::Forget(session.id.clone()));
            }
        }
        let end = result.map(|()| {
            if let Some(text) = active.held.take() {
                active.text.push_str(&text);
                commands.push(Command::Emit(active.output.clone(), TurnEvent::Text(text)));
            }
            if let Some(call) = active.tool_call.take() {
                commands.push(Command::Emit(
                    active.output.clone(),
                    TurnEvent::ToolCall(call),
                ));
            }
            Usage {
                input_tokens: active.input_tokens,
                output_tokens: crate::conversation::output_tokens(&active.text),
            }
        });
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
        let (publish_notices, notices_watch) = watch::channel(Arc::new(Vec::new()));
        let task = tokio::spawn(async move {
            let (mut runtime, catalog, idle) = match started {
                Some((runtime, catalog)) => {
                    let idle = vec![Idle {
                        id: runtime.session_id().into(),
                        settings: Session {
                            tools: Vec::new(),
                            system: None,
                        },
                    }];
                    (Some(runtime), Some(catalog), idle)
                }
                None => (None, None, Vec::new()),
            };
            let mut core = Core {
                catalog,
                idle,
                limit,
                active: Vec::new(),
                waiting: VecDeque::new(),
                notices: Vec::new(),
            };
            let mut read_open = runtime.is_some();
            loop {
                let disconnected: Vec<_> = core
                    .active
                    .iter()
                    .filter(|active| !active.disconnected)
                    .map(|active| (active.request.id.clone(), active.output.clone()))
                    .collect();
                let timer = clock.sleep_until(core.next_deadline());
                let (event, hold) = tokio::select! {
                    biased;
                    work=async {
                        let waits=disconnected.iter().map(|(id,output)|Box::pin(async move {output.closed().await;id.clone()}));
                        futures_util::future::select_all(waits).await.0
                    },if !disconnected.is_empty()=>(Event::Disconnected(work),clock.hold(HoldKind::TurnEvent)),
                    work=recv.recv()=>{
                        let Some(work)=work else {break;};
                        let work=match work {
                            Work::Models {reply,hold}=>{let _=reply.send(Ok(core.catalog.as_ref().map(crate::catalog::Catalog::entries).unwrap_or_default()));drop(hold);continue;}
                            Work::Owner {action,reply,hold}=>{
                                let mut settings=(**publish.borrow()).clone();
                                let mut commands = VecDeque::new();
                                let result=(|| {
                                    match action {
                                        OwnerAction::NewLocalApiKey=>settings.local_api_key=crate::store::random_key()?,
                                        OwnerAction::ChangePassword(hash)=>settings.management_password_hash=hash,
                                        OwnerAction::SetConcurrencyLimit(limit)=>settings.concurrency_limit=limit,
                                        OwnerAction::DismissNotice(id)=>{commands.extend(core.handle(Event::DismissNotice(id),clock.now()));return Ok(());},
                                    }
                                    crate::store::save(&configuration,&settings)?;publish.send_replace(Arc::new(settings.clone()));
                                    commands.extend(core.handle(Event::Limit(settings.concurrency_limit),clock.now()));Ok(())
                                })();
                                let success=result.is_ok();
                                publish_notices.send_replace(Arc::new(core.notices.clone()));
                                let _=reply.send(result);
                                if success {Self::commands(&mut core,&mut runtime,&mut read_open,&clock,&journal,&mut commands).await;}
                                drop(hold);continue;
                            }
                            Work::Turn(work)=>*work,
                        };
                        let (output,ticket)=mpsc::unbounded_channel();
                        let admitted = core.catalog.as_ref().map_or(Ok(()), |catalog| catalog.check(&work.turn.model)).map(|()| ticket);
                        let refused = admitted.is_err();
                        if work.admission.send(admitted).is_err() {
                            journal.complete(crate::journal::Completion {request:work.request,session:None,outcome:crate::journal::Outcome::Disconnected},clock.as_ref());
                            continue;
                        }
                        if refused {continue;}
                        (Event::Accepted(Accepted {turn:work.turn,request:work.request,output}),work.hold)
                    }
                    (event,hold)=async {runtime.as_mut().expect("readable runtime").next().await},if read_open=>{
                        if matches!(event,RuntimeEvent::Closed) {read_open=false;}
                        (Event::Runtime(event),hold)
                    }
                    hold=timer=>(Event::Deadline,hold),
                };
                let mut commands: VecDeque<_> = core.handle(event, clock.now()).into();
                publish_notices.send_replace(Arc::new(core.notices.clone()));
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
                Command::Complete(completion) => journal.complete(completion, clock.as_ref()),
                Command::Prepare { work, settings } => {
                    let result = match runtime.as_mut().filter(|_| *read_open) {
                        Some(runtime) => {
                            runtime
                                .request_session(
                                    work.clone(),
                                    &settings.tools,
                                    settings.system.as_deref(),
                                )
                                .await
                        }
                        None => Err(UNAVAILABLE.into()),
                    };
                    if let Err(error) = result {
                        commands.extend(
                            core.handle(Event::PreparationFailed(work, error), clock.now()),
                        );
                    }
                }
                Command::Prompt {
                    work,
                    session,
                    prompt,
                } => {
                    let result = match runtime.as_mut().filter(|_| *read_open) {
                        Some(runtime) => {
                            runtime
                                .start_prompt(prompt.text, &prompt.model, &session)
                                .await
                        }
                        None => Err(UNAVAILABLE.into()),
                    };
                    if let Err(error) = result {
                        commands.extend(
                            core.handle(Event::PreparationFailed(work, error), clock.now()),
                        );
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
fn stopped<T>(_: T) -> crate::messages::Refusal {
    crate::messages::Message::Upstream("conductor stopped".into()).into()
}
#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
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
    fn a_session_that_leaves_for_good_is_forgotten() {
        let folder = std::env::temp_dir().join(format!("acp2api-forget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let mut core = Core {
            catalog: None,
            active: Vec::new(),
            waiting: VecDeque::new(),
            idle: Vec::new(),
            limit: 1,
            notices: Vec::new(),
        };
        let mut outputs = Vec::new();
        let mut forgotten = Vec::new();
        // One turn on its own new session; `ends` is the update that ends the prompt.
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
        turn(&mut core, "A", "s1", TurnUpdate::Failed("x".into()));
        turn(&mut core, "B", "s2", TurnUpdate::PromptEnded);
        turn(&mut core, "C", "s3", TurnUpdate::PromptEnded);
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(
            forgotten,
            ["s1", "s2"],
            "a failed session and an idle session over the limit"
        );
    }
    #[tokio::test]
    async fn without_a_runtime_a_request_gets_no_model_name_error() {
        let folder =
            std::env::temp_dir().join(format!("acp2api-no-runtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let (conductor, _task) = start_without_runtime(&clock, &journal, &folder);
        let body = br#"{"model":"gemini-3.8-flash","messages":[{"role":"user","content":"Hi"}]}"#;
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
        std::fs::remove_dir_all(&folder).unwrap();
        let unavailable = Some("official runtime is not available".to_owned());
        assert_eq!(end, unavailable, "answer to a request");
        // The frozen #55 steps check a new local API key with GET /v1/models and no runtime.
        assert_eq!(listed, Some(0), "GET /v1/models lists no model");
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
