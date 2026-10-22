//! Client-tool wire assertions use the actual API and the fake's observed MCP exchanges.
use crate::{
    Scenario, TurnPlan, TurnStep,
    client::{Answer, ApiCall},
    endpoint_check,
};
use serde_json::{Value, json};

pub fn tool(name: &str) -> Value {
    json!({"name":name,"description":format!("Read with {name}. 雪"),"inputSchema":{"type":"object","properties":{"path":{"type":"string"},"options":{"type":"array","items":{"type":"integer"}}},"required":["path"],"additionalProperties":false}})
}
pub fn request(endpoint: &str, tools: &[Value], stream: bool, choice: &str) -> ApiCall {
    let mut call = endpoint_check::request(endpoint, "Use an offered tool.", stream);
    call.body["tools"]=Value::Array(tools.iter().map(|t|match endpoint {
        "POST /v1/messages"=>json!({"name":t["name"],"description":t["description"],"input_schema":t["inputSchema"]}),
        "POST /v1/chat/completions"=>json!({"type":"function","function":{"name":t["name"],"description":t["description"],"parameters":t["inputSchema"]}}),
        _=>json!({"type":"function","name":t["name"],"description":t["description"],"parameters":t["inputSchema"]}),
    }).collect());
    call.body["tool_choice"] = if endpoint == "POST /v1/messages" {
        json!({"type":choice})
    } else {
        json!(choice)
    };
    call
}
pub fn plan(names: &[String], arguments: &Value) -> TurnPlan {
    let mut p = TurnPlan::new().chunk("Let ").chunk("me look.");
    for name in names {
        p.steps.push(TurnStep::ClientToolCall {
            name: name.clone(),
            arguments: arguments.to_string(),
        });
    }
    p.end("end_turn")
}
pub async fn send(s: &mut Scenario, call: ApiCall) -> Answer {
    s.client().await.send(call).finish(&mut s.time).await
}
pub fn offers(s: &Scenario, expected: &[Value]) {
    let exchanges = s.fake().record().offered_tools();
    assert!(
        !exchanges.is_empty(),
        "actual MCP tools/list response observed"
    );
    assert_eq!(
        exchanges.last().unwrap()["result"]["tools"],
        json!(expected),
        "complete model tool offer from MCP response"
    );
    let meta = s.fake().record().session_meta();
    for m in meta {
        assert_eq!(
            m["antigravity"]["enabledTools"],
            json!([]),
            "built-in tools disabled"
        );
        assert_eq!(
            m["antigravity"]["eagerMcpServers"],
            json!(["client-tools"]),
            "schemas offered eagerly"
        );
    }
}
pub fn permission_requests(s: &Scenario) -> Vec<Value> {
    lines(s, "command")
        .into_iter()
        .filter(|v| v["method"] == "session/request_permission")
        .collect()
}
pub fn permission_replies(s: &Scenario) -> Vec<Value> {
    lines(s, "got")
        .into_iter()
        .filter(|v| v.get("method").is_none() && v.get("result").is_some())
        .collect()
}
fn lines(s: &Scenario, direction: &str) -> Vec<Value> {
    s.hub
        .entries()
        .iter()
        .filter(|e| e.event["kind"] == direction)
        .filter_map(|e| {
            serde_json::from_str(if direction == "command" {
                e.event["command"]["line"].as_str()?
            } else {
                e.event["line"].as_str()?
            })
            .ok()
        })
        .collect()
}
pub fn cancelled(s: &Scenario, expected: usize) {
    let prompts = s.fake().record().prompts();
    let cancels = s.fake().record().cancels();
    assert_eq!(cancels.len(), expected, "actual session/cancel count");
    for (cancel, prompt) in cancels.iter().zip(prompts) {
        assert_eq!(
            cancel["params"]["sessionId"], prompt.session,
            "cancel matches active ACP session"
        );
        assert!(cancel.get("id").is_none(), "cancel is an ACP notification");
    }
    assert!(
        !permission_requests(s).is_empty(),
        "permission was actually sent"
    );
    assert!(
        permission_replies(s).len() >= expected,
        "permission cancellation replies observed"
    );
    for reply in permission_replies(s) {
        assert_ne!(
            reply["result"]["outcome"]["outcome"], "selected",
            "never selects an allow"
        );
        assert_eq!(
            reply["result"]["outcome"]["outcome"], "cancelled",
            "permission is cancelled"
        );
    }
    assert!(
        s.hub
            .entries()
            .iter()
            .all(|e| e.event["request"]["method"] != "tools/call"),
        "no MCP execution or held call"
    );
}
pub fn calls(answer: &Answer, endpoint: &str) -> Vec<Value> {
    let v = answer.json();
    match endpoint {
        "POST /v1/messages" => v["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["type"] == "tool_use")
            .cloned()
            .collect(),
        "POST /v1/chat/completions" => v["choices"][0]["message"]["tool_calls"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
        _ => v["output"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["type"] == "function_call")
            .cloned()
            .collect(),
    }
}
fn id(value: &Value, prefix: &str) -> String {
    let value = value.as_str().expect("tool identifier");
    assert!(
        value.starts_with(prefix) && value.len() > prefix.len(),
        "tool identifier prefix: {value}"
    );
    value.into()
}
pub fn body(answer: &Answer, endpoint: &str, name: &str, args: &Value) {
    assert_eq!(answer.status, 200, "tool response status");
    let v = answer.json();
    let tool_calls = calls(answer, endpoint);
    assert_eq!(tool_calls.len(), 1, "one client tool call");
    let call = &tool_calls[0];
    match endpoint {
        "POST /v1/messages" => {
            assert_eq!(
                v["content"],
                json!([{"type":"text","text":"Let me look."},{"type":"tool_use","id":id(&call["id"],"toolu_"),"name":name,"input":args}])
            );
            assert_eq!(v["stop_reason"], "tool_use");
        }
        "POST /v1/chat/completions" => {
            assert_eq!(v["choices"][0]["message"]["content"], "Let me look.");
            assert_eq!(call["id"], id(&call["id"], "call_"));
            assert_eq!(call["type"], "function");
            assert_eq!(call["function"]["name"], name);
            assert_eq!(
                serde_json::from_str::<Value>(call["function"]["arguments"].as_str().unwrap())
                    .unwrap(),
                *args
            );
            assert_eq!(v["choices"][0]["finish_reason"], "tool_calls");
        }
        _ => {
            assert_eq!(v["output"].as_array().unwrap().len(), 2);
            assert_eq!(v["output"][0]["content"][0]["text"], "Let me look.");
            assert_eq!(&v["output"][1], call, "tool is the last output item");
            id(&call["id"], "fc_");
            id(&call["call_id"], "call_");
            assert_eq!(call["name"], name);
            assert_eq!(
                serde_json::from_str::<Value>(call["arguments"].as_str().unwrap()).unwrap(),
                *args
            );
            assert_eq!(call["status"], "completed");
            assert_eq!(v["status"], "completed");
        }
    }
}
pub fn stream(answer: &Answer, endpoint: &str, name: &str, args: &Value) {
    assert_eq!(answer.status, 200);
    assert_eq!(
        endpoint_check::text(&answer.events),
        "Let me look.",
        "preceding text includes held last chunk"
    );
    let events: Vec<_> = answer
        .events
        .iter()
        .filter(|e| e.data != "[DONE]")
        .map(|e| serde_json::from_str::<Value>(&e.data).unwrap())
        .collect();
    match endpoint {
        "POST /v1/messages" => {
            let starts: Vec<_> = events
                .iter()
                .filter(|v| {
                    v["type"] == "content_block_start" && v["content_block"]["type"] == "tool_use"
                })
                .collect();
            assert_eq!(starts.len(), 1, "one tool block start");
            let start = starts[0];
            id(&start["content_block"]["id"], "toolu_");
            assert_eq!(start["index"], 1);
            assert_eq!(start["content_block"]["name"], name);
            assert_eq!(start["content_block"]["input"], json!({}));
            let deltas: Vec<_> = events
                .iter()
                .filter(|v| v["delta"]["type"] == "input_json_delta")
                .collect();
            assert!(!deltas.is_empty());
            assert!(deltas.iter().all(|v| v["index"] == start["index"]));
            let text: String = deltas
                .iter()
                .map(|v| v["delta"]["partial_json"].as_str().unwrap())
                .collect();
            assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), *args);
            let kinds: Vec<_> = events.iter().map(|v| v["type"].as_str().unwrap()).collect();
            assert_eq!(
                kinds,
                vec![
                    "message_start",
                    "content_block_start",
                    "content_block_delta",
                    "content_block_delta",
                    "content_block_stop",
                    "content_block_start",
                    "content_block_delta",
                    "content_block_stop",
                    "message_delta",
                    "message_stop"
                ]
            );
            assert_eq!(events[events.len() - 2]["delta"]["stop_reason"], "tool_use");
        }
        "POST /v1/chat/completions" => {
            let calls: Vec<_> = events
                .iter()
                .filter_map(|v| v["choices"][0]["delta"]["tool_calls"].as_array())
                .flatten()
                .collect();
            assert!(!calls.is_empty(), "stream client tool call");
            let first = calls[0];
            id(&first["id"], "call_");
            assert_eq!(first["index"], 0);
            assert_eq!(first["function"]["name"], name);
            assert_eq!(first["type"], "function");
            assert_eq!(
                calls.iter().filter(|v| v.get("id").is_some()).count(),
                1,
                "single stream call ID"
            );
            assert!(calls.iter().all(|v| v["index"] == 0));
            let text: String = calls
                .iter()
                .filter_map(|v| v["function"]["arguments"].as_str())
                .collect();
            assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), *args);
            assert_eq!(
                events.last().unwrap()["choices"][0]["finish_reason"],
                "tool_calls"
            );
            assert_eq!(answer.events.last().unwrap().data, "[DONE]");
        }
        _ => {
            let added = events
                .iter()
                .find(|v| {
                    v["type"] == "response.output_item.added"
                        && v["item"]["type"] == "function_call"
                })
                .expect("stream function call item");
            let call = &added["item"];
            let item_id = id(&call["id"], "fc_");
            id(&call["call_id"], "call_");
            assert_eq!(call["name"], name);
            assert_eq!(call["arguments"], "");
            assert_eq!(call["status"], "in_progress");
            assert_eq!(added["output_index"], 1);
            let deltas: Vec<_> = events
                .iter()
                .filter(|v| v["type"] == "response.function_call_arguments.delta")
                .collect();
            assert!(!deltas.is_empty());
            assert!(
                deltas
                    .iter()
                    .all(|v| v["item_id"] == item_id && v["output_index"] == 1)
            );
            let text: String = deltas
                .iter()
                .map(|v| v["delta"].as_str().unwrap())
                .collect();
            assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), *args);
            let done = events
                .iter()
                .find(|v| v["type"] == "response.function_call_arguments.done")
                .unwrap();
            assert_eq!(done["item_id"], item_id);
            assert_eq!(done["arguments"], text);
            assert_eq!(done["output_index"], 1);
            let item = events
                .iter()
                .find(|v| v["type"] == "response.output_item.done" && v["item"]["id"] == item_id)
                .unwrap();
            assert_eq!(item["item"]["arguments"], text);
            assert_eq!(item["item"]["call_id"], call["call_id"]);
            assert_eq!(item["item"]["name"], name);
            assert_eq!(item["item"]["status"], "completed");
            let final_response = &events.last().unwrap()["response"];
            assert_eq!(final_response["output"][1], item["item"]);
            assert_eq!(final_response["output"].as_array().unwrap().len(), 2);
            assert_eq!(events.last().unwrap()["type"], "response.completed");
            assert_eq!(final_response["status"], "completed");
            for (sequence, event) in events.iter().enumerate() {
                assert_eq!(event["sequence_number"], sequence as u64);
            }
        }
    }
}
