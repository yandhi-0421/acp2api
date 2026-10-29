use crate::{World, places_support as queue, quota_support as quota};
use acp2api_acceptance::{TurnStep, client::Answer, endpoint_check, journal_check};
use cucumber::{given, then, when};

#[given(
    regex = r#"^a prompt turn ended with the stderr error line 429 "RESOURCE_EXHAUSTED" with (a reset time 2 minutes away|no reset time)$"#
)]
async fn signal(w: &mut World, reset: String) {
    quota::seed(w, reset.starts_with('a')).await;
}

#[given("\"/usage\" gives a reset time 10 minutes away")]
async fn usage_reset(w: &mut World) {
    // Captured grammar; time and percentage are synthetic scenario values.
    quota::usage(w, "### Quota Limits\n- **Gemini Models**\n  *(Models within this group: Gemini Flash, Gemini Pro)*\n  - **Weekly Limit Remaining:** 0% remaining (resets Oct 18, 2026, 00:10 UTC)").await;
}

#[given("\"/usage\" gives no reset time")]
async fn no_reset(w: &mut World) {
    quota::usage(w, "Quota unavailable; reset tomorrow.").await;
}

#[when(
    regex = r"^an API client sends a valid request (2 minutes and 1 second|60 minutes minus 1 ms|60 minutes|60 minutes plus 1 ms) later$"
)]
async fn later(w: &mut World, delay: String) {
    let millis = match delay.as_str() {
        "2 minutes and 1 second" => 121_000,
        "60 minutes minus 1 ms" => 3_599_999,
        "60 minutes" => 3_600_000,
        "60 minutes plus 1 ms" => 3_600_001,
        _ => unreachable!(),
    };
    w.scenario.time.advance_to(w.quota.since + millis).await;
    quota::send(
        w,
        endpoint_check::request("POST /v1/messages", quota::PROBE, false),
    )
    .await;
}

fn quota_429(w: &World) -> &Answer {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(answer.status, 429, "FR-036 quota wait refuses admission");
    answer
}

#[then("acp2api refuses the request with HTTP 429")]
async fn quota_status(w: &mut World) {
    quota::answer(w).await;
    quota_429(w);
}

#[then(expr = "acp2api refuses the request with HTTP 429 and the message {string}")]
async fn quota_message(w: &mut World, template: String) {
    quota::answer(w).await;
    let answer = quota_429(w);
    assert_eq!(
        answer.json()["error"]["message"],
        template.replace("<reset time>", quota::RESET),
        "FR-036 M-16"
    );
    assert_eq!(
        answer.json()["error"]["type"],
        "rate_limit_error",
        "FR-033 quota envelope"
    );
}

#[then(expr = "the header {string} has a value of {int} or less")]
fn retry_after(w: &mut World, name: String, maximum: u64) {
    let value = w
        .scenario
        .answer
        .as_ref()
        .unwrap()
        .headers
        .get(&name)
        .expect("FR-033 retry-after")
        .parse::<u64>()
        .unwrap();
    assert!(
        value > 0 && value <= maximum,
        "FR-036 retry-after {value}, expected 1..={maximum}"
    );
}

#[when("a quota signal starts a quota wait")]
async fn quota_signal(w: &mut World) {
    w.request.plan = Some(quota::error(429, Some(quota::RESET)));
    quota::send(
        w,
        endpoint_check::request("POST /v1/messages", "state change signal", false),
    )
    .await;
}

#[then("the log has one line for the change to the state \"quota wait\"")]
fn state_log(w: &mut World) {
    let states = journal_check::records(&w.scenario)
        .into_iter()
        .filter(|record| record["event"] == "state_change" && record["state"] == "Quota wait")
        .count();
    assert_eq!(states, 1, "FR-056 one quota wait state log line");
}

#[then("the terminal shows one line for the change to the state \"quota wait\"")]
fn state_terminal(w: &mut World) {
    let lines = quota::terminal_states(w);
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.trim_end().ends_with(" state Quota wait"))
            .count(),
        1,
        "FR-002 one quota wait terminal state line: {lines:?}"
    );
}

