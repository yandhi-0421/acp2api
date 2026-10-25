use crate::World;
use acp2api_acceptance::{client::Answer, tool_check as t};
use cucumber::{given, then, when};
use serde_json::{Value, json};
#[derive(Debug, Default)]
pub struct ToolState {
    pub endpoint: String,
    calls: Vec<String>,
    args: Value,
    stream: Option<Answer>,
}
#[given(expr = "the request has the client tool {string}")]
fn one(w: &mut World, name: String) {
    w.usage.tools = vec![t::tool(&name)];
    w.scenario.plan(acp2api_acceptance::Setup::normal());
}
#[given(expr = "the request has the client tools {string} and {string}")]
fn two(w: &mut World, a: String, b: String) {
    w.usage.tools = vec![t::tool(&a), t::tool(&b)];
}
#[given(
    regex = r#"^the model sends the text "Let me look\." and then calls "([^"]+)" with the arguments (\{.*\})$"#
)]
fn model_call(w: &mut World, name: String, args: String) {
    w.tools.calls = vec![name];
    w.tools.args = serde_json::from_str(&args).unwrap();
}
#[given(expr = "the model calls {string} and {string} in one prompt turn")]
fn model_two(w: &mut World, a: String, b: String) {
    w.tools.calls = vec![a, b];
    w.tools.args = json!({"path":"a.txt"});
}
#[given(expr = "the model calls {string} when the official runtime offers it")]
fn model_if_offered(w: &mut World, name: String) {
    w.tools.calls = vec![name];
    w.tools.args = json!({"path":"a.txt"});
}
async fn send(w: &mut World, endpoint: String, choice: &str, pair: bool) {
    w.tools.endpoint = endpoint.clone();
    let names = if choice == "none" {
        Vec::new()
    } else {
        w.tools.calls.clone()
    };
    let plan = if names.is_empty() {
        w.usage.plan()
    } else {
        w.usage.with_usage(t::plan(&names, &w.tools.args))
    };
    w.scenario.fake().next_prompt(plan.clone());
    w.scenario.answer = Some(
        t::send(
            &mut w.scenario,
            t::request(&endpoint, &w.usage.tools, false, choice),
        )
        .await,
    );
    if pair {
        w.scenario.fake().next_prompt(plan);
        w.tools.stream = Some(
            t::send(
                &mut w.scenario,
                t::request(&endpoint, &w.usage.tools, true, choice),
            )
            .await,
        );
    }
}
#[when(expr = "an API client sends that request to {string}")]
async fn send_pair(w: &mut World, endpoint: String) {
    if crate::server_tools::has_request(w) {
        crate::server_tools::combined(w, endpoint).await;
        return;
    }
    send(w, endpoint, "auto", true).await;
}
#[when(expr = "an API client sends that request to {string} with \"tool_choice\" set to \"none\"")]
async fn send_none(w: &mut World, endpoint: String) {
    send(w, endpoint, "none", false).await;
}
#[when(
    expr = "an API client sends a valid request to {string} with \"tool_choice\" set to \"auto\""
)]
async fn send_auto(w: &mut World, endpoint: String) {
    send(w, endpoint, "auto", false).await;
}
#[when(
    expr = "an API client sends a request to {string} with the client tools {string} and {string}, each with a description and a schema"
)]
async fn send_tools(w: &mut World, endpoint: String, a: String, b: String) {
    w.usage.tools = vec![t::tool(&a), t::tool(&b)];
    send(w, endpoint, "auto", false).await;
}
#[then(expr = "the official runtime offers the model exactly the tools {string} and {string}")]
fn offer_two(w: &mut World, a: String, b: String) {
    assert_eq!(
        w.usage
            .tools
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [a, b]
    );
    t::offers(&w.scenario, &w.usage.tools);
}
#[then("each tool has the description and the schema of the request")]
fn descriptions(w: &mut World) {
    t::offers(&w.scenario, &w.usage.tools);
}
#[then(expr = "the official runtime offers the model the client tool {string}")]
fn offer_one(w: &mut World, name: String) {
    assert_eq!(w.usage.tools[0]["name"], name);
    t::offers(&w.scenario, &w.usage.tools);
}
#[then(
    regex = r#"^the response holds the text "Let me look\." and then (a "tool_use" block for "read_file"|"tool_calls" with "read_file"|a "function_call" item for "read_file")$"#
)]
fn result(w: &mut World, _shape: String) {
    t::body(
        w.scenario.answer.as_ref().unwrap(),
        &w.tools.endpoint,
        &w.tools.calls[0],
        &w.tools.args,
    );
    t::stream(
        w.tools.stream.as_ref().unwrap(),
        &w.tools.endpoint,
        &w.tools.calls[0],
        &w.tools.args,
    );
    t::cancelled(&w.scenario, 2);
}
#[then(regex = r#"^the tool call has exactly the arguments (\{.*\})$"#)]
fn arguments(w: &mut World, args: String) {
    let args: Value = serde_json::from_str(&args).unwrap();
    t::body(
        w.scenario.answer.as_ref().unwrap(),
        &w.tools.endpoint,
        &w.tools.calls[0],
        &args,
    );
    t::stream(
        w.tools.stream.as_ref().unwrap(),
        &w.tools.endpoint,
        &w.tools.calls[0],
        &args,
    );
}
#[then(
    regex = r#"^the response ends with ("stop_reason" set to "tool_use"|"finish_reason" set to "tool_calls"|the "function_call" item as the last output item)$"#
)]
fn end(w: &mut World, _end: String) {
    t::body(
        w.scenario.answer.as_ref().unwrap(),
        &w.tools.endpoint,
        &w.tools.calls[0],
        &w.tools.args,
    );
}
#[then(expr = "the response has exactly one {string} block")]
fn at_most_one(w: &mut World, kind: String) {
    assert_eq!(kind, "tool_use");
    assert_eq!(
        t::calls(w.scenario.answer.as_ref().unwrap(), &w.tools.endpoint).len(),
        1,
        "exactly one returned tool call"
    );
    t::body(
        w.scenario.answer.as_ref().unwrap(),
        &w.tools.endpoint,
        &w.tools.calls[0],
        &w.tools.args,
    );
    t::stream(
        w.tools.stream.as_ref().unwrap(),
        &w.tools.endpoint,
        &w.tools.calls[0],
        &w.tools.args,
    );
    t::cancelled(&w.scenario, 2);
}
#[then("the response has no tool call")]
fn no_call(w: &mut World) {
    assert!(
        t::calls(w.scenario.answer.as_ref().unwrap(), &w.tools.endpoint).is_empty(),
        "tool_choice none returns no call"
    );
    t::offers(&w.scenario, &[]);
}
#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with (a "tool_search" tool|a client tool with the name "list_resources"|a client tool with the name "read_resource")$"#
)]
async fn reject(w: &mut World, endpoint: String, description: String) {
    w.tools.endpoint = endpoint.clone();
    let name = if description.contains("list_resources") {
        "list_resources"
    } else {
        "read_resource"
    };
    let mut call = t::request(&endpoint, &[t::tool(name)], false, "auto");
    if description.contains("tool_search") {
        call.body["tools"] = json!([{"type":"tool_search"}]);
    }
    w.scenario.answer = Some(t::send(&mut w.scenario, call).await);
}
#[when(expr = "the official runtime sends the permission request {string} for that tool")]
async fn permission(w: &mut World, title: String) {
    assert_eq!(title, "Run read_file?");
    w.tools.calls = vec!["read_file".into()];
    w.tools.args = json!({"path":"a.txt"});
    send(w, "POST /v1/messages".into(), "auto", false).await;
}
#[then("the official runtime gets no allow")]
fn no_allow(w: &mut World) {
    assert!(!t::permission_requests(&w.scenario).is_empty());
    for reply in t::permission_replies(&w.scenario) {
        assert_ne!(reply["result"]["outcome"]["outcome"], "selected");
    }
}
#[then("the official runtime gets a cancel of the prompt turn")]
fn cancel(w: &mut World) {
    t::cancelled(&w.scenario, 1);
}
#[then(expr = "the response holds the tool call of {string}")]
fn call(w: &mut World, name: String) {
    t::body(
        w.scenario.answer.as_ref().unwrap(),
        &w.tools.endpoint,
        &name,
        &w.tools.args,
    );
}
