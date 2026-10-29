use crate::{
    World, places_support as queue, runtime_support as process, sign_in_support as support,
};
use acp2api_acceptance::{Setup, client::ApiCall, fake::FakeRuntime};
use cucumber::{given, then, when};
use serde_json::json;

#[given("the official runtime has no valid sign-in")]
fn unsigned(w: &mut World) {
    w.scenario.plan(Setup::normal());
    w.scenario.fake().set_signed_in(false);
}

#[given("acp2api is in the state \"sign-in required\"")]
async fn required(w: &mut World) {
    support::start(w).await;
}

#[given("the official runtime has a valid sign-in again")]
fn valid_again(w: &mut World) {
    w.scenario.fake().set_signed_in(true);
}

#[given("acp2api is not in lockout")]
fn not_locked(w: &mut World) {
    assert!(
        !w.scenario.root.path.join("data/state.json").exists(),
        "fixture has no stored lockout"
    );
}

#[when("the owner starts acp2api without an API request")]
#[when("the owner starts acp2api for the first time")]
async fn first_start(w: &mut World) {
    support::start(w).await;
}

#[then(expr = "acp2api refuses the request with the message {string}")]
fn refused(w: &mut World, expected: String) {
    assert_eq!(
        expected.replace("<address of the sign-in entry>", &support::sign_in_entry(w)),
        support::message(w)
    );
    support::refusal(w, w.scenario.answer.as_ref().unwrap());
}

#[then("acp2api checks the sign-in one time")]
fn request_check(w: &mut World) {
    assert_eq!(
        process::calls(w, "session/new").len() - w.sign_in.checks_before,
        1,
        "FR-038 one sign-in check for this request"
    );
}

#[then("acp2api checked the sign-in one time")]
fn startup_check(w: &mut World) {
    assert_eq!(
        process::calls(w, "session/new").len(),
        1,
        "FR-038 exactly one startup session/new"
    );
}

#[then("acp2api is in the state \"sign-in required\"")]
async fn required_state(w: &mut World) {
    support::sign_in_required(w).await;
}

#[when("the owner signs in through the management panel")]
async fn sign_in(w: &mut World) {
    support::authenticate(w, true, None).await;
}

#[given("acp2api was in the state \"sign-in required\" and then stopped")]
async fn stopped(w: &mut World) {
    w.startup.initialize().await;
    w.startup.seed_config();
    w.startup.panel_runtime();
    let fake = FakeRuntime::new(w.startup.panel_hub());
    fake.set_signed_in(false);
    support::initialize(&fake);
    w.startup.launch_data(vec![]).await;
    let mut time = acp2api_acceptance::timeline::TimeLine::new(w.startup.panel_hub());
    time.settle().await;
    w.sign_in.sessions_before_restart = fake.record().session_meta().len();
    w.startup.stop().await;
    w.panel.bind(
        w.startup.root(),
        &w.startup.data,
        w.startup.panel_hub(),
        format!("127.0.0.1:{}", w.startup.root().port),
    );
}

#[given("the official runtime now has a valid sign-in")]
fn now_valid(w: &mut World) {
    FakeRuntime::new(w.startup.panel_hub()).set_signed_in(true);
}

#[then("acp2api checked the sign-in one time through a new ACP session")]
async fn restarted_check(w: &mut World) {
    w.panel.time().settle().await;
    let record = FakeRuntime::new(w.startup.panel_hub()).record();
    assert_eq!(
        record.session_meta().len() - w.sign_in.sessions_before_restart,
        1,
        "FR-038 restart checks sign-in exactly once"
    );
    assert_eq!(
        record.client_info().len(),
        2,
        "restart uses a new runtime process"
    );
    assert!(
        record.prompts().is_empty(),
        "startup checks start no prompt turn"
    );
}

#[then("the state file holds no record of the state \"sign-in required\"")]
fn no_record(w: &mut World) {
    let path = w.startup.data.join("state.json");
    match std::fs::read_to_string(path) {
        Ok(text) => assert!(
            !text.contains("sign-in") && !text.contains("sign_in"),
            "FR-038 sign-in required is not persisted"
        ),
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
    }
}

#[when("the owner starts the sign-in in the management panel")]
#[given("the owner started the sign-in in the management panel")]
async fn begin(w: &mut World) {
    support::authenticate(w, false, None).await;
}

