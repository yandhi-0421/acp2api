use super::{Command, NO_UPDATE_LIMIT, ResponseChannel, quota, records, retry};
use crate::{
    Moment,
    conversation::{EffortAsk, PromptRequest, TurnEvent, Usage},
};

pub(super) enum Phase {
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
    pub(super) fn prompt(now: Moment) -> Self {
        Self::Prompt {
            update_deadline: now + NO_UPDATE_LIMIT,
        }
    }

    pub(super) fn settled(&self) -> bool {
        matches!(self, Self::Settling { .. } | Self::QuotaSettling { .. })
    }

    pub(super) fn update_deadline(&self) -> Option<Moment> {
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
    pub(super) phase: Phase,
    pub(super) tool_call: Option<crate::conversation::ToolCall>,
    pub(super) stop: crate::conversation::Stop,
    pub(super) stops: Vec<String>,
    pub(super) stop_prefix: String,
}

impl Active {
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
        let (can_continue, result) = match std::mem::replace(&mut self.phase, Phase::Completing) {
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

    pub(super) fn earliest_stop(&self, text: &str) -> Option<(usize, String)> {
        self.stops
            .iter()
            .filter_map(|stop| text.find(stop).map(|at| (at, stop.clone())))
            .min_by_key(|(at, _)| *at)
    }

    pub(super) fn release_or_cut_reply(
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
