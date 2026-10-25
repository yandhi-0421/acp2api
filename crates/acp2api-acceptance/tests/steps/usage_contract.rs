use crate::{World, usage_contract as check};
use acp2api_acceptance::{TurnPlan, endpoint_check};
use cucumber::{given, then, when};
use serde_json::{Value, json};

#[given(expr = "the prompt turn has the usage updates with \"used\" {int} and then \"used\" {int}")]
fn updates(w: &mut World, first: u64, last: u64) {
    w.usage.active = true;
    w.usage.updates = vec![first, last];
}

#[given(expr = "the prompt turn has the last usage update with \"used\" {int}")]
fn last_update(w: &mut World, used: u64) {
    w.usage.active = true;
    w.usage.updates = vec![used];
}

#[given("the prompt turn ends with no usage update")]
fn no_usage(w: &mut World) {
    w.usage.active = true;
    w.usage.updates.clear();
}

#[given(
    regex = r#"^the model calls "([^"]+)" with the 8 ASCII characters (\{.*\}) as arguments and sends no text$"#
)]
fn output_call(w: &mut World, name: String, arguments: String) {
    w.usage.active = true;
    w.usage.output_call = Some((name, arguments));
}

#[given(expr = "the request has the system prompt {string}")]
fn system(w: &mut World, text: String) {
    w.usage.system = Some(text);
}

#[given(
    expr = "the request has the user text {string}, assistant text {string}, and new user text {string}"
)]
fn conversation(w: &mut World, first: String, reply: String, last: String) {
    w.usage.messages = vec![
        json!({"role":"user","content":first}),
        json!({"role":"assistant","content":reply}),
        json!({"role":"user","content":last}),
    ];
}

#[given(
    regex = r#"^its client tool has the name "([^"]+)", description "([^"]+)", and schema (\{.*\})$"#
)]
fn schema(w: &mut World, name: String, description: String, schema_json: String) {
    let schema: Value = serde_json::from_str(&schema_json).unwrap();
    w.usage.tools = vec![json!({"name":name,"description":description,"inputSchema":schema})];
}

#[given("the request needs a rebuild")]
fn rebuild(w: &mut World) {
    assert!(w.scenario.fake().record().prompts().is_empty());
}

#[given("an ACP session already holds the first user text and assistant text")]
async fn prefix(w: &mut World) {
    let mut call = w.usage.request("POST /v1/messages", false);
    call.body["messages"] = json!([w.usage.messages[0]]);
    w.scenario.fake().next_prompt(
        TurnPlan::new()
            .chunk(w.usage.messages[1]["content"].as_str().unwrap())
            .end("end_turn"),
    );
    let answer = w
        .scenario
        .client()
        .await
        .send(call)
        .finish(&mut w.scenario.time)
        .await;
    assert_eq!(answer.status, 200, "seed ACP session");
    assert_eq!(answer.json()["content"][0]["text"], "EFGH");
    w.usage.continued = true;
}

#[given(
    regex = r#"^the request has one earlier tool call "([^"]+)" with the (object|JSON string) (\{.*\})$"#
)]
fn earlier_call(w: &mut World, name: String, form: String, arguments: String) {
    let arguments = if form == "object" {
        serde_json::from_str(&arguments).unwrap()
    } else {
        json!(arguments)
    };
    w.usage.earlier_call = Some((name, arguments));
}

#[given(expr = "the result of that call is {string}, followed by the user text {string}")]
fn result(w: &mut World, tool_result: String, next: String) {
    w.usage.result = tool_result;
    w.usage.next_text = next;
}

#[given("the request has no system prompt, declared tool, or other message text")]
fn no_other_input(w: &mut World) {
    assert!(w.usage.system.is_none());
    assert!(w.usage.tools.is_empty());
    assert!(w.usage.messages.is_empty());
}

#[when(expr = "an API client sends that request with a stream to {string}")]
async fn stream(w: &mut World, endpoint: String) {
    w.usage
        .send(&mut w.scenario, w.usage.request(&endpoint, true))
        .await;
}

#[when(expr = "an API client sends that request with no stream to {string}")]
async fn body(w: &mut World, endpoint: String) {
    w.usage
        .send(&mut w.scenario, w.usage.request(&endpoint, false))
        .await;
}

#[when(expr = "an API client sends only the user text {string} with a stream to {string}")]
async fn only_text(w: &mut World, text: String, endpoint: String) {
    let call = endpoint_check::request(&endpoint, &text, true);
    w.usage.send(&mut w.scenario, call).await;
}

#[when(
    expr = "an API client sends a valid request with a stream to {string} with \"stream_options.include_usage\" set to {word}"
)]
async fn include_usage(w: &mut World, endpoint: String, include: String) {
    let mut call = w.usage.request(&endpoint, true);
    call.body["stream_options"] = json!({"include_usage":include == "true"});
    w.usage.send(&mut w.scenario, call).await;
}

