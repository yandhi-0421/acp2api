//! System assertions read requests that reached the guarded fake ACP process.
use crate::{Scenario, client::ApiCall, content_check};
use serde_json::{Value, json};
pub const ATTRIBUTION: &str = "x-anthropic-billing-header: synthetic attribution text for issue 41";
pub fn request(endpoint: &str, system: Value, messages: Value, user: &str) -> ApiCall {
    let mut call = ApiCall::one_user_message(endpoint, user);
    match endpoint {
        "POST /v1/messages" => call.body["system"] = system,
        "POST /v1/responses" => {
            call.body["instructions"] = system;
            call.body["input"] = messages;
        }
        "POST /v1/chat/completions" => call.body["messages"] = messages,
        _ => panic!("unsupported system endpoint"),
    }
    call
}
pub fn message(role: &str, name: Option<&str>, text: &str) -> Value {
    let mut value = json!({"role":role,"content":text});
    if let Some(name) = name {
        value["name"] = name.into();
    }
    value
}
pub async fn send(s: &mut Scenario, call: ApiCall) {
    content_check::send(s, call).await;
    assert_eq!(
        s.answer.as_ref().unwrap().status,
        200,
        "system request status"
    );
}
pub fn requests(s: &Scenario) -> Vec<Value> {
    s.hub
        .entries()
        .iter()
        .filter(|e| e.event["kind"] == "got")
        .filter_map(|e| serde_json::from_str(e.event["line"].as_str()?).ok())
        .collect()
}
pub fn addition(s: &Scenario, expected: &str) {
    let values = requests(s);
    let session = values
        .iter()
        .rev()
        .find(|v| v["method"] == "session/new")
        .expect("actual session/new");
    assert_eq!(
        session["params"]["_meta"]["antigravity"]["systemPrompt"]["append"], expected,
        "FR-019 actual session/new system prompt addition"
    );
}
pub fn no_attribution(s: &Scenario, text: &str) {
    let requests = requests(s);
    assert!(
        requests.iter().any(|v| v["method"] == "session/prompt"),
        "actual prompt observed"
    );
    fn contains(value: &Value, text: &str) -> bool {
        match value {
            Value::String(s) => s.contains(text),
            Value::Array(a) => a.iter().any(|v| contains(v, text)),
            Value::Object(o) => o.iter().any(|(k, v)| k.contains(text) || contains(v, text)),
            _ => false,
        }
    }
    assert!(
        requests.iter().all(|v| !contains(v, text)),
        "FR-019 attribution absent from every actual ACP request field"
    );
}
pub fn prompt(s: &Scenario, user: &str) {
    assert_eq!(
        content_check::blocks(s).last().unwrap(),
        &json!([{"type":"text","text":user}]),
        "system text must not enter user prompt"
    );
}
