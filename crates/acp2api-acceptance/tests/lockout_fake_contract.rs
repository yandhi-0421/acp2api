#[path = "support/fake_pipe.rs"]
#[allow(
    dead_code,
    reason = "This target uses the prompt and stderr operations of the shared fake pipe"
)]
mod fake_pipe;
use acp2api_acceptance::{TurnPlan, TurnStep};
use fake_pipe::FakePipe;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_turn_can_end_with_a_session_error_without_releasing_its_old_plan() {
    let mut pipe = FakePipe::new();
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::HoldUntil("held-error".into()));
    plan.steps.push(TurnStep::Chunk("obsolete reply".into()));
    pipe.scenario.fake().next_prompt(plan);
    pipe.prompt("session-A", 11).await;
    pipe.scenario.time.settle().await;
    let mut failure = TurnPlan::new();
    failure.steps.extend([
        TurnStep::SessionStderr("failed session {session}".into()),
        TurnStep::ErrorLine {
            code: 403,
            status: "PERMISSION_DENIED".into(),
            message: "Synthetic suspension".into(),
        },
    ]);
    pipe.scenario.fake().finish_gate("held-error", failure);
    assert_eq!(
        pipe.stdout().await,
        serde_json::json!({"jsonrpc":"2.0","id":11,"result":{"stopReason":"end_turn"}})
    );
    assert_eq!(pipe.stderr().await, "failed session session-A");
    assert_eq!(
        pipe.stderr().await,
        "Error 403, Message: Synthetic suspension, Status: PERMISSION_DENIED, Details: []"
    );
    pipe.finish().await;
}
