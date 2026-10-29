use crate::{World, places_support as queue, prompt_turn as turn, runtime_support as process};
use acp2api_acceptance::{
    TurnPlan, TurnStep, client::Call, conversation_check as conversation, endpoint_check,
    fake::TurnEndPlan, journal_check,
};
use cucumber::{given, then, when};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct EndState {
    live: Option<Call>,
    disconnected_at: u64,
    session: String,
    cut_user: String,
    cut_stop: String,
    cut_reply: String,
}
impl std::fmt::Debug for EndState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EndState").finish_non_exhaustive()
    }
}

#[when(
    regex = r#"^an API client sends a valid request( with a stream)? to "([^"]+)" with "(stop|stop_sequences)" set to (\[.*\])$"#
)]
async fn stop_request(
    w: &mut World,
    stream: String,
    endpoint: String,
    field: String,
    stops: String,
) {
    let mut call = w.request.request(&endpoint, !stream.is_empty());
    call.body[&field] = serde_json::from_str(&stops).unwrap();
    turn::send(w, call).await;
}

#[given(expr = "the next prompt turn ends with the stop reason {string}")]
fn stop_reason(w: &mut World, reason: String) {
    let plan = w.request.next_plan().end(&reason);
    w.request.plan = Some(plan);
}

#[then(expr = "the response has {string} set to {string}")]
fn end_field(w: &mut World, field: String, expected: String) {
    let body = w.scenario.answer.as_ref().unwrap().json();
    let actual = if field == "finish_reason" {
        &body["choices"][0][&field]
    } else {
        &body[&field]
    };
    assert_eq!(actual, &json!(expected), "FR-015/FR-030 {field}");
}

#[then(
    expr = "the response has \"stop_reason\" set to {string} and \"stop_sequence\" set to {string}"
)]
fn anthropic_stop(w: &mut World, reason: String, sequence: String) {
    let body = w.scenario.answer.as_ref().unwrap().json();
    assert_eq!(
        (&body["stop_reason"], &body["stop_sequence"]),
        (&json!(reason), &json!(sequence)),
        "FR-015 Anthropic stop"
    );
}

#[then(expr = "the response has \"status\" set to {string} with the reason {string}")]
fn incomplete(w: &mut World, status: String, reason: String) {
    let body = w.scenario.answer.as_ref().unwrap().json();
    assert_eq!(
        (&body["status"], &body["incomplete_details"]["reason"]),
        (&json!(status), &json!(reason)),
        "FR-030 Responses end"
    );
}

#[then("acp2api answers with HTTP 200 and no error")]
fn no_error(w: &mut World) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(answer.status, 200, "FR-030 normal end");
    assert!(answer.json()["error"].is_null(), "FR-030 no error");
}

#[then(expr = "the text of all stream events together is {string}")]
fn stream_text(w: &mut World, expected: String) {
    assert_eq!(
        endpoint_check::text(&w.scenario.answer.as_ref().unwrap().events),
        expected,
        "FR-015 stream cut across chunks"
    );
}

#[then(expr = "no stream event holds the character {string} of the match")]
fn no_match(w: &mut World, character: String) {
    let events = &w.scenario.answer.as_ref().unwrap().events;
    assert!(!events.is_empty(), "stream events reached the API client");
    for event in events {
        assert!(
            !endpoint_check::text(std::slice::from_ref(event)).contains(&character),
            "FR-015 stop prefix leaked in {}",
            event.data
        );
    }
}

#[given("an API client sent a valid request, and its prompt turn runs")]
async fn running(w: &mut World) {
    w.request.plan = Some(TurnPlan {
        steps: vec![
            TurnStep::Chunk("partial reply unknown to client".into()),
            TurnStep::HoldUntil("disconnect".into()),
        ],
        end: TurnEndPlan::Never,
    });
    let call = endpoint_check::request("POST /v1/chat/completions", "Disconnect test", true);
    w.ends.live = Some(turn::begin(w, call).await);
    w.ends.session = conversation::session(&w.scenario);
    assert!(
        w.scenario
            .hub
            .entries()
            .iter()
            .any(|entry| entry.event["kind"] == "gate" && entry.event["name"] == "disconnect"),
        "prompt turn is running before disconnect"
    );
}

#[when("the API client disconnects")]
async fn disconnect(w: &mut World) {
    w.ends.disconnected_at = w.scenario.hub.now();
    drop(w.ends.live.take().unwrap());
    w.scenario.time.settle().await;
}

