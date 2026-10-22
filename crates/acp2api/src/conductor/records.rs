use crate::conversation::{self, Item, ItemHash, ToolSetHash};
pub(super) struct Record {
    pub session: String,
    pub hashes: Vec<ItemHash>,
    pub tools: ToolSetHash,
    pub used: u64,
}
pub(super) fn select<'a>(
    records: &[Record],
    hashes: &[ItemHash],
    tools: ToolSetHash,
    items: &'a [&'a Item],
) -> Option<(usize, &'a [&'a Item])> {
    records
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            record.tools == tools
                && hashes.starts_with(&record.hashes)
                && conversation::user_suffix(&items[record.hashes.len() - 1..])
        })
        .max_by_key(|(_, record)| (record.hashes.len(), record.used))
        .map(|(index, record)| (index, &items[record.hashes.len() - 1..]))
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
                hashes: hashes[..2].into(),
                tools,
                used: 99,
            },
            Record {
                session: "long-older".into(),
                hashes: hashes[..3].into(),
                tools,
                used: 1,
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
