use crate::World;
use acp2api_acceptance::{
    Setup, TurnPlan, TurnStep,
    client::{ApiCall, Call},
    endpoint_check,
};

pub struct PlacesState {
    pub limit: u8,
    pub maximum: u8,
    started: bool,
    pub calls: Vec<(String, Option<Call>)>,
    pub tool_session: String,
    pub arrival: u64,
    pub queued_status: Option<u16>,
}
impl Default for PlacesState {
    fn default() -> Self {
        Self {
            limit: 2,
            maximum: 3,
            started: false,
            calls: Vec::new(),
            tool_session: String::new(),
            arrival: 0,
            queued_status: None,
        }
    }
}
impl std::fmt::Debug for PlacesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlacesState")
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}

pub async fn start(w: &mut World) {
    if w.places.started {
        return;
    }
    let limit = w.places.limit;
    let maximum = w
        .startup
        .run
        .as_ref()
        .map(|run| run.release.highest)
        .unwrap_or(w.places.maximum);
    w.scenario.plan(
        Setup::normal()
            .highest_concurrency_limit(maximum)
            .configure(move |root, _, _| {
                let path = root.path.join("data/config.toml");
                let mut config: toml::Value =
                    toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                config["concurrency_limit"] = toml::Value::Integer(i64::from(limit));
                std::fs::write(path, toml::to_string(&config).unwrap()).unwrap();
            }),
    );
    w.scenario.start().await;
    w.places.started = true;
}

pub async fn panel(w: &mut World) {
    if w.startup.panel_running() {
        w.panel.startup(&mut w.startup).await;
    } else {
        start(w).await;
        w.panel.scenario(&mut w.scenario).await;
    }
    w.panel.login_initial("B1").await;
    assert_eq!(w.panel.login_answer.as_ref().unwrap().status, 303);
}

pub fn marker(name: &str) -> String {
    format!("queue conversation [{name}]")
}

pub async fn send(w: &mut World, call: ApiCall) -> Call {
    start(w).await;
    assert!(w.places.calls.len() < 40, "places scenario request cap: 40");
    w.request.queue_next(&w.scenario);
    let live = w.scenario.client().await.send(call);
    w.scenario.time.settle().await;
    live
}

pub async fn held(w: &mut World, name: &str, updates: bool) {
    let mut plan = TurnPlan::new();
    if updates {
        for _ in 0..15 {
            plan.steps
                .push(TurnStep::After(std::time::Duration::from_secs(240)));
            plan.steps.push(TurnStep::Usage {
                used: 1000,
                size: 1_048_576,
            });
        }
    }
    plan.steps
        .push(TurnStep::HoldUntil(format!("queue-{name}")));
    plan.steps.push(TurnStep::Chunk(format!("reply {name}")));
    w.request.plan = Some(plan);
    let live = send(
        w,
        endpoint_check::request("POST /v1/messages", &marker(name), true),
    )
    .await;
    w.places.calls.push((name.into(), Some(live)));
}

pub fn prompts(w: &World) -> Vec<String> {
    w.scenario
        .fake()
        .record()
        .prompts()
        .into_iter()
        .map(|p| p.text)
        .collect()
}

pub fn has_prompt(w: &World, name: &str) -> bool {
    prompts(w).iter().any(|text| text.contains(&marker(name)))
}

pub fn take(w: &mut World, name: &str) -> Call {
    w.places
        .calls
        .iter_mut()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
        .take()
        .unwrap()
}

pub async fn end(w: &mut World, name: &str) {
    w.scenario.fake().open_gate(&format!("queue-{name}"));
    w.scenario.time.settle().await;
    w.scenario.time.advance_to(w.scenario.hub.now() + 200).await;
    let live = take(w, name);
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(answer.status, 200, "held prompt finishes through HTTP");
    assert_eq!(
        endpoint_check::text(&answer.events),
        format!("reply {name}")
    );
}

pub async fn full_places(w: &mut World) {
    held(w, "A", false).await;
    held(w, "B", false).await;
    assert_eq!(
        prompts(w).len(),
        2,
        "two occupied places before queue tests"
    );
    w.endpoint.defer();
}

pub async fn queued(w: &mut World, count: usize) {
    assert!(count <= 32, "queue fixture cap: 32 requests");
    let offset = w
        .places
        .calls
        .iter()
        .filter(|(name, _)| name.starts_with('Q'))
        .count();
    for n in offset..offset + count {
        let name = format!("Q{n}");
        w.request.plan = Some(TurnPlan::new().chunk("queued reply"));
        let live = send(
            w,
            endpoint_check::request("POST /v1/messages", &marker(&name), true),
        )
        .await;
        w.places.calls.push((name, Some(live)));
    }
    assert_eq!(
        prompts(w).len(),
        2,
        "queue fixture makes no additional ACP prompt"
    );
}

pub async fn cleanup(w: &mut World) {
    if w.places.calls.is_empty() {
        return;
    }
    w.places.calls.clear();
    w.scenario.time.settle().await;
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 10_000)
        .await;
}
