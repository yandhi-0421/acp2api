use crate::conversation::{self, Conversation, ItemHash, ToolSetHash};
pub(super) struct Record {
    pub hashes: Vec<ItemHash>,
    pub tools: ToolSetHash,
    pub used: u64,
}
pub(super) struct ReadySession {
    pub id: String,
    pub record: Option<Record>,
}
pub(super) fn select(
    sessions: &[ReadySession],
    hashes: &[ItemHash],
    tools: ToolSetHash,
    conversation: &Conversation,
) -> Option<usize> {
    let items = conversation.items();
    sessions
        .iter()
        .enumerate()
        .filter_map(|(index, session)| {
            let record = session.record.as_ref()?;
            let start = record.hashes.len().checked_sub(1)?;
            (record.tools == tools
                && hashes.starts_with(&record.hashes)
                && start < items.len()
                && conversation::user_suffix(&items[start..]))
            .then_some((index, record.hashes.len(), record.used))
        })
        .max_by_key(|(_, length, used)| (*length, *used))
        .map(|(index, _, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::{Item, Part, Role};
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
        let sessions = vec![
            ReadySession {
                id: "short-recent".into(),
                record: Some(Record {
                    hashes: hashes[..2].into(),
                    tools,
                    used: 99,
                }),
            },
            ReadySession {
                id: "long-older".into(),
                record: Some(Record {
                    hashes: hashes[..3].into(),
                    tools,
                    used: 1,
                }),
            },
        ];
        assert_eq!(
            select(&sessions, &hashes, tools, &conversation),
            Some(1),
            "ADR-0003 longest eligible record wins before recency"
        );
    }
}
