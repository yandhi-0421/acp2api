use crate::{World, build_support as build};
use cucumber::{given, then, when};
use serde_json::json;
use std::fs;

#[given("acp2api has no copy of the official runtime")]
async fn no_copy(w: &mut World) {
    build::prepare(w).await;
    assert!(
        !build::place(w, "copy").exists(),
        "no seeded copy in this startup fixture"
    );
}
#[given("the configuration file has no path of agy_acp_server")]
async fn no_path(w: &mut World) {
    build::prepare(w).await;
    w.startup.set("agy_acp_server_path", None);
}
#[given(
    regex = r"^the only build of the official runtime is in (the folder of the acp2api executable|a folder of PATH|the install folder of Zed|the install folder of a JetBrains IDE)$"
)]
async fn only_place(w: &mut World, place: String) {
    build::install(w, &place, "1.4.0", true);
}
#[given("acp2api found no build of the official runtime")]
async fn found_none(w: &mut World) {
    build::prepare(w).await;
    w.startup.launch_data(vec![]).await;
}
#[given("the owner then puts a build of an accepted version into the install folder of Zed")]
async fn added_after_start(w: &mut World) {
    // The accepted hashes must be in the run file before the host reads its release facts.
    let folder = build::install(w, "the install folder of Zed", "1.4.0", false);
    assert!(folder.join(build::program()).is_file());
}
#[given(
    "the install folder of Zed holds agy_acp_server of an accepted version and no localharness_external"
)]
async fn incomplete(w: &mut World) {
    let folder = build::install(w, "the install folder of Zed", "1.4.0", true);
    fs::remove_file(folder.join(build::descriptor())).unwrap();
}
#[given(expr = "the path of agy_acp_server points to the folder {string}")]
async fn configured(w: &mut World, folder: String) {
    build::prepare(w).await;
    let path = build::install(w, &folder, "1.4.0", true).join(build::program());
    w.startup.set(
        "agy_acp_server_path",
        Some(toml::Value::String(path.to_str().unwrap().into())),
    );
    w.startup.launch_data(vec![]).await;
}
#[given("acp2api finds the builds 1.4.0 and 1.5.0, and both are on the list of the release")]
async fn highest(w: &mut World) {
    build::install(w, "a folder of PATH", "1.4.0", true);
    build::install(w, "the install folder of Zed", "1.5.0", true);
}
#[given(expr = "the runtime process runs from the copy of the build {word}")]
async fn running(w: &mut World, version: String) {
    build::prepare(w).await;
    build::install(w, "copy", &version, true);
    // Declare the later found build without putting it into a searched folder yet.
    build::install(w, "listed-later", "1.5.0", true);
    w.startup.launch_data(vec![]).await;
    assert_eq!(
        build::launches(w),
        vec![build::place(w, "copy").join(build::program())],
        "setup starts the actual copy"
    );
}
#[given(
    regex = r"^(?:the owner puts|the install folder of Zed holds) the accepted build (1\.4\.0|1\.5\.0)(?: into the install folder of Zed)?$"
)]
async fn accepted_zed(w: &mut World, version: String) {
    build::install(w, "the install folder of Zed", &version, true);
}
#[given("acp2api found the accepted build 1.5.0")]
async fn found_new(w: &mut World) {
    build::install(w, "the install folder of Zed", "1.5.0", true);
    build::search(w).await;
}
#[given("the only found build is 1.6.0, which is not on the list of the release")]
async fn unlisted(w: &mut World) {
    build::install(w, "the install folder of Zed", "1.6.0", false);
}
#[given("the copy of the official runtime in the data folder has one changed byte")]
async fn changed(w: &mut World) {
    build::prepare(w).await;
    let path = build::install(w, "copy", "1.4.0", true).join(build::descriptor());
    let mut bytes = fs::read(&path).unwrap();
    let index = bytes.windows(6).position(|p| p == b"normal").unwrap() + 1;
    bytes[index] = b'0';
    fs::write(&path, bytes).unwrap();
}
#[given("no other place holds a build of the official runtime")]
fn no_other(w: &mut World) {
    for name in [
        "the folder of the acp2api executable",
        "a folder of PATH",
        "the install folder of Zed",
        "the install folder of a JetBrains IDE",
    ] {
        assert!(
            !build::place(w, name).join(build::program()).exists(),
            "empty search fixture: {name}"
        );
    }
}
#[given("acp2api for macOS arm64 runs")]
async fn arm64(w: &mut World) {
    build::prepare(w).await;
    w.startup.run.as_mut().unwrap().release.platform = "macos-arm64".into();
}
#[given("it has no copy of an accepted version and no usable runtime process")]
fn absent_copy(w: &mut World) {
    assert!(!build::place(w, "copy").exists());
}
#[given(
    "the only found build is a build for macOS x64 whose hashes are on the list of the release"
)]
async fn other_arch(w: &mut World) {
    w.startup.run.as_mut().unwrap().release.builds.clear();
    build::install(w, "a folder of PATH", "1.4.0", true);
    w.startup
        .run
        .as_mut()
        .unwrap()
        .release
        .builds
        .last_mut()
        .unwrap()
        .platform = "macos-x64".into();
    w.startup.launch_data(vec![]).await;
}
#[given("no place that acp2api searches holds a build of the official runtime")]
async fn nowhere(w: &mut World) {
    build::prepare(w).await;
}
#[when(
    regex = r#"^the owner sets the path of agy_acp_server to a build in the folder "([^"]+)" in the management panel$"#
)]
async fn set_path(w: &mut World, folder: String) {
    let path = build::install(w, &folder, "1.4.0", false).join(build::program());
    build::action(
        w,
        "/manage/runtime-path",
        json!({"agy_acp_server_path":path}),
    )
    .await;
}
#[when(
    regex = r#"^the owner sets the path of agy_acp_server to the folder "([^"]+)", which has no localharness_external, in the management panel$"#
)]
async fn set_incomplete(w: &mut World, folder: String) {
    let path = build::install(w, &folder, "1.4.0", false);
    fs::remove_file(path.join(build::descriptor())).unwrap();
    build::action(
        w,
        "/manage/runtime-path",
        json!({"agy_acp_server_path":path.join(build::program())}),
    )
    .await;
}
#[when("the owner starts a new search in the management panel")]
#[when("acp2api searches again")]
async fn search(w: &mut World) {
    build::search(w).await;
}
#[when("the owner changes the build to 1.5.0 in the management panel")]
async fn use_new(w: &mut World) {
    let selected = w
        .startup
        .run
        .as_ref()
        .unwrap()
        .release
        .builds
        .iter()
        .find(|build| build.version == "1.5.0")
        .unwrap();
    let identity = format!(
        "{}:{}",
        selected.agy_acp_server_sha256, selected.localharness_external_sha256
    );
    build::action(w, "/manage/build", json!({"build":identity})).await;
}
#[then(
    regex = r"^the result of the search lists the build in (the folder of the acp2api executable|a folder of PATH|the install folder of Zed|the install folder of a JetBrains IDE)$"
)]
async fn found_place(w: &mut World, place: String) {
    build::assert_found(w, &place).await;
}
#[then(expr = "the result of the search lists the build in the folder {string}")]
async fn found_folder(w: &mut World, folder: String) {
    build::assert_found(w, &folder).await;
}
#[then("the result of the search does not list the install folder of Zed")]
async fn ignores(w: &mut World) {
    let html = build::page(w).await;
    assert!(
        !html.contains(
            build::place(w, "the install folder of Zed")
                .to_str()
                .unwrap()
        ),
        "FR-042 incomplete build must be absent"
    );
}
#[then(expr = "the management panel shows {string}")]
async fn shows(w: &mut World, message: String) {
    let html = build::page(w).await;
    let action = w
        .builds
        .action
        .as_ref()
        .map(acp2api_acceptance::panel_check::text)
        .unwrap_or_default();
    assert!(
        html.contains(&message) || action.contains(&message),
        "FR-042/FR-043 panel must show {message}"
    );
}
#[then(expr = "the path of agy_acp_server still points to the folder {string}")]
fn keeps_path(w: &mut World, folder: String) {
    let path = build::place(w, &folder).join(build::program());
    assert_eq!(
        w.startup.saved().unwrap()["agy_acp_server_path"].as_str(),
        path.to_str(),
        "FR-042 earlier configured executable path stays saved"
    );
}
#[then(expr = "the data folder holds one copy of the official runtime, of the build {word}")]
fn copied(w: &mut World, version: String) {
    build::copied(w, &version);
}
#[then("the runtime process runs from that copy")]
fn from_copy(w: &mut World) {
    build::assert_copy_launch(w);
}
#[then(expr = "the build in use is {word}")]
fn current(w: &mut World, version: String) {
    build::copied(w, &version);
    build::assert_copy_launch(w);
}
#[then("the result of the search lists the build 1.4.0")]
async fn found_old(w: &mut World) {
    let html = build::page(w).await;
    assert!(
        html.contains("1.4.0")
            && html.contains(
                build::place(w, "the install folder of Zed")
                    .to_str()
                    .unwrap()
            ),
        "FR-043 older build appears in real search results"
    );
}
#[then("the management panel shows no \"New build found\"")]
async fn no_new(w: &mut World) {
    assert!(!build::page(w).await.contains("New build found"));
}
#[then("acp2api replaces the runtime process with a runtime process of the build 1.5.0")]
fn replaced(w: &mut World) {
    build::copied(w, "1.5.0");
    let hub = w.startup.panel_hub();
    let servers: Vec<_> = hub
        .entries()
        .into_iter()
        .filter(|e| e.event["hello"]["role"] == "server")
        .collect();
    assert_eq!(
        servers.len(),
        2,
        "FR-043 replacement actually starts a second guarded runtime process"
    );
    assert_ne!(
        servers[0].event["hello"]["pid"],
        servers[1].event["hello"]["pid"]
    );
    assert_eq!(servers[1].event["hello"]["label"], "build-1.5.0");
    assert!(
        hub.entries()
            .iter()
            .any(|e| e.event["kind"] == "process_end"
                && e.event["role"] == "server"
                && e.event["label"] == "normal"),
        "FR-043 replacement stops the earlier runtime process"
    );
}
#[then("the data folder holds no copy of the official runtime")]
fn no_copied(w: &mut World) {
    assert!(
        !build::place(w, "copy").exists(),
        "FR-043 no copy without acceptance"
    );
}
#[then("no runtime process starts from the changed copy")]
fn rejects_changed(w: &mut World) {
    assert!(
        !w.startup
            .panel_hub()
            .entries()
            .iter()
            .any(|e| e.event["hello"]["label"] == build::CHANGED_COPY_LABEL),
        "ADR-0006 changed descriptor hash must prevent every launch of that copy"
    );
}
#[then(
    "acp2api copies the build 1.4.0 from the install folder of Zed, and the runtime process runs from the new copy"
)]
fn repaired_copy(w: &mut World) {
    build::copied(w, "1.4.0");
    build::assert_copy_launch(w);
}
#[then(regex = r#"^acp2api is in the state "runtime unavailable"(?: with the reason "([^"]+)")?$"#)]
async fn unavailable(w: &mut World, reason: String) {
    let answer = build::request(w).await;
    assert_eq!(
        answer.status, 400,
        "FR-040 runtime unavailable refuses an API request"
    );
    let message = answer.json()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        message.starts_with("acp2api: the official runtime is unavailable:"),
        "FR-040 unavailable response must use M-14"
    );
    if !reason.is_empty() {
        assert!(
            message.contains(&reason),
            "FR-040 exact unavailable reason: {reason}"
        );
    }
}
#[then(expr = "a request gets the message {string}")]
async fn unavailable_message(w: &mut World, message: String) {
    let answer = build::request(w).await;
    let expected = message.replace("<versions>", "1.4.0").replace(
        "<address of the management panel>",
        &format!("http://127.0.0.1:{}/", build::root(w).port),
    );
    assert_eq!(
        answer.json()["error"]["message"],
        expected,
        "FR-040 exact M-14 for the unavailable reason"
    );
}
#[then("that build is not an accepted version")]
async fn rejects_arch(w: &mut World) {
    let html = build::page(w).await;
    let path = build::place(w, "a folder of PATH").join(build::program());
    assert!(
        html.contains(path.to_str().unwrap()),
        "FR-043 architecture check observes the found build"
    );
    assert!(
        build::launches(w).is_empty() && !build::place(w, "copy").exists(),
        "FR-043 other architecture cannot be copied or launched"
    );
}
