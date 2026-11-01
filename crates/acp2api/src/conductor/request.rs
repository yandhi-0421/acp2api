use super::quota::{QuotaPhase, QuotaPurpose};
use super::work::ResponseChannel;
use super::{
    Command, Core, Event, NO_UPDATE_LIMIT, Notice, SETTLE_DELAY, Work, WorkId, quota, records,
    records::{OutstandingWork, PendingCompletion},
    retry,
};
use crate::{
    Moment,
    conversation::{EffortAsk, PromptRequest, TurnEvent, Usage},
    messages::{Message, OwnerMessage},
    runtime_driver::TurnUpdate,
};
use agent_client_protocol_schema::v1::StopReason;
use std::sync::Arc;

enum Phase {
    QuotaSettled,
    Preparing(PromptRequest),
    Prompt {
        update_deadline: Moment,
    },
    FailedPrompt {
        at: Moment,
        error: Failure,
        quota_reset: Option<quota::Wait>,
    },
    AwaitingQuotaPreparation {
        wait: quota::Wait,
    },
    AwaitingQuotaEnd {
        wait: quota::Wait,
        at: Moment,
    },
    QuotaSettling {
        at: Moment,
        wait: quota::Wait,
    },
    Settling {
        at: Moment,
        can_continue: bool,
        result: Result<(), Failure>,
        quota_reset: Option<quota::Wait>,
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
            _ => None,
        }
    }
}

#[derive(Clone)]
pub(super) struct Failure {
    pub(super) message: crate::messages::Message,
    pub(super) at: Moment,
}

impl Failure {
    pub(super) fn at(message: crate::messages::Message, at: Moment) -> Self {
        Self { message, at }
    }
}

pub(super) enum Start {
    Prompt(Moment),
    Prepare(PromptRequest),
}

#[must_use]
enum UpdateRoute {
    Reply,
    QuotaSettlement,
}

#[must_use]
pub(super) enum PromptTurnAction {
    Continue,
    Cancel,
}

enum PromptTurnEnd {
    Prompt(Option<StopReason>),
    Failed(crate::messages::Message),
}

pub(super) enum Ending {
    Complete(Result<(), Failure>),
    QueryQuotaReset(quota::Wait),
    QueryRefusal,
}

/// What a request needs when it leaves before its prompt turn settles.
pub(super) enum Exit {
    Settled,
    Retire,
    CancelAndRetire,
}

pub(super) struct Active {
    pub(super) reply_signals: super::lockout::ReplySignals,
    pub(super) reply: ResponseChannel,
    pub(super) request: crate::journal::Request,
    session: Option<String>,
    pub(super) record: records::RequestRecord,
    pub(super) client_tools_hash: crate::conversation::ToolSetHash,
    pub(super) kind: crate::journal::SessionUse,
    held: Option<String>,
    text: String,
    thought_dropped: bool,
    input_tokens: u64,
    pub(super) model: String,
    pub(super) model_name: String,
    pub(super) effort: EffortAsk,
    phase: Phase,
    tool_call: Option<crate::conversation::ToolCall>,
    stop: crate::conversation::Stop,
    stops: Vec<String>,
    stop_prefix: String,
}

impl Active {
    pub(super) fn new(
        request: super::Accepted,
        session: Option<String>,
        start: Start,
        record: records::RequestRecord,
        kind: crate::journal::SessionUse,
    ) -> Self {
        let phase = match start {
            Start::Prompt(now) => Phase::prompt(now),
            Start::Prepare(prompt) => Phase::Preparing(prompt),
        };
        Self {
            reply_signals: super::lockout::ReplySignals::default(),
            reply: request.reply,
            request: request.request,
            session,
            record,
            client_tools_hash: crate::conversation::tool_set_hash(&request.turn.tools),
            kind,
            held: None,
            text: String::new(),
            thought_dropped: false,
            input_tokens: request.turn.input_tokens,
            model: request.model.model,
            model_name: request.turn.model,
            effort: request.turn.effort,
            phase,
            tool_call: None,
            stop: crate::conversation::Stop::EndTurn,
            stops: request.turn.stop,
            stop_prefix: String::new(),
        }
    }

    pub(super) fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub(super) fn update_deadline(&self) -> Option<Moment> {
        self.phase.update_deadline()
    }

