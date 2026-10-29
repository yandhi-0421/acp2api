use crate::{World, build_support as build, panel_tiles_support as tiles, places_support as queue};
use acp2api_acceptance::{Setup, TurnPlan, endpoint_check, panel_check, system_check};
use cucumber::{given, then, when};
use serde_json::json;
#[path = "../support/panel_source.rs"]
mod panel_source;

#[given("one prompt turn runs, and no request waits in the queue")]
async fn occupied(w: &mut World) {
    queue::held(w, "panel", false).await;
    assert!(
        queue::has_prompt(w, "panel"),
        "one real prompt turn is held"
    );
    assert_eq!(w.places.calls.len(), 1);
}

#[given(expr = "the build in use is {word}")]
fn current_build(w: &mut World, version: String) {
    let servers = crate::runtime_support::servers(w);
    assert_eq!(servers.len(), 1, "one runtime process is in use");
    assert_eq!(
        servers[0]["version"], version,
        "fixture runs the named build"
    );
}

#[then(
    expr = "the first tile shows {int} place in use, {int} requests in the queue, and the build in use {word}"
)]
fn figures(w: &mut World, occupied: u8, queued: u8, version: String) {
    let html = tiles::html(w);
    let first = tiles::sections(&html).next();
    assert!(first.is_some(), "FR-054 first tile is present");
    let first = first.unwrap();
    tiles::figure(first, "Places in use", &occupied.to_string());
    tiles::figure(first, "Requests in the queue", &queued.to_string());
    tiles::figure(first, "Build in use", &version);
}

#[given(
    expr = "the runtime process runs from the copy of the build {word} from the install folder of Zed"
)]
async fn from_zed(w: &mut World, version: String) {
    build::prepare(w).await;
    build::install(w, "the install folder of Zed", &version, true);
    build::install(w, "listed-later", "1.5.0", true);
    w.startup.launch_data(vec![]).await;
    build::assert_copy_launch(w);
}

#[then(
    expr = "the tile {string} shows the build {word} with its label {string} and its source, the install folder of Zed"
)]
fn build_source(w: &mut World, title: String, version: String, label: String) {
    let html = tiles::html(w);
    let tile = tiles::tile(&html, &title);
    tiles::figure(tile, "Build in use", &version);
    assert!(
        tiles::visible(tile).contains(&label),
        "FR-054 current build acceptance label"
    );
    let source = build::place(w, "the install folder of Zed").join(build::program());
    let expected = format!("<code>{}</code>", source.display());
    assert_eq!(
        panel_source::source_line(tile),
        Some(expected.as_str()),
        "FR-054 current build retains its original Zed source after a later search"
    );
}

#[then(
    expr = "the tile {string} shows the runtime process, the result of the search, and {string}"
)]
fn process_and_search(w: &mut World, title: String, newer: String) {
    let html = tiles::html(w);
    let text = tiles::visible(tiles::tile(&html, &title));
    tiles::figure(tiles::tile(&html, &title), "Runtime process", "running");
    let entries = w.startup.panel_hub().entries();
    assert!(entries.iter().any(|e| e.event["hello"]["role"] == "server"));
    assert!(
        text.contains(&newer),
        "FR-054 newer accepted build is shown"
    );
    assert!(text.contains("1.5.0"), "FR-054 search result version");
    let path = build::place(w, "the install folder of Zed").join(build::program());
    assert!(
        tiles::tile(&html, &title).contains(path.to_str().unwrap()),
        "FR-054 search result includes its discovered path"
    );
}

#[given("the owner refreshed the quota at a known time")]
async fn quota_result(w: &mut World) {
    queue::start(w).await;
    crate::query_support::panel(w).await;
    w.scenario.time.advance_to(90_000).await;
    w.scenario
        .fake()
        .next_usage(TurnPlan::new().chunk(crate::query_support::RESULT));
    let call = crate::query_support::refresh(w).await;
    let answer = call.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, 200,
        "owner refresh reaches the management endpoint"
    );
    assert_eq!(
        crate::query_support::calls(w),
        1,
        "exactly one /usage result exists"
    );
    assert_eq!(w.queries.refresh_at, 90_000);
}

