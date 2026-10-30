use crate::conversation::{self, Item, RecordHashes, ToolSetHash};
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
#[derive(Default)]
pub(super) struct Sessions {
    records: Vec<Record>,
    startup: Option<String>,
    use_sequence: u64,
}
impl Sessions {
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
    pub(super) fn take(&mut self, index: usize) -> Record {
        self.records.remove(index)
    }
    pub(super) fn next_use(&mut self) -> u64 {
        self.use_sequence += 1;
        self.use_sequence
    }
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
        }
    }
    #[cfg(test)]
    pub(super) fn startup(&self) -> Option<&str> {
        self.startup.as_deref()
    }
}
pub(super) fn select<'a>(
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