    pub(super) fn completion_deadline(&self) -> Option<Moment> {
        match self.phase {
            Phase::Settling { at, .. }
            | Phase::QuotaSettling { at, .. }
            | Phase::FailedPrompt { at, .. } => Some(at),
            Phase::AwaitingQuotaEnd { at, .. } => Some(at),
            _ => None,
        }
    }

    fn ready_to_finish(&self, now: Moment) -> bool {
        matches!(self.phase,
            Phase::Settling { at, .. } | Phase::QuotaSettling { at, .. } if at <= now
        )
    }

    pub(super) fn quota_wait(&self) -> Option<&quota::Wait> {
        match &self.phase {
            Phase::AwaitingQuotaPreparation { wait }
            | Phase::AwaitingQuotaEnd { wait, .. }
            | Phase::QuotaSettling { wait, .. } => Some(wait),
            Phase::FailedPrompt { quota_reset, .. } | Phase::Settling { quota_reset, .. } => {
                quota_reset.as_ref()
            }
            _ => None,
        }
    }

    pub(super) fn exit(&self) -> Exit {
        if matches!(
            self.phase,
            Phase::Settling { .. } | Phase::QuotaSettling { .. }
        ) {
            Exit::Settled
        } else if self.phase.update_deadline().is_some()
            || matches!(
                self.phase,
                Phase::FailedPrompt { .. } | Phase::AwaitingQuotaEnd { .. }
            )
        {
            Exit::CancelAndRetire
        } else {
            Exit::Retire
        }
    }

    fn waiting_for_stdout(&self) -> bool {
        matches!(
            self.phase,
            Phase::FailedPrompt { .. } | Phase::AwaitingQuotaEnd { .. }
        )
    }

    fn stored_error(&self, now: Moment) -> Option<Failure> {
        match &self.phase {
            Phase::FailedPrompt { error, .. } => Some(error.clone()),
            Phase::AwaitingQuotaPreparation { wait } | Phase::AwaitingQuotaEnd { wait, .. } => {
                Some(Failure::at(wait.message(now), now))
            }
            _ => None,
        }
    }

    fn wait_for_stdout(&mut self, error: Failure, now: Moment) -> bool {
        let (at, quota_reset) = match &self.phase {
            Phase::Prompt { .. } => (now.saturating_add(SETTLE_DELAY), None),
            Phase::FailedPrompt {
                at, quota_reset, ..
            } => (*at, quota_reset.clone()),
            Phase::AwaitingQuotaEnd { at, wait } => (*at, Some(wait.clone())),
            _ => return false,
        };
        self.phase = Phase::FailedPrompt {
            at,
            error,
            quota_reset,
        };
        true
    }

    /// Resets the normal no-update deadline without extending a failed turn's wait.
    fn update_received(&mut self, now: Moment) -> UpdateRoute {
        if matches!(self.phase, Phase::Prompt { .. }) {
            self.phase = Phase::prompt(now);
        }
        if matches!(self.phase, Phase::QuotaSettling { .. }) {
            UpdateRoute::QuotaSettlement
        } else {
            UpdateRoute::Reply
        }
    }

    /// Cancels the prompt turn when the text ends the reply at a stop sequence.
    pub(super) fn text_received(
        &mut self,
        text: String,
        now: Moment,
        commands: &mut Vec<Command>,
    ) -> PromptTurnAction {
        if self.tool_call.is_none() && self.release_or_cut_reply(Some(text), now, commands) {
            PromptTurnAction::Cancel
        } else {
            PromptTurnAction::Continue
        }
    }

    fn usage_received(&mut self, used: u64) {
        self.input_tokens = used;
    }

    fn thought_dropped(&mut self) -> Option<Command> {
        if self.thought_dropped {
            return None;
        }
        self.thought_dropped = true;
        Some(Command::Diagnostic(
            self.request.clone(),
            crate::journal::Diagnostic::DroppedThoughtText,
        ))
    }

    pub(super) fn client_tool_called(
        &mut self,
        call: crate::conversation::ToolCall,
    ) -> PromptTurnAction {
        let cancel = matches!(self.phase, Phase::Prompt { .. }) && self.tool_call.is_none();
        if cancel {
            self.tool_call = Some(call);
            PromptTurnAction::Cancel
        } else {
            PromptTurnAction::Continue
        }
    }

