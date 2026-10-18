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
pub(crate) fn output_tokens(_text: &str) -> u64 {
    0
}
