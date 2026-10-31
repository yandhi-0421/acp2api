use super::{
    Accepted, Command, Core, Event, PendingRequest, QUEUE_CAPACITY, ResponseMode, RuntimeState,
    TurnWork, Work, WorkId,
    keep_alive::KEEP_ALIVE_INTERVAL,
    records, request,
    request::Active,
    retry,
    work::{ModelRead, ResponseChannel, Session},
};
use crate::{
    Clock, Hold, HoldKind, Moment,
    conversation::{EffortAsk, PromptRequest, Ticket, TurnEvent, TurnRequest},
    messages::OwnerMessage,
    runtime_builds::LaunchFailure,
};
use tokio::sync::{mpsc, oneshot};

impl Core {
    pub(super) fn refusal(&self, now: Moment) -> Option<crate::messages::Message> {
        if let Some(evidence) = &self.lockout {
            return Some(crate::messages::Message::lockout(
                evidence,
                &self.panel_address,
                false,
            ));
        }
        if let RuntimeState::Unavailable(reason) = self.lifecycle.state() {
            return Some(crate::messages::Message::RuntimeUnavailable(
                reason.message.clone(),
            ));
        }
        if self.sign_in.required() {
            return Some(self.sign_in_message());
        }
        self.quota
            .wait()
            .filter(|wait| wait.deadline > now)
            .map(|wait| wait.message(now))
    }
    pub(super) fn lockout_failure(&self, now: Moment) -> LaunchFailure {
        LaunchFailure::Owner(OwnerMessage::Api(
            self.refusal(now).expect("lockout just started"),
        ))
    }
    pub(super) fn owner_work_refusal(&self) -> Option<crate::messages::Message> {
        if let Some(evidence) = &self.lockout {
            Some(crate::messages::Message::lockout(
                evidence,
                &self.panel_address,
                false,
            ))
        } else if let RuntimeState::Unavailable(reason) = self.lifecycle.state() {
            Some(crate::messages::Message::RuntimeUnavailable(
                reason.message.clone(),
            ))
        } else if matches!(self.lifecycle.state(), RuntimeState::Stopping) {
            Some(crate::messages::Message::Stopped)
        } else {
            None
        }
    }
    pub(super) fn check(
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
    pub(super) fn check_model(
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
    pub(super) fn model_entries(
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
    pub(super) fn queue_full(&self) -> bool {
        self.work.queued() == QUEUE_CAPACITY
    }
    pub(super) fn admit(
        &self,
        work: TurnWork,
        journal: &crate::journal::Journal,
        clock: &dyn Clock,
    ) -> Option<(Event, Hold)> {
        let TurnWork { pending, hold } = work;
        if self.sign_in.required()
            && self.lockout.is_none()
            && !matches!(self.lifecycle.state(), RuntimeState::Unavailable(_))
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
    pub(super) fn accept(
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
    pub(super) fn start_request(
        &mut self,
        id: WorkId,
        request: Accepted,
        checked_session: Option<String>,
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
        let settings = Session::for_turn(&request.turn);
        let (selection, selected) =
            self.sessions
                .select(&request.turn, &settings, checked_session, now);
        commands.extend(selected);
        let records::Selection {
            kind,
            prompt,
            ready,
            record,
        } = selection;
        let prompt = PromptRequest {
            blocks: prompt,
            model: request.model.model.clone(),
            level: request.model.level,
        };
        let work = request.request.id.clone();
        let (session, start) = match ready {
            Some((id, records::Location::Loaded)) => {
                commands.push(Command::Prompt {
                    work: work.clone(),
                    session: id.clone(),
                    prompt,
                });
                (Some(id), request::Start::Prompt(now))
            }
            Some((session, records::Location::Unloaded(cwd))) => {
                commands.push(Command::Prepare {
                    work,
                    settings: settings.clone(),
                    load: Some((session.clone(), cwd)),
                });
                (Some(session), request::Start::Prepare(prompt))
            }
            None => {
                commands.push(Command::Prepare {
                    work,
                    settings: settings.clone(),
                    load: None,
                });
                (None, request::Start::Prepare(prompt))
            }
        };
        self.work.insert(
            id,
            Work::AcceptedWorking(Active::new(request, session, start, record, kind)),
        );
        true
    }
    pub(super) fn refuse_model_reads(
        &mut self,
        message: &crate::messages::Message,
        commands: &mut Vec<Command>,
    ) {
        let queued: Vec<_> = self
            .work
            .iter()
            .filter_map(|(id, work)| matches!(work, Work::ModelsQueued(_)).then_some(*id))
            .collect();
        for id in queued {
            let Work::ModelsQueued(read) = self.work.remove(&id).unwrap() else {
                unreachable!()
            };
            commands.extend(read.replies.into_iter().map(|reply| Command::ModelsRead {
                reply,
                result: Err(OwnerMessage::Api(message.clone())),
            }));
        }
    }
    pub(super) fn refuse_queued(
        &mut self,
        outcome: crate::journal::Outcome,
        message: crate::messages::Message,
        commands: &mut Vec<Command>,
    ) {
        self.refuse_model_reads(&message, commands);
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
    pub(super) fn refuse_request(
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
    pub(super) fn drop_queued(request: Accepted, commands: &mut Vec<Command>) {
        commands.push(Command::Complete(crate::journal::Completion {
            request: request.request,
            session: None,
            session_use: None,
            outcome: crate::journal::Outcome::Disconnected,
        }));
    }
    pub(super) fn start_queued_work(&mut self, now: Moment, commands: &mut Vec<Command>) {
        let mut places = self.work.places();
        while self.lockout.is_none()
            && matches!(self.lifecycle.state(), RuntimeState::Serving)
            && places < usize::from(self.limit)
        {
            let Some(id) = self.work.iter().find_map(|(id, work)| match work {
                Work::ModelsQueued(_) => Some(*id),
                Work::CheckQueued(_) => Some(*id),
                Work::QuotaQueued { .. } if !self.sign_in.required() => Some(*id),
                Work::AcceptedQueued(_)
                    if !self.sign_in.required() && self.quota.wait().is_none() =>
                {
                    Some(*id)
                }
                _ => None,
            }) else {
                break;
            };
            let request = match self.work.remove(&id).unwrap() {
                Work::ModelsQueued(read) => {
                    commands.push(Command::Prepare {
                        work: read.work.clone(),
                        settings: Session {
                            tools: Vec::new(),
                            system: None,
                        },
                        load: None,
                    });
                    self.work.insert(id, Work::ModelsReading(read));
                    places += 1;
                    continue;
                }
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
                    if self.start_quota_query(id, query, session, now, commands) {
                        places += 1;
                    }
                    continue;
                }
                work => work.into_queued().unwrap(),
            };
            if self.start_request(id, request, None, now, commands) {
                places += 1;
            }
        }
    }

    pub(super) fn read_model_names(&mut self, reply: oneshot::Sender<Result<(), OwnerMessage>>) {
        if self.catalog.is_some() || !self.model_names.is_empty() {
            let _ = reply.send(Ok(()));
        } else if let Some(read) = self.work.values_mut().find_map(|work| match work {
            Work::ModelsQueued(read) | Work::ModelsReading(read) => Some(read),
            _ => None,
        }) {
            read.replies.push(reply);
        } else {
            let error = self.owner_work_refusal().or_else(|| {
                self.queue_full()
                    .then_some(crate::messages::Message::QueueFull)
            });
            if let Some(error) = error {
                let _ = reply.send(Err(OwnerMessage::Api(error)));
            } else {
                let id = self.work.next_id();
                self.work.add(Work::ModelsQueued(ModelRead {
                    work: format!("models-{id}"),
                    replies: vec![reply],
                }));
            }
        }
    }
}
