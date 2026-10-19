use acp2api_acceptance::{Scenario, Setup, TurnPlan, TurnStep, endpoint_check as wire};

#[tokio::test]
async fn messages_last_chunk_waits_exactly_200_ms() {
    holds_one("POST /v1/messages").await;
}
#[tokio::test]
async fn responses_last_chunk_waits_exactly_200_ms() {
    holds_one("POST /v1/responses").await;
}
#[tokio::test]
async fn chat_last_chunk_waits_exactly_200_ms() {
    holds_one("POST /v1/chat/completions").await;
}
async fn holds_one(endpoint: &str) {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut plan = TurnPlan::new().chunk("A雪🙂BCDE").usage(987, 1_048_576);
    plan.steps.push(TurnStep::Thought("private thought".into()));
    plan.steps.push(TurnStep::HoldUntil("finish".into()));
    s.fake().next_prompt(
        plan.after(std::time::Duration::from_secs(10))
            .end("end_turn"),
    );
    let call = s
        .client()
        .await
        .send(wire::request(endpoint, "fresh user 雪🙂", true));
    s.time.settle().await;
    assert_eq!(
        wire::text(&call.events()),
        "",
        "held before prompt end: {endpoint}"
    );
    s.fake().open_gate("finish");
    s.time.advance_to(10_000).await;
    assert_eq!(
        wire::text(&call.events()),
        "",
        "held at prompt end: {endpoint}"
    );
    s.time.advance_to(10_199).await;
    assert_eq!(
        wire::text(&call.events()),
        "",
        "held through 199 ms: {endpoint}"
    );
    s.time.advance_to(10_200).await;
    assert_eq!(
        wire::text(&call.events()),
        "A雪🙂BCDE",
        "release at 200 ms: {endpoint}"
    );
    let answer = call.finish(&mut s.time).await;
    wire::verify(&answer, endpoint, "A雪🙂BCDE", 987, 4);
    assert_eq!(s.fake().prompts(), ["fresh user 雪🙂"]);
    s.finish(true).await;
}

#[tokio::test]
async fn bodies_preserve_unicode_and_context_used_for_each_dialect() {
    for endpoint in [
        "POST /v1/messages",
        "POST /v1/responses",
        "POST /v1/chat/completions",
    ] {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        s.fake().next_prompt(
            TurnPlan::new()
                .chunk("A雪🙂BCDE")
                .usage(987, 1_048_576)
                .end("end_turn"),
        );
        let call = s
            .client()
            .await
            .send(wire::request(endpoint, "fresh user 雪🙂", false));
        let answer = call.finish(&mut s.time).await;
        wire::verify(&answer, endpoint, "A雪🙂BCDE", 987, 4);
        assert_eq!(s.fake().prompts(), ["fresh user 雪🙂"]);
        s.finish(true).await;
    }
}
