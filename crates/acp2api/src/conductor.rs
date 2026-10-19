use crate::{
    Clock, Hold, HoldKind, Moment,
    conversation::{Ticket, TurnEvent, TurnRequest, Usage},
    runtime_driver::{Runtime, RuntimeEvent},
};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{mpsc, oneshot};
#[derive(Clone)]
pub(crate) struct Conductor(mpsc::Sender<Work>);
struct Work {
    turn: TurnRequest,
    hold: Hold,
    admission: oneshot::Sender<Ticket>,
}
type Output = mpsc::UnboundedSender<(TurnEvent, Hold)>;
struct Accepted {
    turn: TurnRequest,
    output: Output,
}
struct Active {
    output: Output,
    held: Option<String>,
    text: String,
    input_tokens: u64,
    phase: Phase,
    tools: Vec<crate::conversation::ClientTool>,
    tool_call: Option<crate::conversation::ToolCall>,
}
enum Phase {
    Prompt,
    Settling(Moment),
}
struct Core {
    active: Option<Active>,
    waiting: VecDeque<Accepted>,
    session_tools: Option<Vec<crate::conversation::ClientTool>>,
}
impl Default for Core {
    fn default() -> Self {
        Self {
            active: None,
            waiting: VecDeque::new(),
            session_tools: Some(Vec::new()),
        }
    }
}
enum SessionPlan {
    Current,
    New(Vec<crate::conversation::ClientTool>),
}
enum Event {
    Accepted(Accepted),
    Runtime(RuntimeEvent),
    Deadline,
}
enum Command {
    Prompt {
        text: Vec<crate::conversation::PromptBlock>,
        model: String,
        session: SessionPlan,
    },
    Emit(Output, TurnEvent),
    Permission {
        request: crate::runtime_driver::PermissionId,
        cancel: bool,
    },
}
impl Core {
    fn next_deadline(&self) -> Moment {
        match self.active.as_ref().map(|a| &a.phase) {
            Some(Phase::Settling(at)) => *at,
            _ => u64::MAX,
        }
    }
    fn handle(&mut self, event: Event, now: Moment) -> Vec<Command> {
        let mut commands = Vec::new();
        match event {
            Event::Accepted(request) => self.waiting.push_back(request),
            Event::Runtime(RuntimeEvent::Failed(error)) => {
                self.session_tools = None;
                self.finish(Err(error), &mut commands);
            }
            Event::Runtime(RuntimeEvent::Closed) => {
                self.session_tools = None;
                self.finish(Err("runtime stdout closed".into()), &mut commands)
            }
            Event::Runtime(RuntimeEvent::Text(text)) => {
                if let Some(active) = self.active.as_mut()
                    && active.tool_call.is_none()
                    && let Some(previous) = active.held.replace(text)
                {
                    active.text.push_str(&previous);
                    commands.push(Command::Emit(
                        active.output.clone(),
                        TurnEvent::Text(previous),
                    ));
                }
            }
            Event::Runtime(RuntimeEvent::Permission {
                request,
                title,
                arguments,
            }) => {
                let mut cancel = false;
                if let Some(active) = self.active.as_mut()
                    && matches!(active.phase, Phase::Prompt)
                    && active.tool_call.is_none()
                    && let Some(name) = title.strip_prefix("Run ").and_then(|t| t.strip_suffix('?'))
                    && active.tools.iter().any(|t| t.name == name)
                    && let Some(arguments) = arguments
                {
                    active.tool_call = Some(crate::conversation::ToolCall {
                        name: name.into(),
                        arguments,
                    });
                    cancel = true;
                }
                commands.push(Command::Permission { request, cancel });
            }
            Event::Runtime(RuntimeEvent::Usage(used)) => {
                if let Some(active) = self.active.as_mut() {
                    active.input_tokens = used;
                }
            }
            Event::Runtime(RuntimeEvent::PromptEnded) => {
                if let Some(active) = self.active.as_mut() {
                    active.phase = Phase::Settling(now.saturating_add(200));
                }
            }
            Event::Runtime(RuntimeEvent::Stderr | RuntimeEvent::Other) => {}
            Event::Deadline => {
                if self.next_deadline() <= now {
                    self.finish(Ok(()), &mut commands);
                }
            }
        }
        if self.active.is_none()
            && let Some(request) = self.waiting.pop_front()
        {
            let prompt = crate::conversation::prompt(&request.turn);
            let tools = match request.turn.tool_choice {
                crate::conversation::ToolChoice::Off => Vec::new(),
                crate::conversation::ToolChoice::Auto => request.turn.tools,
            };
            let session = if self.session_tools.as_ref() == Some(&tools) {
                SessionPlan::Current
            } else {
                self.session_tools = Some(tools.clone());
                SessionPlan::New(tools.clone())
            };
            self.active = Some(Active {
                output: request.output,
                held: None,
                text: String::new(),
                input_tokens: 0,
                phase: Phase::Prompt,
                tools: tools.clone(),
                tool_call: None,
            });
            commands.push(Command::Prompt {
                text: prompt,
                model: request.turn.model,
                session,
            });
        }
        commands
    }
    fn finish(&mut self, result: Result<(), String>, commands: &mut Vec<Command>) {
        if let Some(mut active) = self.active.take() {
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
            commands.push(Command::Emit(active.output, TurnEvent::End(end)));
        }
    }
}
impl Conductor {
    pub(crate) fn start(
        mut runtime: Option<Runtime>,
        clock: Arc<dyn Clock>,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (send, mut recv) = mpsc::channel::<Work>(32);
        let task = tokio::spawn(async move {
            let mut core = Core::default();
            let mut read_open = runtime.is_some();
            loop {
                let timer = clock.sleep_until(core.next_deadline());
                let (event, hold) = tokio::select! {
                    work=recv.recv()=> {
                        let Some(work)=work else {break;};
                        let (output,ticket)=mpsc::unbounded_channel();
                        if work.admission.send(ticket).is_err() {continue;}
                        (Event::Accepted(Accepted {turn:work.turn,output}),work.hold)
                    }
                    (event,hold)=async {runtime.as_mut().expect("readable runtime").next().await},if read_open=> {
                        if matches!(event,RuntimeEvent::Closed) {read_open=false;}
                        (Event::Runtime(event),hold)
                    }
                    hold=timer=>(Event::Deadline,hold),
                };
                let mut commands: VecDeque<_> = core.handle(event, clock.now()).into();
                while let Some(command) = commands.pop_front() {
                    match command {
                        Command::Prompt {
                            text,
                            model,
                            session,
                        } => {
                            let result = match runtime.as_mut().filter(|_| read_open) {
                                Some(runtime) => {
                                    let prepared = match session {
                                        SessionPlan::Current => Ok(()),
                                        SessionPlan::New(tools) => {
                                            runtime.new_session(&tools).await
                                        }
                                    };
                                    match prepared {
                                        Ok(()) => runtime.start_prompt(text, &model).await,
                                        Err(error) => Err(error),
                                    }
                                }
                                None => Err("official runtime is not available".into()),
                            };
                            if let Err(error) = result {
                                commands.extend(core.handle(
                                    Event::Runtime(RuntimeEvent::Failed(error)),
                                    clock.now(),
                                ));
                            }
                        }
                        Command::Permission { request, cancel } => {
                            if let Some(runtime) = runtime.as_mut()
                                && let Err(error) = runtime.permission(request, cancel).await
                            {
                                commands.extend(core.handle(
                                    Event::Runtime(RuntimeEvent::Failed(error)),
                                    clock.now(),
                                ));
                            }
                        }
                        Command::Emit(output, event) => {
                            let _ = output.send((event, clock.hold(HoldKind::TurnEvent)));
                        }
                    }
                }
                drop(hold);
            }
            if let Some(runtime) = runtime {
                runtime.stop().await;
            }
        });
        (Self(send), task)
    }
    pub(crate) async fn admit(&self, turn: TurnRequest, hold: Hold) -> Result<Ticket, String> {
        let (admission, reply) = oneshot::channel();
        self.0
            .send(Work {
                turn,
                hold,
                admission,
            })
            .await
            .map_err(|_| "conductor stopped")?;
        reply.await.map_err(|_| "conductor stopped".into())
    }
}
