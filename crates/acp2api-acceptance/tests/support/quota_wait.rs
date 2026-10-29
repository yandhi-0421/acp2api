use crate::World;
use acp2api_acceptance::{
    TurnPlan, TurnStep,
    client::{ApiCall, Call},
    endpoint_check,
};

pub const RESET: &str = "2026-10-18T00:02:00Z";
pub const PROBE: &str = "quota admission probe";

#[derive(Default)]
pub struct QuotaState {
    pub seed: Option<Call>,
    pub since: u64,
}
impl std::fmt::Debug for QuotaState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuotaState").finish_non_exhaustive()
    }
}

pub fn error(code: u16, reset: Option<&str>) -> TurnPlan {
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::SessionStderr(
        "Antigravity SDK error processing prompt for session {session}".into(),
    ));
    plan.steps.push(TurnStep::ErrorLine {
        code,
        status: "RESOURCE_EXHAUSTED".into(),
        message: reset.map_or_else(
            || "Quota exhausted; reset time unavailable.".into(),
            |at| format!("Quota exhausted; resets {at}"),
        ),
    });
    plan
}

pub async fn begin(w: &mut World, call: ApiCall) -> Call {
    w.request.queue_next(&w.scenario);
    let live = w.scenario.client().await.send(call);
    w.scenario.time.settle().await;
    live
}

pub async fn send(w: &mut World, call: ApiCall) {
    let live = begin(w, call).await;
    w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
}

pub async fn seed(w: &mut World, reset: bool) {
    let mut plan = error(429, reset.then_some(RESET));
    if !reset {
        // The next Given sets /usage before virtual time releases the error.
        plan.steps
            .insert(0, TurnStep::After(std::time::Duration::from_millis(1)));
    }
    w.request.plan = Some(plan);
    let call = endpoint_check::request("POST /v1/messages", "quota signal seed", false);
    let live = begin(w, call).await;
    if reset {
        let answer = live.finish(&mut w.scenario.time).await;
        assert!(
            matches!(answer.status, 429 | 502),
            "seed reaches the upstream error path"
        );
        ready(w);
    } else {
        w.quota.since = w.scenario.hub.now() + 1;
        w.quota.seed = Some(live);
    }
}

pub fn ready(w: &mut World) {
    w.scenario.answer = None;
    w.request.plan = Some(TurnPlan::default());
    w.endpoint.defer();
}

pub async fn usage(w: &mut World, text: &str) {
    w.scenario.fake().next_usage(TurnPlan::new().chunk(text));
    let live = w.quota.seed.take().expect("deferred quota signal");
    let answer = live.finish(&mut w.scenario.time).await;
    assert!(
        matches!(answer.status, 429 | 502),
        "seed reaches the upstream error path"
    );
    ready(w);
}

pub async fn answer(w: &mut World) {
    if w.scenario.answer.is_none() {
        let live = w.endpoint.take_live();
        w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
    }
}

pub fn terminal_states(w: &World) -> Vec<String> {
    w.scenario
        .hub
        .entries()
        .iter()
        .filter_map(|entry| entry.event["text"].as_str().map(str::to_owned))
        .filter(|line| line.contains(" state "))
        .collect()
}
