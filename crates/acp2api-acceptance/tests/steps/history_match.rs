use crate::{World, history_support as history};
use cucumber::{given, then, when};

/// A step holds a text in JSON form, so that it can show each CR, tab, and newline.
fn decoded(text: &str) -> String {
    serde_json::from_str(text).expect("a JSON string in the step")
}

#[given(expr = "an API client uses {string}")]
fn uses(w: &mut World, endpoint: String) {
    history::uses(w, endpoint);
}
#[given("the API client offers the client tools Bash, Edit, Read, and Write")]
fn offers(w: &mut World) {
    for name in ["Bash", "Edit", "Read", "Write"] {
        history::offers(w, name, "Use this tool.");
    }
}
#[given(
    regex = r#"^the API client offers the client tool "([^"]+)" with the description ("(?:[^"\\]|\\.)*")$"#
)]
fn offers_one(w: &mut World, name: String, description: String) {
    history::offers(w, &name, &decoded(&description));
}
#[given(regex = r#"^the API client sets the system prompt ("(?:[^"\\]|\\.)*")$"#)]
fn system(w: &mut World, text: String) {
    history::system(w, decoded(&text));
}
#[given(
    regex = r#"^the API client sent the user message ("(?:[^"\\]|\\.)*"), and the official runtime replied ("(?:[^"\\]|\\.)*")$"#
)]
async fn sent_user(w: &mut World, text: String, reply: String) {
    history::sent_user(w, &decoded(&text), &decoded(&reply)).await;
}
#[given(
    regex = r#"^the official runtime called the client tool "([^"]+)" with the arguments (\{.*\})$"#
)]
async fn called(w: &mut World, name: String, arguments: String) {
    history::called(w, "", &name, &arguments).await;
}
#[given(
    regex = r#"^the official runtime replied ("(?:[^"\\]|\\.)*") and called the client tool "([^"]+)" with the arguments (\{.*\})$"#
)]
async fn replied_and_called(w: &mut World, text: String, name: String, arguments: String) {
    history::called(w, &decoded(&text), &name, &arguments).await;
}
#[given(
    regex = r#"^the API client sent the tool result ("(?:[^"\\]|\\.)*") for that tool call, and the official runtime replied ("(?:[^"\\]|\\.)*")$"#
)]
async fn sent_result(w: &mut World, text: String, reply: String) {
    history::sent_result(w, &decoded(&text), &decoded(&reply)).await;
}
#[when(
    regex = r#"^the API client sends that tool call with the arguments (\{.*\}) and the tool result ("(?:[^"\\]|\\.)*")$"#
)]
async fn return_call(w: &mut World, arguments: String, result: String) {
    history::return_call(w, &arguments, &decoded(&result)).await;
}
#[when(
    regex = r#"^the API client sends that tool call with the call ID "([^"]+)" and the tool result ("(?:[^"\\]|\\.)*")$"#
)]
async fn return_id(w: &mut World, id: String, result: String) {
    history::return_id(w, &id, &decoded(&result)).await;
}
#[when(
    regex = r#"^the API client sends that tool call with the name "([^"]+)" and the tool result ("(?:[^"\\]|\\.)*")$"#
)]
async fn return_name(w: &mut World, name: String, result: String) {
    history::return_name(w, &name, &decoded(&result)).await;
}
#[when(
    regex = r#"^the API client sends that reply text as ("(?:[^"\\]|\\.)*"), that tool call with the arguments (\{.*\}), and the tool result ("(?:[^"\\]|\\.)*")$"#
)]
async fn return_text_and_call(w: &mut World, text: String, arguments: String, result: String) {
    history::return_text_and_call(w, &decoded(&text), &arguments, &decoded(&result)).await;
}
#[when(regex = r#"^the API client sends the next user message ("(?:[^"\\]|\\.)*")$"#)]
async fn next_message(w: &mut World, text: String) {
    history::next_message(w, &decoded(&text)).await;
}
#[when(
    regex = r#"^the API client sends the same conversation with ("(?:[^"\\]|\\.)*") changed to ("(?:[^"\\]|\\.)*") and the new user message ("(?:[^"\\]|\\.)*")$"#
)]
async fn changed_text(w: &mut World, old: String, new: String, next: String) {
    history::changed_text(w, &decoded(&old), &decoded(&new), &decoded(&next)).await;
}
#[when(
    regex = r#"^the API client sends the same conversation with that tool call changed to the arguments (\{.*\}) and the new user message ("(?:[^"\\]|\\.)*")$"#
)]
async fn changed_arguments(w: &mut World, arguments: String, next: String) {
    history::changed_arguments(w, &arguments, &decoded(&next)).await;
}
#[when(
    regex = r#"^the API client sends the same conversation with the call ID "([^"]+)" for that tool call and the new user message ("(?:[^"\\]|\\.)*")$"#
)]
async fn changed_id(w: &mut World, id: String, next: String) {
    history::changed_id(w, &id, &decoded(&next)).await;
}
#[then(
    regex = r#"^the official runtime gets only the tool result ("(?:[^"\\]|\\.)*") in the same ACP session$"#
)]
fn continued_with_result(w: &mut World, text: String) {
    history::continued_with_result(w, &decoded(&text));
}
#[then(
    regex = r#"^the official runtime gets only the prompt ("(?:[^"\\]|\\.)*") in the same ACP session$"#
)]
fn continued_with_prompt(w: &mut World, text: String) {
    history::continued_with_prompt(w, &decoded(&text));
}
#[then("the count of ACP sessions stays the same")]
fn same_count(w: &mut World) {
    history::same_session_count(w);
}
#[then("the request does a rebuild")]
fn rebuilt(w: &mut World) {
    history::rebuilt(w);
}
