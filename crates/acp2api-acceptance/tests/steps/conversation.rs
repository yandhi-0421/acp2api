use crate::World;
use acp2api_acceptance::{content_check, conversation_check as c, journal_check, system_check};
use cucumber::{given, then, when};
use serde_json::{Value, json};
#[derive(Default)]
pub struct ConversationState {
    folder_markers: Vec<std::path::PathBuf>,
    session_files: Vec<std::path::PathBuf>,
    system: Option<String>,
    initial: Vec<Value>,
    initial_headers: Vec<(String, String)>,
    initial_cache: Option<Value>,
    session: Option<String>,
    deferred: Option<acp2api_acceptance::client::ApiCall>,
    log_mode: Option<String>,
}
impl std::fmt::Debug for ConversationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConversationState").finish_non_exhaustive()
    }
}
fn history(w: &World, user: &str, reply: &str, next: Option<&str>) -> Value {
    let mut items = w.conversation.initial.clone();
    if items.is_empty() {
        items.push(c::message("user", None, user));
    } else {
        items[0]["content"] = json!(user);
    }
    items.push(c::message("assistant", None, reply));
    if let Some(next) = next {
        items.push(c::message("user", None, next));
    }
    items.into()
}
async fn send(w: &mut World, items: Value) {
    let call = c::call(items, w.conversation.system.as_deref(), Value::Null);
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().status,
        200,
        "real conversation HTTP status"
    );
    assert_eq!(
        journal_check::records(&w.scenario).len(),
        c::prompts(&w.scenario).len(),
        "R1 metadata before completion"
    );
}
async fn first(w: &mut World, call: acp2api_acceptance::client::ApiCall, reply: &str) {
    c::send(&mut w.scenario, call, reply).await;
    w.conversation.session = Some(c::session(&w.scenario));
}
#[given(
    expr = "an API client sent a request to {string} with the system prompt {string} and the user message {string}"
)]
fn initial(w: &mut World, endpoint: String, system: String, user: String) {
    assert_eq!(endpoint, "POST /v1/chat/completions");
    w.conversation.system = Some(system.clone());
    w.conversation.initial = vec![c::message("user", None, &user)];
    let mut call = c::call(json!(w.conversation.initial), Some(&system), Value::Null);
    call.headers
        .push(("x-claude-code-session-id".into(), "abc".into()));
    call.body["prompt_cache_key"] = "key-1".into();
    w.conversation.initial_headers = call.headers.clone();
    w.conversation.initial_cache = call.body.get("prompt_cache_key").cloned();
    w.conversation.deferred = Some(call);
}
#[given(expr = "the official runtime replied {string} in the ACP session {string}")]
async fn initial_reply(w: &mut World, reply: String, label: String) {
    assert_eq!(label, "S1");
    let call = w.conversation.deferred.take().unwrap();
    first(w, call, &reply).await;
}
#[when(
    expr = "the API client sends the same conversation with the reply {string} and the new user message {string}"
)]
async fn continue_request(w: &mut World, reply: String, next: String) {
    send(w, history(w, "Hello", &reply, Some(&next))).await;
}
#[when(
    expr = "the API client sends the conversation with the user message {string}, the reply {string}, and the new user message {string}"
)]
async fn edited(w: &mut World, user: String, reply: String, next: String) {
    let mut call = acp2api_acceptance::client::ApiCall::one_user_message("POST /v1/messages", "");
    call.body["messages"] = history(w, &user, &reply, Some(&next));
    call.body["system"] = json!(w.conversation.system);
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, 200);
}
#[then(expr = "the official runtime gets the prompt {string} in the ACP session {string}")]
fn continued(w: &mut World, text: String, label: String) {
    assert_eq!(label, "S1");
    c::continuation(&w.scenario, w.conversation.session.as_ref().unwrap(), &text);
}
#[then("the official runtime gets no new ACP session")]
fn no_new(w: &mut World) {
    assert_eq!(
        c::session(&w.scenario),
        *w.conversation.session.as_ref().unwrap(),
        "no session/new for continuation"
    );
}
#[then("the official runtime gets a new ACP session")]
fn new_session(w: &mut World) {
    assert_ne!(
        c::session(&w.scenario),
        *w.conversation.session.as_ref().unwrap(),
        "ADR-0003 fresh ACP session"
    );
}
#[then("the first prompt of the new ACP session is:")]
fn rebuilt(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    c::exact_text(
        &w.scenario,
        step.docstring.as_ref().unwrap().trim_matches('\n'),
    );
}
#[then(expr = "the first prompt of the new ACP session is {string}")]
fn first_text(w: &mut World, text: String) {
    c::exact_text(&w.scenario, &text);
}
#[then("the first prompt of the new ACP session starts with the neutral line")]
fn neutral(w: &mut World) {
    assert!(
        c::text(&w.scenario).starts_with(&format!("{}\n\n", c::NEUTRAL)),
        "FR-021 neutral line and exact blank line"
    );
    assert_eq!(c::kind(&w.scenario), "rebuild");
}
#[then(expr = "the new ACP session gets the system prompt addition {string}")]
fn addition(w: &mut World, text: String) {
    system_check::addition(&w.scenario, &text);
}
#[given(
    expr = "an API client sent a conversation whose user message has the text {string} and an image, and the official runtime replied {string}"
)]
async fn image_history(w: &mut World, text: String, reply: String) {
    w.conversation.initial = vec![
        json!({"role":"user","content":[content_check::text("POST /v1/chat/completions", &text), content_check::image("POST /v1/chat/completions", content_check::PNG)]}),
    ];
    first(
        w,
        c::call(
            json!(w.conversation.initial),
            w.conversation.system.as_deref(),
            Value::Null,
        ),
        &reply,
    )
    .await;
}
#[when(
    expr = "the API client sends that conversation with the reply {string} in place of {string}, and the new user message {string}"
)]
async fn edit_image(w: &mut World, reply: String, old: String, next: String) {
    assert_eq!(old, "A cat.");
    let mut items = w.conversation.initial.clone();
    items.push(c::message("assistant", None, &reply));
    items.push(c::message("user", None, &next));
    send(w, items.into()).await;
}
#[then(
    expr = "the first prompt of the new ACP session has the text {string}, the image, and the line {string} in this order"
)]
fn image_position(w: &mut World, text: String, role: String) {
    assert_eq!(
        c::blocks(&w.scenario),
        json!([
            {"type":"text","text":format!("{}\n\n[user]\n{text}",c::NEUTRAL)},
            {"type":"image","mimeType":"image/png","data":content_check::PNG},
            {"type":"text","text":format!("\n{role}\nA dog.")}, {"type":"text","text":"\n[user]\nSure?"}
        ]),
        "FR-021 full image rebuild blocks and position"
    );
}
#[when(expr = "an API client sends a new conversation with only the user message {string}")]
async fn fresh(w: &mut World, text: String) {
    w.conversation.system = None;
    send(w, json!([c::message("user", None, &text)])).await;
}
fn rebuild_lines(w: &World) -> usize {
    journal_check::records(&w.scenario)
        .iter()
        .filter(|v| v["session_use"] == "rebuild")
        .count()
}
#[then("the log has no rebuild line")]
fn no_rebuild(w: &mut World) {
    assert_eq!(rebuild_lines(w), 0);
}
#[then("the log has one rebuild line")]
fn one_rebuild(w: &mut World) {
    assert_eq!(rebuild_lines(w), 1);
}
#[given("the owner restarted acp2api")]
async fn restart(w: &mut World) {
    w.scenario.restart().await;
}
#[given(expr = "the first request had the header {string}")]
fn first_header(w: &mut World, header: String) {
    let (key, value) = header.split_once(": ").unwrap();
    assert_eq!(
        w.conversation.initial_headers,
        vec![(key.into(), value.into())],
        "the first actual API call carries the declared header"
    );
}
#[when(
    expr = "the API client sends the conversation with the edited user message {string} and the header {string}"
)]
async fn edit_header(w: &mut World, text: String, header: String) {
    let mut call = c::call(
        history(w, &text, "Hi.", Some("How are you?")),
        w.conversation.system.as_deref(),
        Value::Null,
    );
    let (key, value) = header.split_once(": ").unwrap();
    call.headers.push((key.into(), value.into()));
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, 200);
}
#[given(expr = "the first request had {string} set to {string}")]
fn first_cache(w: &mut World, field: String, value: String) {
    assert_eq!(field, "prompt_cache_key");
    assert_eq!(
        w.conversation.initial_cache,
        Some(json!(value)),
        "the first actual API call carries the declared cache key"
    );
}
#[when(
    expr = "the API client sends the same conversation with the reply {string}, the new user message {string}, and {string} set to {string}"
)]
async fn cache(w: &mut World, reply: String, next: String, field: String, value: String) {
    let mut call = c::call(
        history(w, "Hello", &reply, Some(&next)),
        w.conversation.system.as_deref(),
        Value::Null,
    );
    call.body[field] = value.into();
    w.request.queue_next(&w.scenario);
    content_check::send(&mut w.scenario, call).await;
    assert_eq!(w.scenario.answer.as_ref().unwrap().status, 200);
}
#[given(
    expr = "an API client sent a request to {string} with the user message {string}, and the official runtime replied {string}"
)]
async fn role_first(w: &mut World, endpoint: String, user: String, reply: String) {
    assert_eq!(endpoint, "POST /v1/chat/completions");
    w.conversation.initial = vec![c::message("user", None, &user)];
    first(
        w,
        c::call(json!(w.conversation.initial), None, Value::Null),
        &reply,
    )
    .await;
}
#[when(
    expr = "the API client sends the next request of that conversation with the user message {string} and then the system message {string}"
)]
async fn late_system(w: &mut World, user: String, system: String) {
    let mut items = history(w, "Hello", "Hi.", None).as_array().unwrap().clone();
    items.push(c::message("user", None, &user));
    items.push(c::message("system", None, &system));
    send(w, items.into()).await;
}
#[when(
    expr = "the API client sends the next request of that conversation with the user messages {string} with the name {string} and {string} with the name {string}"
)]
async fn two_named(w: &mut World, a: String, an: String, b: String, bn: String) {
    let mut items = history(w, "Hello", "Hi.", None).as_array().unwrap().clone();
    items.push(c::message("user", Some(&an), &a));
    items.push(c::message("user", Some(&bn), &b));
    send(w, items.into()).await;
}
#[then("the prompt that the official runtime gets is:")]
fn role_text(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    c::exact_text(
        &w.scenario,
        step.docstring.as_ref().unwrap().trim_matches('\n'),
    );
}
#[then(expr = "the system prompt addition of the ACP session does not hold {string}")]
fn absent_system(w: &mut World, text: String) {
    for request in system_check::requests(&w.scenario)
        .iter()
        .filter(|v| v["method"] == "session/new")
    {
        assert!(
            !request["params"]["_meta"]["antigravity"]["systemPrompt"]["append"]
                .as_str()
                .unwrap_or("")
                .contains(&text)
        );
    }
}
#[given("the request has no earlier conversation")]
fn no_history(_: &mut World) {}
#[given("the request continues the last reply of an ACP session")]
async fn named_history(w: &mut World) {
    role_first(
        w,
        "POST /v1/chat/completions".into(),
        "Earlier".into(),
        "Reply".into(),
    )
    .await;
}
#[when(
    expr = "an API client sends the only new user message {string} with the name {string} to {string}"
)]
async fn one_named(w: &mut World, text: String, name: String, endpoint: String) {
    assert_eq!(endpoint, "POST /v1/chat/completions");
    let mut items = w.conversation.initial.clone();
    if !items.is_empty() {
        items.push(c::message("assistant", None, "Reply"));
    }
    items.push(c::message("user", Some(&name), &text));
    send(w, items.into()).await;
}
#[when(expr = "an API client sends a new conversation to {string} with these messages:")]
async fn named_rebuild(w: &mut World, endpoint: String, #[step] step: &cucumber::gherkin::Step) {
    assert_eq!(endpoint, "POST /v1/chat/completions");
    let items = step
        .table
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            c::message(
                &row[0],
                (!row[1].is_empty()).then_some(row[1].as_str()),
                &row[2],
            )
        })
        .collect::<Vec<_>>();
    send(w, items.into()).await;
}
#[given(
    regex = r"^the request (starts a new conversation|adds one user message to the last conversation|has an edited earlier message)$"
)]
async fn log_history(w: &mut World, mode: String) {
    w.conversation.log_mode = Some(mode.clone());
    if mode != "starts a new conversation" {
        role_first(
            w,
            "POST /v1/chat/completions".into(),
            "Hello".into(),
            "Hi.".into(),
        )
        .await;
    }
}
#[when("an API client sends that request")]
async fn log_request(w: &mut World) {
    let items = match w.conversation.log_mode.as_deref().unwrap() {
        "starts a new conversation" => json!([c::message("user", None, "Hello")]),
        "adds one user message to the last conversation" => {
            history(w, "Hello", "Hi.", Some("Next"))
        }
        _ => history(w, "Edited", "Hi.", Some("Next")),
    };
    send(w, items).await;
}
#[then(expr = "the log line of the request has {string}")]
fn log_kind(w: &mut World, kind: String) {
    assert_eq!(
        c::kind(&w.scenario),
        kind,
        "FR-056 actual last request session use"
    );
}

