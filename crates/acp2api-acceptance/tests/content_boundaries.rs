use acp2api_acceptance::{Scenario, Setup, content_check as c};
use serde_json::{Value, json};
const ENDPOINTS: [&str; 3] = [
    "POST /v1/messages",
    "POST /v1/responses",
    "POST /v1/chat/completions",
];
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
        let key = if endpoint == "POST /v1/responses" {
            "input"
        } else {
            "messages"
        };
        call.body[key]
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
        let key = if endpoint.ends_with("responses") {
            "input"
        } else {
            "messages"
        };
        call.body[key][0]["role"] = "assistant".into();
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
async fn content_parent_lever() {
    let text = std::env::var("ACP2API_LEVER_TEXT").unwrap_or_else(|_| "Fresh lever text".into());
    let image = std::env::var("ACP2API_LEVER_IMAGE").unwrap_or_else(|_| c::PNG.into());
    assert!(
        text.len() <= 4096 && image.len() <= 65536,
        "lever input caps"
    );
    let endpoint = ENDPOINTS[1];
    let mut s = setup();
    c::send(
        &mut s,
        c::request(
            endpoint,
            json!([
                c::text(endpoint, &text),
                c::image(endpoint, &image),
                c::text(endpoint, "After")
            ]),
        ),
    )
    .await;
    assert_eq!(s.answer.as_ref().unwrap().status, 200);
    assert_eq!(
        c::blocks(&s),
        vec![
            json!([{"type":"text","text":text},{"type":"image","data":image,"mimeType":"image/png"},{"type":"text","text":"After"}])
        ]
    );
    println!("ACP blocks: {}", json!(c::blocks(&s)));
    s.finish(true).await;
    let mut s = setup();
    c::no_fetch(&mut s, endpoint, false).await;
    c::refusal(
        &s,
        endpoint,
        "acp2api: the content type input_image with a URL is not supported. Send the data as base64.",
        "unsupported_url",
        None,
    );
    println!("Refusal: {}", s.answer.as_ref().unwrap().json());
    println!("Sink requests: 1 positive control; 0 URL fetches");
    s.finish(true).await;
}
