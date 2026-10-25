use crate::{Scenario, TurnPlan, client::ApiCall, content_check, journal_check, system_check};
use serde_json::{Value, json};
pub const NEUTRAL: &str = "The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.";
pub use crate::system_check::message;
pub fn call(messages: Value, system: Option<&str>, tools: Value) -> ApiCall {
    let mut call = ApiCall::one_user_message("POST /v1/chat/completions", "");
    let mut items = Vec::new();
    if let Some(system) = system {
        items.push(message("system", None, system));
    }
    items.extend(messages.as_array().unwrap().iter().cloned());
    call.body["messages"] = items.into();
    if !tools.is_null() {
        call.body["tools"] = tools;
    }
    call
}
pub async fn send(s: &mut Scenario, call: ApiCall, reply: &str) {
    s.start().await;
    let completed = journal_check::records(s).len();
    s.fake()
        .next_prompt(TurnPlan::new().chunk(reply).end("end_turn"));
    content_check::send(s, call).await;
    assert_eq!(
        s.answer.as_ref().unwrap().status,
        200,
        "conversation reaches real HTTP product"
    );
    // Response completion is the state observation point (design R1).
    assert_eq!(
        journal_check::records(s).len(),
        completed + 1,
        "metadata is committed before HTTP completion"
    );
}
pub fn prompts(s: &Scenario) -> Vec<Value> {
    system_check::requests(s)
        .into_iter()
        .filter(|v| v["method"] == "session/prompt")
        .map(|v| v["params"].clone())
        .collect()
}
pub fn session(s: &Scenario) -> String {
    prompts(s).last().unwrap()["sessionId"]
        .as_str()
        .unwrap()
        .to_owned()
}
pub fn blocks(s: &Scenario) -> Value {
    prompts(s).last().unwrap()["prompt"].clone()
}
pub fn text(s: &Scenario) -> String {
    blocks(s)
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["text"].as_str())
        .collect()
}
pub fn kind(s: &Scenario) -> String {
    journal_check::records(s).last().unwrap()["session_use"]
        .as_str()
        .unwrap()
        .into()
}
pub fn exact_text(s: &Scenario, expected: &str) {
    let actual = s
        .answer
        .as_ref()
        .filter(|answer| answer.status == 200)
        .map(|_| text(s));
    assert_eq!(
        actual.as_deref(),
        Some(expected),
        "FR-020/021 complete ordered prompt text; HTTP response: {:?}",
        s.answer
            .as_ref()
            .map(|answer| (answer.status, answer.json()))
    );
}
pub fn continuation(s: &Scenario, previous: &str, text: &str) {
    assert_eq!(
        session(s),
        previous,
        "ADR-0003 actual ACP session continuation"
    );
    assert_eq!(
        blocks(s),
        json!([{"type":"text","text":text}]),
        "only new content reaches ACP"
    );
    assert_eq!(kind(s), "continue", "actual selected session log kind");
}
pub fn tools(name: &str, description: &str, schema: Value) -> Value {
    json!([{"type":"function","function":{"name":name,"description":description,"parameters":schema}}])
}
