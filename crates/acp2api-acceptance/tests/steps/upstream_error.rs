use crate::{World, prompt_turn as wire};
use acp2api_acceptance::{
    TurnPlan, TurnStep, conversation_check as conversation, endpoint_check, fake::TurnEndPlan,
    journal_check,
};
use cucumber::{given, then, when};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct ErrorState {
    session: String,
    ended_at: u64,
    reply: String,
}

fn error_steps(message: &str) -> Vec<TurnStep> {
    vec![
        TurnStep::SessionStderr(
            "Antigravity SDK error processing prompt for session {session}".into(),
        ),
        TurnStep::ErrorLine {
            code: 503,
            status: "UNAVAILABLE".into(),
            message: message.into(),
        },
    ]
}
fn end_with_error(w: &mut World, delay: u64, message: &str) {
    w.request.plan.as_mut().unwrap().end = TurnEndPlan::StopThenStderr {
        delay: Duration::from_millis(delay),
        steps: error_steps(message),
    };
}

#[given(expr = "the official runtime sends the text chunks {string} and {string}")]
#[given(expr = "the official runtime sends {string} and then {string} as reply text chunks")]
fn chunks(w: &mut World, first: String, last: String) {
    w.request.plan = Some(TurnPlan::new().chunk(&first).chunk(&last));
}

#[given("the prompt turn ends")]
#[given("the prompt turn ends, and no error line comes")]
fn ends(_: &mut World) {}

#[given(
    regex = r"^the error line of the ACP session comes (before the end of the prompt turn|[0-9]+ ms after the end of the prompt turn)$"
)]
fn error_time(w: &mut World, time: String) {
    if time.starts_with("before") {
        let plan = w.request.plan.as_mut().unwrap();
        plan.steps
            .extend(error_steps("The service is unavailable."));
    } else {
        end_with_error(
            w,
            time.split_whitespace().next().unwrap().parse().unwrap(),
            "The service is unavailable.",
        );
    }
}

#[given(regex = r"^the official runtime then sends (a usage update|a chunk of thought text)$")]
fn non_reply(w: &mut World, update: String) {
    w.request
        .plan
        .as_mut()
        .unwrap()
        .steps
        .push(if update == "a usage update" {
            TurnStep::Usage {
                used: 1000,
                size: 1_048_576,
            }
        } else {
            TurnStep::Thought("synthetic thought".into())
        });
}

#[given("its stderr error line comes 150 ms after that end")]
fn delayed_error(w: &mut World) {
    end_with_error(w, 150, "The service is unavailable.");
}

#[given(
    "20 ms after the end, stderr has the line \"Antigravity SDK error processing prompt for session\" with the ID of the ACP session, and then the last line of its traceback"
)]
fn early_error(w: &mut World) {
    end_with_error(w, 20, "The service is unavailable.");
}

#[then(expr = "the stream has the text {string}")]
#[then(expr = "the reply text is only {string}")]
fn text(w: &mut World, expected: String) {
    assert_eq!(
        endpoint_check::text(&w.scenario.answer.as_ref().unwrap().events),
        expected,
        "FR-028 only released reply text reaches the stream"
    );
}

#[then(expr = "the stream has no text {string}")]
fn no_error_text(w: &mut World, excluded: String) {
    assert!(
        !endpoint_check::text(&w.scenario.answer.as_ref().unwrap().events).contains(&excluded),
        "FR-028 held upstream error text must be dropped"
    );
}

#[then(expr = "the stream ends with an {string} event")]
fn error_end(w: &mut World, event: String) {
    let events = &w.scenario.answer.as_ref().unwrap().events;
    assert_eq!(
        events.last().map(|e| e.event.as_str()),
        Some(event.as_str()),
        "FR-028 stream ends with error"
    );
    let body: Value = serde_json::from_str(&events.last().unwrap().data).unwrap();
    assert_eq!(
        body["error"]["type"], "api_error",
        "FR-033 temporary upstream error"
    );
}

