use crate::World;
use acp2api_acceptance::{content_check, field_check as f};
use cucumber::{given, then, when};
use serde_json::Value;
#[derive(Debug, Default)]
pub struct FieldState {
    pub active: bool,
    endpoint: String,
    baseline: Option<Value>,
    expected: Option<(String, String)>,
    no_key: bool,
    readme: String,
}
async fn send(w: &mut World, endpoint: String, field: String, value: Value) {
    let document = field == "citations of a document";
    w.fields.active = true;
    w.fields.endpoint = endpoint.clone();
    w.fields.baseline = Some(f::control(&endpoint, document).await);
    w.fields.expected = Some(f::expected(&field, &value));
    let mut call = f::request(&endpoint, document);
    f::set(&mut call, &field, value);
    content_check::send(&mut w.scenario, call).await;
}
#[when(
    regex = r#"^an API client sends a valid request to "([^"]+)" with the field "([^"]+)"(?: of (a text block|a tool))? set to (.+)$"#
)]
async fn send_field(
    w: &mut World,
    endpoint: String,
    field: String,
    location: String,
    value: String,
) {
    let field = if location.is_empty() {
        field
    } else {
        format!("{field} of {location}")
    };
    send(w, endpoint, field, serde_json::from_str(&value).unwrap()).await;
}
#[when(
    regex = r#"^an API client sends a valid request to "([^"]+)" with a text document block with "citations" set to (.+)$"#
)]
async fn citations(w: &mut World, endpoint: String, value: String) {
    send(
        w,
        endpoint,
        "citations of a document".into(),
        serde_json::from_str(&value).unwrap(),
    )
    .await;
}
#[then("the official runtime gets the same content as for the same request without that field")]
fn unchanged(w: &mut World) {
    assert_eq!(
        f::content(&w.scenario),
        *w.fields.baseline.as_ref().unwrap(),
        "ignored field leaves actual prompt, system, tools, and config unchanged"
    );
}
pub fn refusal(w: &World, message: &str) {
    let expected = w.fields.expected.as_ref();
    let path = if message == f::JSON_MESSAGE {
        None
    } else {
        Some(expected.unwrap().0.as_str())
    };
    f::refusal(
        &w.scenario,
        &w.fields.endpoint,
        message,
        path,
        if message == f::JSON_MESSAGE {
            "invalid_json"
        } else {
            "unsupported_parameter"
        },
    );
}
pub fn expected_refusal(w: &World) {
    refusal(w, &w.fields.expected.as_ref().unwrap().1);
}
#[then(
    regex = r#"^the error message starts with "(acp2api: the request field )" and names the field "([^"]+)"$"#
)]
fn named(w: &mut World, prefix: String, field: String) {
    expected_refusal(w);
    let message = w.scenario.answer.as_ref().unwrap().json()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(message.starts_with(&prefix) && message.contains(&field));
}
#[when(
    regex = r#"^an API client sends to "([^"]+)" (?:the body "(not JSON)"|a valid request with ([0-9]+) levels of objects and arrays)$"#
)]
async fn body(w: &mut World, endpoint: String, invalid: String, depth: String) {
    w.fields.active = true;
    w.fields.endpoint = endpoint.clone();
    f::control(&endpoint, false).await;
    let bytes = if invalid.is_empty() {
        f::deep_body(&endpoint, depth.parse().unwrap())
    } else {
        invalid.into_bytes()
    };
    w.scenario.answer = Some(f::raw(&mut w.scenario, &endpoint, bytes).await);
}
#[given("the request has no local API key")]
fn no_key(w: &mut World) {
    w.fields.no_key = true;
}
#[given("the request has the field \"output_config.format\"")]
fn schema(w: &mut World) {
    w.fields.expected = Some((
        "output_config.format".into(),
        "acp2api: the request field output_config.format is not supported.".into(),
    ));
}
#[when(expr = "an API client sends a request to {string} with \"stream\" set to false")]
async fn error_request(w: &mut World, endpoint: String) {
    w.fields.active = true;
    w.fields.endpoint = endpoint.clone();
    f::control(&endpoint, false).await;
    let mut call = f::request(&endpoint, false);
    call.body["stream"] = false.into();
    if w.fields.no_key {
        w.scenario.answer = Some(
            w.scenario
                .client()
                .await
                .without_local_api_key()
                .send(call)
                .finish(&mut w.scenario.time)
                .await,
        );
    } else {
        f::set(
            &mut call,
            "output_config.format",
            serde_json::json!({"type":"json_schema","schema":{}}),
        );
        content_check::send(&mut w.scenario, call).await;
    }
}
#[then(expr = "the error body has the \"error.type\" {string}")]
fn error_type(w: &mut World, expected: String) {
    if w.fields.no_key {
        acp2api_acceptance::access_check::key_refusal(
            w.scenario.answer.as_ref().unwrap(),
            &w.fields.endpoint,
        );
        assert!(content_check::blocks(&w.scenario).is_empty());
    } else {
        expected_refusal(w);
    }
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().json()["error"]["type"],
        expected
    );
}
#[given("the README of acp2api")]
fn readme(w: &mut World) {
    w.fields.readme =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md")).unwrap();
}
#[then("the README lists these request fields as fields that acp2api ignores:")]
fn documented(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    for row in step.table.as_ref().unwrap().rows.iter().skip(1) {
        assert!(
            f::readme_has(&w.fields.readme, &row[0]),
            "README must document ignored field: {}",
            row[0]
        );
    }
}

#[then(
    regex = r#"^acp2api refuses the request with HTTP 400 and the message "(acp2api: the request field .+ = ".+" is not supported\.)"$"#
)]
fn quoted_refusal(w: &mut World, message: String) {
    refusal(w, &message);
}

#[then(regex = r"^the field policy probe gets this error message: (.+)$")]
fn probe_refusal(w: &mut World, encoded: String) {
    let message: String = serde_json::from_str(&encoded).unwrap();
    refusal(w, &message);
}
