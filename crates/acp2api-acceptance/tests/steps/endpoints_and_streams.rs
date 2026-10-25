use crate::World;
use acp2api_acceptance::{TurnPlan, TurnStep, client::Call, endpoint_check as wire};
use cucumber::{given, then, when};
#[derive(Default)]
pub struct EndpointState {
    path: String,
    stream: bool,
    incremental: bool,
    live: Option<Call>,
}
impl std::fmt::Debug for EndpointState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EndpointState")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}
#[given(
    expr = "the official runtime sends the text chunks {string} and {string}, and then waits 10 s before the end of the prompt turn"
)]
fn chunks(w: &mut World, first: String, last: String) {
    let mut plan = w
        .usage
        .with_usage(TurnPlan::new().chunk(&first))
        .chunk(&last);
    plan.steps.push(TurnStep::HoldUntil("prompt-end".into()));
    w.scenario.fake().next_prompt(
        plan.after(std::time::Duration::from_secs(10))
            .end("end_turn"),
    );
    w.endpoint.incremental = true;
}
async fn send(w: &mut World, path: String, stream: bool) {
    w.endpoint.path = path.clone();
    w.endpoint.stream = stream;
    if !w.endpoint.incremental {
        w.scenario.fake().next_prompt(w.usage.plan());
    }
    let call = w
        .scenario
        .client()
        .await
        .send(w.usage.request(&path, stream));
    if w.endpoint.incremental {
        w.endpoint.live = Some(call);
        w.scenario.time.settle().await;
    } else {
        w.scenario.answer = Some(call.finish(&mut w.scenario.time).await);
    }
}
#[when(expr = "an API client sends a valid request to {string} with \"stream\" set to {word}")]
async fn valid(w: &mut World, path: String, stream: String) {
    send(w, path, stream == "true").await;
}
#[when(expr = "an API client sends a valid request with a stream to {string}")]
async fn stream(w: &mut World, path: String) {
    send(w, path, true).await;
}
#[then(
    regex = r"^acp2api answers with HTTP 200 and (one body|a stream) in the success form of the (.+)$"
)]
fn success(w: &mut World, form: String, _api: String) {
    let answer = w.scenario.answer.as_ref().expect("answer");
    assert_eq!(form == "a stream", w.endpoint.stream);
    assert_eq!(
        answer
            .headers
            .get("content-type")
            .is_some_and(|v| v.starts_with("text/event-stream")),
        w.endpoint.stream,
        "FR-010 reply form"
    );
    wire::verify(answer, &w.endpoint.path, "Hello", 1000, 2);
}
#[then(expr = "the reply text in the answer is {string}")]
fn reply(w: &mut World, text: String) {
    wire::verify(
        w.scenario.answer.as_ref().unwrap(),
        &w.endpoint.path,
        &text,
        1000,
        2,
    );
}
#[then(expr = "acp2api answers with HTTP {int}")]
fn status(w: &mut World, expected: u16) {
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, expected);
}
#[then(expr = "the answer has the content type {string}")]
fn content_type(w: &mut World, expected: String) {
    assert!(
        w.scenario
            .answer
            .as_ref()
            .unwrap()
            .headers
            .get("content-type")
            .is_some_and(|v| v.starts_with(&expected)),
        "FR-010 SSE content type"
    );
}
#[then("the stream has these events in this order:")]
fn order(w: &mut World, #[step] _step: &cucumber::gherkin::Step) {
    wire::verify(
        w.scenario.answer.as_ref().unwrap(),
        &w.endpoint.path,
        "Hello",
        1000,
        2,
    );
}
#[then(expr = "the last event of the stream is {string}")]
fn last(w: &mut World, event: String) {
    assert_eq!(
        w.scenario
            .answer
            .as_ref()
            .unwrap()
            .events
            .last()
            .expect("SSE event")
            .event,
        event
    );
    wire::verify(
        w.scenario.answer.as_ref().unwrap(),
        &w.endpoint.path,
        "Hello",
        1000,
        2,
    );
}
#[then(expr = "the event {string} carries {string}")]
fn carries(w: &mut World, event: String, field: String) {
    let event = w
        .scenario
        .answer
        .as_ref()
        .unwrap()
        .events
        .iter()
        .find(|e| e.event == event)
        .unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&event.data).unwrap();
    for part in field.split('.') {
        v = v[part].clone();
    }
    assert!(v.as_str().is_some_and(|id| id.starts_with("resp_")));
}
#[then(expr = "each line of the stream before the last line is a {string} line with a chunk")]
fn lines(w: &mut World, prefix: String) {
    let answer = w.scenario.answer.as_ref().unwrap();
    let body = std::str::from_utf8(&answer.body).unwrap();
    let lines: Vec<_> = body.lines().filter(|l| !l.is_empty()).collect();
    for line in &lines[..lines.len() - 1] {
        assert!(line.starts_with(&prefix));
        serde_json::from_str::<serde_json::Value>(line.strip_prefix("data: ").unwrap())
            .expect("chunk object");
    }
    wire::verify(answer, &w.endpoint.path, "Hello", 1000, 2);
}
#[then(expr = "the last line of the stream is {string}")]
fn final_line(w: &mut World, last: String) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(
        std::str::from_utf8(&answer.body)
            .unwrap()
            .lines()
            .rfind(|l| !l.is_empty()),
        Some(last.as_str())
    );
}
#[then(expr = "the API client gets a text delta with {string} before the prompt turn ends")]
async fn early(w: &mut World, expected: String) {
    w.scenario.time.settle().await;
    let live = w.endpoint.live.take().unwrap();
    let events = live.events();
    let held = w
        .scenario
        .hub
        .entries()
        .iter()
        .any(|e| e.event["kind"] == "gate" && e.event["name"] == "prompt-end");
    let end = w.scenario.hub.now() + 10_000;
    if held {
        w.scenario.fake().open_gate("prompt-end");
    }
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(
        wire::text(&events),
        expected,
        "FR-010 delta before prompt-end gate release"
    );
    assert!(held, "prompt-end gate was held at observation");
    let delta_at = events
        .iter()
        .find(|e| !wire::text(std::slice::from_ref(e)).is_empty())
        .unwrap()
        .at;
    assert!(delta_at < end);
    wire::verify(&answer, &w.endpoint.path, "Hello", 1000, 2);
    w.scenario.answer = Some(answer);
}