    fn prompt_turn_ended(&mut self, end: PromptTurnEnd, now: Moment) {
        match &self.phase {
            Phase::FailedPrompt {
                error, quota_reset, ..
            } => {
                self.phase = Phase::Settling {
                    quota_reset: quota_reset.clone(),
                    at: now,
                    can_continue: false,
                    result: Err(error.clone()),
                };
            }
            Phase::AwaitingQuotaPreparation { wait } | Phase::AwaitingQuotaEnd { wait, .. } => {
                self.phase = Phase::QuotaSettling {
                    at: now,
                    wait: wait.clone(),
                };
            }
            _ => match end {
                PromptTurnEnd::Prompt(stop_reason) => self.prompt_stopped(stop_reason, now),
                PromptTurnEnd::Failed(error) => {
                    self.phase = Phase::Settling {
                        quota_reset: None,
                        at: now.saturating_add(SETTLE_DELAY),
                        can_continue: false,
                        result: Err(Failure::at(error, now)),
                    };
                }
            },
        }
    }

    pub(super) fn quota_signal(&mut self, wait: quota::Wait, now: Moment) {
        self.phase = match self.phase {
            Phase::Settling { .. } | Phase::QuotaSettling { .. } => {
                Phase::QuotaSettling { at: now, wait }
            }
            Phase::AwaitingQuotaEnd { at, .. } => Phase::AwaitingQuotaEnd { at, wait },
            Phase::FailedPrompt { at, .. } => Phase::AwaitingQuotaEnd { at, wait },
            Phase::Preparing(_) | Phase::AwaitingQuotaPreparation { .. } => {
                Phase::AwaitingQuotaPreparation { wait }
            }
            _ => Phase::AwaitingQuotaEnd {
                wait,
                at: now.saturating_add(SETTLE_DELAY),
            },
        };
    }

    fn quota_failure(&mut self, wait: quota::Wait, now: Moment) {
        self.phase = Phase::QuotaSettling {
            at: if self.waiting_for_stdout() {
                now
            } else {
                now.saturating_add(SETTLE_DELAY)
            },
            wait,
        };
    }

    pub(super) fn prepared(&mut self, session: String, now: Moment) -> Option<PromptRequest> {
        self.session = Some(session);
        if let Phase::AwaitingQuotaPreparation { wait } | Phase::AwaitingQuotaEnd { wait, .. } =
            &self.phase
        {
            self.phase = Phase::QuotaSettling {
                at: now,
                wait: wait.clone(),
            };
            return None;
        }
        match std::mem::replace(&mut self.phase, Phase::prompt(now)) {
            Phase::Preparing(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// Decides what ends the request after its prompt turn.
    pub(super) fn end(
        &mut self,
        result: Result<(), Failure>,
        can_query_reset: bool,
        refusal: Option<crate::messages::Message>,
        now: Moment,
    ) -> Ending {
        if result.is_ok()
            && let Phase::QuotaSettling { wait, .. } = &self.phase
        {
            let wait = wait.clone();
            self.phase = Phase::QuotaSettled;
            if can_query_reset {
                return Ending::QueryQuotaReset(wait);
            }
            return Ending::Complete(Err(Failure::at(
                refusal.unwrap_or_else(|| wait.message(now)),
                now,
            )));
        }
        if result.is_ok() && matches!(self.stop, crate::conversation::Stop::Refusal) {
            return Ending::QueryRefusal;
        }
        Ending::Complete(result)
    }

    pub(super) fn quota_settled(&mut self) {
        self.phase = Phase::QuotaSettled;
    }

    pub(super) fn disconnected(self) -> Command {
        Command::Complete(crate::journal::Completion {
            request: self.request,
            session: self.session,
            session_use: Some(self.kind),
            outcome: crate::journal::Outcome::Disconnected,
        })
    }

    pub(super) fn finish(
        mut self,
        result: Result<(), Failure>,
        now: Moment,
        catalog: Option<&crate::catalog::Catalog>,
    ) -> (
        Vec<Command>,
        Option<retry::Wait>,
        Option<records::RequestRecord>,
    ) {
        let mut commands = Vec::new();
        let (can_continue, result) = match &mut self.phase {
            Phase::Settling {
                can_continue,
                result: pending,
                ..
            } => (
                *can_continue,
                result.and(std::mem::replace(pending, Ok(()))),
            ),
            Phase::Preparing(_)
            | Phase::Prompt { .. }
            | Phase::FailedPrompt { .. }
            | Phase::AwaitingQuotaPreparation { .. }
            | Phase::AwaitingQuotaEnd { .. }
            | Phase::QuotaSettling { .. }
            | Phase::QuotaSettled => (false, result),
        };
        let tool_hash = self
            .tool_call
            .as_ref()
            .map(|call| crate::conversation::tool_call_hash(&call.name, &call.arguments));
        let retry = result
            .as_ref()
            .err()
            .and_then(|failure| retry::Wait::after(&self, failure, catalog));
        if result.as_ref().is_err_and(|failure| {
            !matches!(
                failure.message,
                crate::messages::Message::MaxTurnRequests | crate::messages::Message::NoUpdate
            )
        }) {
            self.held = None;
        }
        self.release_or_cut_reply(None, now, &mut commands);
        let can_continue = can_continue && self.stop.sequence().is_none();
        let end = result.map_err(|failure| failure.message).map(|()| {
            let mut output = self.text.clone();
            if let Some(call) = self.tool_call.take() {
                output.push_str(&call.arguments);
                commands.push(Command::Emit(
                    self.reply.output.clone(),
                    TurnEvent::ToolCall(call),
                ));
            }
            Usage {
                input_tokens: self.input_tokens,
                output_tokens: crate::conversation::estimate_tokens(&output),
            }
        });
        let record = if self.session.is_some() && end.is_ok() && can_continue {
            Some(self.record.with_reply(&self.text, tool_hash))
        } else {
            None
        };
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
            request: self.request,
            session: self.session,
            session_use: Some(self.kind),
            outcome,
        }));
        if end.is_ok() {
            commands.push(Command::Emit(
                self.reply.output.clone(),
                TurnEvent::Stop(self.stop),
            ));
        }
        commands.push(Command::Emit(self.reply.output, TurnEvent::End(end)));
        (commands, retry, record)
    }

