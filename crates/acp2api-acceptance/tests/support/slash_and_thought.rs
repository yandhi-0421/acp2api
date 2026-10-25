use acp2api_acceptance::{client::Answer, endpoint_check};
use serde_json::Value;

pub fn reply(answer: &Answer) -> String {
    if !answer.events.is_empty() {
        return endpoint_check::text(&answer.events);
    }
    let body = answer.json();
    if let Some(content) = body["choices"][0]["message"]["content"].as_str() {
        content.into()
    } else {
        let content = body["content"]
            .as_array()
            .or_else(|| body["output"][0]["content"].as_array())
            .expect("reply content");
        content
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect()
    }
}

pub fn no_thought_shape(value: &Value) {
    match value {
        Value::Object(fields) => {
            assert!(
                !fields.contains_key("reasoning_content"),
                "FR-018 no reasoning_content: {value}"
            );
            assert!(
                !matches!(
                    fields.get("type").and_then(Value::as_str),
                    Some("thinking" | "redacted_thinking" | "reasoning")
                ),
                "FR-018 no thought block or item: {value}"
            );
            for field in fields.values() {
                no_thought_shape(field);
            }
        }
        Value::Array(values) => {
            for value in values {
                no_thought_shape(value);
            }
        }
        _ => {}
    }
}

pub fn thought_input(
    endpoint: &str,
    kind: &str,
    text: &str,
) -> acp2api_acceptance::client::ApiCall {
    let mut call = endpoint_check::request(endpoint, "Continue.", false);
    call.body[if endpoint == "POST /v1/responses" {
        "input"
    } else {
        "messages"
    }] = match kind {
        "a thinking block in an assistant message" => serde_json::json!([
            {"role":"assistant","content":[{"type":"thinking","thinking":text,"signature":"synthetic-signature"},{"type":"text","text":"Earlier reply."}]},
            {"role":"user","content":"Continue."}
        ]),
        "a redacted_thinking block in an assistant message" => serde_json::json!([
            {"role":"assistant","content":[{"type":"redacted_thinking","data":text},{"type":"text","text":"Earlier reply."}]},
            {"role":"user","content":"Continue."}
        ]),
        "a reasoning item" => serde_json::json!([
            {"type":"reasoning","id":"synthetic-reasoning","summary":[{"type":"summary_text","text":text}]},
            {"role":"user","content":"Continue."}
        ]),
        "reasoning_content in an assistant message" => serde_json::json!([
            {"role":"assistant","content":"Earlier reply.","reasoning_content":text},
            {"role":"user","content":"Continue."}
        ]),
        _ => unreachable!("registered thought input kind"),
    };
    call
}
