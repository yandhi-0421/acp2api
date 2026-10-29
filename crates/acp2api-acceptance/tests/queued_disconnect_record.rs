use acp2api_acceptance::{
    Scenario, Setup, TurnPlan, TurnStep, endpoint_check, journal_check, real_limit,
};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_calls_response_header_identifies_its_disconnected_terminal_record() {
    // Three HTTP requests; the hub bounds each capture, and the run has a 30 s cap.
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let mut running = Vec::new();
        for name in ["occupied A", "occupied B"] {
            let mut held = TurnPlan::new();
            held.steps.push(TurnStep::HoldUntil(name.into()));
            s.fake().next_prompt(held);
            running.push(s.client().await.send(endpoint_check::request(
                "POST /v1/messages",
                name,
                true,
            )));
            s.time.settle().await;
        }
        let queued = s.client().await.send(endpoint_check::request(
            "POST /v1/messages",
            "queued request",
            true,
        ));
        s.time.settle().await;
        assert_eq!(queued.status(), Some(200));
        let id = queued.request_id();
        let running_ids: Vec<_> = running.iter().map(|call| call.request_id()).collect();
        drop(queued);
        journal_check::wait_terminal(&s, 1).await;
        let records = journal_check::terminal(&s);
        drop(running);
        s.finish(true).await;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["result"], "disconnected");
        assert!(
            running_ids
                .iter()
                .all(|other| other.is_some() && other != &id),
            "other live calls retain their own response request IDs"
        );
        assert_eq!(
            records[0]["request_id"].as_str(),
            id.as_deref(),
            "the queued call exposes the request ID from its response header"
        );
    })
    .await
    .expect("queued disconnect record cap: 30 s");
}
