use crate::World;
use acp2api_acceptance::{client::ApiCall, server_tool_check as c};
use cucumber::{given, then, when};
use serde_json::json;

#[derive(Default)]
pub struct ServerToolState {
    pub endpoint: Option<String>,
    call: Option<ApiCall>,
}
impl std::fmt::Debug for ServerToolState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerToolState")
            .field("endpoint", &self.endpoint)
            .finish()
    }
}
async fn send(w: &mut World, endpoint: String, call: ApiCall) {
    w.server_tools.endpoint = Some(endpoint);
    w.server_tools.call = Some(ApiCall {
        path: call.path.clone(),
        body: call.body.clone(),
        headers: call.headers.clone(),
        method: call.method.clone(),
    });
    c::send(&mut w.scenario, call).await;
}
fn message(w: &World) -> String {
    w.scenario.answer.as_ref().unwrap().json()["error"]["message"]
        .as_str()
        .unwrap()
        .into()
}
pub fn refusal(w: &World, expected: &str) {
    let code = if expected.contains("Input tag '") {
        "unsupported_advisor_tool"
    } else if expected.starts_with("capability_rejected: tool type ") {
        "unsupported_tool_type"
    } else if expected.contains("server tool") {
        "unsupported_server_tool"
    } else {
        "unsupported_content_type"
    };
    c::envelope(
        &w.scenario,
        w.server_tools.endpoint.as_deref().unwrap(),
        expected,
        code,
    );
}
#[when(expr = "an API client sends a request to {string} with a tool of the type {string}")]
async fn advisor(w: &mut World, endpoint: String, kind: String) {
    let call = c::request(&endpoint, &kind, None);
    send(w, endpoint, call).await;
}
#[when(
    expr = "an API client sends a request to {string} with a tool of the type {string} and the name {string}"
)]
async fn typed(w: &mut World, endpoint: String, kind: String, name: String) {
    let call = c::request(&endpoint, &kind, Some(&name));
    send(w, endpoint, call).await;
}
#[when(expr = "Codex sends a request to {string} with the tool {string}")]
async fn codex(w: &mut World, endpoint: String, kind: String) {
    let call = c::request(&endpoint, &kind, None);
    send(w, endpoint, call).await;
}
#[then(expr = "the message is {string}")]
fn exact(w: &mut World, expected: String) {
    refusal(w, &expected);
}
#[then(
    regex = r#"^acp2api refuses the request with HTTP 400 and the message "(acp2api: the server tool web_search is not supported\. To stop these calls, set web_search = "disabled" in the Codex configuration\.)"$"#
)]
fn codex_message(w: &mut World, expected: String) {
    refusal(w, &expected);
}
#[then(expr = "the message starts with {string}")]
fn starts(w: &mut World, prefix: String) {
    assert!(
        message(w).starts_with(&prefix),
        "server tool message prefix"
    );
    refusal(w, &message(w));
}
#[then(expr = "the message ends with {string}")]
fn ends(w: &mut World, suffix: String) {
    assert!(message(w).ends_with(&suffix), "WebSearch settings sentence");
}
#[then("acp2api refuses the request with HTTP 400")]
fn status(w: &mut World) {
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, 400);
}
#[then("the log has a line for the advisor tool")]
fn log(w: &mut World) {
    c::advisor_log(&w.scenario);
}
#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with the client tool "([^"]+)" and a tool_addition block whose tool is a tool_reference to "([^"]+)"$"#
)]
async fn reference(w: &mut World, endpoint: String, name: String, target: String) {
    let call = c::with_addition(&name, true, &target);
    send(w, endpoint, call).await;
}
#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with the client tool "([^"]+)" and a tool_addition block whose tool is a full tool definition of "([^"]+)"$"#
)]
async fn definition(w: &mut World, endpoint: String, name: String, target: String) {
    let call = c::with_addition(&name, false, &target);
    send(w, endpoint, call).await;
}
#[then(
    "the official runtime gets the same content as for the same request without the tool_addition block"
)]
async fn content(w: &mut World) {
    c::compare_content(&mut w.scenario, w.server_tools.call.as_ref().unwrap()).await;
}
#[given(expr = "a request has a tool of the type {string}")]
fn has_advisor(w: &mut World, kind: String) {
    w.server_tools.call = Some(c::request(c::MESSAGES, &kind, None));
}
#[given("it has a tool_addition block whose tool is a full tool definition")]
fn has_addition(w: &mut World) {
    w.server_tools.call.as_mut().unwrap().body["messages"][0]["content"] =
        json!([c::addition("another_tool", false)]);
}
pub async fn combined(w: &mut World, endpoint: String) {
    let call = w.server_tools.call.take().unwrap();
    send(w, endpoint, call).await;
}
pub fn has_request(w: &World) -> bool {
    w.server_tools.call.is_some()
}
