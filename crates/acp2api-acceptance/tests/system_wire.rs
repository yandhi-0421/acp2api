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
        for stream in [false, true] {
            let mut scenario = setup();
            let mut call = s::request(
                "POST /v1/messages",
                system.clone(),
                Value::Null,
                " raw user\n雪🙂 ",
            );
            call.body["stream"] = stream.into();
            s::send(&mut scenario, call).await;
            s::addition(&scenario, &expected);
            s::prompt(&scenario, " raw user\n雪🙂 ");
            scenario.finish(true).await;
        }
    }
}
#[tokio::test]
async fn named_system_and_developer_parts_keep_their_original_roles() {
    for (endpoint, stream) in [
        ("POST /v1/chat/completions", false),
        ("POST /v1/chat/completions", true),
        ("POST /v1/responses", false),
        ("POST /v1/responses", true),
    ] {
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
        let mut call = s::request(endpoint, system, messages, "user");
        call.body["stream"] = stream.into();
        s::send(&mut scenario, call).await;
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
                vec![
                    json!([{"type":"text","text":"boundary"},{"type":"text","text":"later system"},{"type":"text","text":"later developer"},{"type":"text","text":"last"}])
                ],
                "later system parts stay at their conversation positions; labels belong to issue 47"
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
    let prompts = scenario.fake().record().prompts();
    assert_eq!(prompts.len(), 3);
    assert_ne!(
        prompts[0].session, prompts[1].session,
        "changed system additions never reuse immutable session metadata"
    );
    assert_eq!(
        prompts[1].session, prompts[2].session,
        "unchanged system addition and tools keep current session until issue 47 supplies full matching"
    );
    scenario.finish(true).await;
}
