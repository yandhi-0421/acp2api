use crate::{World, end_lockout_support, lockout_support, quota_support, unavailable_support};
use cucumber::{given, then, when};
use serde_json::json;
use std::fs;

const M_30: &str = "acp2api: the lockout is not in the state file state.json, because the write failed. A restart of acp2api ends this lockout. acp2api tries the write again.";

#[given("acp2api cannot write the state file")]
async fn cannot_write_state_file(w: &mut World) {
    w.scenario.start().await;
    let path = lockout_support::state_path(w);
    fs::write(&path, r#"{"format":1,"lockout":null,"checked_builds":[]}"#).unwrap();
    cannot_make_state_file(w);
}

#[when("acp2api can write the state file")]
fn can_write_state_file(w: &mut World) {
    w.lockout.restore();
}

#[given("a lockout signal started a lockout")]
async fn lockout_signal_started(w: &mut World) {
    lockout_support::send_signal(w, lockout_support::SIGNAL).await;
}

#[when("a request comes")]
async fn request_comes(w: &mut World) {
    quota_support::send(
        w,
        acp2api_acceptance::endpoint_check::request("POST /v1/messages", "lockout retry", false),
    )
    .await;
}

#[then("the state file holds the lockout")]
async fn state_file_holds_lockout(w: &mut World) {
    for _ in 0..200 {
        if lockout_support::state_path(w).is_file() {
            let state = lockout_support::state(w);
            if state["lockout"].is_object() {
                break;
            }
        }
        w.scenario.time.settle().await;
    }
    let state = lockout_support::state(w);
    assert_eq!(state["format"], 1);
    let lockout = &state["lockout"];
    assert!(
        lockout.is_object(),
        "FR-035 state file records the lockout evidence"
    );
    let signal = lockout["signal"].as_str().expect("lockout signal");
    assert!(
        signal.contains("403") || signal.contains("PERMISSION_DENIED"),
        "state file identifies the delivered signal"
    );
}

#[then("the management panel shows the notice that the lockout is not in the state file")]
async fn shows_lockout_notice(w: &mut World) {
    let html = end_lockout_support::page(w).await;
    assert!(
        html.contains(M_30),
        "FR-035/M-30 panel shows notice when state file cannot be written: {html}"
    );
}

#[then("the management panel does not show the notice that the lockout is not in the state file")]
async fn does_not_show_lockout_notice(w: &mut World) {
    let html = end_lockout_support::page(w).await;
    assert!(
        !html.contains(M_30),
        "FR-035/M-30 panel clears notice once state file is written"
    );
}

#[given("acp2api is stopped, and the data folder holds no state file")]
async fn stopped_no_state_file(w: &mut World) {
    w.scenario.stop().await;
    w.scenario
        .root
        .seed(&w.scenario.hub, &acp2api_acceptance::Setup::normal());
    let scenario_data = w.scenario.root.path.join("data");
    let _ = fs::create_dir_all(scenario_data.join("logs"));
    let path = lockout_support::state_path(w);
    if path.exists() {
        if path.is_dir() {
            let _ = fs::remove_dir_all(&path);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
}

#[given("acp2api cannot make the state file")]
fn cannot_make_state_file(w: &mut World) {
    let data_dir = w.scenario.root.path.join("data");
    w.lockout.deny_new_files(data_dir);
}

#[given("the owner started acp2api")]
#[when("the owner started acp2api")]
async fn owner_started_acp2api(w: &mut World) {
    w.scenario.start().await;
}

#[when("acp2api can make the state file")]
fn can_make_state_file(w: &mut World) {
    w.lockout.restore();
}

#[then("the management panel names the state file state.json")]
async fn panel_names_state_file(w: &mut World) {
    let html = end_lockout_support::page(w).await;
    assert!(
        html.contains("state.json"),
        "FR-035 management panel names state file state.json; panel: {html}"
    );
}

#[then("acp2api starts no runtime process")]
fn starts_no_runtime_process(w: &mut World) {
    let launches: Vec<_> = w
        .scenario
        .fake()
        .record()
        .entries
        .into_iter()
        .filter(|e| e.event["kind"] == "launch")
        .collect();
    assert!(
        launches.is_empty(),
        "FR-035 acp2api starts no runtime process when state file cannot be made: {launches:?}"
    );
}

#[then("acp2api started a runtime process")]
fn started_runtime_process(w: &mut World) {
    let launches: Vec<_> = w
        .scenario
        .fake()
        .record()
        .entries
        .into_iter()
        .filter(|e| e.event["kind"] == "launch")
        .collect();
    assert!(
        !launches.is_empty(),
        "FR-040 acp2api started a runtime process: {launches:?}"
    );
}

#[when("the owner asks for a retry in the management panel")]
async fn retry_in_management_panel(w: &mut World) {
    let answer = unavailable_support::action(w, "/manage/retry", json!({})).await;
    assert_eq!(answer.status, 200, "FR-040 management retry accepted");
    w.panel.answer = Some(answer);
}
