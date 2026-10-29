pub(crate) enum Part {
    Text(String),
    Image { media_type: String, base64: String },
    Document { title: Option<String>, text: String },
    PdfNote,
}
#[derive(Clone, Copy)]
pub(crate) enum Role {
    User,
    Assistant,
    System,
    Developer,
}
impl Role {
    fn label(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Developer => "developer",
        }
    }
}
pub(crate) struct SystemPart {
    pub role: Role,
    pub name: Option<String>,
    pub text: String,
}
pub(crate) enum Item {
    Message {
        role: Role,
        name: Option<String>,
        parts: Vec<Part>,
    },
    ToolResult {
        id: String,
        parts: Vec<Part>,
        error: bool,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
}
pub(crate) struct Conversation {
    pub system: Vec<SystemPart>,
    pub earlier: Vec<Item>,
    pub last: Item,
}
pub(crate) fn system_prompt_addition(system: &[SystemPart]) -> Option<String> {
    (!system.is_empty()).then(|| {
        system
            .iter()
            .map(|part| match &part.name {
                Some(name) => format!("[{}: {name}]\n{}", part.role.label(), part.text),
                None => part.text.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolKind {
    Function,
    Custom,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClientTool {
    pub name: String,
    pub description: String,
    pub schema: serde_json::Value,
    pub kind: ToolKind,
    pub namespace: Option<String>,
}
#[derive(Clone, Copy)]
pub(crate) enum ToolChoice {
    Auto,
    Off,
}
#[derive(Clone, Debug)]
pub(crate) struct ToolCall {
    pub kind: ToolKind,
    pub namespace: Option<String>,
    pub name: String,
    pub arguments: String,
}
pub(crate) struct TurnRequest {
    pub stop: Vec<String>,
    pub conversation: Conversation,
    pub model: String,
    pub effort: EffortAsk,
    pub tools: Vec<ClientTool>,
    pub input_tokens: u64,
    pub tool_choice: ToolChoice,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ThinkingLevel {
    Low,
    Medium,
    High,
}
impl ThinkingLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}
#[derive(Clone)]
pub(crate) enum EffortAsk {
    Absent,
    Level(ThinkingLevel),
    Unknown(serde_json::Value),
}
pub(crate) struct PromptRequest {
    pub blocks: Vec<PromptBlock>,
    pub model: String,
    pub level: Option<ThinkingLevel>,
}
pub(crate) enum PromptBlock {
    Text(String),
    Image { media_type: String, base64: String },
}
pub(crate) enum Prompt<'a> {
    New { message: &'a Item },
    Continue { new_items: &'a [&'a Item] },
    Rebuild { conversation_items: &'a [&'a Item] },
}
const NEUTRAL: &str = "The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.";
impl Conversation {
    pub(crate) fn items(&self) -> Vec<&Item> {
        self.earlier
            .iter()
            .chain(std::iter::once(&self.last))
            .collect()
    }
}
fn result_line(history: &[&Item], id: &str, error: bool) -> String {
    let call = history.iter().rev().find_map(|item| match item {
        Item::ToolCall {
            id: call_id,
            name,
            arguments,
        } if call_id == id => Some((name, arguments)),
        _ => None,
    });
    match call {
        Some((name, arguments)) => {
            let (arguments, cut) = match arguments.char_indices().nth(1000) {
                Some((end, _)) => (&arguments[..end], " [arguments cut]"),
                None => (arguments.as_str(), ""),
            };
            let note = if error {
                " (the tool reported an error)"
            } else {
                ""
            };
            format!("Result of your tool call {name} {arguments}{cut}{note}:")
        }
        None => format!("Result of the tool call {id}:"),
    }
}
pub(crate) fn prompt(
    input: Prompt<'_>,
    history: &[&Item],
) -> (crate::journal::SessionUse, Vec<PromptBlock>) {
    use crate::journal::SessionUse;
    let (kind, items) = match &input {
        Prompt::New { message } => (SessionUse::New, std::slice::from_ref(message)),
        Prompt::Continue { new_items } => (SessionUse::Continue, *new_items),
        Prompt::Rebuild { conversation_items } => (SessionUse::Rebuild, *conversation_items),
    };
    let rebuild = matches!(kind, SessionUse::Rebuild);
    let unnamed_user = matches!(
        items,
        [Item::Message {
            role: Role::User,
            name: None,
            ..
        }]
    );
    let mut blocks = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut prefix = if rebuild && index == 0 {
            format!("{NEUTRAL}\n\n")
        } else {
            String::new()
        };
        if index > 0 {
            prefix.push('\n');
        }
        let parts = match item {
            Item::Message { role, name, parts } => {
                if !unnamed_user {
                    let name = name
                        .as_ref()
                        .map(|name| format!(": {name}"))
                        .unwrap_or_default();
                    prefix.push_str(&format!("[{}{name}]\n", role.label()));
                }
                parts
            }
            Item::ToolResult { id, parts, error } => {
                prefix.push_str(&result_line(history, id, *error));
                if parts
                    .iter()
                    .any(|part| !matches!(part, Part::Text(text) if text.is_empty()))
                {
                    prefix.push('\n');
                }
                parts
            }
            Item::ToolCall {
                name, arguments, ..
            } => {
                blocks.push(PromptBlock::Text(format!(
                    "{prefix}You called the tool {name} with the arguments {arguments}."
                )));
                continue;
            }
        };
        for (part_index, part) in parts.iter().enumerate() {
            if part_index > 0
                && (matches!(part, Part::PdfNote) || matches!(parts[part_index - 1], Part::PdfNote))
                && prefix.is_empty()
                && matches!(blocks.last(), Some(PromptBlock::Text(text)) if !text.ends_with('\n'))
            {
                prefix.push('\n');
            }
            let block = match part {
                Part::Text(text) => {
                    PromptBlock::Text(format!("{}{text}", std::mem::take(&mut prefix)))
                }
                Part::Image { media_type, base64 } => {
                    if !prefix.is_empty() {
                        blocks.push(PromptBlock::Text(std::mem::take(&mut prefix)));
                    }
                    PromptBlock::Image {
                        media_type: media_type.clone(),
                        base64: base64.clone(),
                    }
                }
                Part::Document { title, text } => PromptBlock::Text(format!(
                    "{}{}\n{text}\n[end of document]",
                    std::mem::take(&mut prefix),
                    title
                        .as_ref()
                        .map(|title| format!("[document: {title}]"))
                        .unwrap_or_else(|| "[document]".into())
                )),
                Part::PdfNote => PromptBlock::Text(format!(
                    "{}[A PDF document was here. acp2api cannot give PDF documents to the model.]",
                    std::mem::take(&mut prefix)
                )),
            };
            blocks.push(block);
        }
        if !prefix.is_empty() {
            blocks.push(PromptBlock::Text(prefix));
        }
    }
    if let Some(PromptBlock::Text(text)) = blocks
        .iter_mut()
        .find(|block| matches!(block, PromptBlock::Text(_)))
        && text.trim_start().starts_with('/')
    {
        text.insert_str(0, "[user]\n");
    }
    (kind, blocks)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ItemHash([u8; 32]);
#[derive(Clone)]
pub(crate) struct RecordHashes {
    pub system: ItemHash,
    pub messages: Vec<ItemHash>,
}
impl RecordHashes {
    pub(crate) fn is_continued_by(&self, later: &Self) -> bool {
        self.system == later.system && later.messages.starts_with(&self.messages)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ToolSetHash([u8; 32]);
struct ItemHasher(sha2::Sha256);
impl ItemHasher {
    fn new(kind: &str) -> Self {
        use sha2::Digest;
        let mut hash = Self(sha2::Sha256::new());
        hash.field(kind);
        hash
    }
    fn field(&mut self, text: &str) {
        use sha2::Digest;
        self.0.update((text.len() as u64).to_be_bytes());
        self.0.update(text.as_bytes());
    }
    fn text(&mut self, text: &str) {
        let mut normalized = String::new();
        let mut space = false;
        for ch in text.chars() {
            if ch.is_whitespace() {
                if !space {
                    normalized.push(' ');
                }
                space = true;
            } else {
                normalized.push(ch);
                space = false;
            }
        }
        self.field(&normalized);
    }
    fn name(&mut self, name: Option<&str>) {
        self.field(if name.is_some() { "named" } else { "unnamed" });
        if let Some(name) = name {
            self.text(name);
        }
    }
    fn finish(self) -> [u8; 32] {
        use sha2::Digest;
        self.0.finalize().into()
    }
}
fn item_hash(item: &Item) -> ItemHash {
    let (mut hash, parts) = match item {
        Item::Message { role, name, parts } => {
            let mut hash = ItemHasher::new("message");
            hash.field(role.label());
            hash.name(name.as_deref());
            (hash, parts)
        }
        Item::ToolResult { id, parts, error } => {
            let mut hash = ItemHasher::new("tool result");
            hash.field(id);
            hash.field(if *error { "error" } else { "success" });
            (hash, parts)
        }
        Item::ToolCall {
            name, arguments, ..
        } => return tool_call_hash(name, arguments),
    };
    for part in parts {
        match part {
            Part::Text(text) => {
                hash.field("text");
                hash.text(text);
            }
            Part::Image { media_type, base64 } => {
                hash.field("image");
                hash.field(media_type);
                hash.field(base64);
            }
            Part::Document { title, text } => {
                hash.field("document");
                hash.name(title.as_deref());
                hash.text(text);
            }
            Part::PdfNote => hash.field("pdf note"),
        }
    }
    ItemHash(hash.finish())
}
pub(crate) fn tool_call_hash(name: &str, arguments: &str) -> ItemHash {
    let mut hash = ItemHasher::new("tool call");
    hash.text(name);
    hash.text(arguments);
    ItemHash(hash.finish())
}
pub(crate) fn record_hashes(conversation: &Conversation) -> RecordHashes {
    let mut system = ItemHasher::new("system");
    for part in &conversation.system {
        system.field(part.role.label());
        system.name(part.name.as_deref());
        system.text(&part.text);
    }
    RecordHashes {
        system: ItemHash(system.finish()),
        messages: conversation.items().into_iter().map(item_hash).collect(),
    }
}
pub(crate) fn reply_hash(text: &str) -> ItemHash {
    item_hash(&Item::Message {
        role: Role::Assistant,
        name: None,
        parts: vec![Part::Text(text.into())],
    })
}
pub(crate) fn tool_set_hash(tools: &[ClientTool]) -> ToolSetHash {
    let mut members = tools
        .iter()
        .map(|tool| {
            let mut hash = ItemHasher::new("client tool");
            hash.field(match tool.kind {
                ToolKind::Function => "function",
                ToolKind::Custom => "custom",
            });
            hash.name(tool.namespace.as_deref());
            hash.text(&tool.name);
            hash.text(&tool.description);
            hash.text(&tool.schema.to_string());
            hash.finish()
        })
        .collect::<Vec<_>>();
    members.sort_unstable();
    members.dedup();
    let mut hash = ItemHasher::new("tool set");
    for member in members {
        use sha2::Digest;
        hash.0.update(member);
    }
    ToolSetHash(hash.finish())
}
pub(crate) fn continues_after(items: &[&Item], len: usize) -> bool {
    match (&items[len - 1], &items[len..]) {
        (Item::ToolCall { id, .. }, [Item::ToolResult { id: result_id, .. }, following @ ..]) => {
            id == result_id && (following.is_empty() || user_suffix(following))
        }
        (Item::ToolCall { .. }, _) => false,
        (_, following) => user_suffix(following),
    }
}
pub(crate) fn user_suffix(items: &[&Item]) -> bool {
    !items.is_empty()
        && items.iter().all(|item| {
            matches!(
                item,
                Item::Message {
                    role: Role::User | Role::System | Role::Developer,
                    ..
                }
            )
        })
}

pub(crate) fn estimate_tokens(text: &str) -> u64 {
    let ascii = text.chars().filter(char::is_ascii).count() as u64;
    let other = text.chars().filter(|ch| !ch.is_ascii()).count() as u64;
    ascii.div_ceil(4) + other
}

pub(crate) fn input_tokens(conversation: &Conversation, tool_text: &str) -> u64 {
    let mut text = String::new();
    for part in &conversation.system {
        if let Some(name) = &part.name {
            text.push_str(name);
        }
        text.push_str(&part.text);
    }
    for item in conversation.items() {
        let parts = match item {
            Item::Message { name, parts, .. } => {
                if let Some(name) = name {
                    text.push_str(name);
                }
                parts
            }
            Item::ToolResult { parts, .. } => parts,
            Item::ToolCall {
                name, arguments, ..
            } => {
                text.push_str(name);
                text.push_str(arguments);
                continue;
            }
        };
        for part in parts {
            match part {
                Part::Text(value) | Part::Document { text: value, .. } => text.push_str(value),
                Part::Image { .. } | Part::PdfNote => {}
            }
        }
    }
    text.push_str(tool_text);
    estimate_tokens(&text)
}

pub(crate) struct ReplyForm {
    pub stream: bool,
    pub include_usage: bool,
    pub input_tokens: u64,
    pub model_name: String,
    pub tools: Vec<serde_json::Value>,
    pub tool_choice: ToolChoice,
}
pub(crate) struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
pub(crate) enum Stop {
    EndTurn,
    MaxTokens,
    Refusal,
    Sequence(String),
}
impl Stop {
    pub(crate) fn sequence(&self) -> Option<&str> {
        match self {
            Self::Sequence(sequence) => Some(sequence),
            Self::EndTurn | Self::MaxTokens | Self::Refusal => None,
        }
    }
}
pub(crate) enum TurnEvent {
    Stop(Stop),
    Text(String),
    ToolCall(ToolCall),
    End(Result<Usage, crate::messages::Message>),
}
pub(crate) struct Ticket {
    pub events: tokio::sync::mpsc::UnboundedReceiver<(TurnEvent, crate::Hold)>,
    pub hold: crate::Hold,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_before_slash_text_keeps_the_label_with_that_text() {
        let message = Item::Message {
            role: Role::User,
            name: None,
            parts: vec![
                Part::Image {
                    media_type: "image/png".into(),
                    base64: "aW1hZ2U=".into(),
                },
                Part::Text("/x".into()),
            ],
        };
        let (_, blocks) = prompt(Prompt::New { message: &message }, &[&message]);
        assert_eq!(
            blocks.len(),
            2,
            "FR-016 adds no label block before the image"
        );
        assert!(
            matches!(&blocks[..], [PromptBlock::Image { media_type, base64 }, PromptBlock::Text(text)] if media_type == "image/png" && base64 == "aW1hZ2U=" && text == "[user]\n/x"),
            "FR-016 keeps the image first and labels the first text"
        );
    }

    #[test]
    fn a_document_before_slash_text_keeps_its_prompt_without_a_label() {
        let message = Item::Message {
            role: Role::User,
            name: None,
            parts: vec![
                Part::Document {
                    title: None,
                    text: "Document text".into(),
                },
                Part::Text("/x".into()),
            ],
        };
        let (_, blocks) = prompt(Prompt::New { message: &message }, &[&message]);
        let texts: Vec<_> = blocks
            .into_iter()
            .map(|block| match block {
                PromptBlock::Text(text) => text,
                PromptBlock::Image { .. } => panic!("no image in this message"),
            })
            .collect();
        assert_eq!(
            texts,
            ["[document]\nDocument text\n[end of document]", "/x"],
            "FR-016 uses the first prompt text, including document text"
        );
    }

    #[test]
    fn an_empty_tool_result_has_no_blank_line_before_the_next_item() {
        for parts in [vec![Part::Text(String::new())], Vec::new()] {
            let call = Item::ToolCall {
                id: "c".into(),
                name: "read".into(),
                arguments: "{}".into(),
            };
            let result = Item::ToolResult {
                id: "c".into(),
                parts,
                error: false,
            };
            let next = Item::Message {
                role: Role::User,
                name: None,
                parts: vec![Part::Text("Next".into())],
            };
            let history = [&call, &result, &next];
            let (_, blocks) = prompt(
                Prompt::Continue {
                    new_items: &history[1..],
                },
                &history,
            );
            let actual: String = blocks
                .into_iter()
                .map(|block| match block {
                    PromptBlock::Text(text) => text,
                    PromptBlock::Image { .. } => panic!("no image in the request"),
                })
                .collect();
            assert_eq!(actual, "Result of your tool call read {}:\n[user]\nNext");
        }
    }
}
