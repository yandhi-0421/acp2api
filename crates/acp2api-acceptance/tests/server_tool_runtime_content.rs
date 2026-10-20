use acp2api_acceptance::{Scenario, Setup, content_check, server_tool_check as c, system_check};
use serde_json::{Value, json};

async fn runtime_content(with_reference: bool, name: &str) -> Value {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s.fake().catalog(&[acp2api_acceptance::fake::ModelSpec {
        value: "gemini-3.8-flash".into(),
        name: "Gemini 3.8 Flash".into(),
        levels: vec!["low".into(), "medium".into(), "high".into()],
    }]);
    let mut call = c::with_addition(name, true, name);
    call.body["model"] = json!("gemini-3.8-flash-high");
    call.body["output_config"] = json!({"effort":"high"});
    call.body["system"] = json!([{"type":"text","text":"Keep ledger records intact."},{"type":"text","text":"Reply in Chinese. 雪"}]);
    call.body["tools"] = json!([{"type":"custom","name":name,"description":"Read one ledger entry.","input_schema":{"type":"object","properties":{"record_id":{"type":"string"},"include_notes":{"type":"boolean"}},"required":["record_id"],"additionalProperties":false}}]);
    if !with_reference {
        call.body["messages"][0]["content"]
            .as_array_mut()
            .unwrap()
            .remove(1);
    }
    c::send(&mut s, call).await;
    assert_eq!(
        s.answer.as_ref().unwrap().status,
        200,
        "complete runtime content request accepted: {}",
        s.answer.as_ref().unwrap().json()
    );
    let record = s.fake().record();
    let meta = record
        .session_meta()
        .last()
        .expect("actual turn session/new")
        .clone();
    assert_eq!(
        meta,
        json!({"antigravity":{"enabledTools":[],"eagerMcpServers":["client-tools"],"systemPrompt":{"append":"Keep ledger records intact.\n\nReply in Chinese. 雪"}}}),
        "nonempty literal system addition and tool settings"
    );
    let offerings = record.offered_tools();
    let tools =
        offerings.last().expect("actual MCP tools/list response")["result"]["tools"].clone();
    assert_eq!(
        tools,
        json!([{"name":name,"description":"Read one ledger entry.","inputSchema":{"type":"object","properties":{"record_id":{"type":"string"},"include_notes":{"type":"boolean"}},"required":["record_id"],"additionalProperties":false}}]),
        "complete client tool declaration survives reference handling"
    );
    let prompt = content_check::blocks(&s);
    assert_eq!(
        prompt,
        vec![
            json!([{"type":"text","text":"Before the addition. 雪"},{"type":"text","text":"After the addition. 🙂"}])
        ]
    );
    let requests = system_check::requests(&s);
    let configs: Vec<_> = requests
        .iter()
        .filter(|v| {
            matches!(
                v["method"].as_str(),
                Some("session/set_config_option" | "session/set_mode" | "session/set_model")
            )
        })
        .map(|v| json!({"method":v["method"],"params":v["params"]}))
        .collect();
    let new_values: Vec<_> = requests.iter().filter(|v| v["method"]=="session/new").map(|v| json!({"configOptions":v["params"]["configOptions"],"model":v["params"]["model"],"thinking_level":v["params"]["thinking_level"]})).collect();
    let config_count = configs.len();
    let content = json!({"meta":meta,"tools":tools,"prompt":prompt,"config_calls":configs,"new_config_values":new_values});
    println!(
        "reference={with_reference}: actual system addition, tool declarations and prompt checked; config calls={config_count}; actual calls and values follow (model application is not proved)"
    );
    println!("observed complete runtime content: {content}");
    s.finish(true).await;
    content
}

#[tokio::test]
async fn reference_keeps_system_tools_and_current_config_observations() {
    let name = std::env::var("ACP2API_PROBE_NAME").unwrap_or_else(|_| "read_ledger_entry".into());
    assert!(
        !name.is_empty()
            && name.len() <= 80
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    );
    let present = runtime_content(true, &name).await;
    let absent = runtime_content(false, &name).await;
    assert_eq!(
        present, absent,
        "all actual runtime content is unchanged by a valid reference"
    );
}

#[tokio::test]
async fn websearch_settings_message_has_exact_literal_contents() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    c::send(
        &mut s,
        c::request(c::MESSAGES, "web_search_20250305", Some("web_search")),
    )
    .await;
    c::envelope(
        &s,
        c::MESSAGES,
        "capability_rejected: server tool web_search_20250305 (name web_search) is not supported by acp2api. To stop these calls, deny the WebSearch tool in the Claude Code settings.",
        "unsupported_server_tool",
    );
    println!(
        "observed exact WebSearch envelope: {}",
        s.answer.as_ref().unwrap().json()
    );
    s.finish(true).await;
}
