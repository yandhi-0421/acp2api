use crate::World;
use acp2api_acceptance::{
    client::ApiCall, content_check as c, fake::ModelSpec, model_check as check,
};
use cucumber::{given, then, when};
use serde_json::json;
pub const NAMES: [&str; 4] = [
    "gemini-3.8-flash",
    "gemini-3.8-flash-low",
    "gemini-3.8-flash-medium",
    "gemini-3.8-flash-high",
];
#[derive(Debug, Default)]
pub struct ModelState {
    pub endpoint: String,
    pub requested: String,
    models: Vec<ModelSpec>,
}
#[given(
    expr = "the config option {string} of the official runtime has one value {string} with the name {string}"
)]
fn base(w: &mut World, option: String, value: String, name: String) {
    assert_eq!(option, "model");
    w.models.models = vec![ModelSpec {
        value,
        name,
        levels: Vec::new(),
    }];
    w.scenario.fake().catalog(&w.models.models);
}
#[given(expr = "the model {string} has the thinking levels {string}, {string}, and {string}")]
fn levels(w: &mut World, model: String, low: String, medium: String, high: String) {
    assert_eq!(w.models.models[0].value, model);
    w.models.models[0].levels = vec![low, medium, high];
    w.scenario.fake().catalog(&w.models.models);
}
#[when(expr = "an API client sends {string}")]
async fn list(w: &mut World, endpoint: String) {
    assert_eq!(endpoint, "GET /v1/models");
    c::send(&mut w.scenario, check::list()).await;
}
#[then("the list holds exactly these model names:")]
fn exact(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    let expected: Vec<_> = step
        .table
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .skip(1)
        .map(|r| r[0].as_str())
        .collect();
    check::membership(w.scenario.answer.as_ref().unwrap(), &expected);
}
#[then(expr = "the answer has {string} with the value {string}")]
fn field(w: &mut World, key: String, value: String) {
    assert_eq!(w.scenario.answer.as_ref().unwrap().json()[key], value);
}
#[then(
    "each entry of \"data\" has the fields \"id\", \"object\", \"type\", \"display_name\", \"created\", \"created_at\", and \"owned_by\""
)]
fn fields(w: &mut World) {
    check::fields(
        w.scenario.answer.as_ref().unwrap(),
        1_792_281_600,
        "2026-10-18T00:00:00Z",
    );
}
#[then(
    expr = "each entry has {string} with the value {string} and {string} with the value {string}"
)]
fn entry(w: &mut World, a: String, av: String, b: String, bv: String) {
    let value = w.scenario.answer.as_ref().unwrap().json();
    let entries = value["data"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    for e in entries {
        assert_eq!(e[&a], av);
        assert_eq!(e[&b], bv);
    }
}
#[then(expr = "the entry with the {string} {string} has the {string} {string}")]
fn display(w: &mut World, key: String, id: String, field: String, name: String) {
    let v = w.scenario.answer.as_ref().unwrap().json();
    assert_eq!(
        v["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[&key] == id)
            .unwrap()[field],
        name
    );
}
#[given("acp2api started and got no request")]
async fn started(w: &mut World) {
    w.scenario.start().await;
}
#[then("the official runtime has one ACP session from the sign-in check")]
#[then("the official runtime still has only that ACP session")]
fn one_session(w: &mut World) {
    assert_eq!(
        w.scenario.fake().record().session_meta().len(),
        1,
        "one startup ACP session"
    );
    assert_eq!(
        w.scenario.fake().record().client_info().len(),
        1,
        "one startup initialize"
    );
}
#[then("the official runtime got no prompt")]
fn no_prompt(w: &mut World) {
    assert!(w.scenario.fake().prompts().is_empty());
}
#[then("the list holds \"gemini-3.8-flash\" and its three thinking level suffixes")]
fn first(w: &mut World) {
    check::membership(w.scenario.answer.as_ref().unwrap(), &NAMES);
}
#[given("an API client read the list of model names")]
async fn read(w: &mut World) {
    list(w, "GET /v1/models".into()).await;
}
#[given(
    expr = "the official runtime then adds the value {string} with the thinking level {string} to the config option {string}"
)]
fn add(w: &mut World, value: String, level: String, option: String) {
    assert_eq!(option, "model");
    w.models.models.push(ModelSpec {
        name: "Gemini 3.9 Pro".into(),
        value,
        levels: vec![level],
    });
    w.scenario.fake().catalog(&w.models.models);
}
#[when("a request makes a new ACP session")]
async fn new_session(w: &mut World) {
    let mut call = ApiCall::one_user_message("POST /v1/messages", "Hello");
    call.body["system"] = json!("New system prompt.");
    c::send(&mut w.scenario, call).await;
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, 200);
    assert_eq!(
        w.scenario.fake().record().session_meta().len(),
        2,
        "genuine later new ACP session"
    );
    assert_eq!(w.scenario.fake().prompts().len(), 1);
}
#[then(expr = "the list holds the model names {string} and {string}")]
fn added(w: &mut World, base: String, suffix: String) {
    check::membership(
        w.scenario.answer.as_ref().unwrap(),
        &[NAMES[0], NAMES[1], NAMES[2], NAMES[3], &base, &suffix],
    );
}
async fn send(w: &mut World, endpoint: String, model: String, stream: bool) {
    w.models.endpoint = endpoint.clone();
    w.models.requested = model.clone();
    let mut call = acp2api_acceptance::endpoint_check::request(&endpoint, "Hello", stream);
    call.body["model"] = model.into();
    c::send(&mut w.scenario, call).await;
}
#[when(expr = "an API client sends a request to {string} with the model {string}")]
async fn unknown(w: &mut World, endpoint: String, model: String) {
    send(w, endpoint, model, false).await;
}
#[when(expr = "an API client sends a request with a stream to {string} with the model {string}")]
async fn unknown_stream(w: &mut World, endpoint: String, model: String) {
    send(w, endpoint, model, true).await;
}
#[then("the message lists the four model names")]
fn names(w: &mut World) {
    check::refusal(&w.scenario, &w.models.endpoint, &w.models.requested, &NAMES);
}
#[then(
    regex = r"^acp2api refuses the request with HTTP 400 in the error envelope of the (Anthropic Messages API|OpenAI Responses API|OpenAI Chat Completions API)$"
)]
fn envelope(w: &mut World, _api: String) {
    check::refusal(
        &w.scenario,
        &w.models.endpoint,
        &w.models.requested,
        &[NAMES[0]],
    );
}
#[then("the answer has no stream event")]
fn no_event(w: &mut World) {
    let a = w.scenario.answer.as_ref().unwrap();
    assert_eq!(a.status, 400);
    assert!(a.events.is_empty());
    assert!(a.headers["content-type"].starts_with("application/json"));
}
