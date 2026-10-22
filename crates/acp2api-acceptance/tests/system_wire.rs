use acp2api_acceptance::{Scenario, Setup, content_check as c, system_check as s};
use serde_json::{Value, json};
fn setup() -> Scenario {
    let mut scenario = Scenario::default();
    scenario.plan(Setup::normal());
    scenario
}
#[tokio::test]
async fn messages_system_string_and_text_blocks_keep_their_exact_content() {
    for system in [
        json!("  Une règle 雪\n\navec espace.  "),
        json!([{"type":"text","text":"First 雪\ninside"},{"type":"text","text":"Second 🙂"}]),
    ] {
        let expected = if let Some(text) = system.as_str() {
            text.to_owned()
        } else {
            "First 雪\ninside\n\nSecond 🙂".into()
        };
        let mut scenario = setup();
        s::send(
            &mut scenario,
            s::request("POST /v1/messages", system, Value::Null, " raw user\n雪🙂 "),
        )
        .await;
        s::addition(&scenario, &expected);
        s::prompt(&scenario, " raw user\n雪🙂 ");
        scenario.finish(true).await;
    }
}
#[tokio::test]
async fn named_system_and_developer_parts_keep_their_original_roles() {
    for endpoint in ["POST /v1/chat/completions", "POST /v1/responses"] {
        let mut scenario = setup();
        let messages = json!([
            s::message("system", Some("系统 雪"), "First\n\ninside"),
            s::message("developer", Some("规则 🙂"), "Second"),
            s::message("user", None, "user")
        ]);
        let system = if endpoint.ends_with("responses") {
            json!("Base instructions")
        } else {
            Value::Null
        };
        s::send(
            &mut scenario,
            s::request(endpoint, system, messages, "user"),
        )
        .await;
        let expected = if endpoint.ends_with("responses") {
            "Base instructions\n\n[system: 系统 雪]\nFirst\n\ninside\n\n[developer: 规则 🙂]\nSecond"
        } else {
            "[system: 系统 雪]\nFirst\n\ninside\n\n[developer: 规则 🙂]\nSecond"
        };
        s::addition(&scenario, expected);
        s::prompt(&scenario, "user");
        scenario.finish(true).await;
    }
}
#[tokio::test]
async fn extraction_stops_at_the_first_other_role_and_keeps_later_items_in_order() {
    for endpoint in ["POST /v1/chat/completions", "POST /v1/responses"] {
        for boundary in ["user", "assistant"] {
            let mut scenario = setup();
            let messages = json!([
                s::message("developer", None, "initial"),
                s::message(boundary, None, "boundary"),
                s::message("system", Some("late"), "later system"),
                s::message("developer", None, "later developer"),
                s::message("user", None, "last")
            ]);
            s::send(
                &mut scenario,
                s::request(endpoint, Value::Null, messages, "last"),
            )
            .await;
            s::addition(&scenario, "initial");
            assert_eq!(
                c::blocks(&scenario),
                vec![json!([
                    {"type":"text","text":format!("The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.\n\n[{boundary}]\nboundary")},
                    {"type":"text","text":"\n[system: late]\nlater system"},
                    {"type":"text","text":"\n[developer]\nlater developer"},
                    {"type":"text","text":"\n[user]\nlast"}
                ])],
                "rebuild keeps every role, name, item position, and exact separator"
            );
            scenario.finish(true).await;
        }
    }
}
#[tokio::test]
async fn attribution_stripping_uses_only_the_first_text_block_exact_prefix() {
    let cases = [
        (
            json!([{"type":"text","text":s::ATTRIBUTION},{"type":"text","text":"Keep me"}]),
            "Keep me".to_owned(),
            true,
        ),
        (
            json!([{"type":"text","text":"Keep me"},{"type":"text","text":s::ATTRIBUTION}]),
            format!("Keep me\n\n{}", s::ATTRIBUTION),
            false,
        ),
        (
            json!([{"type":"text","text":format!("Before {}",s::ATTRIBUTION)}]),
            format!("Before {}", s::ATTRIBUTION),
            false,
        ),
        (
            json!([{"type":"text","text":format!(" {}",s::ATTRIBUTION)}]),
            format!(" {}", s::ATTRIBUTION),
            false,
        ),
        (
            json!([{"type":"text","text":"x-anthropic-billing-header"}]),
            "x-anthropic-billing-header".to_owned(),
            false,
        ),
        (
            json!([{"type":"text","text":"Unrelated first block"}]),
            "Unrelated first block".to_owned(),
            false,
        ),
        (json!(s::ATTRIBUTION), s::ATTRIBUTION.to_owned(), false),
    ];
    for (system, expected, removed) in cases {
        let mut scenario = setup();
        s::send(
            &mut scenario,
            s::request(
                "POST /v1/messages",
                system,
                Value::Null,
                "Ask something new 雪",
            ),
        )
        .await;
        s::addition(&scenario, &expected);
        s::prompt(&scenario, "Ask something new 雪");
        if removed {
            s::no_attribution(&scenario, s::ATTRIBUTION);
        }
        scenario.finish(true).await;
    }
}
#[tokio::test]
async fn changed_system_addition_with_same_tools_uses_a_new_session() {
    let mut scenario = setup();
    for text in ["First rule 雪", "Second rule 🙂", "Second rule 🙂"] {
        s::send(
            &mut scenario,
            s::request("POST /v1/messages", json!(text), Value::Null, "Same user"),
        )
        .await;
        s::addition(&scenario, text);
    }
    let mut continuation = s::request(
        "POST /v1/messages",
        json!("Second rule 🙂"),
        Value::Null,
        "New user 雪",
    );
    continuation.body["messages"] = json!([
        s::message("user", None, "Same user"),
        s::message("assistant", None, "OK"),
        s::message("user", None, "New user 雪")
    ]);
    s::send(&mut scenario, continuation).await;
    s::addition(&scenario, "Second rule 🙂");
    assert_eq!(
        c::blocks(&scenario),
        vec![
            json!([{"type":"text","text":"Same user"}]),
            json!([{"type":"text","text":"Same user"}]),
            json!([{"type":"text","text":"Same user"}]),
            json!([{"type":"text","text":"New user 雪"}])
        ],
        "fresh conversations send their own user content; continuation sends only new content"
    );
    let prompts = scenario.fake().record().prompts();
    assert_eq!(prompts.len(), 4);
    assert_ne!(
        prompts[0].session, prompts[1].session,
        "changed system additions never reuse immutable session metadata"
    );
    assert_ne!(
        prompts[1].session, prompts[2].session,
        "a separate one-user request does not continue a completed reply"
    );
    assert_ne!(prompts[0].session, prompts[2].session);
    assert_eq!(
        prompts[2].session, prompts[3].session,
        "the full matching history continues the latest matching ACP session"
    );
    scenario.finish(true).await;
}

#[tokio::test]
async fn removed_attribution_never_enters_any_acp_request_field() {
    let mut scenario = setup();
    s::send(&mut scenario,s::request("POST /v1/messages",json!([{"type":"text","text":s::ATTRIBUTION},{"type":"text","text":"Keep the ordinary rule"}]),Value::Null,"Fresh raw text 雪🙂")).await;
    s::no_attribution(&scenario, s::ATTRIBUTION);
    s::prompt(&scenario, "Fresh raw text 雪🙂");
    scenario.finish(true).await;
}
