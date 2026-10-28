use crate::World;
use acp2api_acceptance::{client::ApiCall, timeline::TimeLine};
use cucumber::{given, then, when};

#[given(expr = "acp2api runs for a computer account with the data folder {string}")]
fn first(w: &mut World, data: String) {
    prepare_first(w, &data);
}
fn prepare_first(w: &mut World, data: &str) {
    w.account.first(data);
    w.scenario.hub.close();
    w.scenario.hub = w.account.instances[0].hub.clone();
}
#[given(
    expr = "acp2api runs for computer account {string} with data folder {string} and port {int}"
)]
fn account_a(w: &mut World, name: String, data: String, port: u16) {
    assert_eq!(
        (name.as_str(), port),
        ("A", 8329),
        "Gherkin aliases use an isolated native account root and free port"
    );
    prepare_first(w, &data);
}
#[given(expr = "its system temporary folder is {string}")]
async fn first_temp(w: &mut World, temp: String) {
    launch_first(w, &temp).await;
}
async fn launch_first(w: &mut World, temp: &str) {
    w.account.launch(0, temp).await;
    assert!(
        w.account.instances[0].exited.is_none(),
        "first host must run before the duplicate action"
    );
}
#[given("the first acp2api has a temporary folder and a session cwd folder")]
fn folders(w: &mut World) {
    w.account.keep_folders();
}
#[when(expr = "the same computer account starts acp2api with the data folder {string}")]
fn duplicate(w: &mut World, data: String) {
    w.account.keep_folders();
    w.account.second(&data, false);
}
#[when(expr = "that start uses the system temporary folder {string}")]
async fn second_temp(w: &mut World, temp: String) {
    w.account.launch(1, &temp).await;
}
#[when(expr = "computer account {string} starts acp2api with data folder {string} and port {int}")]
fn account_b(w: &mut World, name: String, data: String, port: u16) {
    assert_eq!((name.as_str(), port), ("B", 8330));
    w.account.second(&data, true);
}
#[then(expr = "the second start fails with the message {string}")]
fn rejected(w: &mut World, message: String) {
    w.account.preserved();
    let actual = &w.account.instances[1];
    assert_eq!(
        actual.exited,
        Some(false),
        "FR-006 duplicate account start must fail before serving or starting a runtime process"
    );
    let expected = message.replace("<address>", &w.account.instances[0].address());
    assert!(
        actual.terminal().lines().any(|l| l == expected),
        "FR-006 exact M-24 with validated first panel address"
    );
}
#[then("the address in the message is the address of the management panel of the first acp2api")]
fn address(w: &mut World) {
    let first = &w.account.instances[0];
    let address = first
        .terminal()
        .lines()
        .find_map(|line| {
            line.split_once("management panel at ")
                .map(|(_, address)| address.to_owned())
        })
        .expect("actual first panel publication");
    assert!(
        w.account.instances[1].terminal().contains(&address),
        "FR-006 duplicate diagnostic uses the actual first publication"
    );
}
async fn answers(w: &mut World, index: usize) {
    let host = &w.account.instances[index];
    w.scenario.hub = host.hub.clone();
    w.request.queue_next(&w.scenario);
    let answer = host
        .client()
        .send(ApiCall::one_user_message(
            "POST /v1/messages",
            &format!("account request {index}"),
        ))
        .finish(&mut TimeLine::new(host.hub.clone()))
        .await;
    assert_eq!(
        answer.status, 200,
        "FR-006 instance {index} answers a real API request"
    );
    assert_eq!(
        answer.json()["content"],
        serde_json::json!([{"type":"text","text":"OK"}]),
        "FR-006 next prompt completes through the guarded fake"
    );
}
#[then("the first acp2api answers the next request")]
async fn first_answers(w: &mut World) {
    answers(w, 0).await;
}
#[then("both instances answer requests on their own ports")]
async fn both(w: &mut World) {
    assert_ne!(
        w.account.instances[0].root.port,
        w.account.instances[1].root.port
    );
    for index in 0..2 {
        answers(w, index).await;
    }
}
#[when("computer account \"B\" stops its acp2api")]
async fn stop_b(w: &mut World) {
    w.account.instances[1].stop().await;
}
#[then("the temporary folder and the session cwd folder of the first acp2api are still there")]
fn preserved(w: &mut World) {
    w.account.preserved();
}
#[given("a first acp2api runs for a computer account")]
async fn live_owner(w: &mut World) {
    prepare_first(w, "D1");
    launch_first(w, "T1").await;
}
#[given(regex = r#"^its account coordination data at "P" is (unreadable|damaged)$"#)]
fn corrupt(w: &mut World, condition: String) {
    w.account.damage(&condition);
}
#[when("a second acp2api starts for that account")]
async fn second(w: &mut World) {
    w.account.second("D2", false);
    w.account.launch_waiting(1, "T2").await;
}
#[then(expr = "it fails with {string}")]
fn unusable(w: &mut World, message: String) {
    assert_eq!(
        w.account.instances[1].exited,
        Some(false),
        "FR-006 unusable account coordination data must stop the new start with M-28"
    );
    let expected = message.replace(
        " at P.",
        &format!(" at {}.", w.account.failed_path.display()),
    );
    assert!(
        w.account.instances[1]
            .terminal()
            .lines()
            .any(|l| l == expected),
        "FR-006 exact M-28 identifies the actual failed path"
    );
}
#[then("it does not delete or replace an account lock")]
fn lock_files(w: &mut World) {
    w.account.preserved_locks();
}
#[then("the first acp2api still answers requests")]
async fn still_answers(w: &mut World) {
    answers(w, 0).await;
}

