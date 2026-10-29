use crate::{World, retry_support as check};
use acp2api_acceptance::{TurnPlan, fake::ModelSpec, tool_check};
use cucumber::{given, then, when};
use serde_json::json;

#[given(
    expr = "the official runtime offers the model name {string} with the thinking levels {string}, {string}, and {string}"
)]
fn catalog(w: &mut World, model: String, low: String, medium: String, high: String) {
    w.scenario.fake().catalog(&[ModelSpec {
        value: model,
        name: "Gemini 3.8 Flash".into(),
        levels: vec![low, medium, high],
    }]);
}

#[given(
    expr = "the request {string} to {string} has the system prompt {string}, the user message {string}, the client tool {string}, and the model {string}"
)]
fn request(
    w: &mut World,
    name: String,
    endpoint: String,
    system: String,
    text: String,
    tool: String,
    model: String,
) {
    assert_eq!(name, "R");
    let mut call = tool_check::request(&endpoint, &[tool_check::tool(&tool)], false, "auto");
    call.body["messages"] = json!([{"role":"user","content":text}]);
    call.body["system"] = json!(system);
    call.body["model"] = json!(model);
    w.retry.endpoint = endpoint;
    w.retry.body = call.body;
}

#[given(
    expr = "the prompt turn of {string} failed at the time 0 s with the stderr error line {int} {string} {string}"
)]
async fn stderr(w: &mut World, name: String, code: u16, status: String, message: String) {
    assert_eq!(name, "R");
    let expected = format!("acp2api: upstream error {code} {status}: {message}");
    check::seed(w, check::upstream(code, status, message), code, &expected).await;
}

#[given(expr = "the prompt turn of {string} failed at the time 0 s with an error of the 503 row")]
async fn temporary(w: &mut World, name: String) {
    assert_eq!(name, "R");
    check::seed(
        w,
        check::upstream(
            503,
            "UNAVAILABLE".into(),
            "The service is currently unavailable.".into(),
        ),
        503,
        "acp2api: upstream error 503 UNAVAILABLE: The service is currently unavailable.",
    )
    .await;
}

#[given(expr = "the prompt turn of {string} ended at the time 0 s with the stop reason {string}")]
async fn stopped(w: &mut World, name: String, reason: String) {
    assert_eq!(name, "R");
    check::seed(
        w,
        TurnPlan::new().end(&reason),
        502,
        "acp2api: the prompt turn reached the limit of model requests of the official runtime.",
    )
    .await;
}

#[when(regex = r#"^the API client sends "R" again at the time ([0-9]+) (ms|s)$"#)]
async fn again(w: &mut World, value: u64, unit: String) {
    let at = if unit == "s" { value * 1000 } else { value };
    check::send_at(w, w.retry.call(), at).await;
}

#[when(
    regex = r#"^the API client sends "R" with the new user message "([^"]+)" at the time ([0-9]+) s$"#
)]
async fn continues(w: &mut World, text: String, seconds: u64) {
    let mut call = w.retry.call();
    call.body["messages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"user","content":text}));
    check::send_at(w, call, seconds * 1000).await;
}

#[when(
    regex = r#"^the API client sends "R" with the model "([^"]+)"(?: and "output_config\.effort" set to "([^"]+)")? at the time ([0-9]+) s$"#
)]
async fn send_model(w: &mut World, model: String, effort: String, seconds: u64) {
    let mut call = w.retry.call();
    call.body["model"] = json!(model);
    if !effort.is_empty() {
        call.body["output_config"] = json!({"effort":effort});
    }
    check::send_at(w, call, seconds * 1000).await;
}

#[when(
    regex = r#"^the API client sends "R" with "(stream|max_tokens)" set to (true|2048) at the time ([0-9]+) s$"#
)]
async fn send_field(w: &mut World, field: String, value: String, seconds: u64) {
    let mut call = w.retry.call();
    call.body[field] = serde_json::from_str(&value).unwrap();
    check::send_at(w, call, seconds * 1000).await;
}

#[when(
    regex = r#"^the API client sends "R" with the system prompt "([^"]+)" at the time ([0-9]+) s$"#
)]
async fn send_system(w: &mut World, system: String, seconds: u64) {
    let mut call = w.retry.call();
    call.body["system"] = json!(system);
    check::send_at(w, call, seconds * 1000).await;
}

#[when(
    regex = r#"^the API client sends "R" with the client tools "([^"]+)" and "([^"]+)" at the time ([0-9]+) s$"#
)]
async fn tools(w: &mut World, first: String, second: String, seconds: u64) {
    let mut call = w.retry.call();
    call.body["tools"] = tool_check::request(
        &w.retry.endpoint,
        &[tool_check::tool(&first), tool_check::tool(&second)],
        false,
        "auto",
    )
    .body["tools"]
        .take();
    check::send_at(w, call, seconds * 1000).await;
}

#[given(expr = "acp2api refused \"R\" at the time {int} s")]
async fn refused(w: &mut World, seconds: u64) {
    check::send_at(w, w.retry.call(), seconds * 1000).await;
    check::refused_without_prompt(w);
}

#[given(expr = "the owner restarted acp2api at the time {int} s")]
async fn restart(w: &mut World, seconds: u64) {
    crate::conversation::restart_at(w, seconds * 1000).await;
}

#[then(regex = r"^acp2api refuses the request with HTTP (502|503)$")]
fn refusal_status(w: &mut World, status: u16) {
    let answer = w.scenario.answer.as_ref().unwrap();
    check::refusal(answer, status);
    if status == 502 {
        assert_eq!(
            answer.headers.get("x-should-retry").map(String::as_str),
            Some("false")
        );
    }
}

#[then(expr = "acp2api refuses the request with HTTP 503 and the header {string}")]
fn refusal_header(w: &mut World, header: String) {
    let answer = w.scenario.answer.as_ref().unwrap();
    check::refusal(answer, 503);
    let (name, value) = header.split_once(": ").unwrap();
    assert_eq!(answer.headers.get(name).map(String::as_str), Some(value));
}

#[then("acp2api refuses the request with HTTP 503 and no prompt")]
fn refused_without_prompt(w: &mut World) {
    check::refused_without_prompt(w);
    assert_eq!(
        w.scenario
            .answer
            .as_ref()
            .unwrap()
            .headers
            .get("retry-after")
            .map(String::as_str),
        Some("1")
    );
}

#[then(expr = "the official runtime gets a prompt for {string}")]
fn prompted(w: &mut World, name: String) {
    assert_eq!(name, "R");
    let (before, _) = w.scenario.hub.active_request();
    let prompts = w.scenario.fake().record().prompts();
    let current = &prompts[before..];
    assert_eq!(current.len(), 1, "FR-029 one prompt after the retry wait");
    assert_eq!(
        current[0].text,
        w.retry.body["messages"][0]["content"].as_str().unwrap(),
        "request conversation reaches ACP"
    );
    assert_eq!(
        current[0].at, w.retry.sent_at,
        "prompt starts at the requested boundary"
    );
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, 200);
}