#[then("the stream ends with an \"error\" event less than 200 ms after the end of the prompt turn")]
fn early_end(w: &mut World) {
    let event = w.scenario.answer.as_ref().unwrap().events.last().unwrap();
    assert_eq!(event.event, "error", "FR-028 early error event");
    assert!(
        event.at < 200,
        "FR-028 complete stderr traceback ends the wait early: {} ms",
        event.at
    );
}

#[given(expr = "the official runtime sends only the reply text chunk {string}")]
fn single_chunk(w: &mut World, text: String) {
    w.request.plan = Some(TurnPlan::new().chunk(&text));
}

#[given("the prompt turn ends 10 s later with no error line")]
fn single_end(w: &mut World) {
    let plan = w.request.plan.as_mut().unwrap();
    plan.steps.push(TurnStep::HoldUntil("single-end".into()));
    plan.steps.push(TurnStep::After(Duration::from_secs(10)));
    w.endpoint.defer();
}

#[then("the stream has no text delta before the end of the prompt turn")]
async fn held_before_end(w: &mut World) {
    w.scenario.time.settle().await;
    assert_eq!(
        endpoint_check::text(&w.endpoint.live().events()),
        "",
        "FR-010/FR-028 single chunk held before prompt end"
    );
    w.scenario.fake().open_gate("single-end");
    w.upstream.ended_at = w.scenario.hub.now() + 10_000;
    w.scenario.time.advance_to(w.upstream.ended_at).await;
}

#[then("the stream has no text delta during the next 199 ms")]
async fn held_in_wait(w: &mut World) {
    w.scenario.time.advance_to(w.upstream.ended_at + 199).await;
    assert_eq!(
        endpoint_check::text(&w.endpoint.live().events()),
        "",
        "FR-028 single chunk held through 199 ms"
    );
}

#[then(
    expr = "after the 200 ms error-line wait, the reply text is {string} and the stream ends normally"
)]
async fn released(w: &mut World, text: String) {
    w.scenario.time.advance_to(w.upstream.ended_at + 200).await;
    let events = w.endpoint.live().events();
    assert_eq!(
        endpoint_check::text(&events),
        text,
        "FR-028 held chunk released at 200 ms"
    );
    let answer = w.endpoint.take_live().finish(&mut w.scenario.time).await;
    assert!(
        answer
            .events
            .iter()
            .all(|event| !matches!(event.event.as_str(), "error" | "response.failed")),
        "normal stream end"
    );
    assert!(
        matches!(
            answer.events.last().unwrap().event.as_str(),
            "message_stop" | "response.completed"
        ) || answer.events.last().unwrap().data == "[DONE]",
        "API normal terminal event"
    );
    w.scenario.answer = Some(answer);
}

#[given(
    expr = "an API client got the reply {string} from a prompt turn in the ACP session {string}"
)]
fn late_first(w: &mut World, reply: String, session: String) {
    assert_eq!(session, "S1");
    w.upstream.reply = reply;
}

#[given("the error line of that ACP session came 201 ms after the end of the prompt turn")]
async fn late_error(w: &mut World) {
    w.request.plan = Some(TurnPlan::new().chunk(&w.upstream.reply));
    end_with_error(w, 201, "The service is unavailable.");
    let call = endpoint_check::request("POST /v1/chat/completions", "Late error test", false);
    wire::send(w, call).await;
    w.upstream.session = conversation::session(&w.scenario);
    w.scenario.time.advance_to(201).await;
}

#[when("the API client sends the next request of that conversation after its retry wait")]
async fn after_retry(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 30_001)
        .await;
    let call = conversation::call(
        json!([
            {"role":"user","content":"Late error test"}, {"role":"assistant","content":w.upstream.reply}, {"role":"user","content":"Continue"}
        ]),
        None,
        Value::Null,
    );
    wire::send(w, call).await;
}

#[then("the official runtime gets a new ACP session for that request")]
fn new_session(w: &mut World) {
    assert_ne!(
        conversation::session(&w.scenario),
        w.upstream.session,
        "FR-028 late error prevents continuation"
    );
}

