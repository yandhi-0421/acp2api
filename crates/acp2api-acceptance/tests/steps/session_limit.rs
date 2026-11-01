use crate::{
    World, runtime_support as process,
    session_limit_support::{self as limit, N},
};
use acp2api_acceptance::rpc_script::RpcReply;
use cucumber::{given, then, when};

const ONE_DAY: u64 = 24 * 60 * 60 * 1000;
const START_LIMIT: u64 = 5 * 60 * 1000;

#[given("the runtime process holds N ACP sessions")]
async fn holds_n(w: &mut World) {
    limit::fill(w, N).await;
}
#[given("the runtime process holds N minus 1 ACP sessions")]
async fn holds_fewer(w: &mut World) {
    limit::fill(w, N - 1).await;
}
#[given("one prompt turn runs")]
async fn one_turn(w: &mut World) {
    limit::running_turn(w).await;
}
#[given("two prompt turns run in these ACP sessions")]
async fn two_turns(w: &mut World) {
    limit::running_turn(w).await;
    limit::running_turn(w).await;
}
#[given("the start of the replacement gives no answer")]
fn silent_replacement(w: &mut World) {
    w.scenario
        .fake()
        .script_call("initialize", RpcReply::Silent);
}
#[when("a request comes that needs a new ACP session")]
async fn needs_new(w: &mut World) {
    limit::new_request(w).await;
}
#[when("a request comes that continues one of these ACP sessions")]
async fn continues(w: &mut World) {
    limit::continuing_request(w, false).await;
}
#[when("a request comes that continues an ACP session of the earlier runtime process")]
async fn continues_earlier(w: &mut World) {
    limit::continuing_request(w, true).await;
}
#[when("24 hours pass with no request")]
async fn one_day(w: &mut World) {
    let end = w.scenario.hub.now() + ONE_DAY;
    w.scenario.time.advance_to(end).await;
}
#[when("the prompt turn that runs ends")]
#[when("the first prompt turn ends")]
#[when("the second prompt turn ends")]
async fn turn_ends(w: &mut World) {
    limit::end_running(w).await;
}
#[when(
    regex = r"^requests of new conversations bring the new runtime process to (N|N minus 1) ACP sessions$"
)]
async fn bring(w: &mut World, target: String) {
    let target = if target == "N" { N } else { N - 1 };
    limit::fill(w, target).await;
}
#[when("5 minutes pass after the start of the replacement")]
async fn start_limit(w: &mut World) {
    let start = limit::last_start(w);
    w.scenario.time.advance_to(start + START_LIMIT - 1).await;
    assert_eq!(
        limit::current(w),
        2,
        "FR-041 no further start before the 5 minutes end"
    );
    w.scenario.time.advance_to(start + START_LIMIT).await;
}
#[then("that request waits in the queue")]
#[then("each of these requests waits in the queue")]
fn waits(w: &mut World) {
    limit::assert_all_wait(w);
}
#[then("the request gets its response from the new runtime process")]
async fn response(w: &mut World) {
    limit::response_from_new_process(w).await;
}
#[then("the two requests get their responses from the new runtime process in the order of arrival")]
async fn responses(w: &mut World) {
    let (first, second) = (
        w.limit.waiting[0].text.clone(),
        w.limit.waiting[1].text.clone(),
    );
    limit::response_from_new_process(w).await;
    limit::response_from_new_process(w).await;
    assert!(
        limit::arrived_in_order(w, &first, &second),
        "FR-050 the prompt turns start in the order of arrival"
    );
}
#[then("the prompt turn of the request starts at once in the same runtime process")]
async fn at_once(w: &mut World) {
    limit::at_once_in_same_process(w).await;
}
#[then(
    regex = r#"^the official runtime gets "session/load" for the continued ACP session in the (same|new) runtime process$"#
)]
async fn loaded(w: &mut World, process: String) {
    let session = w.limit.continued.clone();
    let current = limit::current(w);
    let target = session.clone();
    process::observe(w, |w| limit::loads(w, &target).contains(&current)).await;
    let loads = limit::loads(w, &session);
    assert_eq!(
        loads,
        vec![current],
        "FR-041 ADR-0003 one session/load in the {process} runtime process"
    );
}
#[then("each of these prompt turns ran in the new runtime process")]
fn ran_in_new(w: &mut World) {
    assert_eq!(
        limit::current(w),
        w.limit.process_before_fill,
        "FR-041 the count starts again, so these ACP sessions need no replacement"
    );
    assert!(
        process::alive(process::servers(w).last().unwrap()["pid"].as_u64().unwrap() as u32),
        "FR-041 the new runtime process still runs"
    );
}
#[then("acp2api starts one more runtime process at once")]
async fn one_more(w: &mut World) {
    process::observe(w, |w| limit::current(w) == 3).await;
    assert_eq!(
        limit::current(w),
        3,
        "FR-041 a start with no answer in 5 minutes starts a new runtime process at once"
    );
    limit::assert_no_response(w);
}
