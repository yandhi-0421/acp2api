use acp2api_acceptance::{Scenario, Setup, TurnPlan, TurnStep, endpoint_check as wire};
use serde_json::{Value, json};
use std::time::Duration;

const ENDPOINT: &str = "POST /v1/chat/completions";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_waits_for_confirmed_text_before_its_first_data_chunk() {
    // One HTTP request; existing hub and HTTP readers bound each capture.
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        s.fake().next_prompt(TurnPlan {
            steps: vec![
                TurnStep::HoldUntil("first-text".into()),
                TurnStep::Chunk("First 雪".into()),
                TurnStep::HoldUntil("second-text".into()),
                TurnStep::Chunk(" last🙂".into()),
                TurnStep::Usage {
                    used: 987,
                    size: 1_048_576,
                },
                TurnStep::HoldUntil("prompt-end".into()),
            ],
            ..TurnPlan::new()
        });
        let call = s
            .client()
            .await
            .send(wire::request(ENDPOINT, "start timing", true));
        s.time.settle().await;
        assert_eq!(
            call.status(),
            Some(200),
            "HTTP acceptance precedes the first ACP text"
        );
        assert!(
            call.events().is_empty(),
            "no Chat data chunk before the first ACP text"
        );
        s.fake().open_gate("first-text");
        s.time.settle().await;
        assert!(
            call.events().is_empty(),
            "the newest ACP text stays held while the prompt is open"
        );
        s.fake().open_gate("second-text");
        s.time.settle().await;
        let first = call.events();
        assert_eq!(
            first.len(),
            1,
            "only confirmed text creates the first Chat chunk"
        );
        let chunk: Value = serde_json::from_str(&first[0].data).unwrap();
        assert_eq!(
            chunk["choices"][0]["delta"],
            json!({"role":"assistant","content":"First 雪"})
        );
        assert!(chunk["choices"][0]["finish_reason"].is_null());
        assert_eq!(
            wire::text(&first),
            "First 雪",
            "second text releases the first before prompt completion"
        );
        s.fake().open_gate("prompt-end");
        s.time.settle().await;
        let ended_at = s.hub.now();
        s.time.advance_to(ended_at + 199).await;
        assert_eq!(
            call.events(),
            first,
            "last text and normal completion stay held through 199 ms"
        );
        s.time.advance_to(ended_at + 200).await;
        let answer = call.finish(&mut s.time).await;
        wire::verify(&answer, ENDPOINT, "First 雪 last🙂", 987, 5);
        assert_eq!(
            answer.events.len(),
            4,
            "two text chunks, one normal final chunk, one DONE marker"
        );
        let last: Value = serde_json::from_str(&answer.events[1].data).unwrap();
        assert_eq!(last["choices"][0]["delta"], json!({"content":" last🙂"}));
        assert_eq!(answer.events[1].at, ended_at + 200);
        s.finish(true).await;
    })
    .await
    .expect("Chat stream regression cap: 30 s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_chat_reply_still_finishes_with_an_assistant_role() {
    // One HTTP request; no ACP text precedes normal prompt completion.
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let mut plan = TurnPlan::new().usage(987, 1_048_576);
        plan.steps.push(TurnStep::HoldUntil("empty-end".into()));
        s.fake().next_prompt(plan);
        let call = s
            .client()
            .await
            .send(wire::request(ENDPOINT, "empty reply", true));
        s.time.settle().await;
        assert_eq!(call.status(), Some(200));
        assert!(
            call.events().is_empty(),
            "an empty reply has no data chunk before completion"
        );
        s.fake().open_gate("empty-end");
        s.time.settle().await;
        let ended_at = s.hub.now();
        s.time.advance_to(ended_at + 199).await;
        assert!(
            call.events().is_empty(),
            "empty completion stays held through 199 ms"
        );
        s.time.advance_to(ended_at + 200).await;
        let answer = call.finish(&mut s.time).await;
        wire::verify(&answer, ENDPOINT, "", 987, 0);
        assert_eq!(
            answer.events.len(),
            3,
            "assistant role, one normal final chunk, one DONE marker"
        );
        let first: Value = serde_json::from_str(&answer.events[0].data).unwrap();
        assert_eq!(
            first["choices"][0]["delta"],
            json!({"role":"assistant","content":""})
        );
        assert!(answer.events.iter().all(|event| event.at == ended_at + 200));
        s.finish(true).await;
    })
    .await
    .expect("empty Chat regression cap: 30 s");
}
