use crate::{World, keep_alive_support as keep_alive};
use acp2api_acceptance::{TurnEndPlan, TurnPlan, endpoint_check};
use cucumber::{given, then};
use std::time::Duration;

#[given(expr = "each place holds a prompt turn for the next {int} s")]
async fn occupied(w: &mut World, seconds: u64) {
    keep_alive::occupy(w, seconds).await;
}

#[given(
    expr = "the official runtime sends the text chunks {string} and {string}, and then sends no update until acp2api cancels the prompt turn"
)]
fn silent(w: &mut World, first: String, last: String) {
    keep_alive::silent(w, &first, &last);
}

#[then("the stream has no gap of 300 s or more between two events")]
async fn adjacent(w: &mut World) {
    keep_alive::adjacent(keep_alive::observe(w).await);
}

#[then("its first event arrives less than 300 s after acceptance")]
async fn first(w: &mut World) {
    keep_alive::first(keep_alive::observe(w).await);
}

#[then("it ends less than 300 s after its last event")]
async fn end(w: &mut World) {
    keep_alive::end(keep_alive::observe(w).await);
}

#[then(expr = "the stream has {string} events in the wait")]
async fn pings(w: &mut World, expected: String) {
    let observation = keep_alive::observe(w).await;
    let pings: Vec<_> = observation
        .blocks
        .iter()
        .filter(|block| block.at < observation.wait_end && block.event == expected)
        .collect();
    assert!(!pings.is_empty(), "FR-011 missing ping during queue wait");
    for ping in pings {
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&ping.data).unwrap(),
            serde_json::json!({"type":"ping"})
        );
    }
}

#[then(expr = "each event of the stream in the wait, other than {string}, is a {string} event")]
async fn only_ping(w: &mut World, initial: String, expected: String) {
    let observation = keep_alive::observe(w).await;
    for block in observation
        .blocks
        .iter()
        .filter(|block| block.at < observation.wait_end)
    {
        assert!(block.comments.is_empty(), "Anthropic uses ping events");
        assert!(
            block.event == initial || block.event == expected,
            "FR-011 unexpected Anthropic event during queue wait: {}",
            block.event
        );
    }
}

#[then(
    "each keep-alive event of the stream is an event of the OpenAI Responses API, not a comment line"
)]
async fn responses(w: &mut World) {
    keep_alive::responses_keep_alive(keep_alive::observe(w).await);
}

#[given(expr = "the next prompt turn sends updates with a gap of {int} minutes")]
fn update_gap(w: &mut World, minutes: u64) {
    assert!([4, 5].contains(&minutes), "bounded update-gap fixture");
    let mut plan = TurnPlan::new().chunk("Hel");
    if minutes == 4 {
        plan = plan
            .after(Duration::from_secs(240))
            .chunk("lo")
            .after(Duration::from_secs(240));
    } else {
        plan.end = TurnEndPlan::Never;
    }
    w.request.plan = Some(plan);
    w.endpoint.defer();
}

#[then("the prompt turn continues to its end, and the response has the full reply")]
async fn full_reply(w: &mut World) {
    let live = w.endpoint.take_live();
    let answer = live.finish(&mut w.scenario.time).await;
    assert!(
        w.scenario.fake().record().cancels().is_empty(),
        "FR-051 four-minute gaps must not cancel the prompt"
    );
    endpoint_check::verify(&answer, "POST /v1/messages", "Hello", 1000, 2);
    w.scenario.answer = Some(answer);
}

#[then(
    expr = "the official runtime gets a cancel of the prompt turn, and acp2api answers with HTTP {int} and the message {string}"
)]
async fn no_update(w: &mut World, status: u16, message: String) {
    let live = w.endpoint.take_live();
    let prompts = w.scenario.fake().record().prompts();
    assert_eq!(prompts.len(), 1, "one prompt under the no-update check");
    let deadline = prompts[0].at + 300_000;
    w.scenario.time.advance_to(deadline - 1).await;
    assert!(
        w.scenario.fake().record().cancels().is_empty(),
        "FR-051 no cancellation before five minutes"
    );
    w.scenario.time.advance_to(deadline).await;
    let cancels = w.scenario.fake().record().cancels();
    assert_eq!(
        cancels.len(),
        1,
        "FR-051 no-update deadline at 300000 ms requires session/cancel"
    );
    assert_eq!(cancels[0]["params"]["sessionId"], prompts[0].session);
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, status,
        "FR-033 update timeout is the 503 row"
    );
    assert_eq!(answer.headers["retry-after"], "30");
    assert_eq!(answer.json()["error"]["type"], "api_error");
    assert_eq!(answer.json()["error"]["message"], message);
    w.scenario.answer = Some(answer);
}
