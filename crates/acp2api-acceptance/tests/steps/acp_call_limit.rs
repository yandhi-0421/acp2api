use crate::{World, acp_call_limit_support as limit, acp_call_limit_support::Kind};
use cucumber::{given, then, when};

#[given(
    regex = r#"^(the "session/new" of a request|the "session/load" of a request|the sign-in check of a request|the model read of the owner|the "/usage" of a quota refresh) gives no answer$"#
)]
async fn no_answer(w: &mut World, call: String) {
    limit::begin(w, Kind::parse(&call), false).await;
}
#[given(
    regex = r#"^(the "session/new" of a request|the "session/load" of a request|the sign-in check of a request|the model read of the owner|the "/usage" of a quota refresh) holds its answer$"#
)]
async fn holds_answer(w: &mut World, call: String) {
    limit::begin(w, Kind::parse(&call), true).await;
}
#[given("the \"/usage\" for the reset time of a request gives no answer")]
async fn no_reset_answer(w: &mut World) {
    limit::usage_for_reset(w).await;
}
#[given("the \"/usage\" after the stop reason \"refusal\" of a request gives no answer")]
async fn no_refusal_answer(w: &mut World) {
    limit::usage_after_refusal(w).await;
}
#[given("the \"/usage\" of a quota refresh sends an update after 4 minutes and does not end")]
async fn usage_update(w: &mut World) {
    limit::usage_with_update(w).await;
}
#[given("the \"session/set_config_option\" of a prompt turn gives no answer")]
async fn no_config_answer(w: &mut World) {
    limit::set_config_option(w).await;
}
#[given("a prompt turn with updates runs")]
async fn running(w: &mut World) {
    limit::running_turn(w).await;
}
#[given("a request that needs a \"session/new\" with no answer waits in the queue")]
async fn queued_call(w: &mut World) {
    limit::queued_call(w).await;
}
#[given("another request waits in the queue")]
async fn queued(w: &mut World) {
    limit::queued_request(w).await;
}
#[given("the API client of that call disconnected")]
async fn disconnected(w: &mut World) {
    limit::disconnect(w).await;
}
#[given("that runtime process writes the answer of that call when acp2api stops it")]
async fn late(w: &mut World) {
    limit::late_answer(w).await;
}
#[when("5 minutes pass after acp2api sends that call")]
async fn passes(w: &mut World) {
    limit::pass_limit(w).await;
}
#[when("that call answers 1 ms before 5 minutes pass, and 1 minute more passes")]
async fn answers(w: &mut World) {
    limit::answer_just_in_time(w).await;
}
#[when("the prompt turn with updates ends 6 minutes after that request came")]
async fn ends_later(w: &mut World) {
    limit::end_running_after_wait(w).await;
}
#[when(regex = r#"^a "session/new" passes the time limit at (\d+) minutes$"#)]
async fn passes_at(w: &mut World, minutes: u64) {
    limit::passes_at(w, minutes).await;
}
#[when(regex = r"^the runtime process exits with no cause from acp2api at (\d+) minutes$")]
async fn exits_at(w: &mut World, minutes: u64) {
    limit::exit_at(w, minutes).await;
}
#[when("30 s pass after that message")]
async fn cancel_limit(w: &mut World) {
    limit::pass_cancel_limit(w).await;
}
#[then("acp2api replaces the runtime process at once")]
async fn replaced(w: &mut World) {
    limit::new_process_at_once(w).await;
}
#[then(
    regex = r#"^(the API client|the owner) gets the message "([^"]+)" (with the 503 row|with the 503 row and no stream event|in the management panel)$"#
)]
async fn caller_message(w: &mut World, caller: String, message: String, form: String) {
    match (caller.as_str(), form.as_str()) {
        ("the owner", "in the management panel") => limit::owner_error(w, &message).await,
        ("the API client", "with the 503 row") => limit::caller_error(w, &message, false).await,
        ("the API client", "with the 503 row and no stream event") => {
            limit::caller_error(w, &message, true).await
        }
        other => panic!("unknown result: {other:?}"),
    }
}
#[then(expr = "the prompt turn with updates gets the message {string} with the 503 row")]
async fn running_message(w: &mut World, message: String) {
    limit::running_error(w, &message).await;
}
#[then(
    "the next request gets the quota message with \"retry-after\" for the 60 minutes after the quota signal"
)]
async fn quota_message(w: &mut World) {
    limit::next_quota_error(w).await;
}
#[then(expr = "the next request gets HTTP {int}")]
async fn next(w: &mut World, status: u16) {
    limit::next_request(w, status).await;
}
#[then("the request in the queue gets its response from the new runtime process")]
async fn queued_response(w: &mut World) {
    limit::queued_response(w).await;
}
#[then("the caller of that call gets its normal result")]
async fn normal(w: &mut World) {
    limit::normal_result(w).await;
}
#[then("acp2api starts no new runtime process")]
fn no_new(w: &mut World) {
    limit::no_new_process(w);
}
#[then("acp2api started a new runtime process after each failure")]
fn after_each(w: &mut World) {
    limit::started_after_each_failure(w);
}
#[then("no ACP call uses the ACP session of the late answer")]
fn late_unused(w: &mut World) {
    limit::late_answer_unused(w);
}
