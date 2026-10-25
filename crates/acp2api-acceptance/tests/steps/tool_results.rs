use crate::{World, tool_result_support::long_arguments};
use acp2api_acceptance::{content_check, conversation_check as c, tool_check};
use cucumber::{given, then, when};
use serde_json::{Value, json};

async fn send(w: &mut World) {
    let call = w.tool_results.request();
    w.tool_results.prompt_count = c::prompts(&w.scenario).len();
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
}

async fn model_call(w: &mut World, endpoint: String, name: String, arguments: String) {
    w.tool_results.endpoint = endpoint.clone();
    w.tool_results.tools = vec![tool_check::tool(&name)];
    w.tool_results.messages = vec![c::message("user", None, "Use an offered tool.")];
    w.tool_results.name = name.clone();
    w.tool_results.arguments = arguments.clone();
    w.request.output_call = Some((name, arguments));
    send(w).await;
    w.request.output_call = None;
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(answer.status, 200, "seed model tool call reaches HTTP");
    let body = answer.json();
    let calls = tool_check::calls(answer, &endpoint);
    assert_eq!(calls.len(), 1, "seed model actually returns one tool call");
    w.tool_results.id = calls[0]["id"].as_str().unwrap().into();
    let assistant = if endpoint == "POST /v1/messages" {
        json!({"role":"assistant","content":body["content"]})
    } else {
        json!({"role":"assistant","content":"","tool_calls":body["choices"][0]["message"]["tool_calls"]})
    };
    w.tool_results.messages.push(assistant);
}

#[given(
    regex = r#"^the model called "([^"]+)" with the arguments (\{.*\}) in a request to "([^"]+)"$"#
)]
async fn called(w: &mut World, name: String, arguments: String, endpoint: String) {
    model_call(w, endpoint, name, arguments).await;
}

#[given(expr = "the model called {string} in a request to {string}")]
async fn screenshot(w: &mut World, name: String, endpoint: String) {
    model_call(w, endpoint, name, r#"{"path":"screen.png"}"#.into()).await;
}

#[given(
    regex = r#"^the model called "([^"]+)" with arguments of ([0-9,]+) characters in a request to "([^"]+)"$"#
)]
async fn long_call(w: &mut World, name: String, length: String, endpoint: String) {
    model_call(
        w,
        endpoint,
        name,
        long_arguments(length.replace(',', "").parse().unwrap()),
    )
    .await;
}

#[when(expr = "the API client sends the tool result {string} for that tool call")]
#[when(expr = "the request has the tool result {string} for that tool call")]
async fn result(w: &mut World, text: String) {
    w.tool_results.result(json!(text), false);
    send(w).await;
}

#[when(expr = "the API client sends an error tool result {string} for that tool call")]
async fn error(w: &mut World, text: String) {
    w.tool_results.result(json!(text), true);
    send(w).await;
}

#[then("the first line of the tool result in the prompt is:")]
fn line_docstring(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    first_line(w, step.docstring.as_ref().unwrap().trim_matches('\n'));
}

fn first_line(w: &World, expected: &str) {
    assert_eq!(
        w.tool_results.first_line(&w.scenario).as_deref(),
        Some(expected),
        "FR-023 tool-result first line; HTTP response: {:?}",
        c::response(&w.scenario)
    );
}

#[then(regex = r#"^the first line of the tool result in the prompt is "(.*)"$"#)]
fn line(w: &mut World, expected: String) {
    first_line(w, &expected);
}

#[when(
    expr = "the API client sends a tool result with the text {string}, an image, and the text {string}"
)]
async fn image(w: &mut World, before: String, after: String) {
    w.tool_results.result(
        json!([
            content_check::text(&w.tool_results.endpoint, &before),
            content_check::image(&w.tool_results.endpoint, content_check::PNG),
            content_check::text(&w.tool_results.endpoint, &after)
        ]),
        false,
    );
    send(w).await;
}

#[then(
    regex = r#"^the first line of the tool result in the prompt holds (all 1,000 characters of the arguments, and no mark|only the first 1,000 characters of the arguments, and then the mark " \[arguments cut\]" before the colon)$"#
)]
fn truncated(w: &mut World, part: String) {
    let name = &w.tool_results.name;
    let arguments = &w.tool_results.arguments;
    let expected = if part.starts_with("all") {
        format!("Result of your tool call {name} {arguments}:")
    } else {
        format!(
            "Result of your tool call {name} {} [arguments cut]:",
            arguments.chars().take(1000).collect::<String>()
        )
    };
    first_line(w, &expected);
}

#[given(
    regex = r#"^a request to "([^"]+)" has an earlier tool call of "([^"]+)" with (the JSON string .* as arguments|the input object .*)$"#
)]
fn historical(w: &mut World, endpoint: String, name: String, description: String) {
    w.tool_results.endpoint = endpoint.clone();
    w.tool_results.id = "earlier-call".into();
    w.tool_results.name = name.clone();
    let arguments = description
        .strip_prefix("the input object ")
        .or_else(|| {
            description
                .strip_prefix("the JSON string ")
                .and_then(|text| text.strip_suffix(" as arguments"))
        })
        .unwrap();
    let assistant = if endpoint == "POST /v1/messages" {
        json!({"role":"assistant","content":[{"type":"tool_use","id":"earlier-call","name":name,"input":serde_json::from_str::<Value>(arguments).unwrap()}]})
    } else {
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"earlier-call","type":"function","function":{"name":name,"arguments":arguments}}]})
    };
    w.tool_results.messages = vec![c::message("user", None, "Read the file."), assistant];
}

#[given(expr = "the conversation has no tool call with the ID {string}")]
fn unknown(w: &mut World, id: String) {
    w.tool_results.id = id;
}