    fn prompt_stopped(&mut self, stop_reason: Option<StopReason>, now: Moment) {
        if stop_reason == Some(StopReason::Refusal) {
            self.stop = crate::conversation::Stop::Refusal;
        }
        if stop_reason == Some(StopReason::MaxTokens) && self.stop.sequence().is_none() {
            self.stop = crate::conversation::Stop::MaxTokens;
        }
        self.phase = Phase::Settling {
            quota_reset: None,
            at: now.saturating_add(SETTLE_DELAY),
            result: if stop_reason == Some(StopReason::MaxTurnRequests) {
                Err(Failure::at(crate::messages::Message::MaxTurnRequests, now))
            } else {
                Ok(())
            },
            can_continue: stop_reason == Some(StopReason::EndTurn)
                || stop_reason == Some(StopReason::MaxTokens)
                || stop_reason == Some(StopReason::Cancelled) && self.tool_call.is_some(),
        };
    }

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

impl Core {
    pub(super) fn update(
        &mut self,
        id: WorkId,
        session: String,
        update: TurnUpdate,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        if let TurnUpdate::Usage { size, .. } = &update {
            let model = match &self.work[&id] {
                Work::Retiring(outstanding) => Some(outstanding.model()),
                Work::QuotaRetiring { .. } => None,
                work => Some(work.execution().unwrap().model.as_str()),
            };
            if let Some(model) = model {
                self.context_sizes.insert(model.to_owned(), *size);
            }
        }
        if matches!(
            self.work[&id],
            Work::Retiring(_) | Work::QuotaRetiring { .. }
        ) {
            match update {
                TurnUpdate::PromptEnded { .. } | TurnUpdate::Failed(_) => {
                    let reply = self
                        .work
                        .get_mut(&id)
                        .unwrap()
                        .reply_signals_mut()
                        .unwrap()
                        .clone();
                    self.sessions
                        .remember_reply(session, reply, now.saturating_add(SETTLE_DELAY));
                    self.finish(id, Ok(()), now, commands);
                }
                TurnUpdate::Permission { request, .. }
                | TurnUpdate::RefusedPermission { request, .. } => {
                    commands.push(Command::Permission(request));
                }
                TurnUpdate::Activity
                | TurnUpdate::Text(_)
                | TurnUpdate::Usage { .. }
                | TurnUpdate::ThoughtDropped
                | TurnUpdate::UnpermittedTool(_) => {}
            }
            return;
        }
        if let TurnUpdate::Failed(error) = &update
            && let Some((wait, known)) = self.quota_signal(error, now, wall, commands)
        {
            if known {
                let reply = self
                    .work
                    .get_mut(&id)
                    .unwrap()
                    .reply_signals_mut()
                    .unwrap()
                    .clone();
                self.sessions
                    .remember_reply(session, reply, now.saturating_add(SETTLE_DELAY));
                self.finish(id, Err(Failure::at(wait.message(now), now)), now, commands);
            } else {
                let active = self.work.get_mut(&id).unwrap().execution_mut().unwrap();
                active.quota_failure(wait, now);
                if active.ready_to_finish(now) {
                    self.finish(id, Ok(()), now, commands);
                }
            }
            return;
        }
        let active = self.work.get_mut(&id).unwrap().execution_mut().unwrap();
        let failed_end = active.waiting_for_stdout()
            && matches!(
                update,
                TurnUpdate::PromptEnded { .. } | TurnUpdate::Failed(_)
            );
        let route = active.update_received(now);
        let quota_settlement = matches!(route, UpdateRoute::QuotaSettlement);
        match update {
            TurnUpdate::PromptEnded { stop_reason } => {
                active.prompt_turn_ended(PromptTurnEnd::Prompt(stop_reason), now);
            }
            TurnUpdate::Failed(error) => {
                active.prompt_turn_ended(PromptTurnEnd::Failed(error), now)
            }
            TurnUpdate::Permission { request, .. }
            | TurnUpdate::RefusedPermission { request, .. }
                if quota_settlement =>
            {
                commands.push(Command::Permission(request));
            }
            _ if quota_settlement => {}
            TurnUpdate::Activity => {}
            TurnUpdate::Text(text) => {
                if matches!(
                    active.text_received(text, now, commands),
                    PromptTurnAction::Cancel
                ) {
                    let session = active.session().unwrap().to_owned();
                    commands.push(self.sessions.cancel(session, now));
                }
            }
            TurnUpdate::Usage { used, .. } => active.usage_received(used),
            TurnUpdate::ThoughtDropped => commands.extend(active.thought_dropped()),
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
                if matches!(active.client_tool_called(call), PromptTurnAction::Cancel) {
                    commands.push(self.sessions.cancel(session, now));
                }
                commands.push(Command::Permission(request));
            }
        }
        if failed_end {
            self.finish(id, Ok(()), now, commands);
        }
    }

