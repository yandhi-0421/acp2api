#![allow(clippy::disallowed_methods)]
use crate::{
    World, places_support as queue, query_support as query, quota_support as quota,
    runtime_support as process, session_limit_support as limit, sign_in_facts::captured,
    sign_in_support as sign_in,
};
use acp2api_acceptance::{
    TurnEndPlan, TurnPlan, TurnStep,
    client::{Answer, Call},
    conversation_check as conversation, endpoint_check,
    fake::ModelSpec,
    panel_check, real_limit,
    rpc_script::RpcReply,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// FR-041: the time limit of a non-prompt ACP call.
pub const LIMIT: u64 = 5 * 60 * 1000;
/// FR-041: a prompt turn that does not end 30 s after its cancel is a failure.
pub const CANCEL_LIMIT: u64 = 30_000;
const REQUEST_CAP: usize = 12;
const RUN_CAP: Duration = Duration::from_secs(60);
const LOG_CAP: usize = 4 * 1024 * 1024;
const TEXT: &str = "[call limit request]";
const SYSTEM: &str = "Call limit system prompt.";
const REPLY: &str = "call limit reply";
const RUNNING: &str = "[call limit running prompt turn]";
const QUEUED: &str = "[call limit queued request]";
const QUEUED_REPLY: &str = "call limit queued reply";
const NEXT: &str = "[call limit next request]";
const NEXT_REPLY: &str = "call limit next reply";
const LATE_SESSION: &str = "call-limit-late-session";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    New,
    Load,
    Check,
    ModelRead,
    Refresh,
}
impl Kind {
    pub fn parse(text: &str) -> Self {
        match text {
            "the \"session/new\" of a request" => Self::New,
            "the \"session/load\" of a request" => Self::Load,
            "the sign-in check of a request" => Self::Check,
            "the model read of the owner" => Self::ModelRead,
            "the \"/usage\" of a quota refresh" => Self::Refresh,
            other => panic!("unknown ACP call: {other}"),
        }
    }
}

#[derive(Default)]
pub struct CallLimitState {
    started: Option<Instant>,
    requests: usize,
    kind: Option<Kind>,
    gate: String,
    sent_at: u64,
    processes: usize,
    origin: Option<u64>,
    arrival: u64,
    failed_at: u64,
    caller: Option<Call>,
    running: Option<Call>,
    queued: Option<Call>,
}
impl std::fmt::Debug for CallLimitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallLimitState")
            .field("requests", &self.requests)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// The request cap, the run cap, and the log cap of each scenario of #211.
fn within_caps(w: &mut World) {
    let started = *w.call_limit.started.get_or_insert_with(Instant::now);
    assert!(
        started.elapsed() < real_limit(RUN_CAP),
        "time limit scenario run cap: 60 s"
    );
    let bytes: usize = w
        .scenario
        .hub
        .entries()
        .iter()
        .map(|entry| entry.event.to_string().len())
        .sum();
    assert!(bytes <= LOG_CAP, "time limit scenario log cap: 4 MiB");
}
fn count_request(w: &mut World) {
    within_caps(w);
    w.call_limit.requests += 1;
    assert!(
        w.call_limit.requests <= REQUEST_CAP,
        "time limit scenario request cap: {REQUEST_CAP}"
    );
}
pub async fn advance(w: &mut World, target: u64) {
    within_caps(w);
    w.scenario.time.advance_to(target).await;
    within_caps(w);
}
fn next_gate(w: &World) -> String {
    format!("call-limit-{}", w.call_limit.requests)
}
fn session_result(session: &str) -> Value {
    let mut result = captured("session");
    result["sessionId"] = json!(session);
    result
}
fn hold(w: &World, method: &str, gate: &str, session: &str) {
    w.scenario.fake().script_call(
        method,
        RpcReply::Held {
            gate: gate.into(),
            reply: Box::new(RpcReply::Reply {
                updates: vec![],
                result: session_result(session),
            }),
        },
    );
}
fn held_at(w: &World, gate: &str) -> Option<u64> {
    w.scenario
        .hub
        .entries()
        .iter()
        .find(|entry| entry.event["kind"] == "rpc-held" && entry.event["gate"] == gate)
        .map(|entry| entry.at)
}
async fn request(
    w: &mut World,
    messages: Value,
    system: Option<&str>,
    plan: Option<TurnPlan>,
) -> Call {
    count_request(w);
    if let Some(plan) = plan {
        w.scenario.fake().next_prompt(plan);
    }
    let call = conversation::call(messages, system, Value::Null);
    let live = w.scenario.client().await.send(call);
    w.scenario.time.settle().await;
    live
}
fn user(text: &str) -> Value {
    json!([{"role":"user","content":text}])
}
fn usage_plan(gate: &str, end: TurnEndPlan) -> TurnPlan {
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::HoldUntil(gate.into()));
    plan.steps.push(TurnStep::Chunk(query::RESULT.into()));
    plan.end = end;
    plan
}
async fn signed_out_start(w: &mut World) {
    sign_in::start(w).await;
    w.scenario.fake().set_signed_in(true);
}
async fn usage_sent(w: &mut World) {
    process::observe(w, |w| query::calls(w) > 0).await;
    let queries = w.scenario.fake().record().usage_queries();
    assert_eq!(queries.len(), 1, "setup: one \"/usage\" reaches the fake");
    w.call_limit.sent_at = queries[0].at;
}
async fn record_held(w: &mut World) {
    let gate = w.call_limit.gate.clone();
    let target = gate.clone();
    process::observe(w, |w| held_at(w, &target).is_some()).await;
    w.call_limit.sent_at = held_at(w, &gate).expect("setup: the call reaches the fake ACP agent");
}
fn track(w: &mut World, kind: Kind, gate: String, caller: Call) {
    w.call_limit.kind = Some(kind);
    w.call_limit.gate = gate;
    w.call_limit.caller = Some(caller);
    w.call_limit.processes = process::servers(w).len();
}

