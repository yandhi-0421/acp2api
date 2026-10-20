use super::content::Api;
use crate::messages::{Message, Refusal};
use serde_json::Value;

pub(super) fn check(_api: Api, value: &Value) -> Result<(), Refusal> {
    if value["n"].as_f64().is_some_and(|n| n > 1.0) {
        return Err(reject_value("n", &value["n"]));
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
