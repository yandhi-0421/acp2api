#[path = "support/fake_pipe.rs"]
#[allow(
    dead_code,
    reason = "This target uses only the /usage operations of the shared fake pipe"
)]
mod fake_pipe;
use acp2api_acceptance::{TurnPlan, TurnStep};
use fake_pipe::FakePipe;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn usage_plans_are_independent_of_normal_prompts_and_keep_the_default() {
    let mut pipe = FakePipe::new();
    pipe.scenario
        .fake()
        .next_prompt(TurnPlan::new().chunk("normal reply"));
    pipe.scenario
        .fake()
        .next_usage(TurnPlan::new().chunk("resets Oct 24, 2026, 12:34 UTC"));
    pipe.scenario
        .fake()
        .next_usage(TurnPlan::new().chunk("second quota reply"));
    for (id, command, text) in [
        (1, "/usage", "resets Oct 24, 2026, 12:34 UTC"),
        (2, "normal request", "normal reply"),
        (3, " /usage", "second quota reply"),
        (4, "/usage", "Quota information."),
    ] {
        pipe.send(json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":{"sessionId":"quota-contract","prompt":[{"type":"text","text":command}]}})).await;
        assert_eq!(
            pipe.stdout().await["params"]["update"]["content"]["text"],
            text
        );
        assert_eq!(pipe.stdout().await["id"], id);
    }
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn usage_plan_can_hold_the_reply_until_its_gate_opens() {
    let mut pipe = FakePipe::new();
    let mut plan = TurnPlan::new();
    plan.steps
        .push(TurnStep::HoldUntil("quota-contract".into()));
    plan.steps.push(TurnStep::Chunk("held quota reply".into()));
    pipe.scenario.fake().next_usage(plan);
    pipe.send(json!({"jsonrpc":"2.0","id":1,"method":"session/prompt","params":{"sessionId":"quota-contract","prompt":[{"type":"text","text":"/usage"}]}})).await;
    pipe.scenario.time.settle().await;
    assert!(
        pipe.scenario
            .hub
            .entries()
            .iter()
            .any(|entry| entry.event["kind"] == "gate" && entry.event["name"] == "quota-contract")
    );
    pipe.scenario.fake().open_gate("quota-contract");
    assert_eq!(
        pipe.stdout().await["params"]["update"]["content"]["text"],
        "held quota reply"
    );
    assert_eq!(pipe.stdout().await["id"], 1);
    pipe.finish().await;
}
