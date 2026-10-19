use crate::{
    Scenario,
    client::{Answer, ApiCall},
    tool_check as t,
};
use serde_json::{Value, json};

pub const ENDPOINT: &str = "POST /v1/responses";
pub const GRAMMAR: &str = "start: patch\npatch: /[^\\x00]+/ // 修改 雪🙂";
pub fn custom(name: &str, grammar: &str) -> Value {
    json!({"type":"custom","name":name,"description":"Apply an exact patch 雪🙂","format":{"type":"grammar","syntax":"lark","definition":grammar}})
}
pub fn namespace(name: &str, members: &[&str]) -> Value {
    json!({"type":"namespace","name":name,"description":format!("Namespace {name}"),"tools":members.iter().map(|name| json!({"type":"function","name":name,"description":format!("Member {name}"),"parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}})).collect::<Vec<_>>()})
}
pub fn request(tools: &[Value], stream: bool) -> ApiCall {
    let mut call = t::request(ENDPOINT, &[], stream, "auto");
    call.body["tools"] = json!(tools);
    call
}
pub async fn send(
    s: &mut Scenario,
    tools: &[Value],
    stream: bool,
    name: Option<&str>,
    args: &Value,
) -> Answer {
    s.fake().next_prompt(
        name.map(|name| t::plan(&[name.into()], args))
            .unwrap_or_default(),
    );
    t::send(s, request(tools, stream)).await
}
pub fn offered(s: &Scenario) -> Vec<Value> {
    s.fake()
        .record()
        .offered_tools()
        .last()
        .expect("actual MCP tools/list observed")["result"]["tools"]
        .as_array()
        .unwrap()
        .clone()
}
pub fn offer_custom(s: &Scenario, name: &str, grammar: &str) {
    let offers = offered(s);
    assert_eq!(offers.len(), 1, "one custom tool offered");
    assert_eq!(offers[0]["name"], name);
    assert_eq!(
        offers[0]["inputSchema"],
        json!({"type":"object","properties":{"input":{"type":"string"}},"required":["input"],"additionalProperties":false}),
        "custom tool has exactly one string input"
    );
    let description = offers[0]["description"].as_str().unwrap();
    assert!(
        description.contains(grammar),
        "custom tool description preserves the grammar"
    );
    assert!(
        description.contains("Apply an exact patch 雪🙂"),
        "original description retained"
    );
}
pub fn offer_namespace(s: &Scenario, namespace: &str, members: &[&str]) -> Vec<String> {
    let offers = offered(s);
    assert_eq!(
        offers.len(),
        members.len(),
        "one MCP offering for each namespace member"
    );
    let mut names = Vec::new();
    for (offer, member) in offers.iter().zip(members) {
        assert_eq!(
            offer["description"],
            format!("Namespace {namespace}:\nNamespace {namespace}\n\nMember {member}")
        );
        assert_eq!(offer["inputSchema"]["properties"]["path"]["type"], "string");
        names.push(offer["name"].as_str().unwrap().to_owned());
    }
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "distinct model-facing namespace identities"
    );
    names
}
fn verify_item(item: &Value, kind: &str, name: &str, namespace: Option<&str>, payload: &Value) {
    assert_eq!(item["type"], kind, "original tool kind returned");
    assert_eq!(item["name"], name, "original member name returned");
    assert_eq!(
        item.get("namespace").and_then(Value::as_str),
        namespace,
        "original namespace returned"
    );
    if kind == "function_call" {
        assert_eq!(item["status"], "completed");
    } else {
        assert!(
            item.get("status").is_none(),
            "custom tool items have no status field"
        );
    }
    assert!(item["id"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(
        item["call_id"]
            .as_str()
            .is_some_and(|s| s.starts_with("call_"))
    );
    if kind == "custom_tool_call" {
        assert_eq!(item["input"], *payload, "exact custom input returned");
        assert!(item.get("arguments").is_none());
    } else {
        assert_eq!(
            serde_json::from_str::<Value>(item["arguments"].as_str().unwrap()).unwrap(),
            *payload
        );
    }
}
pub fn output(
    answer: &Answer,
    stream: bool,
    kind: &str,
    name: &str,
    namespace: Option<&str>,
    payload: &Value,
) {
    assert_eq!(
        answer.status, 200,
        "Responses custom/namespace request accepted"
    );
    if !stream {
        let body = answer.json();
        assert_eq!(body["status"], "completed");
        assert_eq!(
            body["output"].as_array().unwrap().len(),
            2,
            "preceding text and exactly one tool call"
        );
        assert_eq!(body["output"][0]["content"][0]["text"], "Let me look.");
        verify_item(&body["output"][1], kind, name, namespace, payload);
        return;
    }
    assert_eq!(crate::endpoint_check::text(&answer.events), "Let me look.");
    let events: Vec<Value> = answer
        .events
        .iter()
        .map(|e| serde_json::from_str(&e.data).unwrap())
        .collect();
    let added = events
        .iter()
        .find(|e| e["type"] == "response.output_item.added" && e["item"]["type"] == kind)
        .expect("stream tool item added");
    assert_eq!(added["output_index"], 1);
    if kind == "function_call" {
        assert_eq!(added["item"]["status"], "in_progress");
    } else {
        assert!(added["item"].get("status").is_none());
    }
    assert_eq!(added["item"]["name"], name);
    assert_eq!(
        added["item"].get("namespace").and_then(Value::as_str),
        namespace
    );
    let (delta_kind, done_kind, field) = if kind == "custom_tool_call" {
        (
            "response.custom_tool_call_input.delta",
            "response.custom_tool_call_input.done",
            "input",
        )
    } else {
        (
            "response.function_call_arguments.delta",
            "response.function_call_arguments.done",
            "arguments",
        )
    };
    assert_eq!(added["item"][field], "");
    let deltas: Vec<_> = events.iter().filter(|e| e["type"] == delta_kind).collect();
    assert!(!deltas.is_empty(), "tool payload delta observed");
    let text: String = deltas
        .iter()
        .map(|e| {
            assert_eq!(e["item_id"], added["item"]["id"]);
            assert_eq!(e["output_index"], 1);
            e["delta"].as_str().unwrap()
        })
        .collect();
    if kind == "custom_tool_call" {
        assert_eq!(json!(text), *payload);
    } else {
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), *payload);
    }
    let done = events
        .iter()
        .find(|e| e["type"] == done_kind)
        .expect("tool payload done observed");
    assert_eq!(done[field], text);
    assert_eq!(done["item_id"], added["item"]["id"]);
    assert_eq!(done["output_index"], 1);
    let completed = events
        .iter()
        .find(|e| {
            e["type"] == "response.output_item.done" && e["item"]["id"] == added["item"]["id"]
        })
        .expect("stream tool item done");
    verify_item(&completed["item"], kind, name, namespace, payload);
    assert_eq!(completed["item"]["call_id"], added["item"]["call_id"]);
    let last = events.last().unwrap();
    assert_eq!(last["type"], "response.completed");
    assert_eq!(last["response"]["output"][1], completed["item"]);
    assert_eq!(last["response"]["output"].as_array().unwrap().len(), 2);
    for (n, event) in events.iter().enumerate() {
        assert_eq!(event["sequence_number"], n as u64);
    }
}