#[given("a quota wait ended at its reset time")]
async fn expired(w: &mut World) {
    quota::seed(w, true).await;
    w.scenario.time.advance_to(120_000).await;
}

#[when("a prompt turn ends with the stderr error line 402")]
async fn payment_signal(w: &mut World) {
    w.request.plan = Some(quota::error(402, Some("2026-10-18T00:04:00Z")));
    quota::send(
        w,
        endpoint_check::request("POST /v1/messages", "new quota signal", false),
    )
    .await;
}

#[then("acp2api is in the state \"quota wait\"")]
fn quota_state(w: &mut World) {
    let lines = quota::terminal_states(w);
    assert!(
        lines
            .last()
            .is_some_and(|line| line.trim_end().ends_with(" state Quota wait")),
        "FR-036 effective state is Quota wait: {lines:?}"
    );
}

#[given(
    "the next prompt turn gets the upstream error 429 RESOURCE_EXHAUSTED with a reset time 120 s away"
)]
fn next_error(w: &mut World) {
    w.request.plan = Some(quota::error(429, Some(quota::RESET)));
}

#[given(expr = "the prompt turns of the requests {string} and {string} run")]
async fn occupied(w: &mut World, a: String, b: String) {
    let mut plan = quota::error(429, Some(quota::RESET));
    plan.steps.insert(0, TurnStep::HoldUntil("quota-A".into()));
    w.request.plan = Some(plan);
    let live = queue::send(
        w,
        endpoint_check::request("POST /v1/messages", &queue::marker(&a), true),
    )
    .await;
    w.places.calls.push((a, Some(live)));
    queue::held(w, &b, false).await;
    assert_eq!(queue::prompts(w).len(), 2, "A and B occupy both places");
}

#[given("two requests wait in the queue")]
async fn two_queued(w: &mut World) {
    queue::queued(w, 2).await;
}

#[when("the prompt turn of \"A\" ends with the stderr error line 429 \"RESOURCE_EXHAUSTED\"")]
async fn fail_a(w: &mut World) {
    w.scenario.fake().open_gate("quota-A");
    w.scenario.time.settle().await;
    let live = queue::take(w, "A");
    w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
}

#[then("the two requests in the queue get HTTP 429")]
async fn flushed(w: &mut World) {
    for name in ["Q0", "Q1"] {
        let live = queue::take(w, name);
        let answer = live.finish(&mut w.scenario.time).await;
        let event = answer
            .events
            .last()
            .expect("accepted queued stream ends with an event");
        let body: serde_json::Value = serde_json::from_str(&event.data).unwrap();
        assert_eq!(
            (event.event.as_str(), body["error"]["type"].as_str()),
            ("error", Some("rate_limit_error")),
            "FR-037 queued {name} gets the 429 stream row"
        );
        assert!(
            !queue::has_prompt(w, name),
            "FR-037 flushed request never reaches ACP"
        );
    }
}

#[then("the prompt turn of \"B\" continues to its end, and \"B\" gets its full reply")]
async fn survivor(w: &mut World) {
    assert!(
        w.scenario.fake().record().cancels().is_empty(),
        "FR-037 quota wait cancels no active turn"
    );
    queue::end(w, "B").await;
}

#[then(regex = r#"^the official runtime (?:got|gets) "/usage" (one time|two times)$"#)]
fn usage_count(w: &mut World, count: String) {
    assert_eq!(
        w.scenario.fake().record().usage_queries().len(),
        if count == "one time" { 1 } else { 2 },
        "FR-034/FR-052 quota query count"
    );
}

#[then(regex = r#"^the official runtime (?:got|gets) no "/usage"$"#)]
fn no_usage(w: &mut World) {
    assert!(
        !w.scenario.fake().record().client_info().is_empty(),
        "positive control: initialized runtime is observable"
    );
    assert_eq!(
        w.scenario.fake().record().usage_queries().len(),
        0,
        "FR-036/FR-052 no unsolicited quota query"
    );
}

#[then(expr = "the quota error body has the \"error.type\" {string}")]
fn quota_error_type(w: &mut World, expected: String) {
    let answer = quota_429(w);
    assert_eq!(answer.json()["error"]["type"], expected);
}
