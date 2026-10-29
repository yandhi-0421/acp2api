use crate::{
    World, lockout_support as support, places_support as queue, quota_support as quota,
    runtime_support as process,
};
use acp2api_acceptance::{TurnPlan, endpoint_check};
use cucumber::{given, then, when};

#[when(
    regex = r#"^a prompt turn ends with (the stderr error line (?:403.*|401.*|".*|with the status .*|429.*|400.*|503.*)|an error of the ACP call .+|an exit of the official runtime)$"#
)]
async fn upstream(w: &mut World, description: String) {
    support::send_signal(w, &description).await;
    // The shared sign-in assertion reads the authenticated management panel.
    support::panel(w).await;
}

#[given(expr = "the official runtime replies {string} to the next prompt, with no error line")]
async fn normal_reply(w: &mut World, text: String) {
    w.request.plan = Some(TurnPlan::new().chunk(&text));
    w.panel.scenario(&mut w.scenario).await;
}

#[given("stderr marks the prompt turn as failed with the error line 400 \"INVALID_ARGUMENT\"")]
fn failed_reply(w: &mut World) {
    w.request.plan = Some(support::plan(
        "the stderr error line 400 \"INVALID_ARGUMENT\"",
        &w.request.reply,
    ));
}

#[then("acp2api is in the state \"lockout\"")]
async fn lockout(w: &mut World) {
    let html = support::panel(w).await;
    assert!(
        html.contains(">Lockout<"),
        "FR-034/FR-035 effective state must be Lockout"
    );
}

#[then("acp2api is in the state \"normal state\"")]
async fn normal(w: &mut World) {
    let html = support::panel(w).await;
    assert!(
        html.contains(">Running<"),
        "FR-034 non-lockout error leaves the normal state"
    );
}

#[given("a prompt turn runs")]
async fn running(w: &mut World) {
    queue::held(w, "A", false).await;
}

#[given("a quota signal then starts a quota wait")]
async fn quota_wait(w: &mut World) {
    w.request.plan = Some(quota::error(429, Some(quota::RESET)));
    quota::send(
        w,
        endpoint_check::request("POST /v1/messages", "quota signal", false),
    )
    .await;
    let html = support::panel(w).await;
    assert!(
        html.contains(">Quota wait<"),
        "setup reaches quota wait while A runs"
    );
}

#[when("the prompt turn that runs ends with the stderr error line 403 \"PERMISSION_DENIED\"")]
async fn fail_running(w: &mut World) {
    support::finish_active(w, "A", "queue-A").await;
}

#[given(expr = "the request {string} waits in the queue")]
async fn queued(w: &mut World, name: String) {
    w.request.plan = Some(TurnPlan::new().chunk("queued reply"));
    let call = queue::send(
        w,
        endpoint_check::request("POST /v1/messages", &queue::marker(&name), false),
    )
    .await;
    assert!(
        !queue::has_prompt(w, &name),
        "request waits behind the two active turns"
    );
    w.places.calls.push((name, Some(call)));
}

#[when("the prompt turn of \"A\" ends with the stderr error line 403 \"PERMISSION_DENIED\"")]
async fn fail_a(w: &mut World) {
    support::finish_active(w, "A", "quota-A").await;
}

#[then(expr = "acp2api refuses {string} with the lockout message")]
async fn queued_refused(w: &mut World, name: String) {
    let answer = queue::take(w, &name).finish(&mut w.scenario.time).await;
    support::refusal(
        w,
        &answer,
        "acp2api: lockout since <start time> after <signal>; acp2api sends no request to the official runtime until the owner ends the lockout at <address of the management panel>.",
    );
    assert!(
        !queue::has_prompt(w, &name),
        "FR-035 refused queue sends no prompt"
    );
}

#[then("no runtime process runs")]
async fn no_process(w: &mut World) {
    process::observe(w, |w| {
        process::process_ids(w)
            .iter()
            .all(|pid| !process::alive(*pid))
    })
    .await;
    assert!(
        process::process_ids(w)
            .iter()
            .all(|pid| !process::alive(*pid)),
        "FR-035 lockout stops the runtime process tree"
    );
}

#[given(
    regex = r#"^the next prompt turn ends with (the stderr error line 403 "PERMISSION_DENIED" with the message ".*")$"#
)]
fn next_signal(w: &mut World, description: String) {
    w.request.plan = Some(support::plan(&description, ""));
}

#[then(regex = r#"^the error body holds the message "(acp2api: lockout .*)"$"#)]
#[then(regex = r#"^acp2api refuses the request with the message "(acp2api: lockout .*)"$"#)]
fn message(w: &mut World, template: String) {
    support::refusal(w, w.scenario.answer.as_ref().unwrap(), &template);
}

