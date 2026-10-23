use acp2api_acceptance::{Scenario, Setup, content_check as c, field_check as f};
use serde_json::{Value, json};
const CHAT: &str = "POST /v1/chat/completions";
const RESPONSES: &str = "POST /v1/responses";
const MESSAGES: &str = "POST /v1/messages";

async fn candidate(endpoint: &str, field: &str, value: Value) -> Scenario {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut call = f::request(endpoint, false);
    f::set(&mut call, field, value);
    c::send(&mut s, call).await;
    s
}
#[tokio::test]
async fn forced_named_choices_and_mixed_include_keep_exact_m02_paths_and_values() {
    for endpoint in [CHAT, RESPONSES, MESSAGES] {
        f::control(endpoint, false).await;
    }
    for (endpoint, field, value, path, label) in [
        (
            CHAT,
            "tool_choice",
            json!("any"),
            "tool_choice",
            "tool_choice = \"any\"",
        ),
        (
            CHAT,
            "tool_choice",
            json!("read_file"),
            "tool_choice",
            "tool_choice = \"read_file\"",
        ),
        (
            MESSAGES,
            "tool_choice",
            json!({"type":"tool","name":"read_file"}),
            "tool_choice.type",
            "tool_choice.type = \"tool\"",
        ),
        (
            RESPONSES,
            "tool_choice",
            json!({"type":"function","name":"read_file"}),
            "tool_choice.type",
            "tool_choice.type = \"function\"",
        ),
        (
            RESPONSES,
            "include",
            json!([
                "reasoning.encrypted_content",
                "message.output_text.logprobs"
            ]),
            "include",
            "include = [\"reasoning.encrypted_content\",\"message.output_text.logprobs\"]",
        ),
    ] {
        let mut s = candidate(endpoint, field, value).await;
        eprintln!("reject {label} on {endpoint}");
        c::refusal(
            &s,
            endpoint,
            &format!("acp2api: the request field {label} is not supported."),
            "unsupported_parameter",
            Some(path),
        );
        s.finish(true).await;
    }
}
#[tokio::test]
async fn store_each_json_kind_and_other_disabled_fields_preserve_all_runtime_content() {
    let mut observed = Vec::new();
    let mut expected = Vec::new();
    for endpoint in [CHAT, RESPONSES, MESSAGES] {
        let baseline = f::control(endpoint, false).await;
        let mut cases: Vec<(&str, Value)> = match endpoint {
            CHAT => vec![
                ("n", json!(1)),
                ("top_logprobs", json!(0)),
                ("modalities", json!(["text"])),
                ("web_search_options", Value::Null),
                ("response_format", json!({"type":"text"})),
            ],
            RESPONSES => vec![
                ("previous_response_id", Value::Null),
                ("conversation", Value::Null),
                ("include", json!([])),
            ],
            _ => vec![("output_config.format", Value::Null)],
        };
        if endpoint == RESPONSES {
            cases.extend(
                [
                    Value::Null,
                    json!(true),
                    json!(false),
                    json!(0),
                    json!(0.25),
                    json!("retained"),
                    json!([]),
                    json!({}),
                    json!({"nested":[false]}),
                ]
                .into_iter()
                .map(|value| ("store", value)),
            );
        }
        for (field, value) in cases {
            let mut s = candidate(endpoint, field, value).await;
            assert_eq!(
                s.answer.as_ref().unwrap().status,
                200,
                "disabled/ignored field {field} accepted"
            );
            observed.push(f::content(&s));
            expected.push(baseline.clone());
            s.finish(true).await;
        }
    }
    assert_eq!(
        observed, expected,
        "each JSON kind of store and each disabled field preserve the literal runtime control"
    );
}
#[tokio::test]
async fn raw_json_grammar_depth_and_shape_remain_distinct_on_each_api() {
    for endpoint in [MESSAGES, CHAT, RESPONSES] {
        f::control(endpoint, false).await;
        let valid = serde_json::to_vec(&f::request(endpoint, false).body).unwrap();
        for bytes in [
            b"not JSON".to_vec(),
            [valid.as_slice(), b" true"].concat(),
            [valid.as_slice(), b" {}"].concat(),
            b"{\"model\":]".to_vec(),
            f::deep_body(endpoint, 129),
        ] {
            let mut s = Scenario::default();
            s.plan(Setup::normal());
            s.answer = Some(f::raw(&mut s, endpoint, bytes).await);
            c::refusal(&s, endpoint, f::JSON_MESSAGE, "invalid_json", None);
            s.finish(true).await;
        }
        for bytes in [
            b"null".to_vec(),
            b"[]".to_vec(),
            b"{\"model\":\"gemini-3.8-flash\"}".to_vec(),
        ] {
            let mut s = Scenario::default();
            s.plan(Setup::normal());
            let answer = f::raw(&mut s, endpoint, bytes).await;
            assert_eq!(
                answer.status, 400,
                "valid JSON with an invalid request shape"
            );
            assert_ne!(
                answer.json()["error"]["message"],
                f::JSON_MESSAGE,
                "shape errors remain distinct from M-07"
            );
            assert_eq!(answer.json()["error"]["type"], "invalid_request_error");
            if endpoint != MESSAGES {
                assert_eq!(answer.json()["error"]["code"], "invalid_request_error");
            }
            assert!(c::blocks(&s).is_empty());
            s.finish(true).await;
        }
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let answer = f::raw(&mut s, endpoint, f::deep_body(endpoint, 128)).await;
        assert_eq!(
            answer.status, 200,
            "exact root-inclusive depth 128 accepted on each API"
        );
        assert_eq!(
            c::blocks(&s),
            vec![json!([{"type":"text","text":f::PROMPT}])]
        );
        s.finish(true).await;
        let mut call = f::request(endpoint, false);
        call.body["escaped_probe"] = json!(format!(
            "{}\\\"{}\\\\{}",
            "[".repeat(140),
            "}".repeat(140),
            "[".repeat(140)
        ));
        let mut bytes = serde_json::to_vec(&call.body).unwrap();
        bytes.extend_from_slice(b" \n\t");
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let answer = f::raw(&mut s, endpoint, bytes).await;
        assert_eq!(
            answer.status, 200,
            "escaped delimiters inside a string do not count as containers; trailing whitespace is allowed"
        );
        s.finish(true).await;
    }
}
