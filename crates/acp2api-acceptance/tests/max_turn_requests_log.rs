use acp2api_acceptance::{Scenario, Setup, TurnPlan, endpoint_check, journal_check, real_limit};
use std::time::Duration;

#[tokio::test]
async fn max_turn_requests_has_its_own_request_log_code() {
    // One HTTP request; the harness bounds ACP frames and captures.
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        s.fake()
            .next_prompt(TurnPlan::new().end("max_turn_requests"));
        let answer = s
            .client()
            .await
            .send(endpoint_check::request(
                "POST /v1/messages",
                "Reach the model request limit",
                false,
            ))
            .finish(&mut s.time)
            .await;
        assert_eq!(answer.status, 502, "M-21 reaches the API client");
        let records = journal_check::request_records(&s);
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0]["error_code"], "max_turn_requests",
            "design M-21 log code"
        );
        s.finish(true).await;
    })
    .await
    .expect("M-21 log regression cap: 30 s");
}