#[then("the log has a line about the late error line")]
fn late_log(w: &mut World) {
    let lines = journal_check::lines(&w.scenario);
    assert!(
        lines
            .iter()
            .any(|line| line.to_ascii_lowercase().contains("late") && line.contains("error")),
        "FR-028 late error log: {lines:?}"
    );
}

#[given(expr = "the next ACP call fails with code {int} and message {string}")]
fn rpc(w: &mut World, code: i64, message: String) {
    w.request.plan = Some(TurnPlan {
        steps: vec![],
        end: TurnEndPlan::RpcError { code, message },
    });
}
#[given("the official runtime sends no stderr error line")]
fn no_stderr(_: &mut World) {}

#[when(expr = "an API client sends a valid request with no stream to {string}")]
async fn no_stream(w: &mut World, endpoint: String) {
    let call = w.request.request(&endpoint, false);
    wire::send(w, call).await;
}

#[then("the request gets HTTP 502 in the error envelope of its API")]
fn rpc_envelope(w: &mut World) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(answer.status, 502, "FR-033 ACP error without HTTP line");
    let body = answer.json();
    assert!(
        body["error"]["message"].is_string(),
        "FR-032 API error envelope"
    );
    assert!(body["error"]["type"].is_string(), "API error type");
    if answer.json().get("type").is_some() {
        assert_eq!(body["type"], "error");
        assert_eq!(body["error"]["type"], "api_error");
    }
}

#[then(expr = "the error message is {string}")]
#[then(expr = "the error body holds the message {string}")]
fn message(w: &mut World, expected: String) {
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().json()["error"]["message"],
        expected,
        "M-18 upstream error text"
    );
}

#[given(
    expr = "the official runtime sent the text chunks {string} and {string} in a streamed request to {string}"
)]
async fn text_stream(w: &mut World, first: String, last: String, endpoint: String) {
    w.request.plan = Some(TurnPlan {
        steps: vec![
            TurnStep::Chunk(first),
            TurnStep::Chunk(last),
            TurnStep::HoldUntil("upstream-error".into()),
        ],
        end: TurnEndPlan::Stop("end_turn".into()),
    });
    w.request
        .plan
        .as_mut()
        .unwrap()
        .steps
        .extend(error_steps("The service is currently unavailable."));
    let call = endpoint_check::request(&endpoint, "Hello", true);
    let live = wire::begin(w, call).await;
    w.endpoint.set_live(live);
}

#[given(expr = "a streamed request to {string} was accepted")]
async fn accepted(w: &mut World, endpoint: String) {
    w.request.plan = Some(TurnPlan {
        steps: vec![TurnStep::HoldUntil("upstream-error".into())],
        end: TurnEndPlan::Stop("end_turn".into()),
    });
    w.request
        .plan
        .as_mut()
        .unwrap()
        .steps
        .extend(error_steps("The service is currently unavailable."));
    let call = endpoint_check::request(&endpoint, "Hello", true);
    let live = wire::begin(w, call).await;
    assert_eq!(live.status(), Some(200), "stream accepted before error");
    w.endpoint.set_live(live);
}

#[when("the prompt turn ends with an error of the 503 row")]
#[when("an error of the 503 row ends the request")]
async fn fail_stream(w: &mut World) {
    w.scenario.fake().open_gate("upstream-error");
    let live = w.endpoint.take_live();
    w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
}

#[then(
    "the stream has a \"content_block_stop\" event for the open text block and then the \"error\" event"
)]
fn block_stop(w: &mut World) {
    let events = &w.scenario.answer.as_ref().unwrap().events;
    let names: Vec<_> = events.iter().map(|event| event.event.as_str()).collect();
    assert!(
        names.ends_with(&["content_block_stop", "error"]),
        "FR-027 close open block before error: {names:?}"
    );
}

