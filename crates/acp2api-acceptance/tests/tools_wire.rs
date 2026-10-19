use acp2api_acceptance::{Scenario, Setup, TurnPlan, tool_check as t};
use serde_json::json;
const ENDPOINTS: [&str; 3] = [
    "POST /v1/messages",
    "POST /v1/chat/completions",
    "POST /v1/responses",
];

#[tokio::test]
async fn fresh_tool_names_schemas_and_nested_arguments_cross_all_dialects() {
    for endpoint in ENDPOINTS {
        for stream in [false, true] {
            let mut s = Scenario::default();
            s.plan(Setup::normal());
            let name = "lookup_weather_fresh";
            let tool = json!({"name":name,"description":"A fresh description 雪🙂","inputSchema":{"type":"object","properties":{"path":{"type":"string"},"n":{"type":"number"},"items":{"type":"array","items":{}}},"required":["path","items"]}});
            let args = json!({"path":"fresh/雪🙂\n\"a\"","n":1.25,"items":[true,null,{"deep":[1,2,"three"]}],"empty":{}});
            s.fake().next_prompt(t::plan(&[name.into()], &args));
            let answer = t::send(
                &mut s,
                t::request(endpoint, std::slice::from_ref(&tool), stream, "auto"),
            )
            .await;
            if stream {
                t::stream(&answer, endpoint, name, &args);
            } else {
                t::body(&answer, endpoint, name, &args);
            }
            t::offers(&s, &[tool]);
            t::cancelled(&s, 1);
            s.finish(true).await;
        }
    }
}
#[tokio::test]
async fn different_request_tool_sets_get_immutable_mcp_paths_and_sessions() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let sets = [
        vec![t::tool("first_read")],
        vec![t::tool("second_write"), t::tool("third_list")],
        vec![],
    ];
    for tools in &sets {
        let answer = t::send(
            &mut s,
            t::request("POST /v1/messages", tools, false, "auto"),
        )
        .await;
        assert_eq!(answer.status, 200);
        t::offers(&s, tools);
    }
    let record = s.fake().record();
    let prompts = record.prompts();
    assert_eq!(prompts.len(), 3);
    let sessions: std::collections::HashSet<_> = prompts.iter().map(|p| &p.session).collect();
    assert_eq!(
        sessions.len(),
        3,
        "different tool sets never mutate an ACP session"
    );
    let offers = record.offered_tools();
    assert_eq!(offers.len(), 3);
    for (offer, expected) in offers.iter().zip(sets) {
        assert_eq!(offer["result"]["tools"], json!(expected));
    }
    s.finish(true).await;
}
#[tokio::test]
async fn tool_choice_none_exposes_no_callable_tool_in_each_dialect() {
    for endpoint in ENDPOINTS {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        s.fake()
            .next_prompt(t::plan(&["forbidden_offer".into()], &json!({"path":"x"})));
        let answer = t::send(
            &mut s,
            t::request(endpoint, &[t::tool("forbidden_offer")], false, "none"),
        )
        .await;
        assert!(
            t::calls(&answer, endpoint).is_empty(),
            "none returns no tool despite unsolicited permission"
        );
        t::offers(&s, &[]);
        assert!(
            s.fake().record().cancels().is_empty(),
            "unoffered tool does not become a client tool"
        );
        let replies = t::permission_replies(&s);
        assert_eq!(replies.len(), 1, "unoffered permission is answered");
        assert_eq!(replies[0]["result"]["outcome"]["outcome"], "cancelled");
        s.finish(true).await;
    }
}
#[tokio::test]
async fn rejection_uses_each_api_envelope_and_starts_no_prompt() {
    for endpoint in ENDPOINTS {
        for name in ["list_resources", "read_resource"] {
            let mut s = Scenario::default();
            s.plan(Setup::normal());
            s.answer = Some(
                t::send(
                    &mut s,
                    t::request(endpoint, &[t::tool(name)], false, "auto"),
                )
                .await,
            );
            acp2api_acceptance::content_check::refusal(
                &s,
                endpoint,
                &format!(
                    "acp2api: the client tool name {name} is not supported, because a built-in tool of the official runtime has this name."
                ),
                "unsupported_tool_name",
                None,
            );
            s.finish(true).await;
        }
    }
}
#[tokio::test]
async fn cancel_completes_a_permission_wait_without_waiting_for_tool_execution() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let args = json!({"path":"not-executed"});
    let mut p = t::plan(&["waiting_tool".into()], &args);
    p.end = acp2api_acceptance::TurnEndPlan::Never;
    s.fake().next_prompt(p);
    let answer = t::send(
        &mut s,
        t::request(
            "POST /v1/messages",
            &[t::tool("waiting_tool")],
            false,
            "auto",
        ),
    )
    .await;
    t::body(&answer, "POST /v1/messages", "waiting_tool", &args);
    t::cancelled(&s, 1);
    s.finish(true).await;
}

#[tokio::test]
async fn empty_text_chunks_before_a_tool_do_not_create_an_empty_text_item() {
    for endpoint in ENDPOINTS {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let mut plan = TurnPlan::new().chunk("").chunk("");
        plan.steps
            .push(acp2api_acceptance::TurnStep::ClientToolCall {
                name: "only_tool".into(),
                arguments: "{}".into(),
            });
        s.fake().next_prompt(plan);
        let answer = t::send(
            &mut s,
            t::request(endpoint, &[t::tool("only_tool")], true, "auto"),
        )
        .await;
        let events: Vec<serde_json::Value> = answer
            .events
            .iter()
            .filter(|e| e.data != "[DONE]")
            .map(|e| serde_json::from_str(&e.data).unwrap())
            .collect();
        match endpoint {
            "POST /v1/messages" => {
                let starts: Vec<_> = events
                    .iter()
                    .filter(|e| e["type"] == "content_block_start")
                    .collect();
                assert_eq!(starts.len(), 1, "tool-only stream opens exactly one block");
                assert_eq!(starts[0]["index"], 0);
                assert_eq!(starts[0]["content_block"]["type"], "tool_use");
            }
            "POST /v1/responses" => {
                let starts: Vec<_> = events
                    .iter()
                    .filter(|e| e["type"] == "response.output_item.added")
                    .collect();
                assert_eq!(
                    starts.len(),
                    1,
                    "tool-only stream opens exactly one output item"
                );
                assert_eq!(starts[0]["output_index"], 0);
                assert_eq!(starts[0]["item"]["type"], "function_call");
                assert_eq!(
                    events.last().unwrap()["response"]["output"]
                        .as_array()
                        .unwrap()
                        .len(),
                    1
                );
            }
            _ => assert!(
                events
                    .iter()
                    .all(|e| e["choices"][0]["delta"].get("content").is_none()),
                "tool-only stream has no text delta"
            ),
        }
        t::cancelled(&s, 1);
        s.finish(true).await;
    }
}
