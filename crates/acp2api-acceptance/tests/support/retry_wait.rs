use crate::World;
use acp2api_acceptance::{
    TurnPlan, TurnStep,
    client::{Answer, ApiCall},
    content_check,
    fake::TurnEndPlan,
};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Default)]
pub struct RetryState {
    pub endpoint: String,
    pub body: Value,
    pub sent_at: u64,
}

impl RetryState {
    pub fn call(&self) -> ApiCall {
        let mut call = ApiCall::one_user_message(&self.endpoint, "");
        call.body = self.body.clone();
        call
    }
}

pub fn upstream(code: u16, status: String, message: String) -> TurnPlan {
    TurnPlan {
        steps: vec![],
        end: TurnEndPlan::StopThenStderr {
            delay: Duration::ZERO,
            steps: vec![
                TurnStep::SessionStderr(
                    "Antigravity SDK error processing prompt for session {session}".into(),
                ),
                TurnStep::ErrorLine {
                    code,
                    status,
                    message,
                },
            ],
        },
    }
}

pub async fn send_at(w: &mut World, call: ApiCall, at: u64) {
    w.scenario.time.advance_to(at).await;
    w.retry.sent_at = at;
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
}

pub async fn seed(w: &mut World, plan: TurnPlan, status: u16, message: &str) {
    // A stop result needs the FR-028 window; immediate stderr evidence does not.
    let complete_at = if matches!(&plan.end, TurnEndPlan::Stop(_)) {
        200
    } else {
        0
    };
    w.request.plan = Some(plan);
    w.scenario.time.advance_to(0).await;
    w.retry.sent_at = 0;
    w.request.queue_next(&w.scenario);
    let call = w.retry.call();
    let live = w.scenario.client().await.send(call);
    w.scenario.time.advance_to(complete_at).await;
    w.scenario.answer = Some(live.finish_current(&mut w.scenario.time).await);
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(
        answer.status,
        status,
        "retry wait seed reaches its error row: {}",
        answer.json()
    );
    assert_eq!(answer.json()["error"]["message"], message);
    let prompts = w.scenario.fake().record().prompts();
    assert_eq!(
        prompts.len(),
        1,
        "one failed prompt is the positive control"
    );
    assert_eq!(prompts[0].at, 0, "seed prompt at virtual time zero");
    assert_eq!(
        prompts[0].text,
        w.retry.body["messages"][0]["content"].as_str().unwrap(),
        "seed conversation reaches ACP"
    );
}

pub fn refused_without_prompt(w: &World) {
    refusal(w.scenario.answer.as_ref().unwrap(), 503);
    let (before, _) = w.scenario.hub.active_request();
    assert_eq!(w.scenario.fake().record().prompts().len(), before);
}

pub fn refusal(answer: &Answer, status: u16) {
    assert_eq!(answer.status, status, "FR-029 retry wait refuses admission");
    assert!(
        answer.events.is_empty(),
        "refusal precedes stream acceptance"
    );
    let body = answer.json();
    assert_eq!(body["type"], "error", "Anthropic error envelope");
    assert_eq!(body["error"]["type"], "api_error", "FR-033 error row");
}
