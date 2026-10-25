use acp2api_acceptance::{
    Scenario, Setup, content_check, conversation_check as c, real_limit, system_check,
    tool_check as t,
};
use serde_json::json;
use std::time::Duration;

async fn continues_after_tool_result(endpoint: &str) {
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let request = || t::request(endpoint, &[t::tool("read")], false, "auto");
        let mut call = request();
        if endpoint == "POST /v1/responses" {
            call.body["input"] = json!([c::message("user", None, "Use an offered tool.")]);
        }
        let mut body = call.body.clone();
        s.fake().next_prompt(t::plan(&["read".into()], &json!({"path":"a.txt"})));
        content_check::send(&mut s, call).await;
        let answer = s.answer.as_ref().unwrap();
        assert_eq!(answer.status, 200);
        let reply = answer.json();
        let calls = t::calls(answer, endpoint);
        assert_eq!(calls.len(), 1);
        let session = c::session(&s);
        let sessions = system_check::requests(&s).iter()
            .filter(|request| request["method"] == "session/new").count();
        let key = if endpoint == "POST /v1/messages" {
            let items = body["messages"].as_array_mut().unwrap();
            items.push(json!({"role":"assistant","content":reply["content"]}));
            items.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":calls[0]["id"],"content":"DATA"}]}));
            "messages"
        } else {
            let items = body["input"].as_array_mut().unwrap();
            items.extend(reply["output"].as_array().unwrap().iter().cloned());
            items.push(json!({"type":"function_call_output","call_id":calls[0]["call_id"],"output":"DATA"}));
            "input"
        };
        let mut call = request();
        call.body = body.clone();
        c::send(&mut s, call, "Read it.").await;
        c::continuation(&s, &session, "Result of your tool call read {\"path\":\"a.txt\"}:\nDATA");
        let reply = s.answer.as_ref().unwrap().json();
        let items = body[key].as_array_mut().unwrap();
        if endpoint == "POST /v1/messages" {
            items.push(json!({"role":"assistant","content":reply["content"]}));
        } else {
            items.extend(reply["output"].as_array().unwrap().iter().cloned());
        }
        items.push(c::message("user", None, "Next"));
        let mut call = request();
        call.body = body;
        c::send(&mut s, call, "Next reply.").await;
        c::continuation(&s, &session, "Next");
        let prompts = c::prompts(&s);
        assert_eq!(prompts.len(), 3, "one ACP prompt per API request");
        assert_eq!(
            system_check::requests(&s).iter().filter(|request| request["method"] == "session/new").count(),
            sessions,
            "the two continued requests create no ACP session"
        );
        s.finish(true).await;
    }).await.expect("tool-result continuation run cap");
}

#[tokio::test]
async fn messages_continue_after_a_tool_result_without_a_rebuild() {
    continues_after_tool_result("POST /v1/messages").await;
}

#[tokio::test]
async fn responses_continue_after_a_tool_result_without_a_rebuild() {
    continues_after_tool_result("POST /v1/responses").await;
}