#[then(expr = "the official runtime gets a sign-in request with the method {string}")]
fn authentication_method(w: &mut World, method: String) {
    let calls = process::calls(w, "authenticate");
    assert_eq!(calls.len(), 1, "FR-039 one ACP authenticate request");
    assert_eq!(
        calls[0]["params"]["methodId"], method,
        "FR-039 oauth-personal only"
    );
}

#[then("the official runtime gets no sign-in request with another method")]
fn no_other_method(w: &mut World) {
    assert!(
        process::calls(w, "authenticate")
            .iter()
            .all(|v| v["params"]["methodId"] == "oauth-personal")
    );
}

#[given(expr = "the official runtime gives the sign-in address {string}")]
fn sign_in_address(w: &mut World, address: String) {
    w.sign_in.address = address;
}

#[then("the management panel shows the time left, which starts at 10 minutes")]
async fn countdown(w: &mut World) {
    let html = support::page(w).await;
    assert!(
        html.contains("10:00") || html.contains("10 minutes"),
        "FR-039 initial sign-in time left is 10 minutes"
    );
    w.scenario
        .time
        .advance_to(w.sign_in.wait_started + 60_000)
        .await;
    let html = support::page(w).await;
    assert!(
        html.contains("9:00") || html.contains("9 minutes"),
        "FR-039 time left follows the virtual clock"
    );
}

#[then(expr = "the management panel shows the sign-in address {string}")]
async fn shows_address(w: &mut World, address: String) {
    assert!(
        support::page(w).await.contains(&address),
        "FR-039 panel displays the address received from ACP stderr"
    );
}

#[then(expr = "the management panel shows the entry {string}")]
async fn cancel_entry(w: &mut World, text: String) {
    let html = support::page(w).await;
    assert!(
        html.contains(&text) && html.contains("/manage/cancel-sign-in"),
        "FR-039 cancellation is an owner action"
    );
}

#[when("the owner cancels the sign-in")]
async fn cancel(w: &mut World) {
    support::cancel(w).await;
}

#[when("10 minutes pass with no sign-in")]
async fn deadline(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.sign_in.wait_started + 599_999)
        .await;
    assert_eq!(
        process::process_ids(w),
        w.sign_in.process_before,
        "FR-039 no replacement before 10 minutes"
    );
    w.scenario
        .time
        .advance_to(w.sign_in.wait_started + 600_000)
        .await;
}

#[then("acp2api replaces the runtime process")]
async fn replaced(w: &mut World) {
    support::replaced(w).await;
}

#[when("the owner starts and cancels the sign-in 3 times in 10 minutes")]
async fn repeated(w: &mut World) {
    let start = w.scenario.hub.now();
    for n in 0..3 {
        w.scenario.time.advance_to(start + n * 180_000).await;
        support::authenticate(w, false, None).await;
        support::cancel(w).await;
        support::replaced(w).await;
    }
}

#[then("acp2api is not in the state \"runtime unavailable\"")]
async fn available(w: &mut World) {
    let answer = w.panel.api(&w.scenario.root.key).await;
    support::model_list_refusal(w, &answer);
    assert_eq!(
        process::servers(w).len(),
        4,
        "FR-039 three planned replacements do not count as failures"
    );
    assert!(process::alive(
        process::servers(w).last().unwrap()["pid"].as_u64().unwrap() as u32
    ));
}

#[when(
    expr = "the owner signs in through the management panel, and the official runtime keeps the token {string}"
)]
async fn with_token(w: &mut World, token: String) {
    support::authenticate(w, true, Some(token)).await;
}

#[then(expr = "the configuration file, the state file, and the logs do not hold {string}")]
async fn token_absent(w: &mut World, token: String) {
    w.scenario.time.settle().await;
    assert_eq!(
        std::fs::read_to_string(
            w.scenario
                .root
                .path
                .join("data/runtime-home/synthetic-token")
        )
        .unwrap(),
        token,
        "fake runtime kept the synthetic token"
    );
    for path in [
        w.scenario.root.path.join("data/config.toml"),
        w.scenario.root.path.join("data/state.json"),
    ] {
        match std::fs::read_to_string(path) {
            Ok(text) => assert!(!text.contains(&token), "FR-039 no token in acp2api storage"),
            Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
        }
    }
    for entry in std::fs::read_dir(w.scenario.root.path.join("data/logs")).unwrap() {
        let entry = entry.unwrap();
        assert!(
            entry.metadata().unwrap().len() <= 4 * 1024 * 1024,
            "per-log cap: 4 MiB"
        );
        assert!(
            !std::fs::read_to_string(entry.path())
                .unwrap()
                .contains(&token),
            "FR-039 no token in logs"
        );
    }
}

