pub(crate) struct TurnRequest {
    pub text: String,
    pub model: String,
}
pub(crate) struct Answer {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
pub(crate) fn prompt(turn: &TurnRequest) -> String {
    turn.text.clone()
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