#[given(expr = "the official runtime kept a session file for the ACP session {string}")]
fn persisted_session(w: &mut World, label: String) {
    assert_eq!(label, "S1");
    let session = w.conversation.session.as_ref().unwrap();
    let requests = system_check::requests(&w.scenario);
    w.conversation.folder_markers = requests
        .iter()
        .filter(|v| v["method"] == "session/new")
        .map(|created| {
            let folder = std::path::PathBuf::from(created["params"]["cwd"].as_str().unwrap());
            assert!(
                folder.is_dir(),
                "the actual ACP session folder exists before restart"
            );
            let marker = folder.join("session-folder-marker");
            std::fs::write(&marker, b"synthetic ACP session cwd marker").unwrap();
            assert!(
                marker.is_file(),
                "the session cwd marker exists before restart"
            );
            marker
        })
        .collect();
    let home = w.scenario.root.path.join("data/runtime-home");
    // Captured layout: docs/research/captures/2026-10-15-probe-p/session-files/summary.json.
    let files = home.join("antigravity-acp/conversations");
    std::fs::create_dir_all(&files).unwrap();
    w.conversation.session_files = ["db", "db-wal", "db-shm", "meta"]
        .map(|suffix| {
            let file = files.join(format!("{session}.{suffix}"));
            std::fs::write(&file, b"synthetic ACP session file").unwrap();
            assert!(file.is_file(), "the session file exists before expiry");
            file
        })
        .into();
    for name in ["settings.json", "acp_token.json"] {
        std::fs::write(home.join(name), b"synthetic sign-in marker").unwrap();
    }
}

