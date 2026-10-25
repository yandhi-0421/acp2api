pub(crate) enum Part {
    Text(String),
    Image { media_type: String, base64: String },
    Document { title: Option<String>, text: String },
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
    pub conversation: Conversation,
    pub model: String,
    pub tools: Vec<ClientTool>,
    pub input_tokens: u64,
    pub tool_choice: ToolChoice,
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
pub(crate) fn prompt(input: Prompt<'_>) -> (crate::journal::SessionUse, Vec<PromptBlock>) {
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
        let Item::Message { role, name, parts } = item;
        let mut prefix = if unnamed_user {
            String::new()
        } else {
            let separator = if index == 0 { "" } else { "\n" };
            let name = name
                .as_ref()
                .map(|name| format!(": {name}"))
                .unwrap_or_default();
            format!("{separator}[{}{name}]\n", role.label())
        };
        if rebuild && index == 0 {
            prefix = format!("{NEUTRAL}\n\n{prefix}");
        }
        for part in parts {
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
            };
            blocks.push(block);
        }
        if !prefix.is_empty() {
            blocks.push(PromptBlock::Text(prefix));
        }
    }
    (kind, blocks)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ItemHash([u8; 32]);
pub(crate) struct RecordHashes {
    pub system: ItemHash,
    pub messages: Vec<ItemHash>,
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
    let Item::Message { role, name, parts } = item;
    let mut hash = ItemHasher::new("message");
    hash.field(role.label());
    hash.name(name.as_deref());
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
        }
    }
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
        let Item::Message { name, parts, .. } = item;
        if let Some(name) = name {
            text.push_str(name);
        }
        for part in parts {
            match part {
                Part::Text(value) | Part::Document { text: value, .. } => text.push_str(value),
                Part::Image { .. } => {}
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
pub(crate) enum TurnEvent {
    Text(String),
    ToolCall(ToolCall),
    End(Result<Usage, crate::messages::Message>),
}
pub(crate) type Ticket = tokio::sync::mpsc::UnboundedReceiver<(TurnEvent, crate::Hold)>;
