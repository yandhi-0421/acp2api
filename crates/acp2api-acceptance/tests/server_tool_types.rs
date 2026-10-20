use acp2api_acceptance::{Scenario, Setup, content_check, server_tool_check as c, tool_check};
use serde_json::json;

fn input(name: &str, fallback: &str) -> String {
    let value = std::env::var(name).unwrap_or_else(|_| fallback.into());
    assert!(
        value.len() <= 80 && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "bounded synthetic probe input"
    );
    value
}

#[tokio::test]
async fn tool_contracts_are_classified_by_api_and_type() {
    let unknown = input("ACP2API_PROBE_TYPE", "unrecognized_tool_20991231");
    let name = input("ACP2API_PROBE_NAME", "look_up_record");
    let date = input("ACP2API_PROBE_DATE", "20260318");
    assert!(date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) && date != "20250305");
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    for endpoint in [
        c::MESSAGES,
        "POST /v1/responses",
        "POST /v1/chat/completions",
    ] {
        c::send(&mut s, c::request(endpoint, &unknown, Some(&name))).await;
        c::envelope(
            &s,
            endpoint,
            &format!(
                "capability_rejected: tool type {unknown} (name {name}) is not supported by acp2api."
            ),
            "unsupported_tool_type",
        );
        println!("unknown type refused on {endpoint}; prompts=0");
    }
    for family in [
        "web_fetch",
        "web_search",
        "code_execution",
        "tool_search_tool_regex",
        "tool_search_tool_bm25",
    ] {
        let kind = format!("{family}_{date}");
        c::send(&mut s, c::request(c::MESSAGES, &kind, Some(&name))).await;
        c::envelope(
            &s,
            c::MESSAGES,
            &format!(
                "capability_rejected: server tool {kind} (name {name}) is not supported by acp2api."
            ),
            "unsupported_server_tool",
        );
    }
    let advisor = format!("advisor_{date}");
    let mut call = c::request(c::MESSAGES, &advisor, None);
    call.body["tools"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"type":unknown,"name":name}));
    call.body["model"] = json!(null);
    call.body["messages"] = json!([{"role":"user","content":[c::addition(&name,false)]}]);
    c::send(&mut s, call).await;
    c::envelope(
        &s,
        c::MESSAGES,
        &format!(
            "Input tag '{advisor}' found using 'type' does not match any of the expected tags"
        ),
        "unsupported_advisor_tool",
    );
    c::advisor_log(&s);
    println!(
        "advisor precedes invalid model, earlier unknown tool, and full addition; actual product log verified"
    );
    for kind in [None, Some("custom"), Some("function")] {
        let mut call = tool_check::request(c::MESSAGES, &[tool_check::tool(&name)], false, "auto");
        if let Some(kind) = kind {
            call.body["tools"][0]["type"] = json!(kind);
        }
        c::send(&mut s, call).await;
        assert_eq!(
            s.answer.as_ref().unwrap().status,
            200,
            "known Anthropic client tool contract accepted"
        );
    }
    assert_eq!(
        content_check::blocks(&s).len(),
        3,
        "only valid client tools start prompts"
    );
    s.finish(true).await;
}

#[tokio::test]
async fn tool_reference_is_removed_only_when_it_names_a_declared_client_tool() {
    let name = input("ACP2API_PROBE_NAME", "load_report");
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let call = c::with_addition(&name, true, &name);
    let mut first = c::with_addition(&name, true, &name);
    first.body = call.body.clone();
    c::send(&mut s, first).await;
    c::compare_content(&mut s, &call).await;
    println!("valid reference present/absent: two real ACP payloads equal");
    s.finish(true).await;
    for reference in [true, false] {
        let mut rejected = Scenario::default();
        rejected.plan(Setup::normal());
        c::send(
            &mut rejected,
            c::with_addition(&name, reference, &format!("{name}_missing")),
        )
        .await;
        c::envelope(
            &rejected,
            c::MESSAGES,
            "acp2api: the content type tool_addition is not supported.",
            "unsupported_content_type",
        );
        println!("invalid addition refused; prompts=0");
        rejected.finish(true).await;
    }
}
