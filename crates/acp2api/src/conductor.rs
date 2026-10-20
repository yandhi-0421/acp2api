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
    pub(crate) highest: u8,
}
pub(crate) enum OwnerAction {
    NewLocalApiKey,
    ChangePassword(String),
    SetConcurrencyLimit(u8),
}
enum Work {
    Owner {
        action: OwnerAction,
        reply: oneshot::Sender<Result<(), String>>,
        hold: Hold,
    },
    Turn(Box<TurnWork>),
    Models {
        reply: oneshot::Sender<Vec<crate::catalog::ModelEntry>>,
        hold: Hold,
    },
}
struct TurnWork {
    turn: TurnRequest,
    request: crate::journal::Request,
    hold: Hold,
    admission: oneshot::Sender<Result<Ticket, crate::messages::Refusal>>,
}
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
    catalog: crate::catalog::Catalog,
    active: Vec<Active>,
    waiting: VecDeque<Accepted>,
    idle: Vec<Idle>,
    limit: u8,
}
enum Event {
    Accepted(Accepted),
    Runtime(RuntimeEvent),
    Deadline,
    Disconnected(String),
    Limit(u8),
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
            Event::Limit(limit) => {
                self.limit = limit;
                self.idle.truncate(usize::from(limit));
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
                            self.catalog = catalog;
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
                    self.update(index, update, now, &mut commands);
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
                self.idle.clear();
                while !self.active.is_empty() {
                    self.finish(
                        0,
                        Err(crate::messages::Message::Upstream(error.clone())),
                        &mut commands,
                    );
                }
            }
            Event::Runtime(RuntimeEvent::Closed) => {
                self.idle.clear();
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
                if let Some(session) = &active.session {
                    commands.push(Command::Cancel(session.id.clone()));
                    commands.push(Command::Safety(crate::journal::SafetyReport {
                        request: active.request.clone(),
                        session: session.id.clone(),
                        kind: crate::journal::SafetyKind::BuiltInToolRan,
                        tool: name.as_str().into(),
                    }));
                }
                self.finish(
                    index,
                    Err(crate::messages::Message::BuiltInToolRan(
                        name.as_str().into(),
                    )),
                    commands,
                );
            }
            TurnUpdate::RefusedPermission { request, name } => {
                if let Some(session) = &active.session {
                    commands.push(Command::Safety(crate::journal::SafetyReport {
                        request: active.request.clone(),
                        session: session.id.clone(),
                        kind: crate::journal::SafetyKind::PermissionRefused,
                        tool: name.as_str().into(),
                    }));
                }
                commands.push(Command::Permission {
                    request,
                    cancel: false,
                });
            }
            TurnUpdate::Permission { request, call } => {
                let mut cancel = false;
                if matches!(active.phase, Phase::Prompt)
                    && active.tool_call.is_none()
                    && let Some(call) = call
                {
                    active.tool_call = Some(call);
                    cancel = true;
                }
                commands.push(Command::Permission { request, cancel });
            }
        }
    }
    fn finish(
        &mut self,
        index: usize,
        result: Result<(), crate::messages::Message>,
        commands: &mut Vec<Command>,
    ) {
        let mut active = self.active.remove(index);
        if result.is_ok()
            && !active.disconnected
            && let Some(session) = &active.session
        {
            self.idle.push(Idle {
                id: session.id.clone(),
                settings: active.settings,
            });
            if self.idle.len() > usize::from(self.limit) {
                self.idle.remove(0);
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
                    (Some(runtime), catalog, idle)
                }
                None => (None, crate::catalog::Catalog::default(), Vec::new()),
            };
            let mut core = Core {
                catalog,
                idle,
                limit,
                active: Vec::new(),
                waiting: VecDeque::new(),
            };
            let mut read_open = runtime.is_some();
            'events: loop {
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
                            Work::Models {reply,hold}=>{let _=reply.send(core.catalog.entries());drop(hold);continue;}
                            Work::Owner {action,reply,hold}=>{
                                let mut settings=(**publish.borrow()).clone();
                                let result=(|| {
                                    match action {
                                        OwnerAction::NewLocalApiKey=>settings.local_api_key=crate::store::random_key()?,
                                        OwnerAction::ChangePassword(password)=>settings.management_password_hash=crate::store::hash_password(&password)?,
                                        OwnerAction::SetConcurrencyLimit(limit)=>settings.concurrency_limit=limit,
                                    }
                                    crate::store::save(&configuration,&settings)?;publish.send_replace(Arc::new(settings.clone()));Ok(())
                                })();
                                let success=result.is_ok();let _=reply.send(result);
                                if success {
                                    let mut commands:VecDeque<_>=core.handle(Event::Limit(settings.concurrency_limit),clock.now()).into();
                                    if !Self::commands(&mut core,&mut runtime,&mut read_open,&clock,&journal,&mut commands).await {break 'events;}
                                }
                                drop(hold);continue;
                            }
                            Work::Turn(work)=>*work,
                        };
                        if let Err(error)=core.catalog.check(&work.turn.model) {let _=work.admission.send(Err(error));continue;}
                        let (output,ticket)=mpsc::unbounded_channel();if work.admission.send(Ok(ticket)).is_err() {continue;}
                        (Event::Accepted(Accepted {turn:work.turn,request:work.request,output}),work.hold)
                    }
                    (event,hold)=async {runtime.as_mut().expect("readable runtime").next().await},if read_open=>{
                        if matches!(event,RuntimeEvent::Closed) {read_open=false;}
                        (Event::Runtime(event),hold)
                    }
                    hold=timer=>(Event::Deadline,hold),
                };
                let mut commands: VecDeque<_> = core.handle(event, clock.now()).into();
                if !Self::commands(
                    &mut core,
                    &mut runtime,
                    &mut read_open,
                    &clock,
                    &journal,
                    &mut commands,
                )
                .await
                {
                    break 'events;
                }
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
    ) -> bool {
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
                Command::Safety(report) => {
                    if journal.safety(report, clock.as_ref()).is_err() {
                        eprintln!("acp2api: cannot write runtime safety event");
                        return false;
                    }
                }
                Command::Complete(completion) => {
                    if journal.complete(completion, clock.as_ref()).is_err() {
                        eprintln!("acp2api: cannot write request metadata");
                        return false;
                    }
                }
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
                        None => Err("official runtime is not available".into()),
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
                        None => Err("official runtime is not available".into()),
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
                Command::Emit(output, event) => {
                    let _ = output.send((event, clock.hold(HoldKind::TurnEvent)));
                }
            }
        }
        true
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
            .map_err(|_| {
                crate::messages::Refusal::from(crate::messages::Message::Upstream(
                    "conductor stopped".into(),
                ))
            })?;
        reply.await.map_err(|_| {
            crate::messages::Refusal::from(crate::messages::Message::Upstream(
                "conductor stopped".into(),
            ))
        })?
    }
    pub(crate) async fn model_names(
        &self,
        hold: Hold,
    ) -> Result<Vec<crate::catalog::ModelEntry>, crate::messages::Refusal> {
        let (reply, result) = oneshot::channel();
        self.send
            .send(Work::Models { reply, hold })
            .await
            .map_err(|_| {
                crate::messages::Refusal::from(crate::messages::Message::Upstream(
                    "conductor stopped".into(),
                ))
            })?;
        result.await.map_err(|_| {
            crate::messages::Refusal::from(crate::messages::Message::Upstream(
                "conductor stopped".into(),
            ))
        })
    }
}
