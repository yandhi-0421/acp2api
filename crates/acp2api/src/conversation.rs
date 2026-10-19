pub(crate) enum Part {
    Text(String),
    Image { media_type: String, base64: String },
    Document { title: Option<String>, text: String },
}
pub(crate) enum Role {
    User,
    Assistant,
    System,
    Developer,
}
pub(crate) enum Item {
    Message { role: Role, parts: Vec<Part> },
}
pub(crate) struct Conversation {
    pub earlier: Vec<Item>,
    pub last: Item,
}
pub(crate) struct TurnRequest {
    pub conversation: Conversation,
    pub model: String,
}
pub(crate) enum PromptBlock {
    Text(String),
    Image { media_type: String, base64: String },
}
pub(crate) struct Answer {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
pub(crate) fn prompt(turn: &TurnRequest) -> Vec<PromptBlock> {
    turn.conversation
        .earlier
        .iter()
        .chain(std::iter::once(&turn.conversation.last))
        .flat_map(|item| match item {
            Item::Message { parts, .. } => parts,
        })
        .map(|part| match part {
            Part::Text(text) => PromptBlock::Text(text.clone()),
            Part::Image { media_type, base64 } => PromptBlock::Image {
                media_type: media_type.clone(),
                base64: base64.clone(),
            },
            Part::Document { title, text } => PromptBlock::Text(format!(
                "{}\n{text}\n[end of document]",
                title
                    .as_ref()
                    .map(|t| format!("[document: {t}]"))
                    .unwrap_or_else(|| "[document]".into())
            )),
        })
        .collect()
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
    End(Result<Usage, String>),
}
pub(crate) type Ticket = tokio::sync::mpsc::UnboundedReceiver<(TurnEvent, crate::Hold)>;