#[when(
    expr = "an API client sends a valid request to {string} with \"stream\" set to true, \"stream_options.include_usage\" set to true, and \"n\" set to 1"
)]
async fn one_choice(w: &mut World, endpoint: String) {
    let mut call = w.usage.request(&endpoint, true);
    call.body["stream_options"] = json!({"include_usage":true});
    call.body["n"] = json!(1);
    w.usage.send(&mut w.scenario, call).await;
}

#[then(expr = "the body of the response carries the usage with {string} set to {int}")]
fn usage(w: &mut World, field: String, expected: u64) {
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().json()["usage"][field].as_u64(),
        Some(expected),
        "FR-025 last usage update"
    );
}

#[then("each cache token field and each reasoning token field in the usage is absent or 0")]
fn zero(w: &mut World) {
    let usage = &w.scenario.answer.as_ref().unwrap().json()["usage"];
    let input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"));
    assert_eq!(
        input.and_then(Value::as_u64),
        Some(5183),
        "real usage is present"
    );
    check::zero_details(usage, "usage");
}

#[then(expr = "the {string} of the {string} event are {int}")]
#[then(expr = "the {string} of the last {string} event are {int}")]
fn estimate(w: &mut World, field: String, event: String, expected: u64) {
    let usage = check::event_usage(w.scenario.answer.as_ref().unwrap(), &event);
    assert_eq!(
        usage[&field].as_u64(),
        Some(expected),
        "FR-025 {event} {field}"
    );
}

#[then(expr = "the {string} event carries {string}")]
fn initial(w: &mut World, event: String, field: String) {
    let usage = check::event_usage(w.scenario.answer.as_ref().unwrap(), &event);
    assert_eq!(
        usage[&field].as_u64(),
        Some(2),
        "FR-025 initial estimate of Hello"
    );
}

#[then(expr = "the last {string} event carries \"input_tokens\" {int} and \"output_tokens\" {int}")]
fn last(w: &mut World, event: String, input: u64, output: u64) {
    let usage = check::event_usage(w.scenario.answer.as_ref().unwrap(), &event);
    assert_eq!(
        usage["input_tokens"].as_u64(),
        Some(input),
        "FR-025 final stream input"
    );
    assert_eq!(
        usage["output_tokens"].as_u64(),
        Some(output),
        "FR-025 final stream output"
    );
}

#[then(expr = "the {string} event carries the usage with {string} {int}")]
fn completed(w: &mut World, event: String, field: String, expected: u64) {
    let usage = check::event_usage(w.scenario.answer.as_ref().unwrap(), &event);
    assert_eq!(
        usage[&field].as_u64(),
        Some(expected),
        "FR-025 completed response usage"
    );
}

#[then(expr = "the last chunk before \"data: [DONE]\" carries the usage with {string} {int}")]
fn final_chunk(w: &mut World, field: String, expected: u64) {
    let chunks = check::chat_chunks(w.scenario.answer.as_ref().unwrap());
    assert_eq!(
        chunks.last().unwrap()["usage"][&field].as_u64(),
        Some(expected),
        "FR-025 last Chat chunk usage"
    );
}

#[then("no chunk of the stream carries the usage")]
fn no_chunk_usage(w: &mut World) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(
        endpoint_check::text(&answer.events),
        "OK",
        "real stream reply"
    );
    for chunk in check::chat_chunks(answer) {
        assert!(
            chunk.get("usage").is_none_or(Value::is_null),
            "FR-025 include_usage false: {chunk}"
        );
    }
}

#[then("acp2api answers with a stream")]
fn is_stream(w: &mut World) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert!(answer.headers["content-type"].starts_with("text/event-stream"));
    assert_eq!(endpoint_check::text(&answer.events), "OK");
}

#[then("the last chunk of the stream before \"data: [DONE]\" carries the usage")]
fn carries_usage(w: &mut World) {
    let chunks = check::chat_chunks(w.scenario.answer.as_ref().unwrap());
    assert_eq!(
        chunks.last().unwrap()["usage"]["prompt_tokens"].as_u64(),
        Some(1000),
        "FR-014 last Chat chunk carries usage"
    );
}

#[then("the stream has one choice")]
fn choice(w: &mut World) {
    let chunks = check::chat_chunks(w.scenario.answer.as_ref().unwrap());
    let choices: Vec<_> = chunks
        .iter()
        .flat_map(|v| v["choices"].as_array().expect("choices array"))
        .collect();
    assert!(!choices.is_empty(), "one streamed choice");
    assert!(choices.iter().all(|v| v["index"] == 0), "only choice zero");
    assert!(
        chunks
            .iter()
            .all(|v| v["choices"].as_array().unwrap().len() <= 1)
    );
}
