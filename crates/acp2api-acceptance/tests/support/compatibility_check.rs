use crate::{World, build_support as build};
use acp2api_acceptance::client::Call;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[derive(Default)]
pub struct CompatibilityState {
    pub page: String,
    pub candidate: Option<PathBuf>,
    pub check: Option<Call>,
    pub request: Option<Call>,
}
impl std::fmt::Debug for CompatibilityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompatibilityState").finish_non_exhaustive()
    }
}

pub async fn found(w: &mut World) {
    build::prepare(w).await;
    let folder = build::install(w, "the install folder of Zed", "1.6.0", false);
    w.compatibility.candidate = Some(folder);
    if w.startup.panel_running() {
        build::search(w).await;
    } else {
        build::install(w, "copy", "1.4.0", true);
        w.startup.launch_data(vec![]).await;
    }
    build::assert_found(w, "the install folder of Zed").await;
}

fn identity(w: &World) -> [String; 2] {
    let folder = w.compatibility.candidate.as_ref().unwrap();
    [
        build::hash(&folder.join(build::program())),
        build::hash(&folder.join(build::descriptor())),
    ]
}

pub fn checked(w: &World) -> Option<Value> {
    let path = w.startup.data.join("state.json");
    if !path.exists() {
        return None;
    }
    let state: Value = serde_json::from_slice(&fs::read(path).expect("actual state file"))
        .expect("state file JSON");
    let [server, harness] = identity(w);
    state["checked_builds"]
        .as_array()
        .expect("checked_builds array")
        .iter()
        .find(|entry| {
            entry["agy_acp_server_sha256"] == server
                && entry["localharness_external_sha256"] == harness
        })
        .cloned()
}

pub async fn begin(w: &mut World) {
    found(w).await;
    let [server, harness] = identity(w);
    let call = w.panel.owner("build-owner").submit(
        "/manage/compatibility-check",
        json!({"build":format!("{server}:{harness}")}),
    );
    w.compatibility.check = Some(call);
    w.scenario.time.settle().await;
    let status = w.compatibility.check.as_ref().unwrap().status();
    assert!(
        status.is_none() || matches!(status, Some(200 | 303)),
        "FR-044 the designed compatibility-check action must accept the selected build (HTTP {status:?})"
    );
}

pub async fn finish(w: &mut World) {
    let call = w.compatibility.check.take().unwrap();
    let answer = call.finish(&mut w.scenario.time).await;
    assert!(
        matches!(answer.status, 200 | 303),
        "FR-044 the designed compatibility-check action must accept the selected build (HTTP {})",
        answer.status
    );
}

pub fn labelled(html: &str, version: &str, label: &str) {
    assert!(
        html.split("<li").skip(1).any(|row| {
            let row = row.split("</li>").next().unwrap();
            row.contains(version) && row.contains(label)
        }),
        "FR-044 build {version} must have its own label {label}"
    );
}
