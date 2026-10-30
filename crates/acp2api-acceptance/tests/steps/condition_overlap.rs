use crate::{
    World, error_support as support, places_support as queue, quota_support as quota,
    runtime_support as process, sign_in_support as sign_in, unavailable_support as unavailable,
};
use acp2api_acceptance::{TurnPlan, endpoint_check, panel_check};
use cucumber::{given, then, when};
use serde_json::json;

#[given("two prompt turns run")]
async fn two_turns(w: &mut World) {
    queue::held(w, "quota", false).await;
    queue::held(w, "sign-in", false).await;
    assert_eq!(queue::prompts(w).len(), 2, "two concurrent signal sources");
}

#[given("one ends with a quota signal whose reset time is 2 minutes away")]
async fn quota_first(w: &mut World) {
    w.scenario
        .fake()
        .finish_gate("queue-quota", quota::error(429, Some(quota::RESET)));
    let answer = queue::take(w, "quota")
        .finish_current(&mut w.scenario.time)
        .await;
    assert_eq!(answer.status, 200, "signal source was accepted as a stream");
}

#[given("the other then ends with a sign-in error")]
async fn sign_in_second(w: &mut World) {
    w.scenario.fake().set_signed_in(false);
    w.scenario
        .fake()
        .finish_gate("queue-sign-in", support::upstream(401, "UNAUTHENTICATED"));
    let answer = queue::take(w, "sign-in")
        .finish_current(&mut w.scenario.time)
        .await;
    assert_eq!(
        answer.status, 200,
        "second signal source was accepted as a stream"
    );
    w.errors.checks_before = process::calls(w, "session/new").len();
}

#[when(
    regex = r"^(a sign-in in the management panel|a successful sign-in check of a request) occurs 30 s later$"
)]
async fn succeeds(w: &mut World, success: String) {
    w.scenario.time.advance_to(30_000).await;
    if success == "a sign-in in the management panel" {
        w.panel.scenario(&mut w.scenario).await;
        w.panel.login_initial("sign-in-owner").await;
        assert_eq!(w.panel.login_answer.as_ref().unwrap().status, 303);
        sign_in::authenticate(w, true, None).await;
    } else {
        w.scenario.fake().set_signed_in(true);
        w.request.plan = Some(TurnPlan::default());
        support::send_current(
            w,
            endpoint_check::request("POST /v1/messages", "successful check during quota", false),
        )
        .await;
        assert_eq!(
            process::calls(w, "session/new").len(),
            w.errors.checks_before + 1,
            "FR-038 request actually checks sign-in during quota wait"
        );
        let answer = w.scenario.answer.as_ref().unwrap();
        assert_eq!(
            answer.status, 429,
            "FR-036/FR-038 successful check preserves quota wait"
        );
        assert_eq!(
            answer.json()["error"]["message"],
            support::state_message(w, "quota wait")
        );
    }
    w.errors.prompts_before = w.scenario.fake().record().prompts().len();
    w.request.plan = Some(TurnPlan::default());
}

#[then(regex = r"^(?:the|a) request gets HTTP (429|400) with (M-16|M-13)$")]
fn refusal(w: &mut World, status: u16, message: String) {
    let state = if message == "M-16" {
        "quota wait"
    } else {
        "sign-in required"
    };
    support::state_refusal(w, w.scenario.answer.as_ref().unwrap(), status, state);
}

#[then("the official runtime gets no prompt for that request")]
fn no_prompt(w: &mut World) {
    assert!(
        w.errors.prompts_before > 0,
        "positive control: the fake saw the signal turns"
    );
    assert_eq!(
        w.scenario.fake().record().prompts().len(),
        w.errors.prompts_before,
        "FR-036 remaining quota condition prevents the prompt"
    );
}

#[then("the first tile shows \"Quota wait\"")]
async fn tile(w: &mut World) {
    w.panel.scenario(&mut w.scenario).await;
    w.panel.login_initial("overlap-owner").await;
    let answer = w.panel.page("overlap-owner").await;
    assert_eq!(answer.status, 200);
    let html = panel_check::text(&answer);
    let first = html.find("<section").expect("first management tile");
    let tile = &html[first..html[first..].find("</section>").unwrap() + first];
    let expected = "Quota wait";
    assert!(
        tile.contains(&format!(">{expected}<")),
        "FR-036 first tile preserves Quota wait: {tile}"
    );
}

#[given("acp2api has a quota wait until the time 120 s and also requires a sign-in")]
async fn quota_and_sign_in(w: &mut World) {
    // Both errors must come from work admitted before either condition starts.
    for name in ["quota", "sign-in"] {
        queue::held(w, name, false).await;
    }
    w.scenario
        .fake()
        .finish_gate("queue-quota", quota::error(429, Some(quota::RESET)));
    let _ = queue::take(w, "quota")
        .finish_current(&mut w.scenario.time)
        .await;
    w.scenario.fake().set_signed_in(false);
    w.scenario
        .fake()
        .finish_gate("queue-sign-in", support::upstream(401, "UNAUTHENTICATED"));
    let _ = queue::take(w, "sign-in")
        .finish_current(&mut w.scenario.time)
        .await;
    w.errors.checks_before = process::calls(w, "session/new").len();
}

#[when(expr = "an API client sends a valid request at the time {int} s")]
async fn at_reset(w: &mut World, at: u64) {
    w.scenario.time.advance_to(at * 1000).await;
    w.request.plan = Some(TurnPlan::default());
    support::send_current(
        w,
        endpoint_check::request("POST /v1/messages", "request at quota expiry", false),
    )
    .await;
}

#[then("its sign-in check fails")]
fn failed_check(w: &mut World) {
    assert_eq!(
        process::calls(w, "session/new").len(),
        w.errors.checks_before + 1,
        "FR-038 expiry still requires one sign-in check"
    );
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().status,
        400,
        "FR-038 failed check refuses before acceptance"
    );
}

#[given("acp2api has an unexpired quota wait and the state \"runtime unavailable\"")]
async fn quota_and_unavailable(w: &mut World) {
    queue::start(w).await;
    quota::seed(w, true).await;
    for _ in 0..3 {
        let before = process::servers(w).len();
        process::exit(w).await;
        if before < 3 {
            process::observe(w, |w| process::servers(w).len() > before).await;
        }
    }
    w.errors.prompts_before = w.scenario.fake().record().prompts().len();
}

#[when("the owner retries and the runtime process starts with a valid sign-in")]
async fn recovered(w: &mut World) {
    let before = process::servers(w).len();
    w.scenario.fake().set_signed_in(true);
    let answer = unavailable::action(w, "/manage/retry", json!({})).await;
    assert_eq!(
        answer.status, 200,
        "FR-040 owner retries through the real panel"
    );
    process::observe(w, |w| process::servers(w).len() > before).await;
    assert_eq!(
        process::servers(w).len(),
        before + 1,
        "runtime recovery starts one process"
    );
    w.request.plan = Some(TurnPlan::default());
}

#[when("an API client sends a valid request before the reset time")]
async fn before_reset(w: &mut World) {
    assert!(w.scenario.hub.now() < 120_000, "quota has not expired");
    support::send_current(
        w,
        endpoint_check::request("POST /v1/messages", "request before quota reset", false),
    )
    .await;
}

#[given("the official runtime still has no valid sign-in")]
fn still_unsigned(w: &mut World) {
    w.scenario.fake().set_signed_in(false);
}
