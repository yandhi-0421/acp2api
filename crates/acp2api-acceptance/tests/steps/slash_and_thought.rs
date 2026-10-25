use crate::{World, slash_and_thought_support as check};
use acp2api_acceptance::{TurnStep, content_check, endpoint_check};
use cucumber::{given, then, when};
use serde_json::{Value, json};

#[when(
    expr = "an API client sends a request to {string} with one user message with the text {string}"
)]
async fn send_text(w: &mut World, endpoint: String, text: String) {
    w.request.queue_next(&w.scenario);
    content_check::send(
        &mut w.scenario,
        endpoint_check::request(&endpoint, &text, false),
    )
    .await;
}

#[when(
    expr = "an API client sends a request to {string} with one user message with one space and then the text {string}"
)]
async fn leading_space(w: &mut World, endpoint: String, text: String) {
    w.request.queue_next(&w.scenario);
    let call = endpoint_check::request(&endpoint, &format!(" {text}"), false);
    content_check::send(&mut w.scenario, call).await;
}

#[when(
    expr = "an API client sends a request to {string} with one user message with the text blocks {string} and {string}"
)]
async fn blocks(w: &mut World, endpoint: String, first: String, last: String) {
    let call = content_check::request(
        &endpoint,
        json!([
            content_check::text(&endpoint, &first),
            content_check::text(&endpoint, &last)
        ]),
    );
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
}

#[then(expr = "the first line of the prompt that the official runtime gets is {string}")]
fn first_line(w: &mut World, expected: String) {
    let prompts = w.scenario.fake().prompts();
    assert_eq!(prompts.len(), 1, "one client prompt");
    assert_eq!(
        prompts[0].lines().next(),
        Some(expected.as_str()),
        "FR-016 white space before slash"
    );
}

#[then("the official runtime runs no slash command")]
fn no_slash(w: &mut World) {
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, 200);
    assert_eq!(
        check::reply(w.scenario.answer.as_ref().unwrap()),
        "OK",
        "normal model reply"
    );
    assert!(
        w.scenario
            .hub
            .entries()
            .iter()
            .all(|entry| entry.event["kind"] != "slash-command"),
        "FR-016 no slash command executed"
    );
}

#[then("the official runtime is still signed in")]
fn signed_in(w: &mut World) {
    assert_eq!(
        w.scenario.fake().prompts().len(),
        1,
        "logout input reaches ACP"
    );
    assert!(
        w.scenario.fake().signed_in(),
        "FR-016 runtime stays signed in after client /logout"
    );
}

#[then("the next request runs")]
async fn next(w: &mut World) {
    let mut call = endpoint_check::request("POST /v1/messages", "Next request.", false);
    call.body["system"] = json!("A separate conversation.");
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().status,
        200,
        "FR-016 next request succeeds"
    );
    assert_eq!(check::reply(w.scenario.answer.as_ref().unwrap()), "OK");
    assert_eq!(w.scenario.fake().prompts().last().unwrap(), "Next request.");
}

#[then(
    expr = "the prompt that the official runtime gets has the text {string} and the text {string}"
)]
fn unchanged_blocks(w: &mut World, first: String, last: String) {
    assert_eq!(
        content_check::blocks(&w.scenario),
        vec![json!([
            {"type":"text","text":first},{"type":"text","text":last}
        ])],
        "FR-016 later slash text stays at its original place"
    );
}

#[then("the prompt has no role label")]
fn no_label(w: &mut World) {
    assert_eq!(
        w.scenario.fake().prompts(),
        ["Run this:/logout"],
        "FR-016 no added role label for later slash"
    );
}

#[given(expr = "the official runtime sends the thought text {string} and then the reply {string}")]
fn runtime_thought(w: &mut World, thought: String, reply: String) {
    w.request.before_reply.push(TurnStep::Thought(thought));
    w.request.reply = reply;
}

#[given(expr = "the official runtime also sends the thought text {string}")]
fn another_thought(w: &mut World, thought: String) {
    w.request.before_reply.push(TurnStep::Thought(thought));
}

#[then(expr = "the answer holds the reply {string}")]
fn reply(w: &mut World, expected: String) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(answer.status, 200, "FR-018 reply succeeds");
    assert_eq!(
        check::reply(answer),
        expected,
        "FR-018 reply survives thought removal"
    );
}

#[then(expr = "the answer holds no text {string}")]
fn no_thought(w: &mut World, text: String) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert!(
        !String::from_utf8_lossy(&answer.body).contains(&text),
        "FR-018 no thought in response body"
    );
    assert!(
        answer
            .events
            .iter()
            .all(|event| !event.data.contains(&text)),
        "FR-018 no thought in stream events"
    );
}

#[then(
    "the answer has no thinking block, no redacted_thinking block, no reasoning item, and no reasoning_content"
)]
fn no_shape(w: &mut World) {
    let answer = w.scenario.answer.as_ref().unwrap();
    if answer.events.is_empty() {
        check::no_thought_shape(&answer.json());
    } else {
        for event in &answer.events {
            assert!(
                !event.event.contains("reasoning") && !event.event.contains("thinking"),
                "FR-018 no thought event"
            );
            if event.data != "[DONE]" {
                check::no_thought_shape(&serde_json::from_str::<Value>(&event.data).unwrap());
            }
        }
    }
}

#[then("the log has one line about the dropped thought text")]
fn dropped(w: &mut World) {
    assert_eq!(
        check::reply(w.scenario.answer.as_ref().unwrap()),
        "Yes.",
        "real reply control"
    );
    let lines = acp2api_acceptance::journal_check::lines(&w.scenario);
    let count = lines
        .iter()
        .filter(|line| {
            let line = line.to_lowercase();
            line.contains("thought") && (line.contains("drop") || line.contains("ignor"))
        })
        .count();
    assert_eq!(
        count, 1,
        "FR-018 one diagnostic for dropped thought text; actual: {lines:?}"
    );
}

#[then(expr = "the log holds no text {string}")]
fn no_log_text(w: &mut World, text: String) {
    let lines = acp2api_acceptance::journal_check::lines(&w.scenario);
    assert!(!lines.is_empty(), "real log control");
    assert!(
        lines.iter().all(|line| !line.contains(&text)),
        "FR-018 logs exclude thought text"
    );
}

#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with (a thinking block in an assistant message|a redacted_thinking block in an assistant message|a reasoning item|reasoning_content in an assistant message) that holds the text "([^"]+)"$"#
)]
async fn input(w: &mut World, endpoint: String, kind: String, text: String) {
    let call = check::thought_input(&endpoint, &kind, &text);
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
}

#[then(expr = "the prompt that the official runtime gets holds no text {string}")]
fn no_input_thought(w: &mut World, text: String) {
    let prompts = w.scenario.fake().prompts();
    assert_eq!(prompts.len(), 1, "FR-018 request reaches ACP");
    assert!(prompts[0].contains("Continue."), "user text control");
    assert!(
        !prompts[0].contains(&text),
        "FR-018 input thought never reaches ACP"
    );
}
