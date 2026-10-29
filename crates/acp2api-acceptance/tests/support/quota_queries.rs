use crate::{World, quota_support as quota};
use acp2api_acceptance::{
    TurnPlan, TurnStep, client::Call, conversation_check as conversation, endpoint_check,
    real_limit,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub const RESULT: &str = "### Quota Limits\n- **Gemini Models**\n  - **Weekly Limit Remaining:** 75% remaining (resets Oct 24, 2026, 12:34 UTC)";

#[derive(Default)]
pub struct QueryState {
    pub history: Vec<Value>,
    pub refresh_at: u64,
    pub refresh: Option<Call>,
    pub sessions: usize,
}
impl std::fmt::Debug for QueryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryState").finish_non_exhaustive()
    }
}

pub async fn panel(w: &mut World) {
    w.panel.scenario(&mut w.scenario).await;
    w.panel.login_initial("quota-owner").await;
    assert_eq!(
        w.panel.login_answer.as_ref().unwrap().status,
        303,
        "owner has a real panel session"
    );
}

pub async fn refresh(w: &mut World) -> Call {
    w.queries.refresh_at = w.scenario.hub.now();
    w.panel
        .owner("quota-owner")
        .submit("/manage/refresh-quota", json!({}))
}

pub fn usage_reply(w: &mut World, quota_available: bool) {
    // Captured Gemini group and percentage grammar, with synthetic scenario values.
    let remaining = if quota_available { 75 } else { 0 };
    let text = format!(
        "### Quota Limits\n- **Gemini Models**\n  *(Models within this group: Gemini Flash, Gemini Pro)*\n  - **Weekly Limit Remaining:** {remaining}% remaining (resets Oct 18, 2026, 00:10 UTC)"
    );
    w.scenario.fake().next_usage(TurnPlan::new().chunk(&text));
}

pub fn calls(w: &World) -> usize {
    w.scenario.fake().record().usage_queries().len()
}

#[allow(
    clippy::disallowed_methods,
    reason = "Bound OS disconnect propagation without a fixed sleep or a virtual-time advance"
)]
pub async fn stopped(w: &mut World, event: &str) {
    let session = conversation::session(&w.scenario);
    let mut call = conversation::call(
        json!([
            {"role":"user","content":"Hello"},
            {"role":"assistant","content":"Hi."},
            {"role":"user","content":"Next event"}
        ]),
        Some("Be kind."),
        Value::Null,
    );
    w.request.plan = Some(match event {
        "a cut at a stop sequence" => {
            call.body["stop"] = json!(["STOP"]);
            TurnPlan::new().chunk("Visible. STOP hidden")
        }
        "a disconnect of the API client" => {
            call.body["stream"] = json!(true);
            TurnPlan {
                steps: vec![
                    TurnStep::Chunk("Visible. ".into()),
                    TurnStep::Chunk("hidden".into()),
                    TurnStep::HoldUntil("query-disconnect".into()),
                ],
                end: acp2api_acceptance::TurnEndPlan::Never,
            }
        }
        "the stderr error line 503 \"UNAVAILABLE\"" => {
            let mut plan = quota::error(503, None);
            if let TurnStep::ErrorLine { status, .. } = &mut plan.steps[1] {
                *status = "UNAVAILABLE".into();
            }
            plan
        }
        "the stop reason \"refusal\", and \"/usage\" shows quota for the model" => {
            usage_reply(w, true);
            TurnPlan::new().chunk("Visible. ").end("refusal")
        }
        _ => unreachable!(),
    });
    let live = quota::begin(w, call).await;
    assert_eq!(
        conversation::session(&w.scenario),
        session,
        "event occurs in the matching S1 session"
    );
    let reply = if event == "a disconnect of the API client" {
        let text = endpoint_check::text(&live.events());
        assert_eq!(
            text, "Visible. ",
            "disconnect setup observes the released text before closing HTTP"
        );
        drop(live);
        let deadline = Instant::now() + real_limit(Duration::from_secs(2));
        while !w
            .scenario
            .fake()
            .record()
            .cancels()
            .iter()
            .any(|call| call["params"]["sessionId"] == session)
            && Instant::now() < deadline
        {
            let hub = w.scenario.hub.clone();
            tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                .await
                .unwrap();
            w.scenario.time.settle().await;
        }
        assert!(
            w.scenario
                .fake()
                .record()
                .cancels()
                .iter()
                .any(|call| call["params"]["sessionId"] == session),
            "disconnect setup reached ACP cancellation"
        );

        text
    } else {
        let answer = live.finish(&mut w.scenario.time).await;
        let reply = if answer.status == 200 {
            answer.json()["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_owned()
        } else {
            String::new()
        };
        w.scenario.answer = Some(answer);
        reply
    };
    w.queries.history = vec![
        json!({"role":"user","content":"Hello"}),
        json!({"role":"assistant","content":"Hi."}),
        json!({"role":"user","content":"Next event"}),
    ];
    if !reply.is_empty() {
        w.queries
            .history
            .push(json!({"role":"assistant","content":reply}));
    }
    w.queries
        .history
        .push(json!({"role":"user","content":"Continue after event"}));
}
