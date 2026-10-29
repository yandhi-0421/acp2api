use crate::World;
use acp2api_acceptance::content_check as c;
use cucumber::{then, when};
use serde_json::{Value, json};
#[derive(Debug, Default)]
pub struct ContentState {
    endpoint: String,
}
async fn send(w: &mut World, endpoint: String, parts: Value) {
    w.content.endpoint = endpoint.clone();
    c::send(&mut w.scenario, c::request(&endpoint, parts)).await;
}
#[when(regex = r#"^an API client sends a request to "([^"]+)" with the input (\[.*\])$"#)]
async fn input_json(w: &mut World, endpoint: String, input: String) {
    w.content.endpoint = endpoint.clone();
    let mut call = c::request(&endpoint, json!(""));
    call.body["input"] = serde_json::from_str(&input).unwrap();
    c::send(&mut w.scenario, call).await;
}
#[when(
    expr = "an API client sends a request to {string} with one user message with these content blocks:"
)]
async fn blocks(w: &mut World, endpoint: String, #[step] step: &cucumber::gherkin::Step) {
    let parts: Vec<_> = step
        .table
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .skip(1)
        .map(|r| {
            if r[0] == "text" {
                c::text(&endpoint, &r[1])
            } else {
                c::image(&endpoint, c::PNG)
            }
        })
        .collect();
    send(w, endpoint, json!(parts)).await;
}
#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with (an image block|an image_url part|an input_image item) with the URL "([^"]+)"$"#
)]
async fn remote_url(w: &mut World, endpoint: String, _block: String, url: String) {
    let p = c::url(&endpoint, &url, false);
    send(w, endpoint, json!([p])).await;
}
#[when(
    expr = "an API client sends a request to {string} with an image_url part with a {string} URL"
)]
async fn data_url(w: &mut World, endpoint: String, _url: String) {
    let p = c::image(&endpoint, c::PNG);
    send(w, endpoint, json!([p])).await;
}
#[when(
    expr = "an API client sends a request to {string} with a content block of the type {string}"
)]
async fn unknown(w: &mut World, endpoint: String, kind: String) {
    let mut call = c::request(&endpoint, json!([{"type":kind}]));
    if kind == "item_reference" {
        call.body["input"] = json!([{"type":kind,"id":"item_1"}]);
    }
    w.content.endpoint = endpoint;
    c::send(&mut w.scenario, call).await;
}
#[when(
    expr = "an API client sends a request to {string} with an input_file item with the \"file_id\" {string}"
)]
async fn file_id(w: &mut World, endpoint: String, id: String) {
    send(w, endpoint, json!([{"type":"input_file","file_id":id}])).await;
}
#[when(
    expr = "an API client sends a request to {string} whose last message has the role {string} and the text {string}"
)]
async fn assistant(w: &mut World, endpoint: String, role: String, text: String) {
    let mut call = c::request(&endpoint, json!(text));
    call.body["messages"][0]["role"] = role.into();
    w.content.endpoint = endpoint;
    c::send(&mut w.scenario, call).await;
}
#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with a PDF document in (the last user message|an earlier user message)$"#
)]
async fn pdf(w: &mut World, endpoint: String, position: String) {
    let mut call = c::request(
        &endpoint,
        json!([c::document(&endpoint, "application/pdf", None)]),
    );
    if position == "an earlier user message" {
        call.body["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"user","content":"Later"}));
    }
    w.content.endpoint = endpoint;
    c::send(&mut w.scenario, call).await;
}
#[when(
    regex = r#"^an API client sends a request to "([^"]+)" with (a document block with .+|an input_file item with .+) with the text "([^"]+)"$"#
)]
async fn document(w: &mut World, endpoint: String, description: String, text: String) {
    let mut doc = if description.contains("source type \"text\"") {
        json!({"type":"document","title":"Notes","source":{"type":"text","media_type":"text/plain","data":text}})
    } else if description.contains("source type \"content\"") {
        json!({"type":"document","source":{"type":"content","content":[{"type":"text","text":text}]}})
    } else {
        c::document(&endpoint, "text/plain", None)
    };
    if description.contains("filename \"notes.txt\"") {
        doc["filename"] = "notes.txt".into();
    }
    send(
        w,
        endpoint.clone(),
        json!([
            c::text(&endpoint, "Before"),
            doc,
            c::text(&endpoint, "After")
        ]),
    )
    .await;
}
#[when(
    expr = "an API client sends a request to {string} with an input_file item with the media type {string}"
)]
async fn unsupported_media(w: &mut World, endpoint: String, media: String) {
    let doc = c::document(&endpoint, &media, None);
    send(w, endpoint, json!([doc])).await;
}
#[then(expr = "acp2api refuses the request with HTTP 400 and the message {string}")]
fn refusal(w: &mut World, message: String) {
    if w.fields.active {
        crate::request_fields::refusal(w, &message);
        return;
    }
    if w.server_tools.endpoint.is_some() {
        crate::server_tools::refusal(w, &message);
        return;
    }
    if message.starts_with("acp2api: the model name ") {
        let expected = message.replace("<names>", &crate::model_names::NAMES.join(", "));
        c::refusal(
            &w.scenario,
            &w.models.endpoint,
            &expected,
            "model_not_found",
            Some("model"),
        );
        return;
    }
    if message.contains("tool tool_search") || message.contains("client tool name") {
        c::refusal(
            &w.scenario,
            &w.tools.endpoint,
            &message,
            if message.contains("tool tool_search") {
                "unsupported_tool"
            } else {
                "unsupported_tool_name"
            },
            None,
        );
        return;
    }
    let (code, param) = if message.contains("request field file_id") {
        ("unsupported_parameter", Some("file_id"))
    } else if message.contains("with a URL") {
        ("unsupported_url", None)
    } else if message.contains("a PDF document") {
        ("unsupported_pdf", None)
    } else if message.contains("media type") {
        ("unsupported_media_type", None)
    } else if message.contains("prefill") {
        ("unsupported_prefill", None)
    } else {
        ("unsupported_content_type", None)
    };
    c::refusal(&w.scenario, &w.content.endpoint, &message, code, param);
}
#[then("the official runtime gets no prompt")]
fn no_prompt(w: &mut World) {
    let (before, _) = w.scenario.hub.active_request();
    assert_eq!(
        w.scenario.fake().record().prompts().len(),
        before,
        "no ACP prompt after the active request"
    );
}
#[then(expr = "acp2api sends no HTTP request to {string}")]
async fn no_fetch(w: &mut World, host: String) {
    assert_eq!(host, "example.com");
    c::no_fetch(&mut w.scenario, &w.content.endpoint, false).await;
}
#[then(
    regex = r#"^the prompt that the official runtime gets has the text "(Look at this:)", the image, and the text "(What is it\?)" in this order$"#
)]
fn ordered(w: &mut World, first: String, last: String) {
    assert_eq!(
        c::blocks(&w.scenario),
        vec![
            json!([{"type":"text","text":first},{"type":"image","data":c::PNG,"mimeType":"image/png"},{"type":"text","text":last}])
        ],
        "ordered ACP image bytes and media type"
    );
}
#[then("the prompt that the official runtime gets holds the image")]
fn holds(w: &mut World) {
    assert_eq!(
        c::blocks(&w.scenario),
        vec![json!([{"type":"image","data":c::PNG,"mimeType":"image/png"}])],
        "data URL image bytes and media type"
    );
}
#[then("the prompt that the official runtime gets holds at the place of the document:")]
fn document_place(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    assert_eq!(
        c::blocks(&w.scenario),
        vec![
            json!([{"type":"text","text":"Before"},{"type":"text","text":step.docstring.as_ref().unwrap().trim_matches('\n')},{"type":"text","text":"After"}])
        ],
        "document delimiters and original position"
    );
}