#[then(expr = "the original error text in the message holds {string}")]
fn original(w: &mut World, expected: String) {
    let body = w.scenario.answer.as_ref().unwrap().json();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message
            .split_once("Original error: ")
            .unwrap()
            .1
            .contains(&expected),
        "FR-035 original appeal text is preserved"
    );
}

#[given("acp2api is in lockout since a lockout signal")]
async fn locked(w: &mut World) {
    support::send_signal(w, support::SIGNAL).await;
    w.lockout.calls_before = acp2api_acceptance::system_check::requests(&w.scenario).len();
}

#[then(expr = "the message holds no text {string}")]
fn excludes(w: &mut World, text: String) {
    let body = w.scenario.answer.as_ref().unwrap().json();
    assert!(
        !body["error"]["message"].as_str().unwrap().contains(&text),
        "FR-035 later refusals omit original error text"
    );
}

#[then("the official runtime gets no request")]
fn no_request(w: &mut World) {
    assert!(
        w.lockout.calls_before > 0,
        "positive control: the fake observed the lockout signal request"
    );
    assert_eq!(
        acp2api_acceptance::system_check::requests(&w.scenario).len(),
        w.lockout.calls_before,
        "FR-035 lockout sends no ACP request"
    );
}

#[then("the state file holds the lockout with the signal, the time, and the original error text")]
fn stored(w: &mut World) {
    let state = support::state(w);
    assert_eq!(state["format"], 1);
    let lockout = &state["lockout"];
    assert!(
        lockout.is_object(),
        "FR-035 state file records the lockout evidence"
    );
    assert_eq!(
        lockout["since"],
        support::SINCE,
        "virtual lockout start time"
    );
    let signal = lockout["signal"].as_str().expect("lockout signal");
    assert!(
        signal.contains("403") || signal.contains("PERMISSION_DENIED"),
        "state file identifies the delivered signal"
    );
    assert!(
        lockout["original_error_text"]
            .as_str()
            .unwrap()
            .contains("is suspended"),
        "state file preserves original error evidence"
    );
}

#[then(expr = "the original error text in the state file holds {string} in place of {string}")]
fn redacted(w: &mut World, replacement: String, personal: String) {
    let state = support::state(w);
    let text = state["lockout"]["original_error_text"].as_str().unwrap();
    assert!(
        text.contains(&format!("Account {replacement} is suspended")),
        "FR-056 redacts URL-encoded email in state evidence"
    );
    assert!(
        !state.to_string().contains(&personal),
        "FR-056 state file contains no original email"
    );
}

#[given("the state file holds a lockout")]
#[given("acp2api is stopped, and the state file holds a lockout")]
async fn saved(w: &mut World) {
    support::saved(w).await;
}

#[given("the state file is damaged")]
async fn damaged(w: &mut World) {
    w.scenario.start().await;
    w.scenario.stop().await;
    std::fs::write(support::state_path(w), "{ damaged state").unwrap();
}

#[given("the state file exists, and acp2api cannot read it")]
async fn unreadable(w: &mut World) {
    support::saved(w).await;
    let path = support::state_path(w);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::metadata(&path).unwrap().permissions();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        w.lockout.unreadable = Some((path.clone(), permissions));
        assert_eq!(
            std::fs::read(path).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied,
            "actual unreadable state file"
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        w.lockout.unreadable = Some(
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&path)
                .unwrap(),
        );
        assert!(std::fs::read(path).is_err(), "actual unreadable state file");
    }
}

#[when("30 days pass")]
async fn days(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 30 * 24 * 60 * 60 * 1000)
        .await;
}

#[when("the owner deletes the state file and starts acp2api")]
async fn deleted(w: &mut World) {
    std::fs::remove_file(support::state_path(w)).unwrap();
    w.scenario.restart().await;
}

#[then("acp2api is not in the state \"lockout\"")]
async fn not_locked(w: &mut World) {
    let html = support::panel(w).await;
    assert!(
        html.contains(">Running<"),
        "FR-035 deletion starts in the normal state"
    );
}

#[then("no request gets the lockout message")]
async fn no_notice(w: &mut World) {
    for endpoint in [
        "POST /v1/messages",
        "POST /v1/chat/completions",
        "POST /v1/responses",
    ] {
        quota::send(
            w,
            endpoint_check::request(endpoint, "after state deletion", false),
        )
        .await;
        let answer = w.scenario.answer.as_ref().unwrap();
        assert_eq!(
            answer.status, 200,
            "FR-035 deletion admits requests to {endpoint}"
        );
        assert_eq!(crate::slash_and_thought_support::reply(answer), "OK");
    }
}

#[then("the data folder holds a new state file")]
fn new_file(w: &mut World) {
    let state = support::state(w);
    assert_eq!(state["format"], 1);
    assert_eq!(
        state.get("lockout"),
        Some(&serde_json::Value::Null),
        "FR-035 new state file has no lockout"
    );
}
