use acp2api_acceptance::{
    Scenario, TurnPlan, TurnStep, endpoint_check, runtime_safety_check as safety, tool_check,
};
use serde_json::{Value, json};

async fn m22(endpoint: &str, stream: bool) {
    let mut s = Scenario::default();
    s.plan(safety::isolated());
    let name = "wire_builtin";
    let mut plan = TurnPlan::new().chunk("Visible ").chunk("held");
    plan.steps.extend(safety::execution_plan(name).steps);
    s.fake().next_prompt(plan);
    s.start_isolated_owner().await;
    let call = endpoint_check::request(endpoint, "error envelope probe", stream);
    let answer = tool_check::send(&mut s, call).await;
    let message = "acp2api: the built-in tool wire_builtin of the official runtime ran with no permission request, so acp2api cancelled the prompt turn.";
    if !stream {
        assert_eq!(answer.status, 502, "M-22 full response status");
        assert_eq!(
            answer.headers.get("x-should-retry").map(String::as_str),
            Some("false")
        );
        if endpoint == "POST /v1/messages" {
            safety::error(&answer, name);
        } else {
            assert_eq!(
                answer.json(),
                json!({"error":{"message":message,"type":"server_error","param":null,"code":"built_in_tool_ran"}})
            );
        }
    } else {
        assert_eq!(answer.status, 200, "accepted stream keeps HTTP 200");
        assert_eq!(
            endpoint_check::text(&answer.events),
            "Visible ",
            "held and unsafe chunks do not escape"
        );
        let last = answer.events.last().expect("stream error event");
        let error: Value = serde_json::from_str(&last.data).unwrap();
        match endpoint {
            "POST /v1/messages" => {
                assert_eq!(last.event, "error");
                assert_eq!(
                    error,
                    json!({"type":"error","error":{"type":"invalid_request_error","message":message}})
                );
                assert_eq!(
                    answer.events[answer.events.len() - 2].event,
                    "content_block_stop"
                );
                assert!(answer.events.iter().all(|e| e.event != "message_stop"));
            }
            "POST /v1/responses" => {
                assert_eq!(last.event, "response.failed");
                assert_eq!(error["type"], "response.failed");
                assert_eq!(error["response"]["status"], "failed");
                assert_eq!(
                    error["response"]["error"],
                    json!({"code":"invalid_prompt","message":message})
                );
                assert!(
                    answer
                        .events
                        .iter()
                        .all(|e| e.event != "response.completed")
                );
                for (index, event) in answer.events.iter().enumerate() {
                    assert_eq!(
                        serde_json::from_str::<Value>(&event.data).unwrap()["sequence_number"],
                        index
                    );
                }
            }
            _ => {
                assert_eq!(last.event, "message");
                assert_eq!(
                    error,
                    json!({"error":{"message":message,"type":"server_error","param":null,"code":"built_in_tool_ran"}})
                );
                assert!(answer.events.iter().all(|e| e.data != "[DONE]"));
            }
        }
    }
    safety::unpermitted_cancelled(&s);
    safety::event_log(&s, "built_in_tool_ran", name);
    s.finish(true).await;
}
#[tokio::test]
async fn m22_messages_body() {
    m22("POST /v1/messages", false).await;
}
#[tokio::test]
async fn m22_chat_body() {
    m22("POST /v1/chat/completions", false).await;
}
#[tokio::test]
async fn m22_responses_body() {
    m22("POST /v1/responses", false).await;
}
#[tokio::test]
async fn m22_messages_stream() {
    m22("POST /v1/messages", true).await;
}
#[tokio::test]
async fn m22_chat_stream() {
    m22("POST /v1/chat/completions", true).await;
}
#[tokio::test]
async fn m22_responses_stream() {
    m22("POST /v1/responses", true).await;
}

