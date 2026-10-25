use crate::{World, places_support as queue};
use cucumber::{given, then, when};

#[given(expr = "the concurrency limit is {int}")]
fn concurrency_limit(w: &mut World, limit: u8) {
    w.places.limit = limit;
}

#[when(
    expr = "API clients send the requests {string}, {string}, and {string} of three conversations"
)]
async fn three(w: &mut World, a: String, b: String, c: String) {
    for name in [a, b, c] {
        queue::held(w, &name, false).await;
    }
}

#[given(expr = "the prompt turns of {string} and {string} run, and {string} waits in the queue")]
async fn running_two(w: &mut World, a: String, b: String, c: String) {
    for name in [a, b, c] {
        queue::held(w, &name, false).await;
    }
    assert_eq!(
        queue::prompts(w).len(),
        2,
        "two prompt turns run before the limit change"
    );
    queue::panel(w).await;
}

#[then(
    expr = "the official runtime gets no call for {string} while the prompt turns of {string} and {string} run"
)]
fn no_third(w: &mut World, c: String, a: String, b: String) {
    assert!(queue::has_prompt(w, &a) && queue::has_prompt(w, &b));
    assert_eq!(
        queue::prompts(w).len(),
        2,
        "FR-049 only two occupied places"
    );
    assert!(
        !queue::has_prompt(w, &c),
        "FR-049 no ACP call for the queued request"
    );
    let sessions = w.scenario.fake().record().session_meta();
    assert_eq!(
        sessions.len(),
        2,
        "FR-049 queued work sends no session/new either"
    );
}

#[when(expr = "the prompt turn of {string} ends")]
#[when(expr = "{string} ends")]
async fn end(w: &mut World, name: String) {
    queue::end(w, &name).await;
}

#[then(expr = "the official runtime gets the first call for {string}")]
#[then(expr = "the prompt turn of {string} starts")]
async fn started(w: &mut World, name: String) {
    assert!(
        queue::has_prompt(w, &name),
        "FR-049/FR-050 queued request {name} starts after release"
    );
    queue::end(w, &name).await;
}

#[then(expr = "the prompt turn of {string} starts at once")]
async fn at_once(w: &mut World, name: String) {
    assert!(
        queue::has_prompt(w, &name),
        "FR-049/FR-022 request {name} starts without another release"
    );
    assert_eq!(
        w.scenario.hub.now(),
        w.places.arrival,
        "FR-049/FR-022 no virtual wait before dispatch"
    );
    queue::end(w, &name).await;
}

#[given(expr = "the prompt turns {string}, {string}, and {string} run, and request {string} waits")]
async fn running_three(w: &mut World, a: String, b: String, c: String, d: String) {
    for name in [a, b, c, d] {
        queue::held(w, &name, false).await;
    }
    assert_eq!(
        queue::prompts(w).len(),
        3,
        "three occupied places before lowering the limit"
    );
}

#[when(expr = "the owner sets the concurrency limit to {int}")]
async fn lower(w: &mut World, limit: u8) {
    queue::panel(w).await;
    let answer = w
        .panel
        .submit(
            "B1",
            "/manage/concurrency",
            serde_json::json!({"concurrency_limit":limit.to_string()}),
        )
        .await;
    assert_eq!(
        answer.status, 200,
        "owner changes the concurrency limit through HTTP"
    );
    w.scenario.time.settle().await;
}

#[then(expr = "the official runtime gets no cancel for {string}, {string}, or {string}")]
fn no_cancel(w: &mut World, a: String, b: String, c: String) {
    for name in [a, b, c] {
        assert!(queue::has_prompt(w, &name), "occupied prompt exists");
    }
    assert_eq!(
        w.scenario.fake().record().cancels().len(),
        0,
        "FR-049 lowering preserves occupied work"
    );
}

#[when(expr = "{string} and {string} end")]
async fn two_end(w: &mut World, a: String, b: String) {
    queue::end(w, &a).await;
    queue::end(w, &b).await;
}

#[then(expr = "the official runtime still has no call for {string}")]
fn still_waits(w: &mut World, name: String) {
    assert!(
        !queue::has_prompt(w, &name),
        "FR-049 occupied count must be below the new limit"
    );
    assert_eq!(
        w.scenario.fake().record().session_meta().len(),
        3,
        "FR-049 no early session/new for the waiting request"
    );
}

