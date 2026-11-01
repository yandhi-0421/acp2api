use acp2api_acceptance::{
    Scenario, Setup, TurnEndPlan, TurnPlan, TurnStep, content_check, conversation_check as c,
    journal_check,
};
use serde_json::{Value, json};
fn msgs(user: &str, reply: &str, next: &str) -> Value {
    json!([
        c::message("user", None, user),
        c::message("assistant", None, reply),
        c::message("user", None, next)
    ])
}
fn scenario() -> Scenario {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s
}
#[tokio::test]
async fn independent_request_never_inherits_hidden_history() {
    let mut s = scenario();
    let tools = c::tools("lookup", "Lookup", json!({"type":"object"}));
    c::send(
        &mut s,
        c::call(
            json!([c::message("user", None, "Unrelated")]),
            Some("Same rule"),
            tools.clone(),
        ),
        "Old reply",
    )
    .await;
    let old = c::session(&s);
    c::send(
        &mut s,
        c::call(
            json!([c::message("user", None, "Fresh")]),
            Some("Same rule"),
            tools,
        ),
        "Fresh reply",
    )
    .await;
    assert_ne!(
        c::session(&s),
        old,
        "fresh request has no completed-record prefix"
    );
    assert_eq!(c::kind(&s), "new");
    assert_eq!(c::blocks(&s), json!([{"type":"text","text":"Fresh"}]));
    s.finish(true).await;
}
async fn all_dialects_continue_only_new_content_and_log_before_completion(endpoint: &str) {
    let mut s = scenario();
    let mut first = c::call(
        json!([c::message("user", None, "First")]),
        None,
        Value::Null,
    );
    first.path = endpoint.split_once(' ').unwrap().1.into();
    if endpoint.ends_with("responses") {
        first.body["input"] = first.body["messages"].take();
    }
    c::send(&mut s, first, "Reply").await;
    let id = c::session(&s);
    let mut next = c::call(msgs("First", "Reply", "New  text\n雪"), None, Value::Null);
    next.path = endpoint.split_once(' ').unwrap().1.into();
    if endpoint.ends_with("responses") {
        next.body["input"] = next.body["messages"].take();
    }
    c::send(&mut s, next, "Second").await;
    c::continuation(&s, &id, "New  text\n雪");
    assert!(
        s.hub.now() >= 400,
        "successful requests advance two settle deadlines"
    );
    s.finish(true).await;
}
#[tokio::test]
async fn all_dialects_continue_only_new_content_and_log_before_completion_1() {
    all_dialects_continue_only_new_content_and_log_before_completion("POST /v1/messages").await;
}
#[tokio::test]
async fn all_dialects_continue_only_new_content_and_log_before_completion_2() {
    all_dialects_continue_only_new_content_and_log_before_completion("POST /v1/chat/completions")
        .await;
}
#[tokio::test]
async fn all_dialects_continue_only_new_content_and_log_before_completion_3() {
    all_dialects_continue_only_new_content_and_log_before_completion("POST /v1/responses").await;
}