#[then("the official runtime gets a cancel of the prompt turn at once")]
#[allow(
    clippy::disallowed_methods,
    reason = "Bound OS disconnect propagation without advancing the virtual clock"
)]
async fn cancel_at_once(w: &mut World) {
    let deadline = Instant::now() + acp2api_acceptance::real_limit(Duration::from_secs(2));
    while turn::frames(w, "session/cancel").is_empty() && Instant::now() < deadline {
        let hub = w.scenario.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
        w.scenario.time.settle().await;
    }
    let cancels = turn::frames(w, "session/cancel");
    assert_eq!(cancels.len(), 1, "FR-031 cancel on disconnect");
    assert_eq!(
        cancels[0]["params"]["sessionId"], w.ends.session,
        "FR-031 cancels the disconnected prompt"
    );
    assert_eq!(
        w.scenario.hub.now(),
        w.ends.disconnected_at,
        "FR-031 cancel without advancing virtual time"
    );
    let call = conversation::call(
        json!([
            {"role":"user","content":"Disconnect test"},
            {"role":"assistant","content":"partial reply unknown to client"},
            {"role":"user","content":"Continue after disconnect"}
        ]),
        None,
        Value::Null,
    );
    turn::send(w, call).await;
    assert_ne!(
        conversation::session(&w.scenario),
        w.ends.session,
        "FR-031 clears the disconnected ACP session even for a matching history"
    );
    assert!(
        conversation::text(&w.scenario).starts_with(conversation::NEUTRAL),
        "FR-021 rebuild after disconnect"
    );
}

#[given(expr = "the API client sent the user message {string} with the stop sequence {string}")]
fn cut_user(w: &mut World, user: String, stop: String) {
    w.ends.cut_user = user;
    w.ends.cut_stop = stop;
}

#[given(expr = "the official runtime replied {string}, so acp2api cut the reply to {string}")]
async fn cut_reply(w: &mut World, reply: String, _cut: String) {
    w.request.reply = reply;
    let mut call = conversation::call(
        json!([{"role":"user","content":w.ends.cut_user}]),
        Some("Be kind."),
        Value::Null,
    );
    call.body["stop"] = json!([w.ends.cut_stop]);
    turn::send(w, call).await;
    w.ends.session = conversation::session(&w.scenario);
    w.ends.cut_reply = crate::slash_and_thought_support::reply(w.scenario.answer.as_ref().unwrap());
}

#[when(
    expr = "the API client sends that conversation with the reply {string} and the new user message {string}"
)]
async fn after_cut(w: &mut World, reply: String, next: String) {
    let call = conversation::call(
        json!([
            {"role":"user","content":w.ends.cut_user},
            {"role":"assistant","content":reply},
            {"role":"user","content":next}
        ]),
        Some("Be kind."),
        Value::Null,
    );
    turn::send(w, call).await;
}

#[then(expr = "the first prompt of the new ACP session holds the reply {string}")]
fn rebuilt_reply(w: &mut World, reply: String) {
    assert_eq!(
        w.ends.cut_reply, reply,
        "FR-021 seed reply was cut before the rebuild"
    );
    assert_ne!(
        conversation::session(&w.scenario),
        w.ends.session,
        "FR-021 rebuild after cut"
    );
    assert!(
        conversation::text(&w.scenario).starts_with(conversation::NEUTRAL),
        "FR-021 neutral line after cut"
    );
    assert!(
        conversation::text(&w.scenario).contains(&format!("[assistant]\n{reply}\n[user]\nGo on.")),
        "FR-021 rebuild includes only the client reply up to the cut"
    );
}

#[then(expr = "that prompt holds no text {string}")]
fn no_uncut(w: &mut World, text: String) {
    assert!(
        !conversation::text(&w.scenario).contains(&text),
        "FR-021 discarded reply suffix absent from rebuild"
    );
}

#[when("the API client of the first request in the queue disconnects")]
async fn queued_disconnect(w: &mut World) {
    let live = queue::take(w, "Q0");
    assert_eq!(
        live.status(),
        Some(200),
        "queued stream was accepted before disconnect"
    );
    let request_id = live
        .request_id()
        .expect("accepted request has x-request-id");
    drop(live);
    let has_disconnected_terminal_record = |w: &World| {
        journal_check::has_disconnected_terminal_record(
            &w.scenario,
            journal_check::RequestRecordKey::RequestId(&request_id),
        )
    };
    process::observe(w, has_disconnected_terminal_record).await;
    assert!(
        has_disconnected_terminal_record(w),
        "FR-031 queued disconnect writes a disconnected terminal record"
    );
}

#[then(expr = "the count of the requests in the queue is {int}")]
async fn queue_count(w: &mut World, count: usize) {
    queue::queued(w, 31 - count).await;
    for (name, expected) in [("freed queue position", 200), ("full queue control", 529)] {
        w.request.plan = Some(TurnPlan::new().chunk("capacity probe"));
        let live = queue::send(w, endpoint_check::request("POST /v1/messages", name, true)).await;
        assert_eq!(
            live.status(),
            Some(expected),
            "FR-031 disconnect leaves exactly one original queued request: {name}"
        );
        w.places.calls.push((name.into(), Some(live)));
    }
}

#[then("the official runtime never gets a prompt for the request of that API client")]
async fn no_disconnected_prompt(w: &mut World) {
    queue::end(w, "A").await;
    let live = queue::take(w, "Q1");
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, 200,
        "the other queued request still completes"
    );
    assert!(queue::has_prompt(w, "Q1"));
    assert!(
        !queue::has_prompt(w, "Q0"),
        "FR-031 disconnected queued request never reaches ACP"
    );
}
