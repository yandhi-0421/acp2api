use crate::World;
use acp2api_acceptance::{
    client::{ApiCall, Call},
    runtime_safety_check,
};
use serde_json::Value;

pub async fn begin(w: &mut World, call: ApiCall) -> Call {
    w.request.queue_next(&w.scenario);
    let live = w.scenario.client().await.send(call);
    w.scenario.time.settle().await;
    assert!(
        !runtime_safety_check::frames(&w.scenario, "session/prompt").is_empty(),
        "request reaches ACP through real HTTP"
    );
    live
}

pub async fn send(w: &mut World, call: ApiCall) {
    let live = begin(w, call).await;
    w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
}

pub fn frames(w: &World, method: &str) -> Vec<Value> {
    runtime_safety_check::frames(&w.scenario, method)
}
