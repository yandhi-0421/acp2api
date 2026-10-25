use super::content::Api;
use crate::messages::{Message, Refusal};
use serde::Deserialize;
use serde_json::Value;

const MAX_DEPTH: usize = 128;
pub(super) fn parse_json(body: &[u8]) -> Result<Value, Refusal> {
    if !nesting_within_max_depth(body) {
        return Err(Message::InvalidJson.into());
    }
    let mut deserializer = serde_json::Deserializer::from_slice(body);
    deserializer.disable_recursion_limit();
    let value = Value::deserialize(&mut deserializer).map_err(|_| Message::InvalidJson)?;
    deserializer.end().map_err(|_| Message::InvalidJson)?;
    Ok(value)
}
pub(crate) fn request_model(body: &[u8]) -> Option<String> {
    let value = parse_json(body).ok()?;
    value.get("model")?.as_str().map(str::to_owned)
}
fn nesting_within_max_depth(body: &[u8]) -> bool {
    let mut depth = 0_usize;
    let mut quoted = false;
    let mut escaped = false;
    for byte in body {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return false;
                    }
                }
                b'}' | b']' => match depth.checked_sub(1) {
                    Some(outer) => depth = outer,
                    None => return false,
                },
                _ => {}
            }
        }
    }
    true
}

pub(super) fn check(api: Api, value: &Value) -> Result<(), Refusal> {
    if value["n"].as_f64().is_some_and(|n| n > 1.0) {
        return Err(Refusal::field("n", Some(&value["n"])));
    }
    match &value["tool_choice"] {
        Value::Null => {}
        Value::String(choice) if matches!(choice.as_str(), "auto" | "none") => {}
        Value::Object(choice) => match choice.get("type").and_then(Value::as_str) {
            Some("auto" | "none") => {}
            Some(_) => {
                return Err(Refusal::field(
                    "tool_choice.type",
                    Some(&value["tool_choice"]["type"]),
                ));
            }
            None => {
                return Err(Refusal::field("tool_choice", Some(&value["tool_choice"])));
            }
        },
        _ => {
            return Err(Refusal::field("tool_choice", Some(&value["tool_choice"])));
        }
    }
    for field in ["web_search_options", "previous_response_id", "conversation"] {
        if enabled(&value[field]) {
            return Err(Refusal::field(field, None));
        }
    }
    if enabled(&value["output_config"]["format"]) {
        return Err(Refusal::field("output_config.format", None));
    }
    for (path, format) in [
        ("response_format.type", &value["response_format"]["type"]),
        ("text.format.type", &value["text"]["format"]["type"]),
    ] {
        if format == "json_schema" {
            return Err(Refusal::field(path, Some(format)));
        }
    }
    for field in ["background", "logprobs"] {
        if enabled(&value[field]) {
            return Err(Refusal::field(field, Some(&value[field])));
        }
    }
    if value["top_logprobs"].as_f64().is_some_and(|n| n > 0.0) {
        return Err(Refusal::field("top_logprobs", Some(&value["top_logprobs"])));
    }
    if value["include"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item.as_str().is_some_and(|name| name.contains("logprobs")))
    }) {
        return Err(Refusal::field("include", Some(&value["include"])));
    }
    if value["modalities"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "audio"))
    {
        return Err(Refusal::field("modalities", Some(&value["modalities"])));
    }
    if enabled(&value["audio"]) {
        return Err(Refusal::field("audio", Some(&value["audio"])));
    }
    if matches!(api, Api::Messages)
        && let Some(messages) = value["messages"].as_array()
    {
        for (index, message) in messages.iter().enumerate() {
            citations(&message["content"], &format!("messages[{index}].content"))?;
        }
    }
    Ok(())
}
fn enabled(value: &Value) -> bool {
    !value.is_null() && value != false
}
fn citations(content: &Value, path: &str) -> Result<(), Refusal> {
    if let Some(blocks) = content.as_array() {
        for (index, block) in blocks.iter().enumerate() {
            let path = format!("{path}[{index}]");
            if block["type"] == "document" && block["citations"]["enabled"] == true {
                return Err(Refusal::field(
                    format!("{path}.citations.enabled"),
                    Some(&block["citations"]["enabled"]),
                ));
            }
        }
    }
    Ok(())
}