#[tokio::test]
async fn permission_identity_controls_shared_client_and_builtin_names() {
    let name = "read_file";
    let tools = [tool_check::tool(name)];
    let mut client = Scenario::default();
    client.plan(safety::isolated());
    client.fake().next_prompt(tool_check::plan(
        &[name.into()],
        &json!({"path":"shared.txt"}),
    ));
    client.start_isolated_owner().await;
    let answer = tool_check::send(
        &mut client,
        tool_check::request("POST /v1/messages", &tools, false, "auto"),
    )
    .await;
    tool_check::body(
        &answer,
        "POST /v1/messages",
        name,
        &json!({"path":"shared.txt"}),
    );
    tool_check::cancelled(&client, 1);
    assert!(
        safety::log_records(&client)
            .iter()
            .all(|r| r.get("event").is_none() || r["event"] == "state_change")
    );
    client.finish(true).await;

    let mut denied = Scenario::default();
    denied.plan(safety::isolated());
    denied
        .fake()
        .next_prompt(safety::denial_plan(name, "Denied shared name"));
    denied.start_isolated_owner().await;
    let answer = tool_check::send(
        &mut denied,
        tool_check::request("POST /v1/messages", &tools, false, "auto"),
    )
    .await;
    safety::denied(&denied);
    safety::reply(&answer, "Denied shared name");
    safety::event_log(&denied, "permission_refused", name);
    denied.finish(true).await;

    let mut builtin = Scenario::default();
    builtin.plan(safety::isolated());
    builtin.fake().next_prompt(safety::execution_plan(name));
    builtin.start_isolated_owner().await;
    let answer = tool_check::send(
        &mut builtin,
        tool_check::request("POST /v1/messages", &tools, false, "auto"),
    )
    .await;
    safety::error(&answer, name);
    safety::unpermitted_cancelled(&builtin);
    tool_check::offers(&builtin, &tools);
    safety::event_log(&builtin, "built_in_tool_ran", name);
    builtin.finish(true).await;
}

#[tokio::test]
async fn builtin_error_cancels_only_its_active_session_while_another_prompt_completes() {
    let mut s = Scenario::default();
    s.plan(safety::isolated());
    let mut healthy = TurnPlan::new();
    healthy.steps.push(TurnStep::HoldUntil("healthy".into()));
    s.fake().next_prompt(healthy.chunk("Parallel normal reply"));
    s.start_isolated_owner().await;
    let client = s.client().await.clone();
    let normal = client.send(acp2api_acceptance::client::ApiCall::one_user_message(
        "POST /v1/messages",
        "healthy parallel conversation",
    ));
    safety::wait_prompts(&mut s, 1).await;
    s.fake()
        .next_prompt(safety::execution_plan("parallel_builtin"));
    let bad = client.send(acp2api_acceptance::client::ApiCall::one_user_message(
        "POST /v1/messages",
        "offending parallel conversation",
    ));
    let error = bad.finish(&mut s.time).await;
    safety::error(&error, "parallel_builtin");
    let prompts = s.fake().record().prompts();
    assert_eq!(prompts.len(), 2);
    assert_ne!(prompts[0].session, prompts[1].session);
    let cancels = s.fake().record().cancels();
    assert_eq!(cancels.len(), 1);
    assert_eq!(
        cancels[0]["params"]["sessionId"], prompts[1].session,
        "only offending active session is cancelled"
    );
    safety::event_log(&s, "built_in_tool_ran", "parallel_builtin");
    s.fake().open_gate("healthy");
    safety::reply(&normal.finish(&mut s.time).await, "Parallel normal reply");
    s.fake()
        .next_prompt(TurnPlan::new().chunk("Subsequent normal reply"));
    let next = safety::send(&mut s, "subsequent distinct conversation").await;
    safety::reply(&next, "Subsequent normal reply");
    assert_eq!(s.fake().record().prompts().len(), 3);
    let records = safety::log_records(&s);
    assert_eq!(
        records.iter().filter(|r| r["result"] == "success").count(),
        2
    );
    assert_eq!(
        records.iter().filter(|r| r["result"] == "failed").count(),
        1
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| r["event"] == "built_in_tool_ran")
            .count(),
        1
    );
    s.finish(true).await;
}