#[then(expr = "the tile {string} shows the last result of {string} and its time")]
fn quota_time(w: &mut World, title: String, command: String) {
    assert_eq!(command, "/usage");
    let html = tiles::html(w);
    let tile = tiles::tile(&html, &title);
    let text = tiles::visible(tile);
    assert!(
        text.contains(&tiles::visible(crate::query_support::RESULT)),
        "FR-054 quota tile holds the last /usage result"
    );
    assert!(
        tile.contains("2026-10-18T00:01:30Z"),
        "FR-054 quota result uses the virtual refresh time"
    );
}

#[then(expr = "the tile {string} shows the refresh entry and the concurrency limit")]
fn quota_controls(w: &mut World, title: String) {
    let html = tiles::html(w);
    let tile = tiles::tile(&html, &title);
    tiles::figure(tile, "Places in use", "0");
    tiles::figure(tile, "Requests in the queue", "0");
    assert!(
        tile.contains("action=\"/manage/refresh-quota\""),
        "FR-054 quota refresh entry"
    );
    assert!(
        tile.contains("name=\"concurrency_limit\""),
        "FR-054 quota tile contains the concurrency limit"
    );
    assert!(
        tile.contains("value=\"2\" selected"),
        "FR-054 quota tile shows the current limit 2"
    );
}

#[given(expr = "acp2api runs on the port {int} with the data folder {string}")]
async fn settings(w: &mut World, port: u16, folder: String) {
    assert_eq!(
        port, 8329,
        "the documented port aliases the leased loopback port"
    );
    w.startup.initialize().await;
    w.startup.data = w.startup.root().path.join(folder);
    std::fs::create_dir_all(&w.startup.data).unwrap();
    w.startup.seed_config();
    w.startup.panel_runtime();
    w.startup.launch_data(vec![]).await;
}

#[then(expr = "the tile {string} shows the port {int} and the data folder {string}")]
fn settings_values(w: &mut World, title: String, port: u16, folder: String) {
    assert_eq!(port, 8329);
    let html = tiles::html(w);
    let tile = tiles::tile(&html, &title);
    tiles::figure(tile, "Port", &w.startup.root().port.to_string());
    let path = w.startup.root().path.join(folder);
    assert!(
        tile.contains(path.to_str().unwrap()),
        "FR-054 settings shows the actual data folder"
    );
}

#[then(expr = "the tile {string} has no entry that changes the port or the data folder")]
fn read_only(w: &mut World, title: String) {
    let html = tiles::html(w);
    let tile = tiles::tile(&html, &title);
    for control in tile.split('<').filter(|tag| {
        tag.starts_with("input ") || tag.starts_with("select ") || tag.starts_with("textarea ")
    }) {
        let tag = control.split('>').next().unwrap();
        let changes_location = ["port", "data_dir", "data-dir"]
            .iter()
            .any(|name| tag.contains(&format!("name=\"{name}\"")));
        assert!(
            !changes_location || tag.contains(" readonly") || tag.contains(" disabled"),
            "FR-054 settings has no port or data-folder edit entry"
        );
    }
}

#[given(expr = "the official runtime is signed in with the Google account {string}")]
async fn account(w: &mut World, email: String) {
    w.scenario.plan(Setup::normal());
    queue::start(w).await;
    assert!(w.scenario.fake().signed_in());
    let encoded = email.replace('@', "%40");
    let line = format!(
        "User tier: paid; account: {email}; upgradeUri: https://accounts.google.com/AccountChooser?Email={encoded}&continue=synthetic"
    );
    w.scenario
        .hub
        .command("normal", json!({"op":"Err","line":line}));
    w.scenario.time.settle().await;
    let delivered = w.scenario.hub.entries().iter().any(|entry| {
        entry.event["kind"] == "command"
            && entry.event["command"]["op"] == "Err"
            && entry.event["command"]["line"]
                .as_str()
                .unwrap()
                .contains(&encoded)
    });
    assert!(
        delivered,
        "synthetic account identity reaches runtime stderr"
    );
}

