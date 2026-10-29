use acp2api_acceptance::{Scenario, Setup, TurnPlan, TurnStep, client::ApiCall, real_limit};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pending_tool_update_resets_the_prompt_no_update_limit() {
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let mut plan = TurnPlan::new().after(Duration::from_secs(240));
        plan.steps.push(TurnStep::SafetyToolReport {
            id: "pending-tool".into(),
            title: Some("view_file".into()),
            status: "pending".into(),
            update: true,
        });
        plan = plan
            .after(Duration::from_secs(240))
            .chunk("Reply after eight minutes");
        s.fake().next_prompt(plan);
        let call = s
            .client()
            .await
            .send(ApiCall::one_user_message("POST /v1/messages", "Hi"));
        let answer = call.finish(&mut s.time).await;
        let cancels = s.fake().record().cancels();
        s.finish(true).await;
        assert_eq!(
            answer.status, 200,
            "FR-051 counts a tool update with no reply text"
        );
        assert_eq!(
            answer.json()["content"][0]["text"],
            "Reply after eight minutes"
        );
        assert!(
            cancels.is_empty(),
            "neither four-minute gap reaches the no-update limit"
        );
    })
    .await
    .expect("one HTTP request; fake/hub capture caps; no-update test run cap: 30 s");
}