#[given("an editor signed in the official runtime with its own runtime home")]
fn editor(w: &mut World) {
    w.scenario.plan(Setup::normal());
    let home = w.scenario.root.path.join("home/.gemini/antigravity-acp");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("settings.json"),
        r#"{"auth":{"type":"oauth-personal"}}"#,
    )
    .unwrap();
    std::fs::write(
        home.parent()
            .unwrap()
            .join("acp2api-synthetic-editor-sign-in"),
        "synthetic editor sign-in",
    )
    .unwrap();
    for _ in 0..5 {
        w.scenario.fake().script_call(
            "session/new",
            acp2api_acceptance::rpc_script::RpcReply::FromCall(
                crate::sign_in_facts::editor_home_reply,
            ),
        );
    }
    w.scenario.fake().set_signed_in(false);
}

#[when(expr = "request {string} arrives with a stream")]
#[given(expr = "the sign-in check of the request {string} runs")]
#[given(expr = "the sign-in check of request {string} waits for a runtime reply")]
async fn the_sign_in_check_waits_for_a_runtime_reply(w: &mut World, name: String) {
    support::held_check(w, &name, false).await;
}

#[given(expr = "request {string} holds a place for its sign-in check")]
#[given(expr = "request {string} waits for its own sign-in check")]
async fn the_request_waits_for_its_sign_in_check(w: &mut World, name: String) {
    support::held_check(w, &name, true).await;
}

#[then(
    expr = "the sign-in check of {string} starts only after the end of the sign-in check of {string}"
)]
async fn serialized(w: &mut World, b: String, a: String) {
    assert!(
        support::held(w, &a),
        "FR-038 first request starts its sign-in check"
    );
    assert_eq!(
        process::calls(w, "session/new").len(),
        w.sign_in.checks_before + 1,
        "FR-049 second check waits for the first place"
    );
    assert_eq!(
        support::live(w, &b).status(),
        None,
        "FR-026 queued check has no status"
    );
    support::release(w, &a);
    let answer = queue::take(w, &a).finish(&mut w.scenario.time).await;
    support::refusal(w, &answer);
    w.scenario.time.settle().await;
    assert_eq!(
        process::calls(w, "session/new").len(),
        w.sign_in.checks_before + 2,
        "FR-049 release dispatches the second check"
    );
}

#[when("330 s pass")]
async fn long_wait(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 330_000)
        .await;
}

#[then(expr = "{string} has no response header, HTTP status, or stream event")]
#[then(expr = "{string} has no HTTP status yet")]
fn no_status(w: &mut World, name: String) {
    assert_eq!(
        support::live(w, &name).status(),
        None,
        "FR-026 pending sign-in admission sends no HTTP response head"
    );
    assert!(
        support::live(w, &name).events().is_empty(),
        "FR-011/FR-026 pending check sends no event"
    );
}

#[then(expr = "{string} counts as one request in the queue")]
async fn one_queued(w: &mut World, _name: String) {
    support::queue_count(w, 1).await;
}

#[when(expr = "the sign-in check of {string} fails")]
async fn failed_check(w: &mut World, name: String) {
    support::release(w, &name);
    w.scenario.time.settle().await;
    let answer = queue::take(w, &name).finish(&mut w.scenario.time).await;
    support::refusal(w, &answer);
    w.scenario.answer = Some(answer);
}

#[then(expr = "the sign-in check of {string} starts")]
#[then(expr = "the official runtime gets one sign-in check for {string}")]
fn check_started(w: &mut World, name: String) {
    assert!(
        support::held(w, &name),
        "FR-038 queued request {name} keeps its own required check"
    );
}

#[then(expr = "{string} gets HTTP 400 with M-13 and no stream event")]
fn stream_refused(w: &mut World, _name: String) {
    support::refusal(w, w.scenario.answer.as_ref().unwrap());
}

#[when(expr = "the check of {string} succeeds")]
async fn succeeded(w: &mut World, name: String) {
    w.scenario.fake().set_signed_in(true);
    support::release(w, &name);
    w.scenario.time.settle().await;
}

#[when("that check succeeds")]
async fn second_success(w: &mut World) {
    w.scenario.fake().set_signed_in(true);
    support::release(w, "B");
    w.scenario.time.settle().await;
}

