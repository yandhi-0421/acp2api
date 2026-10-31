use super::{
    Command, NO_UPDATE_LIMIT, ResponseChannel, SETTLE_DELAY, StopReason, quota, records, retry,
};
use crate::{
    Moment,
    conversation::{EffortAsk, PromptRequest, TurnEvent, Usage},
};

enum Phase {
    AwaitingQuotaQuery,
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

    fn settled(&self) -> bool {
        matches!(self, Self::Settling { .. } | Self::QuotaSettling { .. })
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
pub(super) enum UpdateRoute {
    Reply,
    QuotaSettlement,
}
#[must_use]
pub(super) enum TurnAction {
    Continue,
    Cancel,
}
pub(super) enum TurnEnd {
    Prompt(Option<StopReason>),
    Failed(crate::messages::Message),
}

pub(super) enum Ending {
    Complete(Result<(), Failure>),
    QueryQuotaReset(quota::Wait),
    QueryRefusal,
}

pub(super) struct Active {
    pub(super) reply: ResponseChannel,
    pub(super) request: crate::journal::Request,
    pub(super) session: Option<String>,
    pub(super) record: records::RequestRecord,
    pub(super) client_tools_hash: crate::conversation::ToolSetHash,
    pub(super) kind: crate::journal::SessionUse,
    pub(super) held: Option<String>,
    pub(super) text: String,
    pub(super) thought_dropped: bool,
    pub(super) input_tokens: u64,
    pub(super) model: String,
    pub(super) model_name: String,
    pub(super) effort: EffortAsk,
    phase: Phase,
    tool_call: Option<crate::conversation::ToolCall>,
    stop: crate::conversation::Stop,
    stops: Vec<String>,
    pub(super) stop_prefix: String,
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

    pub(super) fn update_deadline(&self) -> Option<Moment> {
        self.phase.update_deadline()
    }
    pub(super) fn completion_deadline(&self) -> Option<Moment> {
        match self.phase {
            Phase::Settling { at, .. } | Phase::QuotaSettling { at, .. } => Some(at),
            _ => None,
        }
    }
    /// The prompt turn still runs, so that an end of the request must cancel it.
    pub(super) fn prompt_running(&self) -> bool {
        self.phase.update_deadline().is_some()
    }
    pub(super) fn settled(&self) -> bool {
        self.phase.settled()
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
            _ => (false, result),
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

    /// Resets the no-update deadline. A quota phase ignores the reply content of the update.
    pub(super) fn update_received(&mut self, now: Moment) -> UpdateRoute {
        match &mut self.phase {
            Phase::Prompt { .. } => self.phase = Phase::prompt(now),
            Phase::AwaitingQuotaEnd {
                update_deadline: Some(update_deadline),
                ..
            } => {
                *update_deadline = now + NO_UPDATE_LIMIT;
            }
            _ => {}
        }
        if matches!(
            self.phase,
            Phase::AwaitingQuotaEnd { .. } | Phase::QuotaSettling { .. }
        ) {
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
    ) -> TurnAction {
        if self.tool_call.is_none() && self.release_or_cut_reply(Some(text), now, commands) {
            TurnAction::Cancel
        } else {
            TurnAction::Continue
        }
    }

    pub(super) fn turn_ended(&mut self, end: TurnEnd, now: Moment) {
        match &self.phase {
            Phase::AwaitingQuotaEnd { wait, .. } => {
                self.phase = Phase::QuotaSettling {
                    at: now.saturating_add(SETTLE_DELAY),
                    wait: wait.clone(),
                };
            }
            Phase::QuotaSettling { .. } => {}
            _ => match end {
                TurnEnd::Prompt(stop_reason) => self.prompt_ended(stop_reason, now),
                TurnEnd::Failed(error) => {
                    self.phase = Phase::Settling {
                        at: now.saturating_add(SETTLE_DELAY),
                        can_continue: false,
                        result: Err(Failure::at(error, now)),
                    };
                }
            },
        }
    }

    fn prompt_ended(&mut self, stop_reason: Option<StopReason>, now: Moment) {
        if stop_reason == Some(StopReason::Refusal) {
            self.stop = crate::conversation::Stop::Refusal;
        }
        if stop_reason == Some(StopReason::MaxTokens) && self.stop.sequence().is_none() {
            self.stop = crate::conversation::Stop::MaxTokens;
        }
        self.phase = Phase::Settling {
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

    pub(super) fn client_tool_called(&mut self, call: crate::conversation::ToolCall) -> TurnAction {
        let cancel = matches!(self.phase, Phase::Prompt { .. }) && self.tool_call.is_none();
        if cancel {
            self.tool_call = Some(call);
            TurnAction::Cancel
        } else {
            TurnAction::Continue
        }
    }

    pub(super) fn quota_signal(&mut self, wait: quota::Wait) {
        self.phase = match self.phase {
            Phase::Settling { at, .. } | Phase::QuotaSettling { at, .. } => {
                Phase::QuotaSettling { at, wait }
            }
            _ => Phase::AwaitingQuotaEnd {
                wait,
                update_deadline: self.phase.update_deadline(),
            },
        };
    }

    pub(super) fn quota_failure(&mut self, wait: quota::Wait, now: Moment) {
        self.phase = Phase::QuotaSettling {
            at: now.saturating_add(SETTLE_DELAY),
            wait,
        };
    }

    pub(super) fn prepared(&mut self, session: String, now: Moment) -> Option<PromptRequest> {
        self.session = Some(session);
        if let Phase::AwaitingQuotaEnd { wait, .. } = &self.phase {
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
        let result = match (
            result,
            std::mem::replace(&mut self.phase, Phase::AwaitingQuotaQuery),
        ) {
            (Ok(()), Phase::QuotaSettling { wait, .. }) => {
                if can_query_reset {
                    return Ending::QueryQuotaReset(wait);
                }
                Err(Failure::at(
                    refusal.unwrap_or_else(|| wait.message(now)),
                    now,
                ))
            }
            (result, phase) => {
                self.phase = phase;
                result
            }
        };
        if result.is_ok() && matches!(self.stop, crate::conversation::Stop::Refusal) {
            return Ending::QueryRefusal;
        }
        Ending::Complete(result)
    }
    pub(super) fn hold_for_quota_query(&mut self) {
        self.phase = Phase::AwaitingQuotaQuery;
    }
    pub(super) fn quota_wait(&self) -> Option<&quota::Wait> {
        match &self.phase {
            Phase::AwaitingQuotaEnd { wait, .. } | Phase::QuotaSettling { wait, .. } => Some(wait),
            _ => None,
        }
    }
}
