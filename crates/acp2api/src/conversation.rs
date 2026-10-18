pub(crate) struct TurnRequest {
    #[allow(dead_code)]
    pub text: String,
    pub model: String,
}
pub(crate) struct Answer {
    #[allow(dead_code)]
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
pub(crate) fn prompt(_turn: &TurnRequest) -> String {
    String::new()
}
pub(crate) fn output_tokens(_text: &str) -> u64 {
    0
}