#[then(expr = "{string} keeps the place for its prompt turn")]
fn keeps_place(w: &mut World, name: String) {
    assert!(
        queue::has_prompt(w, &name),
        "FR-049 successful check continues into the prompt in its place"
    );
    assert!(
        !support::held(w, "B"),
        "FR-049 B stays queued while A prompts"
    );
}

#[then(expr = "{string} gets HTTP 200 and its prompt turn starts")]
async fn accepted(w: &mut World, name: String) {
    assert_eq!(
        support::live(w, &name).status(),
        Some(200),
        "FR-026 acceptance follows its own passed check"
    );
    assert!(
        queue::has_prompt(w, &name),
        "FR-038 checked request starts a prompt"
    );
    queue::end(w, &name).await;
}

#[given("all places are occupied")]
async fn occupied(w: &mut World) {
    support::start(w).await;
    for name in ["A", "C"] {
        support::held_check(w, name, false).await;
    }
}

#[given("a request waits for its required sign-in check")]
async fn waiting_check(w: &mut World) {
    let live = queue::send(
        w,
        acp2api_acceptance::endpoint_check::request("POST /v1/messages", &queue::marker("B"), true),
    )
    .await;
    w.places.calls.push(("B".into(), Some(live)));
    w.sign_in.queued_before = 1;
    w.sign_in.calls_before_disconnect = process::calls(w, "session/new").len();
    support::queue_count(w, 1).await;
}

#[when("its API client disconnects")]
async fn disconnect(w: &mut World) {
    drop(queue::take(w, "B"));
    let handled = |w: &World| {
        acp2api_acceptance::journal_check::terminal(&w.scenario)
            .iter()
            .any(|record| record["result"] == "disconnected")
    };
    process::observe(w, handled).await;
    assert!(handled(w), "FR-031 queued disconnect reaches acp2api");
}

#[then("the queue count decreases by one")]
async fn decreased(w: &mut World) {
    support::queue_count(w, w.sign_in.queued_before - 1).await;
}

#[then("the official runtime gets no call for that request")]
async fn no_call(w: &mut World) {
    for name in ["A", "C"] {
        support::release(w, name);
    }
    w.scenario.time.settle().await;
    assert_eq!(
        process::calls(w, "session/new").len(),
        w.sign_in.calls_before_disconnect,
        "FR-031 disconnected queued request never checks"
    );
    assert!(!queue::has_prompt(w, "B"));
}

#[given("all places are occupied by sign-in checks")]
async fn full_checks(w: &mut World) {
    for name in ["A", "C"] {
        support::held_check(w, name, false).await;
    }
}

#[given(expr = "{int} requests wait for their own sign-in checks")]
async fn queued_checks(w: &mut World, count: usize) {
    assert!(count <= 32, "sign-in queue request cap: 32");
    for n in 0..count {
        let name = format!("Q{n}");
        let call = queue::send(
            w,
            acp2api_acceptance::endpoint_check::request(
                "POST /v1/messages",
                &queue::marker(&name),
                true,
            ),
        )
        .await;
        w.places.calls.push((name, Some(call)));
    }
}

#[when(expr = "another API client sends a request with a stream to {string}")]
async fn overflow(w: &mut World, endpoint: String) {
    let mut call = ApiCall::one_user_message(&endpoint, "overflow before sign-in check");
    call.body["stream"] = json!(true);
    let live = queue::send(w, call).await;
    w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
}

#[then(expr = "the request gets HTTP 529 with M-17 and {string}")]
fn full(w: &mut World, header: String) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert_eq!(
        answer.status, 529,
        "FR-050 pre-check requests count toward the queue cap"
    );
    let (name, value) = header.split_once(": ").unwrap();
    assert_eq!(answer.headers.get(name).map(String::as_str), Some(value));
    assert_eq!(
        answer.json(),
        json!({"type":"error","error":{"type":"overloaded_error","message":"acp2api: the queue is full (32 requests). Send the request again later."}})
    );
}

#[then("it has no stream event and starts no ACP call")]
fn no_overflow_call(w: &mut World) {
    assert!(w.scenario.answer.as_ref().unwrap().events.is_empty());
    assert_eq!(
        process::calls(w, "session/new").len(),
        w.sign_in.checks_before + 2,
        "FR-050 queue overflow starts no ACP call"
    );
    assert!(process::calls(w, "session/prompt").is_empty());
}
