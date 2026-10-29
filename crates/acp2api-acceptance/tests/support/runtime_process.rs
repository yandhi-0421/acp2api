#![allow(clippy::disallowed_methods)]
use crate::World;
use acp2api_acceptance::{
    Setup, TurnEndPlan, TurnPlan, TurnStep,
    client::{Answer, ApiCall, Call},
    real_limit,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct ProcessState {
    pub calls: Vec<(String, Option<Call>)>,
    pub original: Vec<u32>,
    pub cancelled_at: u64,
    pub cancelled: Option<(String, String)>,
    pub exits: Vec<usize>,
    pub identity_starts: usize,
}
impl std::fmt::Debug for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessState")
            .field("original", &self.original)
            .finish_non_exhaustive()
    }
}
pub fn servers(w: &World) -> Vec<Value> {
    w.scenario
        .hub
        .entries()
        .into_iter()
        .filter(|e| e.event["kind"] == "hello" && e.event["hello"]["role"] == "server")
        .map(|e| e.event["hello"].clone())
        .collect()
}
pub fn calls(w: &World, method: &str) -> Vec<Value> {
    acp2api_acceptance::system_check::requests(&w.scenario)
        .into_iter()
        .filter(|v| v["method"] == method)
        .collect()
}
pub fn process_ids(w: &World) -> Vec<u32> {
    w.scenario
        .hub
        .entries()
        .iter()
        .filter(|e| e.event["kind"] == "hello")
        .map(|e| e.event["hello"]["pid"].as_u64().unwrap() as u32)
        .collect()
}
pub use acp2api_acceptance::setup::alive;

pub async fn observe(w: &mut World, condition: impl Fn(&World) -> bool) {
    let deadline = Instant::now() + real_limit(Duration::from_secs(2));
    loop {
        w.scenario.time.settle().await;
        if condition(w) || Instant::now() >= deadline {
            return;
        }
        let hub = w.scenario.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
    }
}

pub async fn start(w: &mut World) {
    crate::places_support::start(w).await;
    if w.process.original.is_empty() {
        w.process.original = servers(w)
            .iter()
            .map(|v| v["pid"].as_u64().unwrap() as u32)
            .collect();
    }
}
pub async fn exit_at(w: &mut World, minutes: &[u64]) {
    start(w).await;
    let origin = w.scenario.hub.now();
    for (index, minute) in minutes.iter().enumerate() {
        w.scenario.time.advance_to(origin + minute * 60_000).await;
        assert_eq!(
            servers(w).len(),
            index + 1,
            "FR-041 each earlier failure must leave a replacement for the next exit"
        );
        exit(w).await;
        observe(w, |w| servers(w).len() >= index + 2).await;
        w.process.exits.push(servers(w).len());
    }
}
pub async fn held(w: &mut World, name: &str, stream: bool, end: TurnEndPlan) {
    start(w).await;
    let mut plan = TurnPlan::new();
    plan.steps
        .push(TurnStep::HoldUntil(format!("runtime-{name}")));
    plan.steps.push(TurnStep::Chunk(format!("reply {name}")));
    plan.end = end;
    w.request.plan = Some(plan);
    w.request.queue_next(&w.scenario);
    assert!(
        w.process.calls.len() < 8,
        "runtime scenario API request cap: 8"
    );
    let mut call =
        ApiCall::one_user_message("POST /v1/messages", &format!("runtime conversation {name}"));
    call.body["stream"] = json!(stream);
    let live = w.scenario.client().await.send(call);
    w.process.calls.push((name.into(), Some(live)));
    w.scenario.time.settle().await;
}
pub fn take(w: &mut World, name: &str) -> Call {
    w.process
        .calls
        .iter_mut()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
        .take()
        .unwrap()
}
pub fn status(w: &World, name: &str) -> Option<u16> {
    w.process
        .calls
        .iter()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .status()
}
pub async fn answer(w: &mut World, name: &str) -> Answer {
    take(w, name).finish(&mut w.scenario.time).await
}
pub async fn exit(w: &mut World) {
    let before = servers(w).len();
    w.scenario
        .hub
        .command("normal", json!({"op":"Exit","code":17}));
    observe(w, |w| {
        w.scenario
            .hub
            .entries()
            .iter()
            .filter(|e| e.event["kind"] == "process_end" && e.event["role"] == "server")
            .count()
            >= before
    })
    .await;
    assert!(
        w.scenario
            .hub
            .entries()
            .iter()
            .any(|e| e.event["kind"] == "process_end" && e.event["role"] == "server"),
        "fake boundary exit was delivered"
    );
}
pub fn error(answer: &Answer, status: u16, message: &str) {
    assert_eq!(answer.status, status, "runtime failure HTTP row");
    assert_eq!(
        answer.json()["error"]["message"],
        message,
        "runtime failure message"
    );
    if status == 503 {
        assert_eq!(answer.json()["error"]["type"], "api_error");
        assert_eq!(
            answer.headers.get("retry-after").map(String::as_str),
            Some("30")
        );
    }
}

pub async fn pending_start(w: &mut World) {
    pending_start_with(w, Setup::normal(), &[]).await;
}

pub async fn pending_start_with(w: &mut World, setup: Setup, env: &[(&str, std::path::PathBuf)]) {
    use acp2api_acceptance::rpc_script::RpcReply;
    w.scenario
        .fake()
        .script_call("initialize", RpcReply::Silent);
    let run = w.scenario.root.seed(&w.scenario.hub, &setup);
    let path = w.scenario.root.path.join("pending-run.toml");
    std::fs::write(&path, toml::to_string(&run).unwrap()).unwrap();
    w.scenario.launch_host(env, &path);
    observe(w, |w| calls(w, "initialize").len() == 1).await;
    assert_eq!(
        calls(w, "initialize").len(),
        1,
        "verified fake reaches initialize before virtual time moves"
    );
}

pub async fn cleanup(w: &mut World) {
    if w.process.calls.is_empty() {
        return;
    }
    w.process.calls.clear();
    w.scenario.time.settle().await;
}
