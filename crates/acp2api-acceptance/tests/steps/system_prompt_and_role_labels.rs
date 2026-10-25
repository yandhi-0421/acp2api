use crate::World;
use acp2api_acceptance::client::ApiCall;
use cucumber::{then, when};
#[when(expr = "an API client sends a request to {string} with one unnamed user message {string}")]
async fn send(world: &mut World, endpoint: String, text: String) {
    let call = world
        .scenario
        .client()
        .await
        .send(ApiCall::one_user_message(&endpoint, &text));
    let answer = call.finish(&mut world.scenario.time).await;
    assert_eq!(answer.status, 200, "request must reach the product");
    world.scenario.answer = Some(answer);
}
#[then(expr = "the prompt that the official runtime gets is {string}")]
async fn prompt(world: &mut World, expected: String) {
    world.scenario.time.settle().await;
    assert_eq!(
        world.scenario.fake().prompts().last().expect("a prompt"),
        &expected,
        "FR-020 prompt text"
    );
}

#[when(
    expr = "an API client sends a request to {string} with the system prompt {string} and one user message {string}"
)]
async fn system_text(w: &mut World, endpoint: String, system: String, user: String) {
    use acp2api_acceptance::system_check as s;
    s::send(
        &mut w.scenario,
        s::request(&endpoint, system.into(), serde_json::Value::Null, &user),
    )
    .await;
}
#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with (the system blocks|the system message|the instructions) "([^"]+)" and (?:the developer message )?"([^"]+)" and then one user message "([^"]+)"$"#
)]
async fn system_parts(
    w: &mut World,
    endpoint: String,
    _first_kind: String,
    first: String,
    second: String,
    user: String,
) {
    use acp2api_acceptance::system_check as s;
    use serde_json::json;
    let (system, messages) = match endpoint.as_str() {
        "POST /v1/messages" => (
            json!([{"type":"text","text":first},{"type":"text","text":second}]),
            json!(null),
        ),
        "POST /v1/chat/completions" => (
            json!(null),
            json!([
                s::message("system", None, &first),
                s::message("developer", None, &second),
                s::message("user", None, &user)
            ]),
        ),
        _ => (
            json!(first),
            json!([
                s::message("developer", None, &second),
                s::message("user", None, &user)
            ]),
        ),
    };
    s::send(
        &mut w.scenario,
        s::request(&endpoint, system, messages, &user),
    )
    .await;
}
#[when(
    expr = "an API client sends a request to {string} with the system message named {string} and these text blocks:"
)]
async fn named_system(
    w: &mut World,
    endpoint: String,
    name: String,
    #[step] step: &cucumber::gherkin::Step,
) {
    use acp2api_acceptance::system_check as s;
    let kind = if endpoint.ends_with("responses") {
        "input_text"
    } else {
        "text"
    };
    let blocks: Vec<_> = step.table.as_ref().unwrap().rows[1..]
        .iter()
        .map(|row| serde_json::json!({"type":kind,"text":row[0]}))
        .collect();
    s::send(
        &mut w.scenario,
        s::request(
            &endpoint,
            serde_json::Value::Null,
            serde_json::json!([
                {"role":"system","name":name,"content":blocks},
                s::message("user", None, "Hello")
            ]),
            "Hello",
        ),
    )
    .await;
}
#[when(
    expr = "Claude Code sends a request to {string} whose first system block is the attribution block of Claude Code, and whose second system block is {string}"
)]
async fn attribution(w: &mut World, endpoint: String, text: String) {
    use acp2api_acceptance::system_check as s;
    s::send(
        &mut w.scenario,
        s::request(
            &endpoint,
            serde_json::json!([{"type":"text","text":s::ATTRIBUTION},{"type":"text","text":text}]),
            serde_json::Value::Null,
            "Hello",
        ),
    )
    .await;
}
#[then(expr = "the ACP session gets the system prompt addition {string}")]
fn system_addition(w: &mut World, expected: String) {
    acp2api_acceptance::system_check::addition(&w.scenario, &expected);
}
#[then("the ACP session gets this system prompt addition:")]
fn system_docstring(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    let text = step.docstring.as_ref().unwrap();
    let text = text.strip_prefix('\n').unwrap_or(text);
    let text = text.strip_suffix('\n').unwrap_or(text);
    acp2api_acceptance::system_check::addition(&w.scenario, text);
}
#[then("no content that the official runtime gets holds the text of the attribution block")]
fn no_attribution(w: &mut World) {
    acp2api_acceptance::system_check::no_attribution(
        &w.scenario,
        acp2api_acceptance::system_check::ATTRIBUTION,
    );
}