async fn hold_owner(w: &mut World) {
    prepare_first(w, "D1");
    w.account.hold_first().await;
}
#[given("a first acp2api owns the account lock and has not published its panel address")]
async fn unpublished_owner(w: &mut World) {
    hold_owner(w).await;
}
#[given("a second start waits for the first panel address")]
async fn waiting_start(w: &mut World) {
    hold_owner(w).await;
    w.account.second("D2", false);
    w.account.launch_waiting(1, "T2").await;
}
#[when("10 minutes pass before the first address exists")]
async fn ten_minutes(w: &mut World) {
    let mut time = TimeLine::new(w.account.instances[1].hub.clone());
    time.advance_to(time.hub.now() + 600_000).await;
}
#[then("the second start still waits and has shown no M-24")]
fn still_waiting(w: &mut World) {
    assert!(
        !w.account.instances[1].published() && w.account.instances[1].exited.is_none(),
        "FR-006 a duplicate must wait at the startup gate without a deadline or its own panel"
    );
    assert!(
        !w.account.instances[1]
            .terminal()
            .contains("another acp2api runs"),
        "FR-006 no M-24 before a validated first address"
    );
}
#[when("the first acp2api publishes its panel address")]
async fn publishes(w: &mut World) {
    w.account.release_first().await;
}
#[then("the second start fails with M-24 and that address")]
async fn published_duplicate(w: &mut World) {
    w.account.resume_second().await;
    let second = &w.account.instances[1];
    assert_eq!(
        second.exited,
        Some(false),
        "FR-006 the waiting duplicate fails after publication"
    );
    let expected = format!(
        "acp2api: another acp2api runs for this computer account. Its management panel is at {}.",
        w.account.instances[0].address()
    );
    assert!(
        second.terminal().lines().any(|line| line == expected),
        "FR-006 M-24 uses the first validated address"
    );
}
#[when("the owner cancels the second start")]
async fn cancel_start(w: &mut World) {
    w.account.early_publication = w.account.instances[1].published();
    w.account.instances[1].stop().await;
}
#[then("the second start exits with no change to the first instance's files")]
fn cancelled_start(w: &mut World) {
    assert!(
        w.account.instances[1].exited.is_some(),
        "FR-006 the cancelled start exits"
    );
    assert!(
        !w.account.early_publication,
        "FR-006 cancellation of a waiting start must precede its own panel publication"
    );
    w.account.first_files_unchanged();
}
#[then("the first acp2api can still publish its panel address and answer requests")]
async fn first_continues(w: &mut World) {
    w.account.release_first().await;
    answers(w, 0).await;
}
#[when("the first start crashes before it publishes that address")]
fn crashes(w: &mut World) {
    assert!(
        !w.account.instances[0].published(),
        "the real first holder has not published"
    );
    w.account.early_publication = w.account.instances[1].published();
    w.account.instances[0].kill();
}
#[then("the second start becomes the sole instance and publishes its own panel address")]
async fn takes_over(w: &mut World) {
    w.account.resume_second().await;
    assert!(
        !w.account.early_publication,
        "FR-006 the waiting start must not publish before the first lock holder crashes"
    );
    assert!(
        w.account.instances[0].exited.is_some() && w.account.instances[1].published(),
        "FR-006 the survivor becomes the sole published instance"
    );
    answers(w, 1).await;
}
#[then("it shows no M-24 with a stale or absent address")]
fn no_stale_address(w: &mut World) {
    assert!(
        !w.account.instances[1]
            .terminal()
            .contains("another acp2api runs"),
        "FR-006 recovery never reports an absent or stale address"
    );
}