#[when(expr = "{int} hours pass since the last request of the ACP session {string}")]
async fn idle_hours(w: &mut World, hours: u64, label: String) {
    assert_eq!(label, "S1");
    let session = w.conversation.session.as_ref().unwrap();
    let last = w
        .scenario
        .hub
        .entries()
        .into_iter()
        .rfind(|entry| {
            entry.event["kind"] == "got"
                && entry.event["line"].as_str().is_some_and(|line| {
                    let frame: Value = serde_json::from_str(line).unwrap();
                    frame["method"] == "session/prompt" && frame["params"]["sessionId"] == *session
                })
        })
        .unwrap()
        .at;
    w.scenario
        .time
        .advance_to(last + hours * 60 * 60 * 1000)
        .await;
}

#[when("the API client sends this conversation:")]
async fn explicit_history(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    send(
        w,
        serde_json::from_str(step.docstring.as_ref().unwrap()).unwrap(),
    )
    .await;
}

#[then("the session files and ACP session folders are gone and the sign-in files are unchanged")]
fn restart_cleanup(w: &mut World) {
    let home = w.scenario.root.path.join("data/runtime-home");
    let sign_in =
        ["settings.json", "acp_token.json"].map(|name| std::fs::read(home.join(name)).unwrap());
    assert_eq!(
        (
            w.conversation
                .session_files
                .iter()
                .map(|file| file.exists())
                .collect::<Vec<_>>(),
            w.conversation
                .folder_markers
                .iter()
                .map(|file| file.exists())
                .collect::<Vec<_>>(),
            sign_in
        ),
        (
            vec![false; 4],
            vec![false; w.conversation.folder_markers.len()],
            [
                b"synthetic sign-in marker".to_vec(),
                b"synthetic sign-in marker".to_vec()
            ]
        ),
        "ADR-0005: the next start deletes each session file and ACP session cwd and preserves sign-in files"
    );
}
