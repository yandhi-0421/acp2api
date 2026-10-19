use crate::{
    Scenario,
    client::{Answer, ApiCall},
    content_check as c,
};
use serde_json::{Value, json};
pub fn list() -> ApiCall {
    ApiCall {
        method: "GET".into(),
        path: "/v1/models".into(),
        body: Value::Null,
        headers: Vec::new(),
    }
}
pub fn membership(a: &Answer, expected: &[&str]) {
    assert_eq!(a.status, 200, "FR-012 model-list status");
    let value = a.json();
    assert_eq!(value["object"], "list");
    let mut actual: Vec<_> = value["data"]
        .as_array()
        .expect("model entries")
        .iter()
        .map(|e| e["id"].as_str().expect("model name"))
        .collect();
    let mut expected = expected.to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "exact offered model membership without duplicates"
    );
}
pub fn fields(a: &Answer, created: u64, stamp: &str) {
    assert_eq!(a.status, 200);
    let v = a.json();
    let entries = v["data"].as_array().unwrap();
    assert!(!entries.is_empty());
    for e in entries {
        for k in [
            "id",
            "object",
            "type",
            "display_name",
            "created_at",
            "owned_by",
        ] {
            assert!(e[k].is_string(), "string model-list field {k}");
        }
        assert!(e["created"].is_u64());
        assert_eq!(e["created"], created);
        assert_eq!(e["created_at"], stamp);
        assert_eq!(e["object"], "model");
        assert_eq!(e["type"], "model");
        assert!(!e["owned_by"].as_str().unwrap().is_empty());
    }
}
pub fn refusal(s: &Scenario, endpoint: &str, model: &str, names: &[&str]) {
    c::refusal(
        s,
        endpoint,
        &format!(
            "acp2api: the model name {model} is not available. The model names are: {}.",
            names.join(", ")
        ),
        "model_not_found",
        Some("model"),
    );
    let records = crate::journal_check::records(s);
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0]["request_id"],
        s.answer.as_ref().unwrap().headers["x-request-id"]
    );
    assert_eq!(records[0]["error_code"], "model_not_found");
    assert_eq!(records[0]["model"], model);
    for k in ["session", "session_use", "input_tokens", "output_tokens"] {
        assert_eq!(records[0][k], json!(null));
    }
}