#[when("the owner opens each page of the management panel with a panel session")]
async fn all_pages(w: &mut World) {
    let page = queue::view(w).await;
    w.panel_tiles.pages.push(panel_check::text(&page));
    let answer = w.panel.page("anonymous").await;
    assert_eq!(answer.status, 200, "management document is available");
    w.panel_tiles.pages.push(panel_check::text(&answer));
    w.panel.login("wrong", "synthetic-invalid-password").await;
    assert_eq!(w.panel.login_answer.as_ref().unwrap().status, 401);
    w.panel_tiles
        .pages
        .push(panel_check::text(w.panel.login_answer.as_ref().unwrap()));
    assert!(
        w.panel_tiles.pages[0].contains("id=\"local-api-key\""),
        "authenticated page was actually read"
    );
    assert!(
        w.panel_tiles.pages[1].contains("autocomplete=\"current-password\""),
        "login page was actually read"
    );
}

#[then(expr = "no page holds {string}")]
fn no_email(w: &mut World, email: String) {
    assert_eq!(w.panel_tiles.pages.len(), 3);
    for html in &w.panel_tiles.pages {
        assert!(
            !html.contains(&email)
                && !html
                    .to_ascii_lowercase()
                    .contains(&email.replace('@', "%40").to_ascii_lowercase()),
            "FR-054 no management document holds the raw or URL-encoded Google account email"
        );
    }
}

#[when(
    expr = "the owner opens the management panel with a panel session and reloads it {int} times"
)]
async fn reload(w: &mut World, reloads: usize) {
    assert!(reloads <= 3, "panel reload request cap: 4");
    queue::bind_panel(w).await;
    w.panel_tiles.calls_before = tiles::runtime_calls(w);
    assert!(
        w.panel_tiles.calls_before > 0,
        "runtime call observer recorded startup"
    );
    let mut answer = w.panel.view_session("B1").await;
    for view in 0..=reloads {
        if view > 0 {
            answer = w.panel.page("B1").await;
        }
        assert_eq!(answer.status, 200);
        assert!(
            panel_check::text(&answer).contains("id=\"local-api-key\""),
            "view uses the authenticated panel"
        );
    }
}

#[then("the official runtime gets no call")]
async fn no_runtime_call(w: &mut World) {
    assert_eq!(
        tiles::runtime_calls(w),
        w.panel_tiles.calls_before,
        "FR-054 opening and reloading the panel sends no ACP request"
    );
    let call = endpoint_check::request(
        "POST /v1/messages",
        "panel observer positive control",
        false,
    );
    w.request.send(&mut w.scenario, call).await;
    assert!(
        tiles::runtime_calls(w) > w.panel_tiles.calls_before,
        "the same observer detects an API request to the runtime"
    );
}

#[then(expr = "the count of the places in use is {int} during the start")]
async fn during_start(w: &mut World, count: u8) {
    assert_eq!(
        system_check::requests(&w.scenario)
            .iter()
            .filter(|r| r["method"] == "initialize")
            .count(),
        1
    );
    assert!(
        !w.scenario
            .hub
            .entries()
            .iter()
            .any(|e| e.event["kind"] == "command" && e.event["command"]["op"] == "Out"),
        "measurement precedes the held initialize reply"
    );
    w.panel.scenario(&mut w.scenario).await;
    let page = w.panel.view_session("B1").await;
    assert!(
        !w.scenario
            .hub
            .entries()
            .iter()
            .any(|e| e.event["kind"] == "command" && e.event["command"]["op"] == "Out"),
        "panel read leaves the runtime start pending"
    );
    assert_eq!(
        w.scenario.hub.now(),
        0,
        "no virtual startup deadline passed"
    );
    let html = panel_check::text(&page);
    let first = tiles::sections(&html).next();
    assert!(
        first.is_some(),
        "FR-049 first tile is available during runtime initialization"
    );
    tiles::figure(first.unwrap(), "Places in use", &count.to_string());
}
