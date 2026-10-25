use crate::{World, places_support as queue};
use acp2api_acceptance::endpoint_check;
use cucumber::{given, then};

#[given("the queue holds 32 requests")]
async fn full_queue(w: &mut World) {
    queue::full_places(w).await;
    queue::queued(w, 32).await;
}

#[then(expr = "acp2api answers with HTTP {int} and the header {string}")]
async fn refused(w: &mut World, status: u16, header: String) {
    let live = w.endpoint.take_live();
    assert_eq!(
        live.status(),
        Some(status),
        "FR-033 a full queue refuses at acceptance"
    );
    let answer = live.finish(&mut w.scenario.time).await;
    let (name, value) = header.split_once(": ").unwrap();
    assert_eq!(
        answer.headers.get(name).map(String::as_str),
        Some(value),
        "FR-033 full-queue retry header"
    );
    assert!(
        answer.json()["error"].is_object(),
        "FR-033 OpenAI error envelope"
    );
    w.scenario.answer = Some(answer);
}

#[then(
    expr = "acp2api refuses the request at once with HTTP {int}, the header {string}, and the message {string}"
)]
async fn anthropic_full(w: &mut World, status: u16, header: String, message: String) {
    let live = w.endpoint.take_live();
    assert_eq!(
        live.status(),
        Some(status),
        "FR-050 the 33rd queued request is refused at once"
    );
    let answer = live.finish(&mut w.scenario.time).await;
    let (name, value) = header.split_once(": ").unwrap();
    assert_eq!(answer.headers.get(name).map(String::as_str), Some(value));
    assert_eq!(answer.json()["error"]["type"], "overloaded_error");
    assert_eq!(answer.json()["error"]["message"], message);
    w.scenario.answer = Some(answer);
}

#[then("the API client already has the status 200 and the headers of the stream")]
fn accepted_stream(w: &mut World) {
    assert_eq!(
        w.places.queued_status,
        Some(200),
        "FR-026 status 200 arrives while the request is still queued"
    );
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(
        answer.headers.get("content-type").map(String::as_str),
        Some("text/event-stream"),
        "FR-026 accepted stream headers"
    );
    assert_eq!(
        endpoint_check::text(&answer.events),
        "OK",
        "accepted queued stream eventually completes"
    );
}
