use acp2api_acceptance::{Scenario, Setup, content_check as c};
use serde_json::{Value, json};
const ENDPOINTS: [&str; 3] = [
    "POST /v1/messages",
    "POST /v1/responses",
    "POST /v1/chat/completions",
];
fn input_key(endpoint: &str) -> &'static str {
    if endpoint == "POST /v1/responses" {
        "input"
    } else {
        "messages"
    }
}
fn setup() -> Scenario {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s
}
async fn rejected(
    endpoint: &str,
    part: Value,
    message: &str,
    code: &str,
    param: Option<&str>,
    earlier: bool,
) {
    let mut s = setup();
    let mut call = c::request(endpoint, json!([part]));
    if earlier {
        call.body[input_key(endpoint)]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"user","content":"Last text"}));
    }
    call.body["stream"] = true.into();
    c::send(&mut s, call).await;
    c::refusal(&s, endpoint, message, code, param);
    s.finish(true).await;
}
#[tokio::test]
async fn earlier_content_is_validated_in_all_dialects() {
    for endpoint in ENDPOINTS {
        let kind = if endpoint.ends_with("messages") {
            "document"
        } else if endpoint.ends_with("responses") {
            "input_file"
        } else {
            "file"
        };
        rejected(endpoint,c::document(endpoint,"application/pdf",None),&format!("acp2api: a PDF document in a message (content type {kind}) is not supported. Remove that message from the conversation."),"unsupported_pdf",None,true).await;
        rejected(
            endpoint,
            json!({"type":"unknown_earlier"}),
            "acp2api: the content type unknown_earlier is not supported.",
            "unsupported_content_type",
            None,
            true,
        )
        .await;
    }
}
#[tokio::test]
async fn url_files_and_images_are_rejected_before_prompt() {
    for endpoint in ENDPOINTS {
        for file in [false, true] {
            for scheme in ["http", "https"] {
                let kind = c::url(endpoint, "unused", file)["type"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                rejected(endpoint,c::url(endpoint,&format!("{scheme}://127.0.0.1:9/forbidden"),file),&format!("acp2api: the content type {kind} with a URL is not supported. Send the data as base64."),"unsupported_url",None,true).await;
            }
        }
    }
}
#[tokio::test]
async fn local_url_sink_has_positive_reachability_control() {
    for endpoint in ENDPOINTS {
        for file in [false, true] {
            let mut s = setup();
            c::no_fetch(&mut s, endpoint, file).await;
            let kind = c::url(endpoint, "unused", file)["type"]
                .as_str()
                .unwrap()
                .to_owned();
            c::refusal(
                &s,
                endpoint,
                &format!(
                    "acp2api: the content type {kind} with a URL is not supported. Send the data as base64."
                ),
                "unsupported_url",
                None,
            );
            s.finish(true).await;
        }
    }
}
#[tokio::test]
async fn title_precedence_and_ordered_images_across_dialects() {
    for endpoint in ENDPOINTS {
        let mut s = setup();
        let mut doc = c::document(endpoint, "text/plain", Some("Chosen title"));
        if endpoint.ends_with("completions") {
            doc["file"]["filename"] = "fallback.txt".into();
        } else {
            doc["filename"] = "fallback.txt".into();
        }
        c::send(
            &mut s,
            c::request(
                endpoint,
                json!([
                    c::text(endpoint, "Before"),
                    c::image(endpoint, c::PNG),
                    doc,
                    c::text(endpoint, "After")
                ]),
            ),
        )
        .await;
        assert_eq!(
            s.answer.as_ref().unwrap().status,
            200,
            "accepted image and text document"
        );
        assert_eq!(
            c::blocks(&s),
            vec![
                json!([{"type":"text","text":"Before"},{"type":"image","data":c::PNG,"mimeType":"image/png"},{"type":"text","text":"[document: Chosen title]\nLine one.\n[end of document]"},{"type":"text","text":"After"}])
            ],
            "title precedence and ordered ACP blocks"
        );
        s.finish(true).await;
    }
}
#[tokio::test]
async fn file_id_and_prefill_envelopes_in_all_dialects() {
    for endpoint in ENDPOINTS {
        let part = if endpoint.ends_with("messages") {
            json!({"type":"document","source":{"type":"file","file_id":"file_1"}})
        } else if endpoint.ends_with("responses") {
            json!({"type":"input_file","file_id":"file_1"})
        } else {
            json!({"type":"file","file":{"file_id":"file_1"}})
        };
        rejected(
            endpoint,
            part,
            "acp2api: the request field file_id is not supported.",
            "unsupported_parameter",
            Some("file_id"),
            true,
        )
        .await;
        let mut s = setup();
        let mut call = c::request(endpoint, json!("prefill"));
        call.body[input_key(endpoint)][0]["role"] = "assistant".into();
        c::send(&mut s, call).await;
        c::refusal(
            &s,
            endpoint,
            "acp2api: a last message from the assistant (a prefill) is not supported.",
            "unsupported_prefill",
            None,
        );
        s.finish(true).await;
    }
}
#[tokio::test]
async fn unsupported_media_type_envelope_in_all_dialects() {
    for endpoint in ENDPOINTS {
        rejected(
            endpoint,
            c::document(endpoint, "text/csv", None),
            "acp2api: the media type text/csv is not supported.",
            "unsupported_media_type",
            None,
            false,
        )
        .await;
    }
}
#[tokio::test]
async fn request_body_limit_is_32_mb_with_api_413_envelopes() {
    let message = "acp2api: the request body is larger than 32 MB.";
    for endpoint in ENDPOINTS {
        let mut s = setup();
        let mut call = c::request(endpoint, json!([c::text(endpoint, "Hi")]));
        call.body["padding"] = Value::String("a".repeat(2_200_000));
        c::send(&mut s, call).await;
        assert_eq!(s.answer.as_ref().unwrap().status, 200, "body over 2 MB");
        assert_eq!(
            c::blocks(&s),
            vec![json!([{"type":"text","text":"Hi"}])],
            "prompt of a body over 2 MB"
        );
        s.finish(true).await;

        let mut s = setup();
        let mut call = c::request(endpoint, json!([c::text(endpoint, "Hi")]));
        call.body["padding"] = Value::String("a".repeat(32 * 1024 * 1024));
        c::send(&mut s, call).await;
        let a = s.answer.as_ref().unwrap();
        assert_eq!(a.status, 413, "body over 32 MB");
        let expected = if endpoint.ends_with("messages") {
            json!({"type":"error","error":{"type":"request_too_large","message":message}})
        } else {
            json!({"error":{"message":message,"type":"invalid_request_error","param":null,"code":"request_too_large"}})
        };
        assert_eq!(a.json(), expected, "413 envelope");
        assert!(c::blocks(&s).is_empty(), "413 starts no ACP prompt");
        s.finish(true).await;
    }
}
