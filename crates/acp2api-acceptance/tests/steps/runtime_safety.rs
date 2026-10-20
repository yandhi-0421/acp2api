use crate::World;
use acp2api_acceptance::{client::Answer, panel_check::Panel, runtime_safety_check as safety};
use cucumber::{given, then, when};
#[derive(Debug, Default)]
pub struct SafetyState {
    name: String,
    reply: String,
    permission: bool,
    panel: Option<Panel>,
    next: Option<Answer>,
}
#[given(expr = "the data folder of acp2api is {string}")]
fn data(w: &mut World, name: String) {
    assert_eq!(name, "D");
    w.scenario.plan(safety::isolated());
}
#[given(expr = "the folder {string} of the owner names the MCP server {string}")]
fn global(w: &mut World, folder: String, server: String) {
    assert_eq!(folder, "~/.gemini");
    assert_eq!(server, "owner-mcp");
    w.scenario.plan(safety::isolated());
}
#[when("acp2api starts a runtime process")]
async fn start(w: &mut World) {
    w.scenario.plan(safety::isolated());
    w.scenario.start_isolated_owner().await;
}
#[then(expr = "the runtime home of the runtime process is a folder in {string}")]
fn home(w: &mut World, data: String) {
    assert_eq!(data, "D");
    safety::home(&w.scenario);
}
#[when("an API client sends a request with no client tool")]
async fn no_tools(w: &mut World) {
    w.scenario
        .fake()
        .next_prompt(acp2api_acceptance::TurnPlan::new().chunk("Pure reply"));
    w.scenario.answer = Some(safety::send(&mut w.scenario, "no client tools").await);
    safety::reply(w.scenario.answer.as_ref().unwrap(), "Pure reply");
}
#[then(expr = "the official runtime offers the model no tool of {string}")]
fn global_absent(w: &mut World, server: String) {
    assert_eq!(server, "owner-mcp");
    safety::configuration(&w.scenario);
}
#[then(
    "the official runtime gets no file system capability and no terminal capability from acp2api"
)]
fn capabilities(w: &mut World) {
    safety::capabilities(&w.scenario);
}
#[given(
    expr = "the official runtime sends the permission request of the built-in tool {string} in a prompt turn"
)]
fn built_in_permission(w: &mut World, name: String) {
    w.scenario.plan(safety::isolated());
    w.safety.name = name;
    w.safety.permission = true;
}
#[given(expr = "the official runtime then replies {string}")]
fn reply(w: &mut World, text: String) {
    w.safety.reply = text;
}
#[given(
    expr = "the built-in tool {string} of the official runtime runs with no permission request in the next prompt turn"
)]
fn built_in_run(w: &mut World, name: String) {
    w.scenario.plan(safety::isolated());
    w.safety.name = name;
}
async fn send(w: &mut World) {
    w.scenario.fake().next_prompt(if w.safety.permission {
        safety::denial_plan(&w.safety.name, &w.safety.reply)
    } else {
        safety::execution_plan(&w.safety.name)
    });
    w.scenario.answer = Some(safety::send(&mut w.scenario, "runtime safety request").await);
}
#[when("an API client sends a valid request")]
async fn valid(w: &mut World) {
    send(w).await;
}
#[when(expr = "an API client sends a valid request with \"stream\" set to {word}")]
async fn valid_body(w: &mut World, stream: String) {
    assert_eq!(stream, "false");
    send(w).await;
}
#[then("the official runtime gets a refusal of that permission request")]
fn denied(w: &mut World) {
    safety::denied(&w.scenario);
}
#[then(expr = "the reply text of the response is {string}")]
fn reply_text(w: &mut World, text: String) {
    safety::reply(w.scenario.answer.as_ref().unwrap(), &text);
}
#[then("the log has a line about the refused permission request")]
fn refusal_log(w: &mut World) {
    safety::event_log(&w.scenario, "permission_refused", &w.safety.name);
}
#[then(expr = "acp2api answers with HTTP {int} and the message {string}")]
fn failure(w: &mut World, status: u16, message: String) {
    assert_eq!(status, 502);
    safety::error(w.scenario.answer.as_ref().unwrap(), &w.safety.name);
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().json()["error"]["message"],
        message
    );
}
#[then(expr = "the log has one line about the built-in tool {string}")]
fn execution_log(w: &mut World, name: String) {
    safety::event_log(&w.scenario, "built_in_tool_ran", &name);
}
#[given(expr = "the built-in tool {string} ran with no permission request in one prompt turn")]
async fn ran(w: &mut World, name: String) {
    built_in_run(w, name);
    send(w).await;
}
#[then(expr = "the management panel shows one notice {string}")]
async fn notice(w: &mut World, text: String) {
    w.safety.panel = Some(safety::panel(&mut w.scenario).await);
    safety::notice_page(w.safety.panel.as_mut().unwrap(), &text).await;
    safety::error(w.scenario.answer.as_ref().unwrap(), &w.safety.name);
    safety::event_log(&w.scenario, "built_in_tool_ran", &w.safety.name);
}
#[then("the notice is still there after a reload of the management panel")]
async fn reload(w: &mut World) {
    safety::notice_page(
        w.safety.panel.as_mut().unwrap(),
        &safety::notice(&w.safety.name),
    )
    .await;
}
#[when("the owner dismisses the notice")]
async fn dismiss(w: &mut World) {
    safety::dismiss(w.safety.panel.as_mut().unwrap()).await;
}
#[then("the management panel shows no notice")]
async fn no_notice(w: &mut World) {
    safety::no_notice(
        w.safety.panel.as_mut().unwrap(),
        &safety::notice(&w.safety.name),
    )
    .await;
}
#[when("an API client sends a valid request of another conversation")]
async fn next(w: &mut World) {
    w.scenario
        .fake()
        .next_prompt(acp2api_acceptance::TurnPlan::new().chunk("Next conversation completes"));
    w.safety.next =
        Some(safety::send(&mut w.scenario, "a different conversation after the event").await);
}
#[then("acp2api is in the normal state")]
fn normal(w: &mut World) {
    safety::reply(
        w.safety.next.as_ref().unwrap(),
        "Next conversation completes",
    );
    assert_eq!(
        w.scenario.fake().record().cancels().len(),
        1,
        "only the offending turn is cancelled"
    );
}
#[then("the official runtime gets a prompt for the request")]
fn prompted(w: &mut World) {
    let prompts = w.scenario.fake().record().prompts();
    assert_eq!(prompts.len(), 2, "both conversations reach the runtime");
    assert_eq!(prompts[1].text, "a different conversation after the event");
    safety::reply(
        w.safety.next.as_ref().unwrap(),
        "Next conversation completes",
    );
}
#[when("two requests of two conversations make two new ACP sessions")]
async fn two_sessions(w: &mut World) {
    w.scenario.plan(safety::isolated());
    safety::two_conversations(&mut w.scenario).await;
}
#[then("the cwd of each ACP session is a folder in the system temporary folder")]
#[then("each cwd was empty at the start of its ACP session")]
#[then("the two cwd folders are different")]
fn cwd(w: &mut World) {
    safety::cwd(&w.scenario);
}
