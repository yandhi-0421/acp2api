use crate::{World, thinking_support as check};
use acp2api_acceptance::{
    client::ApiCall, content_check, endpoint_check, fake::ModelSpec, field_check,
};
use cucumber::{given, then, when};
use serde_json::json;

#[given(expr = "the model {string} has only the thinking levels {string} and {string}")]
fn levels(w: &mut World, model: String, first: String, last: String) {
    w.scenario.fake().catalog(&[ModelSpec {
        value: model,
        name: "Gemini 3.8 Flash".into(),
        levels: vec![first, last],
    }]);
}

async fn send(w: &mut World, endpoint: &str, model: String, edit: impl FnOnce(&mut ApiCall)) {
    w.request.queue_next(&w.scenario);
    let mut call = endpoint_check::request(endpoint, "Hello", false);
    call.body["model"] = json!(model);
    edit(&mut call);
    content_check::send(&mut w.scenario, call).await;
}

#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with the model "([^"]+)" and "(reasoning\.effort|reasoning_effort|output_config\.effort)" set to "([^"]+)"$"#
)]
async fn effort(w: &mut World, endpoint: String, model: String, field: String, value: String) {
    send(w, &endpoint, model, |call| {
        field_check::set(call, &field, json!(value))
    })
    .await;
}

#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with the model "([^"]+)" and "thinking" with the type "(disabled|adaptive|enabled)"(?: and "budget_tokens" (\d+))?$"#
)]
async fn thinking(w: &mut World, endpoint: String, model: String, kind: String, budget: String) {
    send(w, &endpoint, model, |call| {
        call.body["thinking"] = json!({"type":kind});
        if !budget.is_empty() {
            call.body["thinking"]["budget_tokens"] = json!(budget.parse::<u64>().unwrap());
        }
    })
    .await;
}

#[when(
    expr = "an API client sends a request to {string} with the model {string} and no effort field"
)]
async fn absent(w: &mut World, endpoint: String, model: String) {
    send(w, &endpoint, model, |_| {}).await;
}

#[when(
    expr = "an API client sends a request to {string} with the model {string}, \"output_config.effort\" set to {string}, and \"thinking\" with the type {string}"
)]
async fn precedence(w: &mut World, endpoint: String, model: String, effort: String, kind: String) {
    send(w, &endpoint, model, |call| {
        call.body["output_config"] = json!({"effort":effort});
        call.body["thinking"] = json!({"type":kind});
    })
    .await;
}

#[then(expr = "the prompt turn runs with the thinking level {string}")]
fn selected(w: &mut World, level: String) {
    check::level_at_prompt(&w.scenario, &w.models.models[0].value, &level);
}

#[then(expr = "the log has a line with the value {string}")]
fn logged(w: &mut World, value: String) {
    let lines = acp2api_acceptance::journal_check::lines(&w.scenario);
    assert!(
        lines.iter().any(|line| line.contains(&value)),
        "FR-013 log records unknown effort {value}; actual: {lines:?}"
    );
}

#[then("the API client gets no error")]
fn success(w: &mut World) {
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().status,
        200,
        "FR-013 unknown effort causes no error"
    );
}

#[then(expr = "the response has {string} with the value {string}")]
fn response_model(w: &mut World, field: String, expected: String) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(answer.status, 200, "FR-013 response succeeds");
    assert_eq!(
        answer.json()[field],
        expected,
        "FR-013 requested model name in response"
    );
}