#[tokio::test]
async fn tool_order_keeps_client_new_text_exact() {
    let mut s = scenario();
    let a = c::tools(
        "a",
        "A  tool",
        json!({"type":"object","properties":{"x":{"type":"string"}}}),
    );
    let b = c::tools("b", "B", json!({"type":"object"}));
    let tools = json!([a[0], b[0]]);
    c::send(
        &mut s,
        c::call(
            json!([c::message("user", None, "First \n text")]),
            Some("Be  kind"),
            tools,
        ),
        "Old \n reply",
    )
    .await;
    let id = c::session(&s);
    c::send(
        &mut s,
        c::call(
            msgs("First \n text", "Old \n reply", "New  \n text"),
            Some("Be  kind"),
            json!([b[0], a[0]]),
        ),
        "Second",
    )
    .await;
    c::continuation(&s, &id, "New  \n text");
    s.finish(true).await;
}
async fn changed_system_or_each_tool_metadata_defeats_prefix(change: &str) {
    let mut s = scenario();
    let tool = c::tools("a", "A", json!({"type":"object"}));
    c::send(
        &mut s,
        c::call(
            json!([c::message("user", None, "First")]),
            Some("Rule"),
            tool.clone(),
        ),
        "Reply",
    )
    .await;
    let old = c::session(&s);
    let mut changed = tool;
    let system = if change == "system" {
        "Other rule"
    } else {
        "Rule"
    };
    match change {
        "name" => changed[0]["function"]["name"] = "b".into(),
        "description" => changed[0]["function"]["description"] = "B".into(),
        "schema" => {
            changed[0]["function"]["parameters"] = json!({"type":"object","required":["x"]})
        }
        _ => {}
    }
    c::send(
        &mut s,
        c::call(msgs("First", "Reply", "Next"), Some(system), changed),
        "Next reply",
    )
    .await;
    assert_ne!(c::session(&s), old, "metadata change {change} must rebuild");
    assert_eq!(c::kind(&s), "rebuild");
    assert_eq!(
        c::text(&s),
        format!(
            "{}\n\n[user]\nFirst\n[assistant]\nReply\n[user]\nNext",
            c::NEUTRAL
        )
    );
    s.finish(true).await;
}
#[tokio::test]
async fn changed_system_or_each_tool_metadata_defeats_prefix_1() {
    changed_system_or_each_tool_metadata_defeats_prefix("system").await;
}
#[tokio::test]
async fn changed_system_or_each_tool_metadata_defeats_prefix_2() {
    changed_system_or_each_tool_metadata_defeats_prefix("name").await;
}
#[tokio::test]
async fn changed_system_or_each_tool_metadata_defeats_prefix_3() {
    changed_system_or_each_tool_metadata_defeats_prefix("description").await;
}
#[tokio::test]
async fn changed_system_or_each_tool_metadata_defeats_prefix_4() {
    changed_system_or_each_tool_metadata_defeats_prefix("schema").await;
}

async fn role_name_part_boundary_and_image_identity_are_not_text_only(change: &str) {
    let mut s = scenario();
    let parts = if change == "image" {
        json!([
            content_check::text("POST /v1/chat/completions", "First"),
            content_check::image("POST /v1/chat/completions", content_check::PNG)
        ])
    } else {
        json!([{"type":"text","text":"First"},{"type":"text","text":"Second"}])
    };
    let first = json!({"role":"user","name":"Alice","content":parts});
    let prefix = if change == "role" {
        vec![c::message("user", None, "Anchor")]
    } else {
        Vec::new()
    };
    let mut original = prefix.clone();
    original.push(first.clone());
    c::send(&mut s, c::call(original.into(), None, Value::Null), "Reply").await;
    let old = c::session(&s);
    let mut edited = first;
    match change {
        "role" => edited["role"] = "developer".into(),
        "name" => edited["name"] = "Bob".into(),
        "boundary" => edited["content"] = json!([{"type":"text","text":"FirstSecond"}]),
        "image" => {
            edited["content"][1]["image_url"]["url"] = format!(
                "data:image/png;base64,{}",
                content_check::PNG.replace("CYII=", "CYIA=")
            )
            .into()
        }
        _ => unreachable!(),
    }
    let mut items = prefix;
    items.extend([
        edited,
        c::message("assistant", None, "Reply"),
        c::message("user", None, "Next"),
    ]);
    c::send(
        &mut s,
        c::call(items.into(), None, Value::Null),
        "Next reply",
    )
    .await;
    assert_ne!(c::session(&s), old, "item identity change {change}");
    assert_eq!(c::kind(&s), "rebuild");
    s.finish(true).await;
}
#[tokio::test]
async fn role_name_part_boundary_and_image_identity_are_not_text_only_1() {
    role_name_part_boundary_and_image_identity_are_not_text_only("role").await;
}
#[tokio::test]
async fn role_name_part_boundary_and_image_identity_are_not_text_only_2() {
    role_name_part_boundary_and_image_identity_are_not_text_only("name").await;
}
#[tokio::test]
async fn role_name_part_boundary_and_image_identity_are_not_text_only_3() {
    role_name_part_boundary_and_image_identity_are_not_text_only("boundary").await;
}
#[tokio::test]
async fn role_name_part_boundary_and_image_identity_are_not_text_only_4() {
    role_name_part_boundary_and_image_identity_are_not_text_only("image").await;
}

