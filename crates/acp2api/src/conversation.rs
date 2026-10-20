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
    if system.is_empty() {
        return None;
    }
    Some(
        system
            .iter()
            .map(|part| {
                if let Some(name) = &part.name {
                    let role = part.role.label();
                    format!("[{role}: {name}]\n{}", part.text)
                } else {
                    part.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
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
    pub schema: String,
    pub kind: ToolKind,
    pub namespace: Option<String>,
}
impl ClientTool {
    // Reserve the prefix and escape plain names too, so no namespace member can alias one.
    pub(crate) fn mcp_name(&self) -> String {
        fn hex(text: &str) -> String {
            text.as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        }
        match &self.namespace {
            Some(namespace) => format!("__acp2api_n_{}_{}", hex(namespace), hex(&self.name)),
            None if self.name.starts_with("__acp2api_") => {
                format!("__acp2api_p_{}", hex(&self.name))
            }
            None => self.name.clone(),
        }
    }
}
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
    pub tool_choice: ToolChoice,
}
pub(crate) enum PromptBlock {
    Text(String),
    Image { media_type: String, base64: String },
}
pub(crate) struct Answer {
    pub text: String,
    pub tool_call: Option<ToolCall>,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
pub(crate) const NEUTRAL: &str = "The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.";
pub(crate) enum PromptFor<'a> {
    Continue { new_items: &'a [&'a Item] },
    Rebuild { conversation: &'a Conversation },
}
impl Role {
    fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::Developer => "developer",
        }
    }
}
impl Conversation {
    pub(crate) fn items(&self) -> Vec<&Item> {
        self.earlier
            .iter()
            .chain(std::iter::once(&self.last))
            .collect()
    }
}
pub(crate) fn prompt(input: PromptFor<'_>) -> Vec<PromptBlock> {
    let (items, rebuild) = match input {
        PromptFor::Continue { new_items } => (new_items.to_vec(), false),
        PromptFor::Rebuild { conversation } => {
            (conversation.items(), !conversation.earlier.is_empty())
        }
    };
    let unnamed_user = !rebuild
        && matches!(
            items.as_slice(),
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
    blocks
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ItemHash([u8; 32]);
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
pub(crate) fn record_hashes(conversation: &Conversation) -> Vec<ItemHash> {
    let mut system = ItemHasher::new("system");
    for part in &conversation.system {
        system.field(part.role.label());
        system.name(part.name.as_deref());
        system.text(&part.text);
    }
    std::iter::once(ItemHash(system.finish()))
        .chain(conversation.items().into_iter().map(item_hash))
        .collect()
}
pub(crate) fn reply_hash(text: &str) -> ItemHash {
    item_hash(&Item::Message {
        role: Role::Assistant,
        name: None,
        parts: vec![Part::Text(text.into())],
    })
}
pub(crate) fn tool_call_hash(call: &ToolCall) -> ItemHash {
    let mut hash = ItemHasher::new("tool call");
    hash.field(match call.kind {
        ToolKind::Function => "function",
        ToolKind::Custom => "custom",
    });
    hash.name(call.namespace.as_deref());
    hash.field(&call.name);
    hash.text(&call.arguments);
    ItemHash(hash.finish())
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
            hash.text(&tool.schema);
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

pub(crate) fn output_tokens(text: &str) -> u64 {
    let ascii = text.chars().filter(char::is_ascii).count() as u64;
    let other = text.chars().filter(|ch| !ch.is_ascii()).count() as u64;
    ascii.div_ceil(4) + other
}

pub(crate) struct ReplyForm {
    pub stream: bool,
    pub model_name: String,
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
