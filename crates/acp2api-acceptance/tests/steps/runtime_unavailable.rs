use crate::{
    World, build_support as build, compatibility_support as check, runtime_support as process,
    unavailable_support as support,
};
use acp2api_acceptance::{build_fixture, client::ApiCall, setup::file_hash};
use cucumber::{given, then, when};
use serde_json::json;

#[given("acp2api found only builds that are not accepted versions")]
async fn unaccepted(w: &mut World) {
    build::prepare(w).await;
    build::install(w, "the install folder of Zed", "1.6.0", false);
    w.startup.launch_data(vec![]).await;
    build::assert_found(w, "the install folder of Zed").await;
}

#[given("the runtime process had 3 failures in 10 minutes")]
async fn failed(w: &mut World) {
    support::three_failures(w).await;
}

#[given("acp2api is in the state \"runtime unavailable\"")]
async fn unavailable(w: &mut World) {
    support::three_failures(w).await;
    let answer = build::request(w).await;
    process::error(
        &answer,
        400,
        &format!(
            "acp2api: the official runtime is unavailable: the runtime process failed 3 times in 10 minutes. Accepted versions: 1.4.0. See http://127.0.0.1:{}/.",
            w.scenario.root.port
        ),
    );
}

#[given("an accepted version of the official runtime is now available")]
async fn accepted(w: &mut World) {
    let folder = support::install(w, "path", "1.4.0", "normal");
    let run: acp2api_acceptance::adapters::RunFile =
        toml::from_str(&std::fs::read_to_string(w.scenario.root.path.join("run.toml")).unwrap())
            .unwrap();
    assert!(run.release.builds.iter().any(|listed| {
        listed.agy_acp_server_sha256 == file_hash(&folder.join(build_fixture::SERVER))
            && listed.localharness_external_sha256
                == file_hash(&folder.join(build_fixture::HARNESS))
    }));
}

#[given("the official runtime has a valid sign-in")]
fn signed_in(w: &mut World) {
    assert!(
        w.scenario.fake().signed_in(),
        "normal fake has a valid sign-in"
    );
}

#[given("no condition other than \"runtime unavailable\" stops requests")]
fn clear_conditions(w: &mut World) {
    assert_eq!(
        process::calls(w, "session/new").len(),
        3,
        "each prior start passed its sign-in check"
    );
    assert!(
        process::calls(w, "session/prompt").is_empty(),
        "no prompt supplied a quota or lockout signal"
    );
    let state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(w.scenario.root.path.join("data/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        state.get("lockout"),
        Some(&serde_json::Value::Null),
        "normal fixture has no stored lockout"
    );
}

#[when("30 minutes pass with no event that starts a new try")]
async fn idle(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 1_800_000)
        .await;
}

#[then("acp2api started no runtime process after the third failure")]
fn no_automatic_start(w: &mut World) {
    assert_eq!(
        process::servers(w).len(),
        3,
        "FR-040 no automatic runtime start during 30 virtual minutes"
    );
    assert_eq!(
        w.scenario
            .hub
            .entries()
            .iter()
            .filter(|entry| entry.event["kind"] == "launch")
            .count(),
        3,
        "FR-040 no launch attempt after the third failure"
    );
}

#[when("the owner uses the retry entry of the management panel")]
async fn retry(w: &mut World) {
    let answer = support::action(w, "/manage/retry", json!({})).await;
    assert_eq!(
        answer.status, 200,
        "FR-040 management retry must start a new try"
    );
}

#[when("the owner sets a new path of agy_acp_server")]
async fn new_path(w: &mut World) {
    let folder = support::install(w, "owner-path", "1.4.0", "normal");
    let answer = support::action(
        w,
        "/manage/runtime-path",
        json!({"agy_acp_server_path":folder.join(build_fixture::SERVER)}),
    )
    .await;
    assert_eq!(answer.status, 200, "FR-040 new path starts a new try");
}

#[when("the owner starts a new search")]
async fn search(w: &mut World) {
    let answer = support::action(w, "/manage/search", json!({})).await;
    assert_eq!(answer.status, 200, "FR-040 new search starts a new try");
}

#[when("a compatibility check of a found build passes")]
async fn checked(w: &mut World) {
    let folder = support::install(w, "path", "1.6.0", "build-1.6.0");
    let search = support::action(w, "/manage/search", json!({})).await;
    assert_eq!(
        search.status, 200,
        "discover the candidate through the management panel"
    );
    crate::compatibility_facts::configure(&w.scenario.fake(), crate::compatibility_facts::echo);
    let identity = check::identity(&folder).join(":");
    let answer = support::action(w, "/manage/compatibility-check", json!({"build":identity})).await;
    assert_eq!(
        answer.status, 200,
        "FR-040 a passed compatibility check starts a new try"
    );
    let entry = check::checked(&folder, &w.scenario.root.path.join("data"))
        .expect("the real compatibility check saved both candidate hashes");
    assert_eq!(
        entry["version"], "1.6.0",
        "the real compatibility check passed"
    );
}

#[then("the start of a runtime process succeeds")]
async fn started(w: &mut World) {
    process::observe(w, |w| process::servers(w).len() > 3).await;
    let servers = process::servers(w);
    assert!(
        servers.len() > 3,
        "FR-040 the recovery event must start a runtime process"
    );
    assert!(
        process::alive(servers.last().unwrap()["pid"].as_u64().unwrap() as u32),
        "FR-040 recovered runtime process is alive"
    );
    let answer = w
        .scenario
        .client()
        .await
        .send(ApiCall::one_user_message(
            "GET /v1/models",
            "recovered catalog",
        ))
        .finish(&mut w.scenario.time)
        .await;
    assert_eq!(
        answer.status, 200,
        "FR-040 successful start ends runtime unavailable and completes the sign-in check"
    );
    assert!(
        answer.json()["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["id"] == "gemini-3.8-flash"),
        "recovered runtime supplied the catalog"
    );
}
