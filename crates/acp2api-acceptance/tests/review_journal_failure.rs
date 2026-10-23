#![cfg(unix)]

use acp2api_acceptance::{Scenario, Setup, TurnPlan, client::ApiCall};
use std::time::{Duration, Instant};

fn failure_lines(s: &Scenario) -> usize {
    s.hub
        .entries()
        .iter()
        .filter(|e| {
            e.event["text"]
                .as_str()
                .is_some_and(|t| t.contains("cannot write request metadata"))
        })
        .count()
}

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn a_failed_log_write_keeps_serving_with_the_original_status() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s.start().await;
    std::fs::remove_dir_all(s.root.path.join("data/logs")).unwrap();
    for _ in 0..2 {
        s.fake()
            .next_prompt(TurnPlan::new().chunk("ok").end("end_turn"));
        let a = s
            .client()
            .await
            .send(ApiCall::one_user_message("POST /v1/chat/completions", "Hi"))
            .finish(&mut s.time)
            .await;
        assert_eq!(a.status, 200, "request after a failed log write");
    }
    let mut wrong = ApiCall::one_user_message("POST /v1/chat/completions", "Hi");
    wrong
        .headers
        .push(("x-api-key".into(), "deliberately-wrong".into()));
    let a = s.client().await.send(wrong).finish(&mut s.time).await;
    assert_eq!(a.status, 401, "refusal after a failed log write");
    let end = Instant::now() + Duration::from_secs(5);
    while failure_lines(&s) < 3 && Instant::now() < end {
        let hub = s.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(end))
            .await
            .unwrap();
    }
    assert_eq!(
        failure_lines(&s),
        3,
        "one stderr line for each dropped line"
    );
    s.finish(true).await;
}
