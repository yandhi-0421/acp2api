use super::{Accepted, Active, Command, Failure, PendingRequest, Phase, Session, retry};
use crate::conversation::{self, Item, Prompt, PromptRequest, RecordHashes, ToolSetHash};
use crate::{
    Moment,
    conversation::{TurnEvent, Usage},
};
use std::collections::BTreeMap;
pub(super) enum Location {
    Loaded,
    Unloaded(std::path::PathBuf),
}
pub(super) struct Record {
    pub location: Location,
    pub session: String,
    pub hashes: RecordHashes,
    pub tools: ToolSetHash,
    pub used: u64,
    pub expires: crate::Moment,
}
pub(super) struct OutstandingWork {
    model: String,
    request: crate::journal::Request,
    session: Option<String>,
    completion: PendingCompletion,
}
enum PendingCompletion {
    Disconnected(Option<crate::journal::SessionUse>),
    AlreadyCompleted,
}
impl OutstandingWork {
    pub(super) fn disconnected_check(request: PendingRequest) -> Self {
        Self {
            model: request.turn.model,
            request: request.request,
            session: None,
            completion: PendingCompletion::Disconnected(None),
        }
    }
    pub(super) fn request_id(&self) -> &str {
        &self.request.id
    }
    pub(super) fn model(&self) -> &str {
        &self.model
    }
    pub(super) fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }
    pub(super) fn prepared(mut self, session: String) -> Vec<Command> {
        self.session = Some(session);
        self.finish()
    }
    pub(super) fn finish(self) -> Vec<Command> {
        let mut commands = Vec::new();
        if let Some(session) = &self.session {
            commands.push(Command::Forget(session.clone()));
        }
        if let PendingCompletion::Disconnected(kind) = self.completion {
            commands.push(Command::Complete(crate::journal::Completion {
                request: self.request,
                session: self.session,
                session_use: kind,
                outcome: crate::journal::Outcome::Disconnected,
            }));
        }
        commands
    }
}
pub(super) struct RequestRecord {
    hashes: RecordHashes,
    tools: ToolSetHash,
    used: u64,
    expires: Moment,
}
impl RequestRecord {
    pub(super) fn hashes(&self) -> &RecordHashes {
        &self.hashes
    }
    #[cfg(test)]
    pub(super) fn tools(&self) -> ToolSetHash {
        self.tools
    }
    #[cfg(test)]
    pub(super) fn fixture(
        hashes: RecordHashes,
        tools: ToolSetHash,
        used: u64,
        expires: Moment,
    ) -> Self {
        Self {
            hashes,
            tools,
            used,
            expires,
        }
    }
}
pub(super) enum Completion {
    Finished,
    Interrupted,
    Quota(Option<String>),
}
pub(super) struct Completed {
    pub(super) outstanding: Option<OutstandingWork>,
    pub(super) retry: Option<retry::Wait>,
}
#[derive(Default)]
pub(super) struct Sessions {
    records: Vec<Record>,
    startup: Option<String>,
    use_sequence: u64,
    cancels: BTreeMap<String, Moment>,
}
impl Sessions {
    #[cfg(test)]
    pub(super) fn records(&self) -> &[Record] {
        &self.records
    }
    pub(super) fn retained(&self) -> Vec<(String, std::path::PathBuf)> {
        self.records
            .iter()
            .filter_map(|record| match &record.location {
                Location::Unloaded(cwd) => Some((record.session.clone(), cwd.clone())),
                Location::Loaded => None,
            })
            .collect()
    }
    pub(super) fn unload(&mut self, mut cwd: impl FnMut(&str) -> std::path::PathBuf) {
        for record in &mut self.records {
            if matches!(record.location, Location::Loaded) {
                record.location = Location::Unloaded(cwd(&record.session));
            }
        }
    }
    pub(super) fn replaced(&mut self, startup: Option<String>) {
        self.startup = startup;
    }
    pub(super) fn models_ready(&mut self, session: String) -> Option<super::Command> {
        self.startup.replace(session).map(super::Command::Forget)
    }
    pub(super) fn take_startup(&mut self) -> Option<String> {
        self.startup.take()
    }
    #[cfg(test)]
    pub(super) fn remember(&mut self, record: Record) {
        self.records.push(record);
    }
    pub(super) fn forget(&mut self, session: String) -> Vec<super::Command> {
        self.records.retain(|record| record.session != session);
        if self.startup.as_ref() == Some(&session) {
            self.startup = None;
        }
        vec![super::Command::Forget(session)]
    }
    pub(super) fn forget_idle(&mut self) -> Vec<super::Command> {
        let idle = self.records.drain(..).map(|record| record.session);
        idle.chain(self.startup.take())
            .map(super::Command::Forget)
            .collect()
    }
    pub(super) fn expire(&mut self, now: crate::Moment) -> Vec<super::Command> {
        let mut commands = Vec::new();
        self.records.retain(|record| {
            if record.expires <= now {
                commands.push(super::Command::Forget(record.session.clone()));
                false
            } else {
                true
            }
        });
        commands
    }
    pub(super) fn start_request(
        &mut self,
        request: Accepted,
        mut checked_session: Option<String>,
        now: Moment,
    ) -> (Active, Vec<Command>) {
        let mut commands = Vec::new();
        let client_tools_hash = crate::conversation::tool_set_hash(&request.turn.tools);
        let settings = Session::for_turn(&request.turn);
        let hashes = crate::conversation::record_hashes(&request.turn.conversation);
        let tools_hash = crate::conversation::tool_set_hash(&settings.tools);
        let items = request.turn.conversation.items();
        let matched = select(&self.records, &hashes, tools_hash, &items);
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
                .map(|session| (session, Location::Loaded)),
            None if settings.system.is_none() && settings.tools.is_empty() => {
                self.startup.take().map(|id| (id, Location::Loaded))
            }
            None => None,
        };
        if matched.is_some() {
            commands.extend(checked_session.map(Command::Forget));
        }
        self.use_sequence += 1;
        let record = RequestRecord {
            hashes,
            tools: tools_hash,
            used: self.use_sequence,
            expires: now.saturating_add(24 * 60 * 60 * 1000),
        };
        let work = request.request.id.clone();
        let (session, phase) = match ready {
            Some((id, Location::Loaded)) => {
                commands.push(Command::Prompt {
                    work: work.clone(),
                    session: id.clone(),
                    prompt,
                });
                (Some(id), Phase::prompt(now))
            }
            Some((session, Location::Unloaded(cwd))) => {
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
        (
            Active {
                reply: request.reply,
                request: request.request,
                session,
                record,
                client_tools_hash,
                kind,
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
            },
            commands,
        )
    }
    pub(super) fn next_deadline(&self) -> Option<Moment> {
        self.records
            .iter()
            .map(|record| record.expires)
            .chain(self.cancels.values().copied())
            .min()
    }
    pub(super) fn cancellation_due(&self, now: Moment) -> bool {
        self.cancels.values().any(|at| *at <= now)
    }
    pub(super) fn cancel(&mut self, session: String, now: Moment) -> Command {
        self.cancels
            .entry(session.clone())
            .or_insert_with(|| now.saturating_add(super::CANCEL_TIMEOUT));
        Command::Cancel(session)
    }
    pub(super) fn prompt_ended(&mut self, session: &str) {
        self.cancels.remove(session);
    }
    pub(super) fn clear_cancels(&mut self) {
        self.cancels.clear();
    }
    pub(super) fn runtime_failed(&mut self) {
        self.clear_cancels();
        self.startup = None;
    }
    pub(super) fn disconnect(
        &mut self,
        active: Active,
        now: Moment,
    ) -> (Option<OutstandingWork>, Vec<Command>) {
        let mut commands = Vec::new();
        let ended = matches!(
            active.phase,
            Phase::Settling { .. } | Phase::QuotaSettling { .. }
        );
        if active.phase.update_deadline().is_some() {
            commands.push(self.cancel(active.session.as_ref().unwrap().clone(), now));
        }
        let outstanding = OutstandingWork {
            model: active.model,
            request: active.request,
            session: active.session,
            completion: PendingCompletion::Disconnected(Some(active.kind)),
        };
        if ended {
            commands.extend(outstanding.finish());
            (None, commands)
        } else {
            (Some(outstanding), commands)
        }
    }
    pub(super) fn complete_request(
        &mut self,
        mut active: Active,
        result: Result<(), Failure>,
        mode: Completion,
        now: Moment,
        catalog: Option<&crate::catalog::Catalog>,
    ) -> (Completed, Vec<Command>) {
        let mut commands = Vec::new();
        let outstanding = if matches!(mode, Completion::Interrupted)
            && !matches!(
                active.phase,
                Phase::Settling { .. } | Phase::QuotaSettling { .. }
            ) {
            Some(OutstandingWork {
                model: active.model.clone(),
                request: active.request.clone(),
                session: active.session.clone(),
                completion: PendingCompletion::AlreadyCompleted,
            })
        } else {
            None
        };
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
        let retry = result
            .as_ref()
            .err()
            .and_then(|failure| retry::Wait::after(&active, failure, catalog));
        if result.as_ref().is_err_and(|failure| {
            !matches!(
                failure.message,
                crate::messages::Message::MaxTurnRequests | crate::messages::Message::NoUpdate
            )
        }) {
            active.held = None;
        }
        active.release_or_cut_reply(None, now, &mut commands);
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
                        .record
                        .hashes
                        .messages
                        .push(crate::conversation::reply_hash(&active.text));
                }
                if let Some(hash) = tool_hash {
                    active.record.hashes.messages.push(hash);
                }
                self.records.push(Record {
                    location: Location::Loaded,
                    session: session.clone(),
                    hashes: active.record.hashes,
                    tools: active.record.tools,
                    used: active.record.used,
                    expires: active.record.expires,
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
        match mode {
            Completion::Quota(session) => commands.extend(
                session
                    .filter(|session| Some(session) != forget.as_ref())
                    .map(Command::Forget),
            ),
            Completion::Finished | Completion::Interrupted => {}
        }
        if outstanding.is_none() {
            commands.extend(forget.map(Command::Forget));
        }
        (Completed { outstanding, retry }, commands)
    }
    #[cfg(test)]
    pub(super) fn fixture(
        records: Vec<Record>,
        startup: Option<String>,
        use_sequence: u64,
    ) -> Self {
        Self {
            records,
            startup,
            use_sequence,
            cancels: BTreeMap::new(),
        }
    }
    #[cfg(test)]
    pub(super) fn startup(&self) -> Option<&str> {
        self.startup.as_deref()
    }
}
fn select<'a>(
    records: &[Record],
    hashes: &RecordHashes,
    tools: ToolSetHash,
    items: &'a [&'a Item],
) -> Option<(usize, &'a [&'a Item])> {
    records
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            record.tools == tools
                && record.hashes.is_continued_by(hashes)
                && conversation::continues_after(items, record.hashes.messages.len())
        })
        .max_by_key(|(_, record)| (record.hashes.messages.len(), record.used))
        .map(|(index, record)| (index, &items[record.hashes.messages.len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{Conversation, Part, Role};
    #[test]
    fn longest_record_precedes_recency() {
        fn user(text: &str) -> Item {
            Item::Message {
                role: Role::User,
                name: None,
                parts: vec![Part::Text(text.into())],
            }
        }
        let conversation = Conversation {
            system: vec![],
            earlier: vec![user("A"), user("B")],
            last: user("C"),
        };
        let hashes = conversation::record_hashes(&conversation);
        let tools = conversation::tool_set_hash(&[]);
        let records = vec![
            Record {
                location: Location::Loaded,
                session: "short-recent".into(),
                hashes: RecordHashes {
                    system: hashes.system,
                    messages: hashes.messages[..1].into(),
                },
                tools,
                used: 99,
                expires: 0,
            },
            Record {
                location: Location::Loaded,
                session: "long-older".into(),
                hashes: RecordHashes {
                    system: hashes.system,
                    messages: hashes.messages[..2].into(),
                },
                tools,
                used: 1,
                expires: 0,
            },
        ];
        let items = conversation.items();
        assert_eq!(
            select(&records, &hashes, tools, &items).map(|(index, _)| index),
            Some(1),
            "ADR-0003 longest eligible record wins before recency"
        );
    }
}