#[then(expr = "the stream has a \"response.failed\" event with the code {string}")]
fn failed_response(w: &mut World, code: String) {
    let event = w.scenario.answer.as_ref().unwrap().events.last().unwrap();
    assert_eq!(
        event.event, "response.failed",
        "FR-027 Responses error event"
    );
    let data: Value = serde_json::from_str(&event.data).unwrap();
    assert_eq!(
        data["response"]["error"]["code"], code,
        "FR-027 Responses error code"
    );
}

#[then(expr = "\"error.headers\" of the event holds {string} with the value {int}")]
fn error_headers(w: &mut World, header: String, value: u64) {
    let data: Value = serde_json::from_str(
        &w.scenario
            .answer
            .as_ref()
            .unwrap()
            .events
            .last()
            .unwrap()
            .data,
    )
    .unwrap();
    assert_eq!(
        data["response"]["error"]["headers"][&header]
            .as_u64()
            .or_else(|| data["response"]["error"]["headers"][&header]
                .as_str()
                .and_then(|v| v.parse().ok())),
        Some(value),
        "FR-027 retry-after in error.headers"
    );
}

#[then("the stream has one \"data:\" line with an \"error\" object")]
fn chat_error(w: &mut World) {
    let events = &w.scenario.answer.as_ref().unwrap().events;
    let errors: Vec<_> = events
        .iter()
        .filter_map(|event| serde_json::from_str::<Value>(&event.data).ok())
        .filter(|body| body.get("error").is_some())
        .collect();
    assert_eq!(errors.len(), 1, "FR-027 one Chat error data line");
}

#[then("the stream ends after that line with no \"data: [DONE]\"")]
fn no_done(w: &mut World) {
    let answer = w.scenario.answer.as_ref().unwrap();
    let last = answer.events.last().unwrap();
    assert!(
        serde_json::from_str::<Value>(&last.data).unwrap()["error"].is_object(),
        "FR-027 last line is the error"
    );
    assert!(
        !std::str::from_utf8(&answer.body)
            .unwrap()
            .contains("data: [DONE]"),
        "FR-027 error stream has no DONE"
    );
}

#[given(
    expr = "the next prompt turn gets the stderr error line with the code {int}, the status {string}, and the message {string}"
)]
#[given(
    expr = "the stderr then has the error line with the code {int}, the status {string}, and the message {string}"
)]
fn stderr_error(w: &mut World, code: u16, status: String, message: String) {
    let plan = w.request.plan.get_or_insert_with(TurnPlan::new);
    plan.steps.extend([
        TurnStep::SessionStderr(
            "Antigravity SDK error processing prompt for session {session}".into(),
        ),
        TurnStep::ErrorLine {
            code,
            status,
            message,
        },
    ]);
}

#[given(expr = "the stderr of the official runtime has the line {string}")]
fn stderr(w: &mut World, line: String) {
    w.request
        .plan
        .get_or_insert_with(TurnPlan::new)
        .steps
        .push(TurnStep::Stderr(line));
}

#[given(expr = "the stderr of the official runtime has an error line with the message {string}")]
fn error_message(w: &mut World, message: String) {
    w.request.plan = Some(TurnPlan::new());
    end_with_error(w, 0, &message);
}

#[then(expr = "the log holds the code {int}, the status {string}, and the message {string}")]
fn parsed_log(w: &mut World, code: u16, status: String, message: String) {
    let lines = journal_check::lines(&w.scenario);
    assert!(
        lines.iter().any(|line| line.contains(&code.to_string())
            && line.contains(&status)
            && line.contains(&message)),
        "FR-056 parsed stderr error in log: {lines:?}"
    );
}
#[then(expr = "the log holds {string}")]
fn logged(w: &mut World, text: String) {
    let lines = journal_check::lines(&w.scenario);
    assert!(
        lines.iter().any(|line| line.contains(&text)),
        "FR-056 error text in log: expected {text:?}, actual {lines:?}"
    );
}
#[then(expr = "the log does not hold {string}")]
fn not_logged(w: &mut World, text: String) {
    assert!(
        journal_check::lines(&w.scenario)
            .iter()
            .all(|line| !line.contains(&text)),
        "FR-056 raw stderr or personal data must not reach logs"
    );
}
