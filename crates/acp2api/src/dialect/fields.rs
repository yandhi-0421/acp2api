use super::content::Api;
use crate::messages::{Message, Refusal};
use serde_json::Value;

pub(super) fn check(_api: Api, value: &Value) -> Result<(), Refusal> {
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
    Ok(())
}
fn reject_value(path: impl Into<String>, value: &Value) -> Refusal {
    Message::FieldValue {
        path: path.into(),
        value: value.clone(),
    }
    .into()
}
