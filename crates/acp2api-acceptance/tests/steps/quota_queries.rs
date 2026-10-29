use crate::{World, places_support as queue, query_support as query, quota_support as quota};
use acp2api_acceptance::{
    TurnPlan, TurnStep, conversation_check as conversation, endpoint_check, panel_check,
};
use cucumber::{given, then, when};
use serde_json::{Value, json};

#[given(
    "the next prompt turn ends with the stop reason \"refusal\", and no error channel has a quota text"
)]
fn refusal(w: &mut World) {
    w.request.plan = Some(w.request.next_plan().end("refusal"));
}

#[given(regex = r#"^"/usage" shows (quota|no quota) for the model of the request$"#)]
fn available(w: &mut World, quota_text: String) {
    query::usage_reply(w, quota_text == "quota");
}

#[then("the response ends with \"stop_reason\" set to \"refusal\"")]
fn end(w: &mut World) {
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().json()["stop_reason"],
        "refusal",
        "FR-034 refusal after quota query"
    );
}

#[given("the owner has the management panel open")]
async fn open(w: &mut World) {
    query::panel(w).await;
    let page = w.panel.page("quota-owner").await;
    assert_eq!(page.status, 200);
    assert!(
        panel_check::text(&page).contains("id=\"local-api-key\""),
        "authenticated panel stays open"
    );
}

#[when("one hour passes with no refresh of the quota and no API request")]
async fn idle(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 3_600_000)
        .await;
    let page = w.panel.page("quota-owner").await;
    assert_eq!(
        page.status, 200,
        "open panel remains readable after an hour"
    );
}

#[given("the owner refreshed the quota in the management panel")]
async fn first_refresh(w: &mut World) {
    query::panel(w).await;
    let live = query::refresh(w).await;
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(answer.status, 200, "first quota refresh succeeds");
    assert_eq!(
        query::calls(w),
        1,
        "first refresh reaches /usage before the boundary test"
    );
}

#[when(regex = r"^the owner refreshes the quota again (59999 ms|60 s|60001 ms) later$")]
async fn second_refresh(w: &mut World, time: String) {
    let millis = if time == "60 s" {
        60_000
    } else {
        time.split_whitespace().next().unwrap().parse().unwrap()
    };
    w.scenario
        .time
        .advance_to(w.queries.refresh_at + millis)
        .await;
    let live = query::refresh(w).await;
    w.panel.answer = Some(live.finish(&mut w.scenario.time).await);
}

#[given("\"/usage\" gives a result")]
async fn result(w: &mut World) {
    query::panel(w).await;
    w.scenario
        .fake()
        .next_usage(TurnPlan::new().chunk(query::RESULT));
}

#[when("the owner refreshes the quota in the management panel")]
async fn refresh(w: &mut World) {
    let live = query::refresh(w).await;
    w.panel.answer = Some(live.finish(&mut w.scenario.time).await);
}

#[then("the tile \"quota\" shows that result and the time of that result")]
async fn tile(w: &mut World) {
    let page = w.panel.page("quota-owner").await;
    let html = panel_check::text(&page);
    assert!(
        html.contains("Quota") || html.contains("quota"),
        "FR-052 quota tile"
    );
    assert!(
        html.contains("75% remaining") && html.contains("resets Oct 24, 2026, 12:34 UTC"),
        "FR-052 last /usage result"
    );
    assert!(
        html.contains("2026-10-18T00:00:00Z"),
        "FR-052 quota result UTC time"
    );
    assert_eq!(
        query::calls(w),
        1,
        "rendering the result sends no second query"
    );
}

#[given("a \"/usage\" for a refresh by the owner runs")]
async fn held_usage(w: &mut World) {
    queue::start(w).await;
    query::panel(w).await;
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::HoldUntil("quota-refresh".into()));
    plan.steps.push(TurnStep::Chunk(query::RESULT.into()));
    w.scenario.fake().next_usage(plan);
    w.queries.refresh = Some(query::refresh(w).await);
    w.scenario.time.settle().await;
    assert_eq!(
        query::calls(w),
        1,
        "owner refresh is running before A arrives"
    );
    w.queries.sessions = w.scenario.fake().record().session_meta().len();
}

#[when(expr = "the request {string} comes")]
async fn arrival(w: &mut World, name: String) {
    w.request.plan = Some(TurnPlan::new().chunk("reply A"));
    let live = queue::send(
        w,
        endpoint_check::request("POST /v1/messages", &queue::marker(&name), false),
    )
    .await;
    w.places.calls.push((name, Some(live)));
}

#[then(
    regex = r#"^the official runtime gets no call for "([^"]+)" until the end of the "/usage"$"#
)]
async fn place(w: &mut World, name: String) {
    assert!(
        !queue::has_prompt(w, &name),
        "FR-049 quota query occupies the only place"
    );
    assert_eq!(
        w.scenario.fake().record().session_meta().len(),
        w.queries.sessions,
        "FR-049 no session/new for A while /usage holds the place"
    );
    w.scenario.fake().open_gate("quota-refresh");
    let refresh = w
        .queries
        .refresh
        .take()
        .unwrap()
        .finish(&mut w.scenario.time)
        .await;
    assert_eq!(refresh.status, 200);
    let answer = queue::take(w, &name).finish(&mut w.scenario.time).await;
    assert_eq!(answer.status, 200);
    assert!(
        queue::has_prompt(w, &name),
        "FR-049 request starts after quota query release"
    );
}

#[given(
    regex = r#"^the next prompt turn in the ACP session "S1" ends with (a cut at a stop sequence|a disconnect of the API client|the stderr error line 503 "UNAVAILABLE"|the stop reason "refusal", and "/usage" shows quota for the model)$"#
)]
async fn stopped(w: &mut World, event: String) {
    query::stopped(w, &event).await;
}

#[given("30 s pass after that end")]
async fn retry_wait(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 30_000)
        .await;
}

#[when("the API client sends the next request of that conversation")]
async fn next(w: &mut World) {
    let call = conversation::call(json!(w.queries.history), Some("Be kind."), Value::Null);
    quota::send(w, call).await;
}
