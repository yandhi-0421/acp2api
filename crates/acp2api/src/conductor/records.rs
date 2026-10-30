use super::{Active, Command, PendingRequest, Session};
use crate::Moment;
use crate::conversation::{self, Item, Prompt, RecordHashes, ToolSetHash};
use std::collections::BTreeMap;
pub(super) enum Location {
    Loaded,
    Unloaded(std::path::PathBuf),
}
pub(super) struct Record {
    pub location: Location,
    pub session: String,
    pub record: RequestRecord,
}
pub(super) struct OutstandingWork {
    model: String,
    request: crate::journal::Request,
    session: Option<String>,
    completion: PendingCompletion,
}
pub(super) enum PendingCompletion {
    Disconnected(Option<crate::journal::SessionUse>),
    AlreadyCompleted,
}
impl OutstandingWork {
    pub(super) fn from_active(active: &Active, completion: PendingCompletion) -> Self {
        Self {
            model: active.model.clone(),
            request: active.request.clone(),
            session: active.session.clone(),
            completion,
        }
    }
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
    pub(super) fn prepared(&mut self, session: String) {
        self.session = Some(session);
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
    pub(super) fn with_reply(
        mut self,
        text: &str,
        tool_hash: Option<crate::conversation::ItemHash>,
    ) -> Self {
        if tool_hash.is_none() || !text.is_empty() {
            self.hashes.messages.push(conversation::reply_hash(text));
        }
        if let Some(hash) = tool_hash {
            self.hashes.messages.push(hash);
        }
        self
    }
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
pub(super) struct Selection {
    pub(super) kind: crate::journal::SessionUse,
    pub(super) prompt: Vec<crate::conversation::PromptBlock>,
    pub(super) ready: Option<(String, Location)>,
    pub(super) record: RequestRecord,
}
#[derive(Default)]
pub(super) struct AcpSessions {
    records: Vec<Record>,
    startup: Option<String>,
    use_sequence: u64,
    cancels: BTreeMap<String, Moment>,
}
impl AcpSessions {
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
    pub(super) fn models_ready(&mut self, session: String) -> Option<Command> {
        self.startup.replace(session).map(Command::Forget)
    }
    pub(super) fn take_startup(&mut self) -> Option<String> {
        self.startup.take()
    }
    #[cfg(test)]
    pub(super) fn remember(&mut self, record: Record) {
        self.records.push(record);
    }
    pub(super) fn forget(&mut self, session: String) -> Command {
        self.records.retain(|record| record.session != session);
        if self.startup.as_ref() == Some(&session) {
            self.startup = None;
        }
        Command::Forget(session)
    }
    pub(super) fn forget_idle(&mut self) -> Vec<Command> {
        let idle = self.records.drain(..).map(|record| record.session);
        idle.chain(self.startup.take())
            .map(Command::Forget)
            .collect()
    }
    pub(super) fn expire(&mut self, now: crate::Moment) -> Vec<Command> {
        let mut commands = Vec::new();
        self.records.retain(|record| {
            if record.record.expires <= now {
                commands.push(Command::Forget(record.session.clone()));
                false
            } else {
                true
            }
        });
        commands
    }
    pub(super) fn select(
        &mut self,
        turn: &crate::conversation::TurnRequest,
        settings: &Session,
        mut checked_session: Option<String>,
        now: Moment,
    ) -> (Selection, Vec<Command>) {
        let mut commands = Vec::new();
        let hashes = crate::conversation::record_hashes(&turn.conversation);
        let tools_hash = crate::conversation::tool_set_hash(&settings.tools);
        let items = turn.conversation.items();
        let matched = select(&self.records, &hashes, tools_hash, &items);
        let prompt = match matched {
            Some((_, new_items)) => Prompt::Continue { new_items },
            None if turn.conversation.earlier.is_empty() => Prompt::New {
                message: &turn.conversation.last,
            },
            None => Prompt::Rebuild {
                conversation_items: &items,
            },
        };
        let (kind, prompt) = crate::conversation::prompt(prompt, &items);
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
        (
            Selection {
                kind,
                prompt,
                ready,
                record,
            },
            commands,
        )
    }
    pub(super) fn next_deadline(&self) -> Option<Moment> {
        self.records
            .iter()
            .map(|record| record.record.expires)
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
    pub(super) fn cancel_ended(&mut self, session: &str) {
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
        let ended = active.phase.settled();
        if active.phase.update_deadline().is_some() {
            commands.push(self.cancel(active.session.as_ref().unwrap().clone(), now));
        }
        let outstanding = OutstandingWork::from_active(
            &active,
            PendingCompletion::Disconnected(Some(active.kind)),
        );
        if ended {
            commands.extend(outstanding.finish());
            (None, commands)
        } else {
            (Some(outstanding), commands)
        }
    }
    pub(super) fn keep(
        &mut self,
        session: String,
        record: Option<RequestRecord>,
    ) -> Option<String> {
        match record {
            Some(record) => {
                self.records.push(Record {
                    location: Location::Loaded,
                    session,
                    record,
                });
                None
            }
            None => Some(session),
        }
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
            record.record.tools == tools
                && record.record.hashes.is_continued_by(hashes)
                && conversation::continues_after(items, record.record.hashes.messages.len())
        })
        .max_by_key(|(_, record)| (record.record.hashes.messages.len(), record.record.used))
        .map(|(index, record)| (index, &items[record.record.hashes.messages.len()..]))
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
                record: RequestRecord {
                    hashes: RecordHashes {
                        system: hashes.system,
                        messages: hashes.messages[..1].into(),
                    },
                    tools,
                    used: 99,
                    expires: 0,
                },
            },
            Record {
                location: Location::Loaded,
                session: "long-older".into(),
                record: RequestRecord {
                    hashes: RecordHashes {
                        system: hashes.system,
                        messages: hashes.messages[..2].into(),
                    },
                    tools,
                    used: 1,
                    expires: 0,
                },
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
