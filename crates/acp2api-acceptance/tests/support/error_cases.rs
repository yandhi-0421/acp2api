use crate::{World, quota_support as quota, sign_in_support as sign_in};
use acp2api::Clock as _;
use acp2api_acceptance::{
    TurnPlan, TurnStep,
    adapters::HarnessClock,
    client::{Answer, ApiCall, Call},
    hub::HubLink,
};
use serde_json::Value;

#[derive(Debug, Default)]
pub struct ErrorCases {
    pub prompts_before: usize,
    pub checks_before: usize,
    pub stop: Option<tokio::task::JoinHandle<()>>,
}
impl Drop for ErrorCases {
    fn drop(&mut self) {
        if let Some(task) = self.stop.take() {
            task.abort();
        }
    }
}

pub fn upstream(code: u16, status: &str) -> TurnPlan {
    TurnPlan {
        steps: vec![
            TurnStep::SessionStderr(
                "Antigravity SDK error processing prompt for session {session}".into(),
            ),
            TurnStep::ErrorLine {
                code,
                status: status.into(),
                message: "Synthetic error-status test".into(),
            },
        ],
        ..TurnPlan::new()
    }
}

async fn begin(w: &mut World, call: ApiCall) -> Call {
    w.errors.prompts_before = w.scenario.fake().record().prompts().len();
    w.request.queue_next(&w.scenario);
    let live = w.scenario.client().await.send(call);
    w.scenario.time.settle().await;
    live
}

pub async fn send(w: &mut World, call: ApiCall) {
    let live = begin(w, call).await;
    w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
}

pub async fn send_current(w: &mut World, call: ApiCall) {
    let live = begin(w, call).await;
    w.scenario.answer = Some(live.finish_current(&mut w.scenario.time).await);
}

pub async fn stop_next_tick(w: &mut World) {
    let clock = HarnessClock::new(HubLink::new(&w.scenario.hub));
    let sleep = clock.sleep_until(w.scenario.hub.now() + 1);
    let hub = w.scenario.hub.clone();
    w.errors.stop = Some(tokio::spawn(async move {
        let hold = sleep.await;
        hub.stop();
        drop(hold);
    }));
    w.scenario.time.settle().await;
}

pub fn state_message(w: &World, state: &str) -> String {
    match state {
        "quota wait" => format!(
            "acp2api: the Google quota is used up until {}. acp2api starts no prompt turn before that time.",
            quota::RESET
        ),
        "sign-in required" => sign_in::message(w),
        "runtime unavailable" => format!(
            "acp2api: the official runtime is unavailable: the runtime process failed 3 times in 10 minutes. Accepted versions: 1.4.0. See http://127.0.0.1:{}/.",
            w.scenario.root.port
        ),
        _ => panic!("unknown state in the acceptance contract: {state}"),
    }
}

pub fn state_refusal(w: &World, answer: &Answer, status: u16, state: &str) {
    assert_eq!(
        answer.status, status,
        "FR-012/FR-037 effective state status"
    );
    assert_eq!(
        answer.json()["error"]["message"],
        state_message(w, state),
        "FR-032 message of the effective state"
    );
    assert!(answer.events.is_empty(), "unstreamed request has no events");
}

pub fn event(w: &World, name: &str) -> Value {
    let events = &w.scenario.answer.as_ref().unwrap().events;
    let matches: Vec<_> = events.iter().filter(|event| event.event == name).collect();
    assert_eq!(matches.len(), 1, "FR-027 exactly one {name} event");
    serde_json::from_str(&matches[0].data).expect("error event JSON")
}
