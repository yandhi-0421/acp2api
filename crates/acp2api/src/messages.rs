use std::fmt;

pub(crate) enum Message {
    Field(String),
    FieldValue {
        path: String,
        value: serde_json::Value,
    },
    Model {
        requested: String,
        names: Vec<String>,
    },
    Content(String),
    Media(String),
    Url(String),
    Pdf(String),
    Prefill,
    ToolSearch,
    ToolName(String),
    AdvisorTool(String),
    ServerTool {
        kind: String,
        name: String,
    },
    CodexWebSearch,
    ToolType {
        kind: String,
        name: String,
    },
    InvalidJson,
    Invalid(String),
    Key,
    Upstream(String),
    BuiltInToolRan(String),
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
            Message::Upstream(_) | Message::BuiltInToolRan(_) => Row::Other502,
            Message::Model { .. }
            | Message::FieldValue { .. }
            | Message::Field(_)
            | Message::Content(_)
            | Message::Media(_)
            | Message::Url(_)
            | Message::Pdf(_)
            | Message::ToolSearch
            | Message::ToolName(_)
            | Message::AdvisorTool(_)
            | Message::ServerTool { .. }
            | Message::CodexWebSearch
            | Message::ToolType { .. }
            | Message::Prefill
            | Message::InvalidJson
            | Message::Invalid(_) => Row::Rejected,
        }
    }
    pub fn param(&self) -> Option<&str> {
        match &self.message {
            Message::Field(field) => Some(field),
            Message::FieldValue { path, .. } => Some(path),
            Message::Model { .. } => Some("model"),
            _ => None,
        }
    }
}
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model { requested, names } => write!(
                f,
                "acp2api: the model name {requested} is not available. The model names are: {}.",
                names.join(", ")
            ),
            Self::Field(field) => write!(f, "acp2api: the request field {field} is not supported."),
            Self::FieldValue { path, value } => write!(f, "acp2api: the request field {path} = {value} is not supported."),
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
            Self::ServerTool { kind, name } => {
                write!(f, "capability_rejected: server tool {kind} (name {name}) is not supported by acp2api.")?;
                if kind == "web_search_20250305" {
                    f.write_str(" To stop these calls, deny the WebSearch tool in the Claude Code settings.")?;
                }
                Ok(())
            }
            Self::CodexWebSearch => f.write_str("acp2api: the server tool web_search is not supported. To stop these calls, set web_search = \"disabled\" in the Codex configuration."),
            Self::ToolType { kind, name } => write!(
                f,
                "capability_rejected: tool type {kind} (name {name}) is not supported by acp2api."
            ),
            Self::AdvisorTool(kind) => write!(
                f,
                "Input tag '{kind}' found using 'type' does not match any of the expected tags"
            ),
            Self::ToolName(name) => write!(
                f,
                "acp2api: the client tool name {name} is not supported, because a built-in tool of the official runtime has this name."
            ),
            Self::Prefill => f.write_str(
                "acp2api: a last message from the assistant (a prefill) is not supported.",
            ),
            Self::BuiltInToolRan(name) => write!(f, "acp2api: the built-in tool {name} of the official runtime ran with no permission request, so acp2api cancelled the prompt turn."),
            Self::InvalidJson => f.write_str("acp2api: the request body is not valid JSON, or it has more than 128 levels."),
            Self::Invalid(message) | Self::Upstream(message) => f.write_str(message),
            Self::Key => f.write_str("acp2api: the local API key is absent or wrong."),
        }
    }
}