#[tokio::test]
async fn most_recent_equal_records_and_retention_above_concurrency() {
    let mut s = scenario();
    let first = json!([c::message("user", None, "First")]);
    c::send(&mut s, c::call(first.clone(), None, Value::Null), "Reply").await;
    let first_id = c::session(&s);
    c::send(&mut s, c::call(first.clone(), None, Value::Null), "Reply").await;
    let recent = c::session(&s);
    assert_ne!(
        recent, first_id,
        "independent identical requests create distinct completed records"
    );
    for i in 0..4 {
        c::send(
            &mut s,
            c::call(
                json!([c::message("user", None, &format!("Other {i}"))]),
                None,
                Value::Null,
            ),
            "Other reply",
        )
        .await;
    }
    c::send(
        &mut s,
        c::call(msgs("First", "Reply", "Next"), None, Value::Null),
        "Next reply",
    )
    .await;
    c::continuation(&s, &recent, "Next");
    s.finish(true).await;
}
#[tokio::test]
async fn busy_record_does_not_block_a_free_place() {
    let mut s = scenario();
    s.plan(Setup::normal());
    c::send(
        &mut s,
        c::call(
            json!([c::message("user", None, "First")]),
            None,
            Value::Null,
        ),
        "Reply",
    )
    .await;
    let old = c::session(&s);
    s.fake().next_prompt(TurnPlan {
        steps: vec![
            TurnStep::HoldUntil("busy".into()),
            TurnStep::Chunk("Busy reply".into()),
        ],
        end: TurnEndPlan::Stop("end_turn".into()),
    });
    let pending = s
        .client()
        .await
        .send(c::call(msgs("First", "Reply", "Next"), None, Value::Null));
    s.time.settle().await;
    c::send(
        &mut s,
        c::call(msgs("First", "Reply", "Other"), None, Value::Null),
        "Other reply",
    )
    .await;
    assert_ne!(
        c::session(&s),
        old,
        "busy matching record must rebuild without waiting"
    );
    assert_eq!(c::kind(&s), "rebuild");
    s.fake().open_gate("busy");
    assert_eq!(pending.finish(&mut s.time).await.status, 200);
    s.finish(true).await;
}
#[tokio::test]
async fn client_tool_end_cannot_continue_as_normal_end_turn() {
    let mut s = scenario();
    let tools = c::tools("lookup", "Lookup", json!({"type":"object"}));
    s.fake().next_prompt(TurnPlan {
        steps: vec![TurnStep::ClientToolCall {
            name: "lookup".into(),
            arguments: "{}".into(),
        }],
        end: TurnEndPlan::Never,
    });
    content_check::send(
        &mut s,
        c::call(
            json!([c::message("user", None, "First")]),
            None,
            tools.clone(),
        ),
    )
    .await;
    assert_eq!(s.answer.as_ref().unwrap().status, 200);
    let old = c::session(&s);
    c::send(
        &mut s,
        c::call(msgs("First", "", "Next"), None, tools),
        "Reply",
    )
    .await;
    assert_ne!(
        c::session(&s),
        old,
        "standalone user history cannot complete a pending client tool exchange"
    );
    assert_eq!(c::kind(&s), "rebuild");
    assert_eq!(journal_check::request_records(&s).len(), 2);
    s.finish(true).await;
}

#[tokio::test]
async fn ignored_content_fields_do_not_enter_history_identity() {
    let mut s = scenario();
    let mut first =
        acp2api_acceptance::client::ApiCall::one_user_message("POST /v1/messages", "First");
    first.body["messages"][0]["content"] =
        json!([{"type":"text","text":"First","cache_control":{"type":"ephemeral"}}]);
    c::send(&mut s, first, "Reply").await;
    let id = c::session(&s);
    let mut next = acp2api_acceptance::client::ApiCall::one_user_message("POST /v1/messages", "");
    next.body["messages"] = msgs("First", "Reply", "Next");
    c::send(&mut s, next, "New reply").await;
    c::continuation(&s, &id, "Next");
    s.finish(true).await;
}