/// Starts the call and keeps its answer. The test releases the answer, or never does.
pub async fn begin(w: &mut World, kind: Kind, answers: bool) {
    let gate = next_gate(w);
    let reply = answers.then(|| TurnPlan::new().chunk(REPLY));
    let caller = match kind {
        Kind::New => {
            process::start(w).await;
            hold(w, "session/new", &gate, "call-limit-session");
            request(w, user(TEXT), Some(SYSTEM), reply).await
        }
        Kind::Load => {
            limit::fill(w, 2).await;
            process::exit(w).await;
            process::observe(w, |w| process::servers(w).len() == 2).await;
            assert_eq!(process::servers(w).len(), 2, "setup: a new runtime process");
            let mut conversation = w.limit.conversations.remove(0);
            conversation
                .messages
                .push(json!({"role":"user","content":TEXT}));
            hold(w, "session/load", &gate, &conversation.session);
            request(w, json!(conversation.messages), None, reply).await
        }
        Kind::Check => {
            signed_out_start(w).await;
            hold(w, "session/new", &gate, "call-limit-session");
            w.request.plan = Some(TurnPlan::new().chunk(REPLY));
            count_request(w);
            let call = endpoint_check::request("POST /v1/messages", &queue::marker("limit"), true);
            queue::send(w, call).await
        }
        Kind::ModelRead => {
            signed_out_start(w).await;
            hold(w, "session/new", &gate, "call-limit-session");
            count_request(w);
            let live = w
                .panel
                .owner("sign-in-owner")
                .submit("/manage/models", json!({}));
            w.scenario.time.settle().await;
            live
        }
        Kind::Refresh => {
            process::start(w).await;
            query::panel(w).await;
            let end = TurnEndPlan::Stop("end_turn".into());
            w.scenario.fake().next_usage(usage_plan(&gate, end));
            count_request(w);
            let live = query::refresh(w).await;
            w.scenario.time.settle().await;
            live
        }
    };
    track(w, kind, gate, caller);
    match kind {
        Kind::Refresh => usage_sent(w).await,
        _ => record_held(w).await,
    }
}
/// FR-041: an update of "/usage" comes 4 minutes after the call, and no end follows.
pub async fn usage_with_update(w: &mut World) {
    let gate = next_gate(w);
    process::start(w).await;
    query::panel(w).await;
    let mut plan = TurnPlan::new()
        .after(Duration::from_secs(240))
        .chunk("Partial");
    plan.steps.push(TurnStep::HoldUntil(gate.clone()));
    plan.end = TurnEndPlan::Never;
    w.scenario.fake().next_usage(plan);
    count_request(w);
    let live = query::refresh(w).await;
    w.scenario.time.settle().await;
    track(w, Kind::Refresh, gate, live);
    usage_sent(w).await;
}
pub async fn usage_for_reset(w: &mut World) {
    let gate = next_gate(w);
    process::start(w).await;
    count_request(w);
    quota::seed(w, false).await;
    let plan = usage_plan(&gate, TurnEndPlan::Never);
    w.scenario.fake().next_usage(plan);
    let live = w.quota.seed.take().expect("deferred quota signal");
    track(w, Kind::Refresh, gate, live);
    let settled = w.scenario.hub.now() + 300;
    advance(w, settled).await;
    usage_sent(w).await;
}
pub async fn usage_after_refusal(w: &mut World) {
    let gate = next_gate(w);
    process::start(w).await;
    count_request(w);
    w.request.plan = Some(TurnPlan::new().chunk("Visible. ").end("refusal"));
    let plan = usage_plan(&gate, TurnEndPlan::Never);
    w.scenario.fake().next_usage(plan);
    let call = endpoint_check::request("POST /v1/messages", "call limit refusal", false);
    let live = quota::begin(w, call).await;
    track(w, Kind::Refresh, gate, live);
    let settled = w.scenario.hub.now() + 300;
    advance(w, settled).await;
    usage_sent(w).await;
}
pub async fn set_config_option(w: &mut World) {
    let gate = next_gate(w);
    w.scenario.fake().catalog(&[ModelSpec {
        value: "gemini-3.8-flash".into(),
        name: "Gemini 3.8 Flash".into(),
        levels: vec!["low".into(), "high".into()],
    }]);
    process::start(w).await;
    hold(w, "session/set_config_option", &gate, "unused");
    count_request(w);
    let mut call = conversation::call(user(TEXT), None, Value::Null);
    call.body["reasoning_effort"] = json!("low");
    let live = w.scenario.client().await.send(call);
    w.scenario.time.settle().await;
    track(w, Kind::New, gate, live);
    record_held(w).await;
}
/// FR-041: 4 minutes after its start, this prompt turn gets an update.
pub async fn running_turn(w: &mut World) {
    process::start(w).await;
    let mut plan = TurnPlan::new()
        .after(Duration::from_secs(240))
        .usage(1000, 1_048_576);
    plan.steps
        .push(TurnStep::HoldUntil("call-limit-running".into()));
    let plan = plan.chunk(REPLY);
    let live = request(w, user(RUNNING), None, Some(plan)).await;
    assert!(
        limit::prompt(w, RUNNING).is_some(),
        "setup: the prompt turn runs"
    );
    w.call_limit.running = Some(live);
}
pub async fn queued_request(w: &mut World) {
    let plan = TurnPlan::new().chunk(QUEUED_REPLY);
    let live = request(w, user(QUEUED), None, Some(plan)).await;
    assert_eq!(live.status(), None, "setup: the request waits in the queue");
    assert!(
        limit::prompt(w, QUEUED).is_none(),
        "setup: the queued request has no prompt turn"
    );
    w.call_limit.queued = Some(live);
}
pub async fn queued_call(w: &mut World) {
    let gate = next_gate(w);
    w.call_limit.arrival = w.scenario.hub.now();
    hold(w, "session/new", &gate, "call-limit-session");
    let live = request(w, user(TEXT), Some(SYSTEM), None).await;
    assert_eq!(
        held_at(w, &gate),
        None,
        "setup: the queued request sends no ACP call"
    );
    track(w, Kind::New, gate, live);
}
pub async fn end_running_after_wait(w: &mut World) {
    advance(w, w.call_limit.arrival + 6 * 60 * 1000).await;
    w.scenario.fake().open_gate("call-limit-running");
    let live = w.call_limit.running.take().expect("a prompt turn runs");
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, 200,
        "FR-041 a wait in the queue does not count toward the time limit"
    );
    record_held(w).await;
    assert!(
        w.call_limit.sent_at >= w.call_limit.arrival + 6 * 60 * 1000,
        "setup: the request stays in the queue for 6 minutes"
    );
}
pub async fn disconnect(w: &mut World) {
    drop(w.call_limit.caller.take().expect("a caller waits"));
    w.scenario.time.settle().await;
}
/// Moves to 1 ms before the limit, checks that nothing ended, then moves to the limit.
pub async fn pass_limit(w: &mut World) {
    let at = w.call_limit.sent_at + LIMIT;
    advance(w, at - 1).await;
    assert_eq!(
        process::servers(w).len(),
        w.call_limit.processes,
        "FR-041 no failure before the 5 minutes end"
    );
    if let Some(caller) = &w.call_limit.caller {
        assert_eq!(
            caller.status(),
            None,
            "FR-041 the caller has no result before the 5 minutes end"
        );
    }
    advance(w, at).await;
}
pub async fn answer_just_in_time(w: &mut World) {
    let sent = w.call_limit.sent_at;
    advance(w, sent + LIMIT - 1).await;
    let gate = w.call_limit.gate.clone();
    match w.call_limit.kind.unwrap() {
        Kind::Refresh => w.scenario.fake().open_gate(&gate),
        _ => w.scenario.fake().release_call(&gate),
    }
    w.scenario.time.settle().await;
    advance(w, sent + LIMIT - 1 + 60_000).await;
}
pub async fn normal_result(w: &mut World) {
    let kind = w.call_limit.kind.unwrap();
    let live = w.call_limit.caller.take().expect("a caller waits");
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, 200,
        "FR-041 a call that answers in time has its normal result"
    );
    match kind {
        Kind::New | Kind::Load => assert_eq!(
            answer.json()["choices"][0]["message"]["content"],
            REPLY,
            "FR-041 the prompt turn runs after the answer"
        ),
        Kind::Check => assert_eq!(
            endpoint_check::text(&answer.events),
            REPLY,
            "FR-041 the prompt turn runs after the check"
        ),
        Kind::ModelRead => assert!(
            panel_check::text(&answer).contains("Model names read."),
            "FR-041 the model read has its normal result"
        ),
        Kind::Refresh => assert!(
            panel_check::text(&answer).contains("Quota refreshed."),
            "FR-041 the refresh has its normal result"
        ),
    }
}
pub fn no_new_process(w: &World) {
    assert_eq!(
        process::servers(w).len(),
        w.call_limit.processes,
        "FR-041 this call gives no new runtime process"
    );
}
pub async fn new_process_at_once(w: &mut World) {
    let expected = w.call_limit.processes + 1;
    process::observe(w, |w| process::servers(w).len() >= expected).await;
    assert_eq!(
        process::servers(w).len(),
        expected,
        "FR-041 a call past the time limit gives one new runtime process"
    );
    let launch = w
        .scenario
        .hub
        .entries()
        .into_iter()
        .rfind(|entry| entry.event["kind"] == "launch")
        .expect("a runtime process started");
    assert_eq!(
        launch.at,
        w.scenario.hub.now(),
        "FR-041 the new runtime process starts with no virtual wait"
    );
    w.call_limit.processes = expected;
}
async fn response(w: &mut World, live: Call, status: u16) -> Answer {
    w.scenario.time.settle().await;
    assert_eq!(
        live.status(),
        Some(status),
        "FR-041 HTTP status of the caller at the time limit"
    );
    live.finish_current(&mut w.scenario.time).await
}
async fn error_row(w: &mut World, live: Call, message: &str, stream_free: bool) {
    let answer = response(w, live, 503).await;
    assert_eq!(
        answer.json()["error"]["message"],
        message,
        "FR-033 the message of the 503 row"
    );
    assert_eq!(
        answer.headers.get("retry-after").map(String::as_str),
        Some("30"),
        "FR-033 the 503 row has retry-after: 30"
    );
    if stream_free {
        assert!(
            answer.events.is_empty(),
            "FR-041 a sign-in check ends before acceptance, with no stream event"
        );
    }
    w.call_limit.failed_at = w.scenario.hub.now();
}
pub async fn caller_error(w: &mut World, message: &str, stream_free: bool) {
    let live = w.call_limit.caller.take().expect("a caller waits");
    error_row(w, live, message, stream_free).await;
}
pub async fn running_error(w: &mut World, message: &str) {
    let live = w.call_limit.running.take().expect("a prompt turn runs");
    error_row(w, live, message, false).await;
}
pub async fn owner_error(w: &mut World, message: &str) {
    let live = w.call_limit.caller.take().expect("a caller waits");
    let answer = response(w, live, 422).await;
    assert!(
        panel_check::text(&answer).contains(message),
        "FR-041 the management panel shows M-20"
    );
}
pub async fn next_quota_error(w: &mut World) {
    let live = request(w, user(NEXT), None, None).await;
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(answer.status, 429, "FR-041 the quota wait continues");
    let message = answer.json()["error"]["message"].clone();
    assert!(
        message
            .as_str()
            .is_some_and(|text| text.starts_with("acp2api: the Google quota is used up until ")),
        "FR-036 M-16 for the next request, not {message}"
    );
    let retry: u64 = answer.headers["retry-after"].parse().unwrap();
    assert!(
        (3299..=3300).contains(&retry),
        "FR-036 60 minutes less the 5 minutes of the call: {retry}"
    );
}
pub async fn next_request(w: &mut World, status: u16) {
    let plan = TurnPlan::new().chunk(NEXT_REPLY);
    let live = request(w, user(NEXT), None, Some(plan)).await;
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(answer.status, status, "FR-041 the next request");
    assert_eq!(
        answer.json()["choices"][0]["message"]["content"],
        NEXT_REPLY,
        "FR-041 the next request runs a prompt turn"
    );
    let (at, _) = limit::prompt(w, NEXT).expect("FR-041 the prompt of the next request");
    assert_eq!(
        at,
        limit::current(w),
        "FR-041 the new runtime process runs it"
    );
}
pub async fn queued_response(w: &mut World) {
    let live = w.call_limit.queued.take().expect("a request waits");
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(answer.status, 200, "FR-050 the queued request stays");
    assert_eq!(
        answer.json()["choices"][0]["message"]["content"],
        QUEUED_REPLY,
        "FR-041 the queued request gets its own response"
    );
    let (at, _) = limit::prompt(w, QUEUED).expect("FR-041 the queued prompt");
    assert_eq!(
        at,
        limit::current(w),
        "FR-041 the new runtime process runs it"
    );
}
/// Each failure needs a call that started after the earlier failure.
pub async fn passes_at(w: &mut World, minutes: u64) {
    process::start(w).await;
    let now = w.scenario.hub.now();
    let origin = *w.call_limit.origin.get_or_insert(now);
    advance(w, origin + minutes * 60_000 - LIMIT).await;
    begin(w, Kind::New, false).await;
    pass_limit(w).await;
    caller_error(
        w,
        "acp2api: the runtime process failed, so acp2api ended the prompt turn.",
        false,
    )
    .await;
}
pub async fn exit_at(w: &mut World, minutes: u64) {
    let origin = w.call_limit.origin.expect("an earlier failure");
    advance(w, origin + minutes * 60_000).await;
    let before = process::servers(w).len();
    process::exit(w).await;
    process::observe(w, |w| process::servers(w).len() > before).await;
    assert_eq!(
        process::servers(w).len(),
        before + 1,
        "FR-041 each earlier failure leaves a runtime process"
    );
}
pub fn started_after_each_failure(w: &World) {
    assert_eq!(
        process::servers(w).len(),
        4,
        "FR-041 3 failures outside 10 minutes do not stop new runtime processes"
    );
}
pub async fn late_answer(w: &mut World) {
    let call = process::calls(w, "session/new")
        .pop()
        .expect("a session/new reached the fake");
    let line = json!({"jsonrpc":"2.0","id":call["id"],"result":session_result(LATE_SESSION)});
    w.scenario.hub.command(
        "normal",
        json!({"op":"AnswerAtStdinEnd","line":line.to_string()}),
    );
    process::observe(w, |w| has_entry(w, "stdin-end-armed")).await;
    assert!(
        has_entry(w, "stdin-end-armed"),
        "setup: the fake keeps the late answer"
    );
}
fn has_entry(w: &World, kind: &str) -> bool {
    w.scenario
        .hub
        .entries()
        .iter()
        .any(|entry| entry.event["kind"] == kind)
}
pub fn late_answer_unused(w: &World) {
    if cfg!(not(windows)) {
        assert!(
            has_entry(w, "stdin-end-answer"),
            "FR-041 the replaced runtime process wrote its late answer"
        );
    }
    assert!(
        limit::prompt(w, NEXT).is_some(),
        "FR-041 the next request has a prompt turn"
    );
    let uses: Vec<_> = limit::calls(w)
        .into_iter()
        .filter(|(_, call)| call["params"]["sessionId"] == LATE_SESSION)
        .collect();
    assert_eq!(uses, vec![], "FR-041 the late answer makes no ACP session");
}
pub async fn pass_cancel_limit(w: &mut World) {
    let at = w.call_limit.failed_at + CANCEL_LIMIT;
    advance(w, at - 1).await;
    no_new_process(w);
    advance(w, at).await;
}
pub async fn cleanup(w: &mut World) {
    let live = [
        w.call_limit.caller.take(),
        w.call_limit.running.take(),
        w.call_limit.queued.take(),
    ];
    if live.iter().any(Option::is_some) {
        drop(live);
        w.scenario.time.settle().await;
    }
}