#[given(expr = "the platform maximum concurrency limit is {int}, as its release notes name")]
fn platform_maximum(w: &mut World, maximum: u8) {
    w.places.maximum = maximum;
    w.places.limit = maximum.min(2);
}

#[when("the owner opens the management panel with a panel session")]
async fn panel(w: &mut World) {
    queue::panel(w).await;
    w.panel.answer = Some(w.panel.page("B1").await);
}

#[then(regex = r"^the entry for the concurrency limit offers only (1|1 and 2|1, 2, and 3)$")]
fn options(w: &mut World, choices: String) {
    let expected: Vec<u8> = choices
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u8>().unwrap())
        .collect();
    let html = acp2api_acceptance::panel_check::text(w.panel.answer.as_ref().unwrap());
    let select = html
        .split("name=\"concurrency_limit\"")
        .nth(1)
        .expect("concurrency limit entry")
        .split("</select>")
        .next()
        .unwrap();
    let actual: Vec<u8> = select
        .split("<option value=\"")
        .skip(1)
        .map(|s| s.split('"').next().unwrap().parse::<u8>().unwrap())
        .collect();
    assert_eq!(
        actual, expected,
        "FR-049 panel choices obey the platform maximum"
    );
}

#[given(expr = "the prompt turn of the request {string} runs")]
async fn running(w: &mut World, name: String) {
    queue::held(w, &name, false).await;
    assert!(queue::has_prompt(w, &name), "occupied request reaches ACP");
}

#[given(expr = "the request {string} comes, and then the request {string} comes")]
async fn arrivals(w: &mut World, b: String, c: String) {
    queue::held(w, &b, false).await;
    queue::held(w, &c, false).await;
    assert_eq!(queue::prompts(w).len(), 1, "both arrivals wait behind A");
}

#[then(expr = "the prompt turn of {string} starts before the prompt turn of {string}")]
async fn fifo(w: &mut World, b: String, c: String) {
    assert!(
        queue::has_prompt(w, &b),
        "FR-050 first arrival starts first"
    );
    assert!(
        !queue::has_prompt(w, &c),
        "FR-050 second arrival still waits"
    );
    queue::end(w, &b).await;
    assert!(
        queue::has_prompt(w, &c),
        "FR-050 second arrival starts after the first ends"
    );
    let prompts = queue::prompts(w);
    assert!(
        prompts.iter().position(|p| p.contains(&queue::marker(&b)))
            < prompts.iter().position(|p| p.contains(&queue::marker(&c))),
        "FR-050 ACP prompt order is FIFO"
    );
    queue::end(w, &c).await;
}

#[given(expr = "the prompt turn of the request {string} runs for 60 minutes")]
async fn long_turn(w: &mut World, name: String) {
    queue::held(w, &name, true).await;
}

#[given(expr = "the request {string} waits in the queue, and its API client stays connected")]
async fn long_wait(w: &mut World, name: String) {
    queue::held(w, &name, false).await;
    assert!(
        !queue::has_prompt(w, &name),
        "B waits behind the occupied place"
    );
    w.scenario.time.advance_to(60 * 60 * 1000).await;
}

#[given("each place holds a prompt turn")]
async fn occupied(w: &mut World) {
    queue::full_places(w).await;
}

#[given(expr = "{int} requests wait in the queue")]
async fn queued(w: &mut World, count: usize) {
    queue::queued(w, count).await;
}

#[then("the request waits in the queue")]
async fn waiting(w: &mut World) {
    assert_eq!(
        queue::prompts(w).len(),
        2,
        "FR-050 request waits while both places are occupied"
    );
    let live = w.endpoint.take_live();
    w.places.queued_status = live.status();
    queue::end(w, "A").await;
    for (_, call) in w
        .places
        .calls
        .iter_mut()
        .filter(|(name, _)| name.starts_with('Q'))
    {
        let answer = call.take().unwrap().finish(&mut w.scenario.time).await;
        assert_eq!(
            answer.status, 200,
            "queued API client stays connected until dispatch"
        );
    }
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, 200,
        "FR-050 queued request survives until dispatch"
    );
    w.scenario.answer = Some(answer);
    queue::cleanup(w).await;
}
