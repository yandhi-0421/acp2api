//! Wire assertions sourced from FR-010 and the saved endpoint contract, section 3.
use crate::client::{Answer, ApiCall, SseEvent};
use serde_json::{Value, json};
pub const MODEL: &str = "gemini-3.8-flash";
pub fn request(endpoint: &str, text: &str, stream: bool) -> ApiCall {
    let mut call = ApiCall::one_user_message(endpoint, text);
    if endpoint == "POST /v1/responses" {
        call.body = json!({"model":MODEL,"input":text,"stream":stream});
    } else {
        call.body["stream"] = json!(stream);
    }
    call
}
pub fn text(events: &[SseEvent]) -> String {
    events
        .iter()
        .filter_map(|e| {
            let v: Value = serde_json::from_str(&e.data).ok()?;
            if e.event == "content_block_delta" {
                v["delta"]["text"].as_str().map(str::to_owned)
            } else if e.event == "response.output_text.delta" {
                v["delta"].as_str().map(str::to_owned)
            } else {
                v["choices"][0]["delta"]["content"]
                    .as_str()
                    .map(str::to_owned)
            }
        })
        .collect()
}
fn id(v: &Value, prefix: &str) -> String {
    let id = v.as_str().expect("wire identifier");
    assert!(
        id.starts_with(prefix) && id.len() > prefix.len(),
        "identifier prefix: {id}"
    );
    id.to_owned()
}
fn usage(v: &Value, input: &str, output: &str, used: u64, tokens: u64) {
    assert_eq!(v[input], used, "context-used input accounting");
    assert_eq!(v[output], tokens, "reply token estimate");
    if input != "input_tokens" || v.get("total_tokens").is_some() {
        assert_eq!(v["total_tokens"], used + tokens);
    }
}
pub fn verify(answer: &Answer, endpoint: &str, expected: &str, used: u64, tokens: u64) {
    assert_eq!(answer.status, 200, "FR-010 success status for {endpoint}");
    let anthropic = endpoint.starts_with("POST /v1/messages");
    let responses = endpoint == "POST /v1/responses";
    if answer.events.is_empty() {
        assert!(answer.headers["content-type"].starts_with("application/json"));
        let v = answer.json();
        assert_eq!(v["model"], MODEL);
        if anthropic {
            id(&v["id"], "msg_");
            assert_eq!(v["type"], "message");
            assert_eq!(v["role"], "assistant");
            assert_eq!(v["content"], json!([{"type":"text","text":expected}]));
            assert_eq!(v["stop_reason"], "end_turn");
            assert!(v["stop_sequence"].is_null());
            usage(&v["usage"], "input_tokens", "output_tokens", used, tokens);
        } else if responses {
            verify_response(&v, expected, used, tokens);
        } else {
            id(&v["id"], "chatcmpl-");
            assert_eq!(v["object"], "chat.completion");
            assert!(v["created"].is_u64());
            assert_eq!(v["choices"].as_array().unwrap().len(), 1);
            assert_eq!(
                v["choices"][0],
                json!({"index":0,"message":{"role":"assistant","content":expected,"refusal":null},"finish_reason":"stop","logprobs":null})
            );
            usage(
                &v["usage"],
                "prompt_tokens",
                "completion_tokens",
                used,
                tokens,
            );
        }
        return;
    }
    assert!(answer.headers["content-type"].starts_with("text/event-stream"));
    assert_eq!(text(&answer.events), expected, "assembled stream reply");
    if anthropic {
        let values: Vec<Value> = answer
            .events
            .iter()
            .map(|e| serde_json::from_str(&e.data).unwrap())
            .collect();
        let names: Vec<_> = answer.events.iter().map(|e| e.event.as_str()).collect();
        assert_eq!(&names[..2], &["message_start", "content_block_start"]);
        assert_eq!(
            &names[names.len() - 3..],
            &["content_block_stop", "message_delta", "message_stop"]
        );
        assert!(
            names[2..names.len() - 3]
                .iter()
                .all(|n| *n == "content_block_delta")
        );
        assert!(names.len() >= 6);
        for (e, v) in answer.events.iter().zip(&values) {
            assert_eq!(v["type"], e.event);
        }
        let m = &values[0]["message"];
        id(&m["id"], "msg_");
        assert_eq!(m["model"], MODEL);
        assert_eq!(m["type"], "message");
        assert_eq!(m["role"], "assistant");
        assert_eq!(m["content"], json!([]));
        assert!(m["stop_reason"].is_null());
        assert_eq!(values[1]["index"], 0);
        assert_eq!(values[1]["content_block"], json!({"type":"text","text":""}));
        for v in &values[2..values.len() - 3] {
            assert_eq!(v["index"], 0);
            assert_eq!(v["delta"]["type"], "text_delta");
        }
        assert_eq!(values[values.len() - 3]["index"], 0);
        let delta = &values[values.len() - 2];
        assert_eq!(
            delta["delta"],
            json!({"stop_reason":"end_turn","stop_sequence":null})
        );
        usage(
            &delta["usage"],
            "input_tokens",
            "output_tokens",
            used,
            tokens,
        );
    } else if responses {
        let values: Vec<Value> = answer
            .events
            .iter()
            .map(|e| serde_json::from_str(&e.data).unwrap())
            .collect();
        let names: Vec<_> = answer.events.iter().map(|e| e.event.as_str()).collect();
        assert_eq!(
            &names[..4],
            &[
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added"
            ]
        );
        assert_eq!(
            &names[names.len() - 4..],
            &[
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed"
            ]
        );
        assert!(
            names[4..names.len() - 4]
                .iter()
                .all(|n| *n == "response.output_text.delta")
        );
        assert!(names.len() >= 9);
        let response_id = id(&values[0]["response"]["id"], "resp_");
        let item_id = id(&values[2]["item"]["id"], "msg_");
        for (i, (e, v)) in answer.events.iter().zip(&values).enumerate() {
            assert_eq!(v["type"], e.event);
            assert_eq!(v["sequence_number"], i as u64);
        }
        for v in &values[..2] {
            assert_eq!(v["response"]["id"], response_id);
            assert_eq!(v["response"]["status"], "in_progress");
            assert_eq!(v["response"]["output"], json!([]));
            assert!(v["response"]["usage"].is_null());
        }
        assert_eq!(values[2]["output_index"], 0);
        assert_eq!(values[2]["item"]["role"], "assistant");
        assert_eq!(values[2]["item"]["status"], "in_progress");
        for v in &values[3..values.len() - 2] {
            assert_eq!(v["item_id"], item_id);
            assert_eq!(v["output_index"], 0);
            assert_eq!(v["content_index"], 0);
        }
        assert_eq!(values[values.len() - 4]["text"], expected);
        assert_eq!(values[values.len() - 3]["part"]["text"], expected);
        let item = &values[values.len() - 2];
        assert_eq!(item["output_index"], 0);
        assert_eq!(item["item"]["id"], item_id);
        let final_response = &values.last().unwrap()["response"];
        assert_eq!(final_response["id"], response_id);
        assert_eq!(final_response["output"][0], item["item"]);
        verify_response(final_response, expected, used, tokens);
    } else {
        assert_eq!(answer.events.last().unwrap().data, "[DONE]");
        assert_eq!(
            answer.events.iter().filter(|e| e.data == "[DONE]").count(),
            1
        );
        let values: Vec<Value> = answer.events[..answer.events.len() - 1]
            .iter()
            .map(|e| {
                assert_eq!(e.event, "message");
                serde_json::from_str(&e.data).expect("chat chunk JSON")
            })
            .collect();
        let ident = id(&values[0]["id"], "chatcmpl-");
        for v in &values {
            assert_eq!(v["id"], ident);
            assert_eq!(v["object"], "chat.completion.chunk");
            assert_eq!(v["model"], MODEL);
            assert!(v["created"].is_u64());
            assert_eq!(v["created"], values[0]["created"]);
            assert_eq!(v["choices"].as_array().unwrap().len(), 1);
            assert_eq!(v["choices"][0]["index"], 0);
        }
        assert_eq!(values[0]["choices"][0]["delta"]["role"], "assistant");
        for v in &values[..values.len() - 1] {
            assert!(v["choices"][0]["finish_reason"].is_null());
        }
        assert_eq!(
            values.last().unwrap()["choices"][0]["finish_reason"],
            "stop"
        );
        assert_eq!(values.last().unwrap()["choices"][0]["delta"], json!({}));
    }
}
fn verify_response(v: &Value, expected: &str, used: u64, tokens: u64) {
    id(&v["id"], "resp_");
    assert_eq!(v["object"], "response");
    assert_eq!(v["status"], "completed");
    assert_eq!(v["model"], MODEL);
    assert!(v["created_at"].is_u64());
    assert!(v["error"].is_null());
    assert!(v["incomplete_details"].is_null());
    let output = v["output"].as_array().expect("output array");
    assert_eq!(output.len(), 1);
    id(&output[0]["id"], "msg_");
    assert_eq!(output[0]["type"], "message");
    assert_eq!(output[0]["status"], "completed");
    assert_eq!(output[0]["role"], "assistant");
    assert_eq!(
        output[0]["content"],
        json!([{"type":"output_text","text":expected,"annotations":[],"logprobs":[]}])
    );
    usage(&v["usage"], "input_tokens", "output_tokens", used, tokens);
}