#[when(
    expr = "an API client sends a request to {string} with a tool result for {string} with the text {string}"
)]
async fn unknown_result(w: &mut World, endpoint: String, id: String, text: String) {
    assert_eq!(w.tool_results.id, id);
    w.tool_results.endpoint = endpoint;
    w.tool_results.messages = vec![c::message("user", None, "Read the file.")];
    w.tool_results.result(json!(text), false);
    send(w).await;
}

#[given(
    expr = "an earlier tool call of the conversation called {string} with arguments of 5,000 characters"
)]
async fn earlier_long_call(w: &mut World, name: String) {
    model_call(
        w,
        "POST /v1/chat/completions".into(),
        name,
        long_arguments(5000),
    )
    .await;
}

#[when(expr = "the API client sends a request of that conversation that does a rebuild")]
async fn rebuild(w: &mut World) {
    w.tool_results.messages[0]["content"] = json!("Edited request: use an offered tool.");
    w.tool_results
        .messages
        .push(c::message("user", None, "Continue."));
    let previous = c::session(&w.scenario);
    send(w).await;
    if let Some(prompt) = c::prompts(&w.scenario).get(w.tool_results.prompt_count) {
        assert_ne!(
            prompt["sessionId"], previous,
            "request selects a new ACP session"
        );
    }
}

#[then(
    expr = "the first prompt of the new ACP session holds a sentence that starts with {string} and ends with {string}"
)]
fn sentence(w: &mut World, start: String, end: String) {
    let text = w.tool_results.text(&w.scenario);
    assert!(
        text.as_ref().is_some_and(|text| text
            .lines()
            .any(|line| line.starts_with(&start) && line.ends_with(&end))),
        "FR-023 rebuild tool-call sentence; actual prompt: {text:?}; HTTP response: {:?}",
        c::response(&w.scenario)
    );
}

#[then("the arguments in that sentence have all 5,000 characters")]
fn full_arguments(w: &mut World) {
    let sentence = format!(
        "You called the tool {} with the arguments {}.",
        w.tool_results.name, w.tool_results.arguments
    );
    assert!(
        w.tool_results
            .text(&w.scenario)
            .is_some_and(|text| text.lines().any(|line| line == sentence)),
        "FR-023 rebuild retains all 5,000 argument characters"
    );
}

#[given(
    regex = r#"^the model called "([^"]+)" with the arguments (\{.*\}), and the tool result "([^"]+)" went into the continued ACP session$"#
)]
async fn continued_result(w: &mut World, name: String, arguments: String, result: String) {
    model_call(w, "POST /v1/chat/completions".into(), name, arguments).await;
    let session = c::session(&w.scenario);
    w.tool_results.result(json!(result), false);
    send(w).await;
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().status,
        200,
        "setup requires a tool result in a continued ACP session"
    );
    assert_eq!(
        c::session(&w.scenario),
        session,
        "setup continues the actual ACP session"
    );
    w.tool_results
        .messages
        .push(w.scenario.answer.as_ref().unwrap().json()["choices"][0]["message"].clone());
}

#[then("the first prompt of the new ACP session has these lines in this order:")]
fn rebuild_lines(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    let expected = step.table.as_ref().unwrap().rows[1..]
        .iter()
        .map(|row| row[0].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        w.tool_results.text(&w.scenario).is_some_and(|text| text
            .lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|lines| lines.join("\n") == expected)),
        "FR-023 rebuilt tool-result lines retain order and adjacency"
    );
}

#[given(
    regex = r#"^an API client sent a request to "([^"]+)" with the client tool "([^"]+)", and the model called "([^"]+)" with the arguments (\{.*\})$"#
)]
async fn role_call(w: &mut World, endpoint: String, tool: String, name: String, arguments: String) {
    assert_eq!(tool, name);
    model_call(w, endpoint, name, arguments).await;
}

#[when(
    expr = "the API client sends the next request of that conversation with the tool result {string} and then the system message {string}"
)]
async fn late_system(w: &mut World, result: String, text: String) {
    w.tool_results.result(json!(result), false);
    w.tool_results
        .messages
        .push(c::message("system", None, &text));
    send(w).await;
}

#[when(
    expr = "an API client sends a request to {string} with a tool result that holds the text {string} and a PDF document"
)]
async fn pdf(w: &mut World, endpoint: String, text: String) {
    w.tool_results.endpoint = endpoint.clone();
    w.tool_results.id = "pdf-call".into();
    w.tool_results.result_text = text.clone();
    w.tool_results.messages = vec![
        c::message("user", None, "Read the PDF."),
        json!({"role":"assistant","content":[{"type":"tool_use","id":"pdf-call","name":"read_file","input":{"path":"a.pdf"}}]}),
    ];
    let pdf = json!({"type":"document","source":{"type":"base64","media_type":"application/pdf","data":"JVBERi0xLjQKJSVFT0YK"}});
    w.tool_results
        .result(json!([content_check::text(&endpoint, &text), pdf]), false);
    send(w).await;
}

#[then(expr = "the tool result in the prompt holds the text {string}")]
fn retained_text(w: &mut World, expected: String) {
    assert!(
        w.tool_results
            .text(&w.scenario)
            .is_some_and(|text| text.lines().any(|line| line == expected)),
        "FR-017 tool result retains surrounding text"
    );
}

#[then(expr = "the tool result in the prompt holds the line {string} at the place of the PDF")]
fn pdf_note(w: &mut World, note: String) {
    let expected = format!("{}\n{note}", w.tool_results.result_text);
    assert!(
        w.tool_results
            .text(&w.scenario)
            .is_some_and(|text| text.contains(&expected)),
        "FR-017 PDF note follows the unchanged tool-result text"
    );
}
