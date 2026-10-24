use crate::conversation::{self, Item, RecordHashes, ToolSetHash};
pub(super) struct Record {
    pub session: String,
    pub hashes: RecordHashes,
    pub tools: ToolSetHash,
    pub used: u64,
    pub expires: crate::Moment,
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
                && hashes.system == record.hashes.system
                && hashes.messages.starts_with(&record.hashes.messages)
                && conversation::user_suffix(&items[record.hashes.messages.len()..])
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
