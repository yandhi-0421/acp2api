use std::fmt;

pub(crate) enum Message {
    Field(String),
    Content(String),
    Media(String),
    Url(String),
    Pdf(String),
    Prefill,
    ToolSearch,
    ToolName(String),
    Invalid(String),
    Key,
    Upstream(String),
}
pub(crate) enum Row {
    Key,
    Rejected,
    Other502,
}
pub(crate) struct Refusal {
    pub message: Message,
}
impl From<Message> for Refusal {
    fn from(message: Message) -> Self {
        Self { message }
    }
}
impl Refusal {
    pub fn invalid(message: impl Into<String>) -> Self {
        Message::Invalid(message.into()).into()
    }
    pub fn row(&self) -> Row {
        match self.message {
            Message::Key => Row::Key,
            Message::Upstream(_) => Row::Other502,
            Message::Field(_)
            | Message::Content(_)
            | Message::Media(_)
            | Message::Url(_)
            | Message::Pdf(_)
            | Message::ToolSearch
            | Message::ToolName(_)
            | Message::Prefill
            | Message::Invalid(_) => Row::Rejected,
        }
    }
    pub fn param(&self) -> Option<&str> {
        if let Message::Field(field) = &self.message {
            Some(field)
        } else {
            None
        }
    }
}
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Field(field) => write!(f, "acp2api: the request field {field} is not supported."),
            Self::Content(kind) => write!(f, "acp2api: the content type {kind} is not supported."),
            Self::Media(media) => write!(f, "acp2api: the media type {media} is not supported."),
            Self::Url(kind) => write!(
                f,
                "acp2api: the content type {kind} with a URL is not supported. Send the data as base64."
            ),
            Self::Pdf(kind) => write!(
                f,
                "acp2api: a PDF document in a message (content type {kind}) is not supported. Remove that message from the conversation."
            ),
            Self::ToolSearch => f.write_str("acp2api: the tool tool_search is not supported."),
            Self::ToolName(name) => write!(
                f,
                "acp2api: the client tool name {name} is not supported, because a built-in tool of the official runtime has this name."
            ),
            Self::Prefill => f.write_str(
                "acp2api: a last message from the assistant (a prefill) is not supported.",
            ),
            Self::Invalid(message) | Self::Upstream(message) => f.write_str(message),
            Self::Key => f.write_str("Invalid API key"),
        }
    }
}
