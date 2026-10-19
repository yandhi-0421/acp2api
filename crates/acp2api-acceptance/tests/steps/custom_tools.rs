use crate::World;
use acp2api_acceptance::{client::Answer, custom_tool_check as c, tool_check as t};
use cucumber::{given, then, when};
use serde_json::{Value, json};
#[derive(Debug, Default)]
pub struct CustomToolState {
    tools: Vec<Value>,
    answers: Vec<Answer>,
}
#[when(expr = "Codex sends a request to {string} with the custom tool {string} with a grammar")]
async fn offer_custom(w: &mut World, endpoint: String, name: String) {
    assert_eq!(endpoint, c::ENDPOINT);
    w.custom_tools.tools = vec![c::custom(&name, c::GRAMMAR)];
    let a = c::send(
        &mut w.scenario,
        &w.custom_tools.tools,
        false,
        None,
        &json!({}),
    )
    .await;
    assert_eq!(
        a.status, 200,
        "custom tool declaration accepted before MCP offer"
    );
}
#[then(
    expr = "the official runtime offers the model the tool {string} with one string argument {string}"
)]
fn custom_schema(w: &mut World, name: String, input: String) {
    assert_eq!(input, "input");
    c::offer_custom(&w.scenario, &name, c::GRAMMAR);
}
#[then("the description of that tool holds the grammar")]
fn grammar(w: &mut World) {
    let name = w.custom_tools.tools[0]["name"].as_str().unwrap();
    c::offer_custom(&w.scenario, name, c::GRAMMAR);
}
#[given(expr = "Codex sent a request with the custom tool {string}")]
fn custom(w: &mut World, name: String) {
    w.custom_tools.tools = vec![c::custom(&name, c::GRAMMAR)];
}
#[when(expr = "the model calls {string} with the input {string}")]
async fn custom_call(w: &mut World, name: String, input: String) {
    for stream in [false, true] {
        w.custom_tools.answers.push(
            c::send(
                &mut w.scenario,
                &w.custom_tools.tools,
                stream,
                Some(&name),
                &json!({"input":input}),
            )
            .await,
        );
    }
}
#[then(expr = "the response has a {string} item for {string} with the input {string}")]
fn custom_result(w: &mut World, kind: String, name: String, input: String) {
    for (answer, stream) in w.custom_tools.answers.iter().zip([false, true]) {
        c::output(answer, stream, &kind, &name, None, &json!(input));
    }
    c::offer_custom(&w.scenario, &name, c::GRAMMAR);
    t::cancelled(&w.scenario, 2);
}
#[when(
    expr = "Codex sends a request to {string} with the namespace tool {string} that holds the tools {string} and {string}"
)]
async fn namespace_offer(w: &mut World, endpoint: String, namespace: String, a: String, b: String) {
    assert_eq!(endpoint, c::ENDPOINT);
    w.custom_tools.tools = vec![c::namespace(&namespace, &[&a, &b])];
    let answer = c::send(
        &mut w.scenario,
        &w.custom_tools.tools,
        false,
        None,
        &json!({}),
    )
    .await;
    assert_eq!(
        answer.status, 200,
        "namespace declaration accepted before MCP offer"
    );
}
#[then(
    expr = "the official runtime offers the model two tools, one for {string} and one for {string}"
)]
fn namespace_members(w: &mut World, a: String, b: String) {
    c::offer_namespace(&w.scenario, &[&a, &b]);
}
#[given(
    expr = "Codex sent a request with the namespace tool {string} that holds the tool {string}"
)]
fn setup_namespace(w: &mut World, namespace: String, member: String) {
    w.custom_tools.tools = vec![c::namespace(&namespace, &[&member])];
}
#[when(expr = "the model calls the tool for {string}")]
async fn namespace_call(w: &mut World, member: String) {
    let answer = c::send(
        &mut w.scenario,
        &w.custom_tools.tools,
        false,
        None,
        &json!({}),
    )
    .await;
    assert_eq!(
        answer.status, 200,
        "namespace declaration accepted before MCP call"
    );
    let names = c::offer_namespace(&w.scenario, &[&member]);
    for stream in [false, true] {
        w.custom_tools.answers.push(
            c::send(
                &mut w.scenario,
                &w.custom_tools.tools,
                stream,
                Some(&names[0]),
                &json!({"path":"文档/雪🙂.txt"}),
            )
            .await,
        );
    }
}
#[then(expr = "the response has a {string} item for {string} with the namespace {string}")]
fn namespace_result(w: &mut World, kind: String, name: String, namespace: String) {
    for (answer, stream) in w.custom_tools.answers.iter().zip([false, true]) {
        c::output(
            answer,
            stream,
            &kind,
            &name,
            Some(&namespace),
            &json!({"path":"文档/雪🙂.txt"}),
        );
    }
    t::cancelled(&w.scenario, 2);
}