    pub(super) fn runtime_prepared(
        &mut self,
        work_id: Option<WorkId>,
        work: String,
        result: Result<(String, crate::catalog::Catalog), Message>,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        if let Some(id) = work_id {
            match result {
                Ok((session, catalog)) => {
                    self.catalog = Some(catalog);
                    self.sign_in.clear_required();
                    match self.work.get_mut(&id).unwrap() {
                        Work::ModelsReading(_) => {
                            commands.extend(self.sessions.models_ready(session));
                            self.finish(id, Ok(()), now, commands);
                        }
                        Work::Checking(_) => {
                            commands.push(Command::AdmitChecked { id, session });
                        }
                        Work::Retiring(outstanding) => {
                            outstanding.prepared(session);
                            self.finish(id, Ok(()), now, commands);
                        }
                        entry => {
                            let active = entry.execution_mut().unwrap();
                            if let Some(prompt) = active.prepared(session.clone(), now) {
                                if let Some(message) = self.refusal(now) {
                                    self.finish(id, Err(Failure::at(message, now)), now, commands);
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
                Err(error) if matches!(self.work[&id], Work::ModelsReading(_)) => {
                    let _ = self.quota_signal(&error, now, wall, commands);
                    self.finish(id, Err(Failure::at(error, now)), now, commands);
                }
                Err(error) => {
                    if let Some((wait, known)) = self.quota_signal(&error, now, wall, commands) {
                        let pending_model = match &self.work[&id] {
                            Work::Checking(request) => Some(request.turn.model.clone()),
                            Work::Retiring(outstanding) => Some(outstanding.model().to_owned()),
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
                                commands,
                            );
                            if !known {
                                self.enqueue_reset_query(wait, pending_model);
                            }
                        } else {
                            let mut active = self.work.remove(&id).unwrap().into_active().unwrap();
                            active.quota_settled();
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
                        self.finish(id, Err(Failure::at(error, now)), now, commands);
                    }
                }
            }
        } else if let Ok((session, _)) = result {
            commands.push(Command::Forget(session));
        }
    }

    pub(super) fn fail_request(
        &mut self,
        id: WorkId,
        session: String,
        error: crate::messages::Message,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        if self.work[&id]
            .execution()
            .is_some_and(|active| matches!(active.exit(), Exit::CancelAndRetire))
        {
            commands.push(self.sessions.cancel(session, now));
        }
        self.end_request(id, Failure::at(error, now), now, commands);
    }

    fn end_request(
        &mut self,
        id: WorkId,
        error: Failure,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        let active = self.work.remove(&id).unwrap().into_active().unwrap();
        let outstanding = (!matches!(active.exit(), Exit::Settled))
            .then(|| OutstandingWork::from_active(&active, PendingCompletion::AlreadyCompleted));
        let (forget, completion) = self.complete_request(active, Err(error), now);
        commands.extend(completion);
        if let Some(outstanding) = outstanding {
            self.work.restore(id, Work::Retiring(outstanding));
        } else {
            commands.extend(forget.map(Command::Forget));
        }
    }

    pub(super) fn finish(
        &mut self,
        id: WorkId,
        result: Result<(), Failure>,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        match self.work.remove(&id).unwrap() {
            Work::ModelsQueued(read) | Work::ModelsReading(read) => {
                let result = result.map_err(|failure| OwnerMessage::Api(failure.message));
                commands.extend(read.replies.into_iter().map(|reply| Command::ModelsRead {
                    reply,
                    result: result.clone(),
                }));
            }
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
            Work::Retiring(outstanding) => commands.extend(outstanding.finish()),
            Work::QuotaRetiring { session, .. } => commands.push(Command::Forget(session)),
            work => {
                let mut active = work.into_active().unwrap();
                let can_query_reset = active
                    .quota_wait()
                    .is_some_and(|wait| self.can_query_quota_reset(wait));
                match active.end(result, can_query_reset, self.refusal(now), now) {
                    Ending::Complete(result) => {
                        let (forget, completion) = self.complete_request(active, result, now);
                        commands.extend(completion);
                        commands.extend(forget.map(Command::Forget));
                    }
                    Ending::QueryQuotaReset(wait) => {
                        let session = active.session().map(str::to_owned);
                        self.enqueue_quota(
                            QuotaPurpose::Reset {
                                wait,
                                model: Some(active.model.clone()),
                                response: Some(active),
                            },
                            session,
                        );
                    }
                    Ending::QueryRefusal => {
                        let model = active.model.clone();
                        let session = active.session().map(str::to_owned);
                        self.enqueue_quota(
                            QuotaPurpose::Refusal {
                                model,
                                response: Some(active),
                            },
                            session,
                        );
                    }
                }
            }
        }
    }

    pub(super) fn complete_request(
        &mut self,
        active: Active,
        result: Result<(), Failure>,
        now: Moment,
    ) -> (Option<String>, Vec<Command>) {
        self.keep_quota_wait_reset_query(&active);
        let session = active.session().map(str::to_owned);
        let (commands, retry, record) = active.finish(result, now, self.catalog.as_ref());
        self.retry_waits.extend(retry);
        let forget = session.and_then(|session| self.sessions.keep(session, record));
        (forget, commands)
    }

    pub(super) fn end_all_work(
        &mut self,
        refusal: &'static str,
        message: impl Fn(WorkId) -> crate::messages::Message,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        self.sessions.clear_cancels();
        for work in self.work.values() {
            if let Work::QuotaWorking {
                phase: QuotaPhase::Prompt(session) | QuotaPhase::FailedPrompt { session, .. },
                ..
            } = work
            {
                commands.push(Command::Cancel(session.clone()));
            }
            if let Some(active) = work.execution()
                && matches!(active.exit(), Exit::CancelAndRetire)
                && let Some(session) = active.session()
            {
                commands.push(Command::Cancel(session.to_owned()));
            }
        }
        while let Some(id) = self.work.keys().next().copied() {
            let error = message(id);
            if matches!(self.work[&id], Work::AcceptedQueued(_)) {
                let request = self.work.remove(&id).unwrap().into_queued().unwrap();
                Self::refuse_request(
                    request,
                    crate::journal::Outcome::Refused(refusal),
                    error,
                    commands,
                );
            } else {
                self.finish(id, Err(Failure::at(error, now)), now, commands);
            }
        }
    }

    pub(super) fn client_disconnected(
        &mut self,
        request_id: &str,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        let id = self.work.by_request(request_id).unwrap();
        if let Work::QuotaQueued { query, .. } | Work::QuotaWorking { query, .. } =
            self.work.get_mut(&id).unwrap()
        {
            query.disconnect(commands);
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
                    self.work.restore(
                        id,
                        Work::Retiring(OutstandingWork::disconnected_check(request)),
                    );
                }
                Work::AcceptedQueued(request) => Self::drop_queued(request, commands),
                Work::AcceptedWorking(active) => {
                    self.keep_quota_wait_reset_query(&active);
                    let (outstanding, disconnected) = self.sessions.disconnect(active, now);
                    commands.extend(disconnected);
                    if let Some(outstanding) = outstanding {
                        self.work.restore(id, Work::Retiring(outstanding));
                    }
                }
                Work::Retiring(_)
                | Work::QuotaRetiring { .. }
                | Work::ModelsQueued(_)
                | Work::ModelsReading(_)
                | Work::QuotaQueued { .. }
                | Work::QuotaWorking { .. } => unreachable!(),
            }
        }
    }

    pub(super) fn upstream_error_reported(
        &mut self,
        id: Option<WorkId>,
        session: String,
        message: crate::messages::Message,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        if id.is_none() && self.work.has_queued_session(&session) {
            let _ = self.quota_signal(&message, now, wall, commands);
            return;
        }
        if let Some((wait, known)) = self.quota_signal(&message, now, wall, commands) {
            if let Some(id) = id {
                if let Some(active) = self.work.get_mut(&id).unwrap().execution_mut() {
                    if known {
                        let error = wait.message(now);
                        if !active.wait_for_stdout(Failure::at(error.clone(), now), now) {
                            self.end_request(id, Failure::at(error, now), now, commands);
                        }
                    } else {
                        active.quota_signal(wait, now);
                        if active.ready_to_finish(now) {
                            self.finish(id, Ok(()), now, commands);
                        }
                    }
                } else if !known && let Work::Retiring(outstanding) = &self.work[&id] {
                    let model = outstanding.model().to_owned();
                    self.enqueue_reset_query(wait, Some(model));
                }
            } else {
                commands.push(self.sessions.forget(session));
                if !known {
                    self.enqueue_reset_query(wait, None);
                }
            }
        } else if let Some(id) = id {
            if !matches!(
                self.work[&id],
                Work::Retiring(_) | Work::QuotaRetiring { .. }
            ) {
                let active = self.work.get_mut(&id).unwrap().execution_mut().unwrap();
                if !active.wait_for_stdout(Failure::at(message.clone(), now), now) {
                    self.fail_request(id, session, message, now, commands);
                }
            }
        } else {
            commands.push(self.sessions.forget(session));
        }
    }

    pub(super) fn end_occupying_work(
        &mut self,
        message: crate::messages::Message,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        let signal = self.work.values_mut().find_map(|work| {
            let key = work.key().to_owned();
            let (error, signal) = work.reply_signals_mut()?.ended_error()?;
            Some((key, error, signal))
        });
        if let Some((key, error, signal)) = signal {
            let id = self.work.by_request(&key);
            self.start_lockout(signal, error, id, now, wall, commands);
            return;
        }
        while let Some(id) = self
            .work
            .iter()
            .find_map(|(id, work)| work.occupies_place().then_some(*id))
        {
            let failure = self.work[&id]
                .execution()
                .and_then(|active| active.stored_error(now))
                .unwrap_or_else(|| Failure::at(message.clone(), now));
            self.finish(id, Err(failure), now, commands);
        }
    }

    pub(super) fn deadline_reached(
        &mut self,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        self.retire_idle(now, commands);
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
            let error = self.work[&id]
                .execution()
                .and_then(|active| match &active.phase {
                    Phase::AwaitingQuotaEnd { wait, .. } => Some(Failure::at(
                        self.refusal(now).unwrap_or_else(|| wait.message(now)),
                        now,
                    )),
                    Phase::FailedPrompt { error, .. } => Some(error.clone()),
                    _ => None,
                });
            if let Some(error) = error {
                let session = self.work[&id]
                    .execution()
                    .unwrap()
                    .session()
                    .unwrap()
                    .to_owned();
                commands.push(self.sessions.cancel(session, now));
                self.end_request(id, error, now, commands);
                continue;
            }
            match &self.work[&id] {
                Work::QuotaWorking { .. } => {
                    self.quota_event(id, Event::Deadline, None, now, wall, commands)
                }
                _ => self.finish(id, Ok(()), now, commands),
            }
        }
    }
}
