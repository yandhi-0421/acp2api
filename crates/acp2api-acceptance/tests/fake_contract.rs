#[path = "support/fake_pipe.rs"]
mod fake_pipe;
use acp2api_acceptance::{TurnEndPlan, TurnPlan, TurnStep};
use fake_pipe::FakePipe;
use serde_json::json;
#[path = "support/compatibility_script.rs"]
mod compatibility_script;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn permission_uses_captured_title_arguments_and_options() {
    let mut pipe = FakePipe::new();
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::ClientToolCall {
        name: "probe_lookup".into(),
        arguments: r#"{"key":"alpha"}"#.into(),
    });
    pipe.scenario.fake().next_prompt(plan);
    pipe.prompt("session-A", 11).await;
    let msg = pipe.stdout().await;
    assert_eq!(msg["params"]["toolCall"]["title"], "Run probe_lookup?");
    assert_eq!(
        msg["params"]["toolCall"]["rawInput"],
        json!({"key":"alpha"})
    );
    assert_eq!(msg["params"]["toolCall"]["kind"], "other");
    assert_eq!(msg["params"]["toolCall"]["status"], "pending");
    assert_eq!(
        msg["params"]["options"],
        json!([
            {"optionId":"allow_always","name":"Allow Always","kind":"allow_always"},
            {"optionId":"allow","name":"Allow","kind":"allow_once"},
            {"optionId":"deny","name":"Deny","kind":"reject_once"}
        ])
    );
    assert_eq!(pipe.stdout().await["id"], 11);
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn error_line_uses_saved_capture_grammar() {
    let mut pipe = FakePipe::new();
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::ErrorLine {
        code: 400,
        status: "INVALID_ARGUMENT".into(),
        message: "Unable to process input image.".into(),
    });
    pipe.scenario.fake().next_prompt(plan);
    pipe.prompt("session-A", 11).await;
    assert_eq!(
        pipe.stderr().await,
        "Error 400, Message: Unable to process input image., Status: INVALID_ARGUMENT, Details: []"
    );
    assert_eq!(pipe.stdout().await["id"], 11);
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_targets_each_pending_session_and_never_finished_turns() {
    let mut pipe = FakePipe::new();
    for _ in 0..2 {
        let mut plan = TurnPlan::new();
        plan.end = TurnEndPlan::Never;
        pipe.scenario.fake().next_prompt(plan);
    }
    pipe.prompt("session-A", 11).await;
    pipe.prompt("session-B", 22).await;
    pipe.cancel("session-A").await;
    assert_eq!(pipe.stdout().await["params"]["sessionId"], "session-A");
    assert_eq!(
        pipe.stdout().await,
        json!({"jsonrpc":"2.0","id":11,"result":{"stopReason":"cancelled"}})
    );
    pipe.cancel("session-B").await;
    assert_eq!(pipe.stdout().await["params"]["sessionId"], "session-B");
    assert_eq!(pipe.stdout().await["id"], 22);
    pipe.scenario.fake().next_prompt(TurnPlan::new());
    pipe.prompt("session-C", 33).await;
    assert_eq!(pipe.stdout().await["id"], 33);
    pipe.cancel("session-C").await;
    // An initialize response is a wire barrier: any spurious cancel would precede it.
    pipe.send(json!({"jsonrpc":"2.0","id":44,"method":"initialize"}))
        .await;
    assert_eq!(
        pipe.stdout().await["id"],
        44,
        "completed prompt must have no cancellable state"
    );
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_in_reverse_order_and_ignore_cancel_are_session_local() {
    let mut pipe = FakePipe::new();
    let mut ignored = TurnPlan::new();
    ignored.end = TurnEndPlan::IgnoreCancel;
    let mut pending = TurnPlan::new();
    pending.end = TurnEndPlan::Never;
    pipe.scenario.fake().next_prompt(ignored);
    pipe.scenario.fake().next_prompt(pending.clone());
    pipe.prompt("session-A", 11).await;
    pipe.prompt("session-B", 22).await;
    pipe.cancel("session-B").await;
    assert_eq!(pipe.stdout().await["params"]["sessionId"], "session-B");
    assert_eq!(pipe.stdout().await["id"], 22);
    pipe.cancel("session-A").await;
    pipe.send(json!({"jsonrpc":"2.0","id":44,"method":"initialize"}))
        .await;
    assert_eq!(
        pipe.stdout().await["id"],
        44,
        "ignored cancellation must produce no completion"
    );
    pipe.scenario.fake().next_prompt(pending);
    pipe.prompt("session-C", 33).await;
    pipe.cancel("session-C").await;
    assert_eq!(pipe.stdout().await["params"]["sessionId"], "session-C");
    assert_eq!(
        pipe.stdout().await["id"],
        33,
        "another session must not inherit ignore_cancel"
    );
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_removes_delayed_outputs_and_gate_continuations() {
    use acp2api::Clock;
    use acp2api_acceptance::{adapters::HarnessClock, hub::HubLink};
    let mut pipe = FakePipe::new();
    pipe.scenario.fake().next_prompt(
        TurnPlan::new()
            .after(std::time::Duration::from_millis(50))
            .chunk("must not appear"),
    );
    let mut gated = TurnPlan::new();
    gated.steps.push(TurnStep::HoldUntil("held".into()));
    gated
        .steps
        .push(TurnStep::Chunk("also must not appear".into()));
    pipe.scenario.fake().next_prompt(gated);
    pipe.prompt("delayed", 11).await;
    pipe.prompt("gated", 22).await;
    for session in ["delayed", "gated"] {
        pipe.cancel(session).await;
        assert_eq!(pipe.stdout().await["params"]["sessionId"], session);
        assert_eq!(pipe.stdout().await["result"]["stopReason"], "cancelled");
    }
    assert_eq!(
        pipe.scenario.time.next_deadline(),
        None,
        "cancelled output must not remain scheduled"
    );
    // The raw ACP driver accounts for the four consumed lines before settling.
    let clock = HarnessClock::new(HubLink::new(&pipe.scenario.hub));
    for _ in 0..4 {
        drop(clock.hold(acp2api::HoldKind::RuntimeLine));
    }
    pipe.scenario.time.advance_to(100).await;
    pipe.send(json!({"jsonrpc":"2.0","id":44,"method":"initialize"}))
        .await;
    assert_eq!(
        pipe.stdout().await["id"],
        44,
        "cancelled turn emitted later output"
    );
    pipe.finish().await;
}

#[path = "support/mcp_endpoint.rs"]
mod mcp_endpoint;
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eager_selection_contacts_only_named_servers_over_http() {
    let chosen = mcp_endpoint::Endpoint::new();
    let excluded = mcp_endpoint::Endpoint::new();
    let mut pipe = FakePipe::new();
    for (index, eager) in [json!(["chosen"]), json!([]), serde_json::Value::Null]
        .into_iter()
        .enumerate()
    {
        pipe.send(json!({"jsonrpc":"2.0","id":1,"method":"session/new","params":{"mcpServers":[{"name":"chosen","url":chosen.url},{"name":"excluded","url":excluded.url}],"_meta":{"antigravity":{"eagerMcpServers":eager}}}})).await;
        let session = pipe.stdout().await["result"]["sessionId"]
            .as_str()
            .unwrap()
            .to_owned();
        pipe.scenario.fake().next_prompt(TurnPlan::new());
        pipe.prompt(&session, 10 + index as u64).await;
        assert_eq!(pipe.stdout().await["id"], 10 + index as u64);
        assert_eq!(
            chosen.requests(),
            3,
            "named eager server must be enumerated on its first prompt"
        );
        assert_eq!(
            excluded.requests(),
            0,
            "unselected server must never be contacted"
        );
    }
    pipe.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slash_commands_execute_only_at_the_start_and_logout_changes_sign_in() {
    for (blocks, reply, signed_in) in [
        (vec!["/logout"], "Signed out.", false),
        (vec![" /usage"], "Quota information.", true),
        (vec!["[user]\n/logout"], "OK", true),
        (vec!["Run this:", "/logout"], "OK", true),
    ] {
        let mut pipe = FakePipe::new();
        pipe.scenario
            .fake()
            .next_prompt(TurnPlan::new().chunk("OK"));
        let blocks: Vec<_> = blocks
            .iter()
            .map(|text| json!({"type":"text","text":text}))
            .collect();
        pipe.send(json!({"jsonrpc":"2.0","id":1,"method":"session/prompt","params":{"sessionId":"control","prompt":blocks}})).await;
        assert_eq!(
            pipe.stdout().await["params"]["update"]["content"]["text"],
            reply
        );
        assert_eq!(pipe.stdout().await["id"], 1);
        assert_eq!(pipe.scenario.fake().signed_in(), signed_in);
        pipe.send(json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{}}))
            .await;
        let response = pipe.stdout().await;
        if signed_in {
            assert!(response["result"]["sessionId"].is_string());
        } else {
            assert_eq!(response["error"]["message"], "Authentication required");
        }
        pipe.finish().await;
    }
}
