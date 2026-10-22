use crate::{Scenario, client::ApiCall, content_check, journal_check, tool_check};
use serde_json::{Value, json};

pub const MESSAGES: &str = "POST /v1/messages";
pub fn request(endpoint: &str, kind: &str, name: Option<&str>) -> ApiCall {
    let mut call = content_check::request(endpoint, json!("Check server tool refusal."));
    let mut tool = json!({"type":kind});
    if let Some(name) = name {
        tool["name"] = json!(name);
    }
    call.body["tools"] = json!([tool]);
    call
}
pub fn addition(name: &str, reference: bool) -> Value {
    json!({"type":"tool_addition","tool":if reference {
        json!({"type":"tool_reference","name":name})
    } else {
        json!({"name":name,"description":"Another client tool","input_schema":{"type":"object"}})
    }})
}
pub fn with_addition(name: &str, reference: bool, target: &str) -> ApiCall {
    let mut call = tool_check::request(MESSAGES, &[tool_check::tool(name)], false, "auto");
    call.body["messages"][0]["content"] = json!([
        {"type":"text","text":"Before the addition. 雪"},
        addition(target, reference),
        {"type":"text","text":"After the addition. 🙂"}
    ]);
    call
}
pub async fn send(s: &mut Scenario, call: ApiCall) {
    content_check::send(s, call).await;
}
pub fn envelope(s: &Scenario, endpoint: &str, message: &str, code: &str) {
    content_check::refusal(s, endpoint, message, code, None);
}
pub fn advisor_log(s: &Scenario) {
    let answer = s.answer.as_ref().unwrap();
    let id = answer.headers.get("x-request-id").expect("HTTP request ID");
    let records = journal_check::records(s);
    let found: Vec<_> = records.iter().filter(|r| r["request_id"] == *id).collect();
    assert_eq!(
        found.len(),
        1,
        "one actual product log line for the advisor request"
    );
    assert_eq!(found[0]["result"], "refused");
    assert_eq!(
        found[0]["error_code"], "unsupported_advisor_tool",
        "advisor log classification"
    );
    assert!(found[0]["session"].is_null());
}
pub async fn compare_content(s: &mut Scenario, call: &ApiCall) {
    assert_eq!(
        s.answer.as_ref().unwrap().status,
        200,
        "valid reference accepted"
    );
    let with = content_check::blocks(s);
    assert_eq!(with.len(), 1, "one ACP prompt with reference");
    let mut absent = ApiCall {
        path: call.path.clone(),
        body: call.body.clone(),
        headers: call.headers.clone(),
        method: call.method.clone(),
    };
    absent.body["messages"][0]["content"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["type"] != "tool_addition");
    send(s, absent).await;
    assert_eq!(
        s.answer.as_ref().unwrap().status,
        200,
        "control request accepted"
    );
    let both = content_check::blocks(s);
    assert_eq!(both.len(), 2, "one actual ACP prompt for each request");
    assert_eq!(
        both[0], both[1],
        "tool_reference does not change ACP content"
    );
    assert_eq!(
        both[0],
        json!([
            {"type":"text","text":"Before the addition. 雪"},
            {"type":"text","text":"After the addition. 🙂"}
        ]),
        "comparison preserves all neighboring content"
    );
}
