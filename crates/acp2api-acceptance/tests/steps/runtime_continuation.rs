use crate::{World, runtime_support as process};
use acp2api_acceptance::{client::Call, conversation_check as conversation, rpc_script::RpcReply};
use cucumber::{given, then, when};
use serde_json::{Value, json};

#[derive(Default)]
pub struct ReloadState {
    session: String,
    call: Option<Call>,
    files: Vec<std::path::PathBuf>,
    messages: Vec<Value>,
    tools: Value,
    prompts: usize,
}
impl std::fmt::Debug for ReloadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReloadState")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}
pub fn cleanup(w: &mut World) {
    w.reload.call.take();
}
#[given(expr = "an ACP session holds the client tool call {string} and its result {string}")]
async fn tool_history(w: &mut World, name: String, result: String) {
    w.reload.tools = conversation::tools(
        &name,
        "Look up the weather",
        json!({"type":"object","properties":{}}),
    );
    w.reload.messages = vec![json!({"role":"user","content":"Weather?"})];
    w.request.output_call = Some((name.clone(), "{}".into()));
    w.request.queue_next(&w.scenario);
    let call = conversation::call(json!(w.reload.messages), None, w.reload.tools.clone());
    let answer = w
        .scenario
        .client()
        .await
        .send(call)
        .finish(&mut w.scenario.time)
        .await;
    assert_eq!(
        answer.status, 200,
        "the client tool call reaches the API client"
    );
    let assistant = answer.json()["choices"][0]["message"].clone();
    assert_eq!(assistant["tool_calls"][0]["function"]["name"], name);
    let id = assistant["tool_calls"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let session = conversation::session(&w.scenario);
    w.reload.messages.push(assistant);
    w.reload
        .messages
        .push(json!({"role":"tool","tool_call_id":id,"content":result}));
    w.request.reply = "Hi.".into();
    w.request.output_call = None;
    w.request.queue_next(&w.scenario);
    let call = conversation::call(json!(w.reload.messages), None, w.reload.tools.clone());
    let answer = w
        .scenario
        .client()
        .await
        .send(call)
        .finish(&mut w.scenario.time)
        .await;
    assert_eq!(
        answer.status, 200,
        "the tool result continues the ACP session"
    );
    assert_eq!(conversation::session(&w.scenario), session);
    assert_eq!(answer.json()["choices"][0]["message"]["content"], "Hi.");
    w.reload
        .messages
        .push(answer.json()["choices"][0]["message"].clone());
    w.reload.session = session;
    w.reload.prompts = conversation::prompts(&w.scenario).len();
}
#[given("the official runtime kept a session file for that ACP session")]
fn replay(w: &mut World) {
    let session = &w.reload.session;
    let folder = w
        .scenario
        .root
        .path
        .join("data/runtime-home/antigravity-acp/conversations");
    std::fs::create_dir_all(&folder).unwrap();
    w.reload.files = ["db", "db-wal", "db-shm", "meta"]
        .map(|suffix| {
            let path = folder.join(format!("{session}.{suffix}"));
            std::fs::write(&path, b"synthetic persisted client tool history").unwrap();
            path
        })
        .into();
    // This history notification replays the earlier permission's tool call; it executes no tool.
    let updates = vec![
        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"tool_call","toolCallId":"tool-1","title":"Run lookup?","status":"completed"}}}),
    ];
    let result: Value = w
        .scenario
        .hub
        .entries()
        .iter()
        .filter_map(|e| {
            e.event["command"]["line"]
                .as_str()
                .and_then(|line| serde_json::from_str::<Value>(line).ok())
        })
        .find_map(|v| (v["result"]["sessionId"] == *session).then(|| v["result"].clone()))
        .expect("catalog of the persisted ACP session");
    w.scenario
        .fake()
        .script_call("session/load", RpcReply::Reply { updates, result });
}
#[when("the API client sends the next match of that ACP session")]
async fn next_match(w: &mut World) {
    w.reload
        .messages
        .push(json!({"role":"user","content":"How are you?"}));
    w.request.reply = "Fine.".into();
    w.request.queue_next(&w.scenario);
    let call = conversation::call(json!(w.reload.messages), None, w.reload.tools.clone());
    w.reload.call = Some(w.scenario.client().await.send(call));
    process::observe(w, |w| {
        !process::calls(w, "session/load").is_empty()
            || w.reload.call.as_ref().unwrap().status().is_some()
    })
    .await;
}
#[then(expr = "the official runtime gets {string} for that ACP session in a new runtime process")]
fn loaded(w: &mut World, method: String) {
    assert_eq!(method, "session/load");
    let loads = process::calls(w, &method);
    assert_eq!(
        loads.len(),
        1,
        "ADR-0003 the next match loads the recorded ACP session after a runtime process exit"
    );
    assert_eq!(loads[0]["params"]["sessionId"], w.reload.session);
    let servers = process::servers(w);
    assert_eq!(servers.len(), 2);
    assert_ne!(servers[0]["pid"], servers[1]["pid"]);
    assert!(
        w.reload.files.iter().all(|file| file.is_file()),
        "ADR-0005 retained records keep all session files across replacement"
    );
    let folder = loads[0]["params"]["cwd"].as_str().expect("load cwd");
    assert!(
        std::path::Path::new(folder).is_dir(),
        "ADR-0005 load recreates an absent session cwd"
    );
}
#[then(expr = "the official runtime gets only the prompt {string} in that ACP session")]
fn only_new(w: &mut World, text: String) {
    let prompts = process::calls(w, "session/prompt");
    assert_eq!(prompts.len(), w.reload.prompts + 1);
    let last = prompts.last().unwrap();
    assert_eq!(last["params"]["sessionId"], w.reload.session);
    assert_eq!(
        last["params"]["prompt"],
        json!([{"type":"text","text":text}])
    );
}
#[then(expr = "the API client gets the reply {string} after the replay of the completed tool call")]
async fn reply(w: &mut World, text: String) {
    let answer = w
        .reload
        .call
        .take()
        .unwrap()
        .finish(&mut w.scenario.time)
        .await;
    assert_eq!(
        answer.status, 200,
        "FR-047 a replayed completed tool call must not cause M-22"
    );
    assert_eq!(answer.json()["choices"][0]["message"]["content"], text);
    assert_eq!(conversation::kind(&w.scenario), "continue");
}
