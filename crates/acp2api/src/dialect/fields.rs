use super::content::Api;
use crate::messages::{Message, Refusal};
use serde_json::Value;

pub(super) fn check(api: Api, value: &Value) -> Result<(), Refusal> {
    if value["n"].as_f64().is_some_and(|n| n > 1.0) {
        return Err(reject_value("n", &value["n"]));
    }
    match &value["tool_choice"] {
        Value::String(choice) if !matches!(choice.as_str(), "auto" | "none") => {
            return Err(reject_value("tool_choice", &value["tool_choice"]));
        }
        Value::Object(choice) => {
            if let Some(kind) = choice.get("type").and_then(Value::as_str)
                && !matches!(kind, "auto" | "none")
            {
                return Err(reject_value(
                    "tool_choice.type",
                    &value["tool_choice"]["type"],
                ));
            }
        }
        _ => {}
    }
    for field in ["web_search_options", "previous_response_id", "conversation"] {
        if enabled(&value[field]) {
            return Err(Message::Field(field.into()).into());
        }
    }
    if enabled(&value["output_config"]["format"]) {
        return Err(Message::Field("output_config.format".into()).into());
    }
    for (path, format) in [
        ("response_format.type", &value["response_format"]["type"]),
        ("text.format.type", &value["text"]["format"]["type"]),
    ] {
        if format == "json_schema" {
            return Err(reject_value(path, format));
        }
    }
    for field in ["background", "logprobs"] {
        if enabled(&value[field]) {
            return Err(reject_value(field, &value[field]));
        }
    }
    if value["top_logprobs"].as_f64().is_some_and(|n| n > 0.0) {
        return Err(reject_value("top_logprobs", &value["top_logprobs"]));
    }
    if value["include"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item.as_str().is_some_and(|name| name.contains("logprobs")))
    }) {
        return Err(reject_value("include", &value["include"]));
    }
    if value["modalities"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "audio"))
    {
        return Err(reject_value("modalities", &value["modalities"]));
    }
    if enabled(&value["audio"]) {
        return Err(reject_value("audio", &value["audio"]));
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
                return Err(reject_value(
                    format!("{path}.citations.enabled"),
                    &block["citations"]["enabled"],
                ));
            }
        }
    }
    Ok(())
}
fn reject_value(path: impl Into<String>, value: &Value) -> Refusal {
    Message::FieldValue {
        path: path.into(),
        value: value.clone(),
    }
    .into()
}
