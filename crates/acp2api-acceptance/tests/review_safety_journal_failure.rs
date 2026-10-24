#![cfg(unix)]

use acp2api_acceptance::real_limit;
use acp2api_acceptance::{
    Scenario, TurnPlan, client::ApiCall, runtime_safety_check as safety, tool_check,
};
use std::time::{Duration, Instant};

fn failure_lines(s: &Scenario) -> usize {
    s.hub
        .entries()
        .iter()
        .filter(|e| {
            e.event["text"]
                .as_str()
                .is_some_and(|t| t.contains("cannot write runtime safety event"))
        })
        .count()
}

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn a_failed_safety_log_write_keeps_serving() {
    let mut s = Scenario::default();
    s.plan(safety::isolated());
    s.fake()
        .next_prompt(safety::denial_plan("view_file", "Denied"));
    s.start_isolated_owner().await;
    std::fs::remove_dir_all(s.root.path.join("data/logs")).unwrap();
    let call = || ApiCall::one_user_message("POST /v1/messages", "Hi");
    let answer = tool_check::send(&mut s, call()).await;
    safety::reply(&answer, "Denied");
    s.fake()
        .next_prompt(TurnPlan::new().chunk("ok").end("end_turn"));
    let answer = tool_check::send(&mut s, call()).await;
    safety::reply(&answer, "ok");
    let end = Instant::now() + real_limit(Duration::from_secs(5));
    while failure_lines(&s) < 1 && Instant::now() < end {
        let hub = s.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(end))
            .await
            .unwrap();
    }
    assert_eq!(failure_lines(&s), 1, "one stderr line for the dropped line");
    s.finish(true).await;
}
