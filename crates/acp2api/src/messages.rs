use std::fmt;

#[derive(Clone, Debug)]
pub(crate) struct HttpError {
    pub code: u16,
    pub status: String,
    pub message: String,
}
#[derive(Clone, serde::Deserialize)]
pub(crate) struct AcpError {
    pub code: i64,
    pub message: String,
}
#[derive(Clone)]
pub(crate) enum Message {
    Field {
        path: String,
        value: Option<serde_json::Value>,
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
    TooLarge,
    QueueFull,
    QuotaWait {
        until: String,
        retry_after: u64,
    },
    RuntimeUnavailable(String),
    RuntimeFailed,
    MaxTurnRequests,
    Stopped,
    Upstream(String),
    UpstreamHttp(HttpError),
    UpstreamAcp(AcpError),
    BuiltInToolRan(String),
}
pub(crate) enum Row {
    Key,
    Rejected,
    TooLarge,
    QueueFull,
    Quota(u64),
    Other502,
    Temporary503,
}
pub(crate) struct Refusal {
    pub message: Message,
    pub hold: crate::Hold,
}
impl From<Message> for Refusal {
    fn from(message: Message) -> Self {
        Self {
            message,
            hold: crate::Hold::empty(),
        }
    }
}
impl Refusal {
    pub fn invalid(message: impl Into<String>) -> Self {
        Message::Invalid(message.into()).into()
    }
    pub fn field(path: impl Into<String>, value: Option<&serde_json::Value>) -> Self {
        Message::Field {
            path: path.into(),
            value: value.cloned(),
        }
        .into()
    }
    pub fn row(&self) -> Row {
        match self.message {
            Message::Key => Row::Key,
            Message::TooLarge => Row::TooLarge,
            Message::QueueFull => Row::QueueFull,
            Message::QuotaWait { retry_after, .. } => Row::Quota(retry_after),
            Message::RuntimeFailed => Row::Temporary503,
            Message::UpstreamHttp(ref error) if (500..600).contains(&error.code) => {
                Row::Temporary503
            }
            Message::UpstreamHttp(_)
            | Message::UpstreamAcp(_)
            | Message::Upstream(_)
            | Message::MaxTurnRequests
            | Message::BuiltInToolRan(_) => Row::Other502,
            Message::RuntimeUnavailable(_)
            | Message::Stopped
            | Message::Model { .. }
            | Message::Field { .. }
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
            Message::Field { path, .. } => Some(path),
            Message::Model { .. } => Some("model"),
            _ => None,
        }
    }
}
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeUnavailable(message) => f.write_str(message),
            Self::Model { requested, names } => write!(
                f,
                "acp2api: the model name {requested} is not available. The model names are: {}.",
                names.join(", ")
            ),
            Self::Field { path, value: None } => write!(f, "acp2api: the request field {path} is not supported."),
            Self::Field { path, value: Some(value) } => write!(f, "acp2api: the request field {path} = {value} is not supported."),
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
            Self::Stopped => f.write_str("acp2api: acp2api stopped before the end of this request."),
            Self::RuntimeFailed => f.write_str("acp2api: the runtime process failed, so acp2api ended the prompt turn."),
            Self::MaxTurnRequests => f.write_str("acp2api: the prompt turn reached the limit of model requests of the official runtime."),
            Self::InvalidJson => f.write_str("acp2api: the request body is not valid JSON, or it has more than 128 levels."),
            Self::Invalid(message) | Self::Upstream(message) => f.write_str(message),
            Self::UpstreamHttp(error) => write!(f, "acp2api: upstream error {} {}: {}", error.code, error.status, error.message),
            Self::UpstreamAcp(error) => write!(f, "acp2api: upstream ACP error {}: {}", error.code, error.message),
            Self::Key => f.write_str("acp2api: the local API key is absent or wrong."),
            Self::QueueFull => write!(f, "acp2api: the queue is full ({} requests). Send the request again later.", crate::conductor::QUEUE_CAPACITY),
            Self::QuotaWait { until, .. } => write!(f, "acp2api: the Google quota is used up until {until}. acp2api starts no prompt turn before that time."),
            Self::TooLarge => f.write_str("acp2api: the request body is larger than 32 MB."),
        }
    }
}
