use crate::{World, runtime_support as process};
use acp2api_acceptance::{Setup, TurnEndPlan, client::ApiCall};
use cucumber::{given, then, when};
use serde_json::json;

#[given(expr = "acp2api started in the normal state with the concurrency limit {int}")]
async fn started(w: &mut World, limit: u8) {
    w.places.limit = limit;
    process::start(w).await;
}
#[when("two API clients send requests of two conversations at the same time")]
async fn concurrent(w: &mut World) {
    process::held(w, "A", true, TurnEndPlan::IgnoreCancel).await;
    process::held(w, "B", false, TurnEndPlan::Stop("end_turn".into())).await;
}
#[then("the two prompt turns run at the same time")]
fn overlapping(w: &mut World) {
    assert_eq!(
        process::calls(w, "session/prompt").len(),
        2,
        "FR-041 both held turns have reached ACP before either ends"
    );
    assert_ne!(
        process::calls(w, "session/prompt")[0]["params"]["sessionId"],
        process::calls(w, "session/prompt")[1]["params"]["sessionId"]
    );
    assert_eq!(
        process::status(w, "B"),
        None,
        "B has not completed while A runs"
    );
    assert_eq!(process::calls(w, "session/cancel").len(), 0);
}
#[then("one runtime process runs")]
fn one(w: &mut World) {
    let servers = process::servers(w);
    assert_eq!(
        servers.len(),
        1,
        "FR-041 one runtime process for both ACP sessions"
    );
    assert!(process::alive(servers[0]["pid"].as_u64().unwrap() as u32));
}
#[given("acp2api is in the normal state with one runtime process")]
async fn idle_setup(w: &mut World) {
    process::start(w).await;
}
#[when("60 minutes pass with no request")]
async fn idle(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 3_600_000)
        .await;
}
#[then("the same runtime process still runs")]
fn still_running(w: &mut World) {
    assert_eq!(
        process::servers(w)
            .iter()
            .map(|s| s["pid"].as_u64().unwrap() as u32)
            .collect::<Vec<_>>(),
        w.process.original
    );
    assert!(
        w.process.original.iter().all(|pid| process::alive(*pid)),
        "FR-041 same runtime process survives idle time"
    );
}
#[given(expr = "the prompt turns of the requests {string} and {string} run in the runtime process")]
async fn two_running(w: &mut World, a: String, b: String) {
    w.scenario.hub.enable_debug223();
    process::held(w, &a, true, TurnEndPlan::IgnoreCancel).await;
    process::held(w, &b, false, TurnEndPlan::Stop("end_turn".into())).await;
    assert_eq!(process::calls(w, "session/prompt").len(), 2);
}
#[when("the runtime process exits, and acp2api did not cause the exit")]
async fn exited(w: &mut World) {
    process::exit(w).await;
}
#[then(expr = "{string} and {string} each get the error of the 503 row with the message {string}")]
async fn both_failed(w: &mut World, a: String, b: String, message: String) {
    process::observe(w, |w| process::status(w, &b).is_some()).await;
    assert_eq!(
        process::status(w, &b),
        Some(503),
        "FR-041 M-20 must end every running prompt after runtime exit"
    );
    let b = process::answer(w, &b).await;
    process::error(&b, 503, &message);
    let a = process::answer(w, &a).await;
    let events: Vec<_> = a
        .events
        .iter()
        .filter(|e| e.event == "error")
        .map(|e| serde_json::from_str::<serde_json::Value>(&e.data).unwrap())
        .collect();
    assert_eq!(
        events,
        vec![json!({"type":"error","error":{"type":"api_error","message":message}})],
        "FR-041 streamed A gets exactly one M-20 error"
    );
    assert!(!a.events.iter().any(|e| e.event == "message_stop"));
}
#[then("acp2api starts a new runtime process at once")]
async fn replaced(w: &mut World) {
    process::observe(w, |w| process::servers(w).len() >= 2).await;
    assert_eq!(
        process::servers(w).len(),
        2,
        "FR-041 replacement starts without a further request or virtual delay"
    );
    assert_ne!(process::servers(w)[0]["pid"], process::servers(w)[1]["pid"]);
    let starts: Vec<_> = w
        .scenario
        .hub
        .entries()
        .into_iter()
        .filter(|e| e.event["kind"] == "launch")
        .collect();
    assert_eq!(
        starts.last().unwrap().at,
        w.scenario.hub.now(),
        "FR-041 replacement has no virtual retry wait"
    );
}
#[given(
    expr = "the API client of {string} disconnected, so acp2api cancelled the prompt turn of {string}"
)]
async fn disconnected(w: &mut World, a: String, cancelled: String) {
    assert_eq!(a, cancelled);
    w.scenario.hub.debug223(
        "step_drop",
        format!(
            "name={a} session={:?} v={} counts={:?}",
            w.prompt_sessions.get(&a),
            w.scenario.hub.now(),
            w.scenario.hub.counts()
        ),
    );
    drop(process::take(w, &a));
    process::observe(w, |w| !process::calls(w, "session/cancel").is_empty()).await;
    w.scenario.hub.debug223(
        "observation_end",
        format!(
            "v={} cancels={} counts={:?}",
            w.scenario.hub.now(),
            process::calls(w, "session/cancel").len(),
            w.scenario.hub.counts()
        ),
    );
    for entry in w.scenario.hub.entries() {
        if entry.event["kind"] == "terminal"
            && let Some(line) = entry.event["text"]
                .as_str()
                .filter(|s| s.contains("[DEBUG-223]"))
        {
            eprint!("{line}");
        }
    }
    assert_eq!(
        process::calls(w, "session/cancel").len(),
        1,
        "disconnect reaches cancel at ACP boundary"
    );
    w.process.cancelled_at = w.scenario.hub.now();
    let session = process::calls(w, "session/cancel")[0]["params"]["sessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    w.process.cancelled = Some((a, session));
}
#[when(expr = "the prompt turn of {string} does not end 30 s after its cancel")]
async fn cancel_timeout(w: &mut World, a: String) {
    assert_eq!(a, w.process.cancelled.as_ref().unwrap().0);
    w.scenario
        .time
        .advance_to(w.process.cancelled_at + 29_999)
        .await;
    let sibling = w
        .process
        .calls
        .iter()
        .find(|(_, call)| call.is_some())
        .unwrap()
        .0
        .clone();
    assert_eq!(
        process::status(w, &sibling),
        None,
        "the sibling still runs before the 30 s boundary"
    );
    w.scenario
        .time
        .advance_to(w.process.cancelled_at + 30_000)
        .await;
}
#[then(expr = "{string} gets the error of the 503 row with the message {string}")]
async fn sibling_failed(w: &mut World, b: String, message: String) {
    process::observe(w, |w| process::status(w, &b).is_some()).await;
    assert_eq!(
        process::status(w, &b),
        Some(503),
        "FR-041 B gets M-20 at the 30 s cancel deadline"
    );
    process::error(&process::answer(w, &b).await, 503, &message);
}
#[when(expr = "the prompt turn of {string} ends 20 s after its cancel")]
async fn cancelled_end(w: &mut World, a: String) {
    assert_eq!(a, w.process.cancelled.as_ref().unwrap().0);
    w.scenario
        .time
        .advance_to(w.process.cancelled_at + 20_000)
        .await;
    let session = &w.process.cancelled.as_ref().unwrap().1;
    let prompt = process::calls(w, "session/prompt")
        .into_iter()
        .find(|call| call["params"]["sessionId"] == *session)
        .unwrap();
    w.scenario.hub.command("normal", json!({"op":"Out","line":json!({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"cancelled"}}).to_string()}));
    w.scenario.time.settle().await;
    w.scenario
        .time
        .advance_to(w.process.cancelled_at + 30_000)
        .await;
}
#[then(expr = "the prompt turn of {string} continues to its end")]
async fn sibling_finishes(w: &mut World, b: String) {
    assert_eq!(
        process::status(w, &b),
        None,
        "B remains live after A ends within 30 s"
    );
    w.scenario.fake().open_gate(&format!("runtime-{b}"));
    let answer = process::answer(w, &b).await;
    assert_eq!(answer.status, 200);
    assert_eq!(answer.json()["content"][0]["text"], format!("reply {b}"));
    assert_eq!(
        process::servers(w).len(),
        1,
        "a timely cancel does not replace the runtime process"
    );
}
#[given("the next start of a runtime process gives no answer")]
async fn silent_start(w: &mut World) {
    process::pending_start(w).await;
}
#[when("5 minutes pass after the start")]
async fn start_deadline(w: &mut World) {
    w.scenario.time.advance_to(299_999).await;
    assert_eq!(
        process::servers(w).len(),
        1,
        "no replacement before five minutes"
    );
    w.scenario.time.advance_to(300_000).await;
}
#[when(
    regex = r"^the runtime process exits with no cause from acp2api at the times (0 minutes, 4 minutes, and 9 minutes|0 minutes, 5 minutes, and 11 minutes)$"
)]
async fn failures(w: &mut World, times: String) {
    let minutes: Vec<u64> = times
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap())
        .collect();
    process::exit_at(w, &minutes).await;
}
#[then("acp2api starts a new runtime process after each exit")]
fn after_each(w: &mut World) {
    assert_eq!(
        w.process.exits,
        vec![2, 3, 4],
        "FR-041 failures outside the ten-minute window do not stop replacement"
    );
}
#[given(expr = "the prompt turn of the request {string} with no stream runs")]
async fn nonstream(w: &mut World, name: String) {
    process::held(w, &name, false, TurnEndPlan::IgnoreCancel).await;
}
#[given(expr = "the request {string} with no stream waits in the queue")]
async fn queued(w: &mut World, name: String) {
    process::held(w, &name, false, TurnEndPlan::Stop("end_turn".into())).await;
    assert_eq!(process::calls(w, "session/prompt").len(), 1);
}
#[when("the owner stops acp2api in its terminal")]
async fn stop(w: &mut World) {
    w.scenario.hub.stop();
    process::observe(w, |w| {
        !process::calls(w, "session/cancel").is_empty()
            || process::process_ids(w)
                .iter()
                .all(|pid| !process::alive(*pid))
    })
    .await;
}
#[then(expr = "the official runtime gets a cancel of the prompt turn of {string}")]
fn shutdown_cancel(w: &mut World, name: String) {
    let session = w
        .prompt_sessions
        .get(&name)
        .expect("named request reached ACP");
    assert!(
        process::calls(w, "session/cancel")
            .iter()
            .any(|v| v["params"]["sessionId"] == session.as_str()),
        "FR-035/FR-045 cancels the running prompt of {name}"
    );
}
#[then(expr = "{string} and {string} each get HTTP {int} with the message {string}")]
async fn shutdown_responses(w: &mut World, a: String, b: String, code: u16, message: String) {
    for name in [a, b] {
        process::error(&process::answer(w, &name).await, code, &message);
    }
}
#[given("the runtime process runs with two harness processes")]
async fn children(w: &mut World) {
    process::start(w).await;
    w.scenario
        .hub
        .command("normal", json!({"op":"StartHarness","count":2}));
    process::observe(w, |w| process::process_ids(w).len() == 3).await;
    w.process.original = process::process_ids(w);
    assert_eq!(
        w.process.original.len(),
        3,
        "two real harness children started"
    );
    assert!(w.process.original.iter().all(|pid| process::alive(*pid)));
}
#[when("a kill of acp2api ends it")]
async fn kill(w: &mut World) {
    w.scenario.kill().await;
}
#[then("the runtime process and its two harness processes have ended with no step of the owner")]
async fn tree_ended(w: &mut World) {
    process::observe(w, |w| {
        w.process.original.iter().all(|pid| !process::alive(*pid))
    })
    .await;
    assert_eq!(
        w.process
            .original
            .iter()
            .map(|pid| process::alive(*pid))
            .collect::<Vec<_>>(),
        vec![false; 3],
        "FR-045 OS confirms runtime process and both harness children ended"
    );
}
#[given(expr = "the version of acp2api is {string}")]
fn configured_version(w: &mut World, version: String) {
    w.scenario
        .plan(Setup::normal().configure(move |_, _, run| run.release.version = version.clone()));
}
#[given("Claude Code sends requests to acp2api")]
fn claude_code(_: &mut World) {}
#[when("acp2api starts the first runtime process")]
async fn first(w: &mut World) {
    w.process.identity_starts = 1;
    identity_start(w).await;
}
async fn identity_start(w: &mut World) {
    w.scenario.start().await;
    let mut call = ApiCall::one_user_message("POST /v1/messages", "identity check");
    call.headers
        .push(("user-agent".into(), "Claude-Code/2.1.299".into()));
    w.request.queue_next(&w.scenario);
    let answer = w
        .scenario
        .client()
        .await
        .send(call)
        .finish(&mut w.scenario.time)
        .await;
    assert_eq!(answer.status, 200);
}
#[when("acp2api starts a runtime process that replaces another")]
async fn identity_replacement(w: &mut World) {
    w.process.identity_starts = 2;
    identity_start(w).await;
    process::exit(w).await;
    process::observe(w, |w| process::servers(w).len() == 2).await;
}
#[then(
    expr = "the official runtime gets {string} with the name {string}, the title {string}, and the version {string}"
)]
fn identity(w: &mut World, field: String, name: String, title: String, version: String) {
    assert_eq!(field, "clientInfo");
    assert_eq!(
        w.scenario.fake().record().client_info(),
        vec![json!({"name":name,"title":title,"version":version}); w.process.identity_starts],
        "FR-048 every start gets the acp2api identity"
    );
}
