use acp2api_acceptance::{
    Scenario, Setup, content_check, conversation_check as c, real_limit, system_check,
    tool_check as t,
};
use serde_json::{Value, json};
use std::time::Duration;

fn events(s: &Scenario) -> Vec<Value> {
    s.answer
        .as_ref()
        .unwrap()
        .events
        .iter()
        .filter(|e| e.data != "[DONE]")
        .map(|e| serde_json::from_str(&e.data).unwrap())
        .collect()
}
async fn rewritten_stream_call_continues(endpoint: &str) {
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let tools = [t::tool("read")];
        let mut call = t::request(endpoint, &tools, true, "auto");
        if endpoint == "POST /v1/responses" { call.body["input"] = json!([c::message("user", None, "Use an offered tool.")]); }
        let mut body = call.body.clone();
        s.fake().next_prompt(t::plan(&["read".into(), "read".into()], &json!({"path":"a.txt"})));
        content_check::send(&mut s, call).await;
        assert_eq!(s.answer.as_ref().unwrap().status, 200);
        let ev = events(&s);
        let session = c::session(&s);
        let sessions = system_check::requests(&s).iter().filter(|r| r["method"] == "session/new").count();
        let id: String;
        match endpoint {
            "POST /v1/messages" => {
                let ids: Vec<_> = ev.iter().filter(|v| v["content_block"]["type"] == "tool_use").collect();
                assert_eq!(ids.len(), 1, "one tool call when the model makes two");
                id = ids[0]["content_block"]["id"].as_str().unwrap().into();
                assert!(id.starts_with("toolu_") && id.len() == 6 + 32, "{id}");
                let items = body["messages"].as_array_mut().unwrap();
                items.push(json!({"role":"assistant","content":[{"type":"text","text":"Let me look."},{"type":"tool_use","id":id,"name":"read","input":{"replace_all":false,"path":"a.txt"}}]}));
                items.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":"DATA"}]}));
            }
            "POST /v1/chat/completions" => {
                let ids: Vec<_> = ev.iter().filter_map(|v| v["choices"][0]["delta"]["tool_calls"][0]["id"].as_str()).collect();
                assert_eq!(ids.len(), 1);
                id = ids[0].into();
                assert!(id.starts_with("call_") && id.len() == 5 + 32, "{id}");
                let items = body["messages"].as_array_mut().unwrap();
                items.push(json!({"role":"assistant","content":"Let me look.","tool_calls":[{"id":id,"type":"function","function":{"name":"read","arguments":"{\"replace_all\":false,\"path\":\"a.txt\"}"}}]}));
                items.push(json!({"role":"tool","tool_call_id":id,"content":"DATA"}));
            }
            _ => {
                let done = ev.iter().find(|v| v["type"] == "response.completed").unwrap();
                let out = done["response"]["output"].as_array().unwrap();
                id = out[1]["call_id"].as_str().unwrap().into();
                assert!(id.starts_with("call_") && id.len() == 5 + 32, "{id}");
                let added = ev.iter().find(|v| v["type"] == "response.output_item.added" && v["item"]["type"] == "function_call").unwrap();
                assert_eq!(added["item"]["call_id"], id.as_str(), "stream added item and final body");
                let items = body["input"].as_array_mut().unwrap();
                items.push(out[0].clone());
                let mut changed = out[1].clone();
                changed["arguments"] = json!("{\"replace_all\":false,\"path\":\"a.txt\"}");
                items.push(changed);
                items.push(json!({"type":"function_call_output","call_id":id,"output":"DATA"}));
            }
        }
        let mut next = t::request(endpoint, &tools, true, "auto");
        next.body = body;
        s.fake().next_prompt(acp2api_acceptance::TurnPlan::new().chunk("Fine.").end("end_turn"));
        content_check::send(&mut s, next).await;
        assert_eq!(s.answer.as_ref().unwrap().status, 200);
        assert_eq!(c::kind(&s), "continue", "{endpoint}: {}", c::text(&s));
        c::continuation(&s, &session, "Result of your tool call read {\"replace_all\":false,\"path\":\"a.txt\"}:\nDATA");
        assert_eq!(system_check::requests(&s).iter().filter(|r| r["method"] == "session/new").count(), sessions);
        assert_eq!(s.fake().record().prompts().len(), 2, "two-request cap: initial call and returned result");
        s.finish(true).await;
    }).await.expect("run cap");
}
#[tokio::test]
async fn messages_stream_returns_the_recorded_call_id() {
    rewritten_stream_call_continues("POST /v1/messages").await;
}
#[tokio::test]
async fn chat_stream_returns_the_recorded_call_id() {
    rewritten_stream_call_continues("POST /v1/chat/completions").await;
}
#[tokio::test]
async fn responses_stream_returns_the_recorded_call_id() {
    rewritten_stream_call_continues("POST /v1/responses").await;
}
