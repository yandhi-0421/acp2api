use crate::{World, places_support as queue, runtime_support as process, sign_in_facts::captured};
use acp2api_acceptance::{
    TurnPlan, TurnStep,
    client::{Answer, Call},
    endpoint_check,
    fake::FakeRuntime,
    panel_check,
    rpc_script::RpcReply,
};
use serde_json::json;

#[derive(Debug, Default)]
pub struct SignInState {
    pub started: bool,
    pub checks_before: usize,
    pub sessions_before_restart: usize,
    pub process_before: Vec<u32>,
    pub address: String,
    pub wait_started: u64,
    pub queued_before: usize,
    pub calls_before_disconnect: usize,
}

pub fn initialize(fake: &FakeRuntime) {
    for _ in 0..5 {
        fake.script_call(
            "initialize",
            RpcReply::Reply {
                updates: vec![],
                result: captured("initialize"),
            },
        );
    }
}

pub async fn start(w: &mut World) {
    if w.sign_in.started {
        return;
    }
    let fake = w.scenario.fake();
    fake.set_signed_in(false);
    initialize(&fake);
    queue::start(w).await;
    w.sign_in.started = true;
    w.sign_in.checks_before = process::calls(w, "session/new").len();
    w.panel.scenario(&mut w.scenario).await;
    w.panel.login_initial("sign-in-owner").await;
    assert_eq!(
        w.panel.login_answer.as_ref().unwrap().status,
        303,
        "real panel session"
    );
}

pub fn message(w: &World) -> String {
    format!(
        "acp2api: the official runtime is not signed in. Sign in at http://127.0.0.1:{}/#sign-in.",
        w.scenario.root.port
    )
}

pub fn refusal(w: &World, answer: &Answer) {
    assert_eq!(
        answer.status, 400,
        "FR-038 failed sign-in check gives HTTP 400"
    );
    assert_eq!(
        answer.json(),
        json!({"type":"error","error":{"type":"invalid_request_error","message":message(w)}}),
        "FR-038 M-13 includes the sign-in entry"
    );
    assert!(
        answer.events.is_empty(),
        "FR-026 refusal has no stream event"
    );
    assert!(
        !answer
            .headers
            .get("content-type")
            .is_some_and(|v| v.starts_with("text/event-stream"))
    );
}

pub async fn held_check(w: &mut World, name: &str, success: bool) {
    let reply = if success {
        let mut result = captured("session");
        result["sessionId"] = json!(format!("checked-{name}"));
        RpcReply::Reply {
            updates: vec![],
            result,
        }
    } else {
        RpcReply::Error(json!({"code":-32000,"message":"Authentication required"}))
    };
    w.scenario.fake().script_call(
        "session/new",
        RpcReply::Held {
            gate: format!("sign-in-{name}"),
            reply: Box::new(reply),
        },
    );
    let mut plan = TurnPlan::new();
    plan.steps
        .push(TurnStep::HoldUntil(format!("queue-{name}")));
    w.request.plan = Some(plan.chunk(&format!("reply {name}")));
    let live = queue::send(
        w,
        endpoint_check::request("POST /v1/messages", &queue::marker(name), true),
    )
    .await;
    w.places.calls.push((name.into(), Some(live)));
}

pub fn held(w: &World, name: &str) -> bool {
    w.scenario
        .hub
        .entries()
        .iter()
        .any(|e| e.event["kind"] == "rpc-held" && e.event["gate"] == format!("sign-in-{name}"))
}

pub fn release(w: &mut World, name: &str) {
    assert!(
        held(w, name),
        "FR-038/FR-049 request {name} reaches its own sign-in check"
    );
    w.scenario.fake().release_call(&format!("sign-in-{name}"));
}

pub fn live<'a>(w: &'a World, name: &str) -> &'a Call {
    w.places
        .calls
        .iter()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
        .as_ref()
        .unwrap()
}

pub async fn authenticate(w: &mut World, complete: bool, token: Option<String>) {
    let address = if w.sign_in.address.is_empty() {
        "https://accounts.example.com/signin".into()
    } else {
        w.sign_in.address.clone()
    };
    w.scenario.fake().script_call(
        "authenticate",
        RpcReply::Authentication {
            address,
            token,
            result: complete.then(|| captured("authenticated")),
        },
    );
    w.sign_in.process_before = process::process_ids(w);
    w.sign_in.wait_started = w.scenario.hub.now();
    let answer = owner_action(w, "/manage/sign-in").await;
    assert_eq!(
        answer.status, 200,
        "FR-039 authenticated POST /manage/sign-in starts the sign-in"
    );
    w.scenario.time.settle().await;
}

pub async fn cancel(w: &mut World) {
    let answer = owner_action(w, "/manage/cancel-sign-in").await;
    assert_eq!(
        answer.status, 200,
        "FR-039 authenticated cancellation ends the wait"
    );
    w.scenario.time.settle().await;
}

pub async fn owner_action(w: &mut World, path: &str) -> Answer {
    let before = process::calls(w, "authenticate").len();
    let denied = w.panel.submit("anonymous", path, json!({})).await;
    assert_eq!(
        denied.status, 401,
        "FR-039 owner action requires a panel session: {path}"
    );
    assert_eq!(
        process::calls(w, "authenticate").len(),
        before,
        "unauthenticated owner action starts no authentication"
    );
    w.panel.submit("sign-in-owner", path, json!({})).await
}

pub async fn replaced(w: &mut World) {
    process::observe(w, |w| {
        process::process_ids(w).len() > w.sign_in.process_before.len()
    })
    .await;
    assert!(
        process::process_ids(w).len() > w.sign_in.process_before.len(),
        "FR-039 sign-in wait end starts a replacement"
    );
    assert!(
        w.sign_in
            .process_before
            .iter()
            .all(|pid| !process::alive(*pid)),
        "FR-039 old runtime process tree ended"
    );
}

pub async fn page(w: &mut World) -> String {
    let answer = w.panel.page("sign-in-owner").await;
    assert_eq!(
        answer.status, 200,
        "authenticated management panel is readable"
    );
    panel_check::text(&answer)
}

pub async fn sign_in_required(w: &mut World) {
    let html = page(w).await;
    assert!(
        html.contains("Sign-in required"),
        "FR-038 startup and failed sign-in expose Sign-in required"
    );
    assert!(
        html.contains("id=\"sign-in\""),
        "FR-039 sign-in entry has the designed fragment"
    );
}

pub async fn queue_count(w: &mut World, expected: usize) {
    let html = page(w).await;
    // FR-054 names the visible figure; its value is read independently of the fake's call count.
    let words = html.replace('<', " <");
    let plain = words
        .split('>')
        .filter_map(|part| part.split('<').next())
        .collect::<Vec<_>>()
        .join(" ");
    let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        plain.contains(&format!("Requests in the queue {expected}")),
        "FR-050 panel queue figure must be {expected}"
    );
}
