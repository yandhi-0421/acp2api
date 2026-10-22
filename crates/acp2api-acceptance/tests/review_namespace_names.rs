use acp2api_acceptance::{Scenario, Setup, custom_tool_check as c, tool_check as t};
use serde_json::json;

// The runtime shows a tool of the MCP server "client-tools" to the model as
// `mcp_client-tools_<name>` (docs/research/2026-10-18-earlier-cli-bridge-findings.md, RT-3).
const RUNTIME_PREFIX: &str = "mcp_client-tools_";

#[tokio::test]
async fn namespace_members_get_readable_bounded_names_that_map_back() {
    let long_namespace = "external_mcp_server_with_a_rather_long_namespace";
    let long_members = [
        "read_a_rather_long_member_tool_name_alpha",
        "read_a_rather_long_member_tool_name_beta",
    ];
    let tools = vec![
        c::namespace("documents_context", &["read_context"]),
        c::namespace(long_namespace, &long_members),
        c::namespace("docs", &["read"]),
        json!({"type":"function","name":"docs__read","description":"Plain","parameters":{"type":"object"}}),
    ];
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let answer = c::send(&mut s, &tools, false, None, &json!({})).await;
    assert_eq!(answer.status, 200);
    let names: Vec<String> = c::offered(&s)
        .iter()
        .map(|offer| offer["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names.len(), 5);
    assert_eq!(
        names[0], "documents_context__read_context",
        "the model sees the namespace and the member name"
    );
    for name in &names {
        assert!(
            RUNTIME_PREFIX.len() + name.len() <= 64,
            "{RUNTIME_PREFIX}{name} has at most 64 characters"
        );
    }
    assert_ne!(names[1], names[2], "cut names stay distinct");
    assert_eq!(names[4], "docs__read", "a plain name stays the same");
    assert!(
        names[3].starts_with("docs__read") && names[3] != names[4],
        "a colliding member keeps its readable name and gets a suffix"
    );
    let args = json!({"path":"a.txt"});
    let expected = [
        ("read_context", Some("documents_context")),
        (long_members[0], Some(long_namespace)),
        (long_members[1], Some(long_namespace)),
        ("read", Some("docs")),
        ("docs__read", None),
    ];
    for (name, (member, namespace)) in names.iter().zip(expected) {
        let answer = c::send(&mut s, &tools, false, Some(name), &args).await;
        c::output(&answer, false, "function_call", member, namespace, &args);
    }
    // The first request ends with no tool call, so prompts 1 to 5 get the cancels.
    t::cancelled_prompts(&s, 5, &[1, 2, 3, 4, 5]);
    s.finish(true).await;
}

#[tokio::test]
async fn malformed_custom_and_namespace_declarations_are_refused() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    for (tool, message) in [
        (
            json!({"type":"custom","name":"patch","format":{"type":"regex"}}),
            "unsupported custom tool format",
        ),
        (
            json!({"type":"custom","name":"patch","format":{"type":"grammar","syntax":"lark"}}),
            "definition",
        ),
        (
            json!({"type":"custom","name":"patch","format":{"type":"grammar","definition":"start: /x/"}}),
            "syntax",
        ),
        (
            json!({"type":"namespace","name":"docs","tools":{"read":{}}}),
            "namespace tools must be an array",
        ),
    ] {
        let answer = t::send(&mut s, c::request(&[tool], false)).await;
        assert_eq!(answer.status, 400, "{message}");
        let error = answer.json()["error"]["message"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(error.contains(message), "{error:?} names {message}");
    }
    assert!(
        s.fake().record().prompts().is_empty(),
        "no refused request reaches the runtime"
    );
    s.finish(true).await;
}
