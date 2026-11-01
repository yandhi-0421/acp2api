use crate::{
    World, panel_document as document, panel_states_support as support, places_support as queue,
};
use acp2api_acceptance::{build_fixture, panel_visual};
use cucumber::{given, then, when};
use serde_json::json;

#[given("the owner opens the management panel with a panel session")]
async fn open_session(w: &mut World) {
    w.panel.answer = Some(queue::view(w).await);
}

#[when("the owner exits the management panel")]
async fn exit(w: &mut World) {
    let cookies = w.panel.owner("B1").cookies.clone();
    assert!(!cookies.is_empty(), "the browser has a panel session");
    w.panel.owner("exit-replay").cookies = cookies.clone();
    let cookie = cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    let origin = format!("http://{}", w.panel.address);
    let call = w
        .panel
        .owner("B1")
        .client
        .send(acp2api_acceptance::client::ApiCall {
            method: "POST".into(),
            path: "/manage/exit".into(),
            body: json!({}),
            headers: vec![
                ("cookie".into(), cookie),
                ("origin".into(), origin),
                (
                    "content-type".into(),
                    "application/x-www-form-urlencoded".into(),
                ),
            ],
        });
    let answer = call.finish_current(&mut w.panel.time()).await;
    assert_eq!(answer.status, 303, "FR-009 the panel exit succeeds");
    w.panel.owner("B1").remember(&answer);
    w.panel.answer = Some(answer);
}

#[then("the management panel asks for the management password")]
async fn password_after_exit(w: &mut World) {
    let answer = w.panel.page("B1").await;
    assert_eq!(answer.status, 200);
    let html = document::html(&answer);
    assert!(html.contains("action=\"/manage/login\""));
    assert!(html.contains("autocomplete=\"current-password\""));
    assert!(!html.contains("id=\"local-api-key\""));
}

#[then("the old panel session cookie gives no access to the management panel")]
async fn replay_after_exit(w: &mut World) {
    let answer = w.panel.page("exit-replay").await;
    assert_eq!(answer.status, 200);
    let html = document::html(&answer);
    assert!(html.contains("autocomplete=\"current-password\""));
    assert!(!html.contains("id=\"local-api-key\""));
    let action = w
        .panel
        .submit("exit-replay", "/manage/language", json!({"language": "en"}))
        .await;
    assert_eq!(action.status, 401, "the ended session cannot act as owner");
}

#[given("the owner started the sign-in")]
async fn signing_in(w: &mut World) {
    crate::sign_in_support::start(w).await;
    crate::sign_in_support::authenticate(w, false, None).await;
}

#[given("no notice exists")]
async fn no_notice(w: &mut World) {
    queue::start(w).await;
    assert!(
        w.scenario.fake().record().prompts().is_empty(),
        "no prompt could have produced a notice"
    );
}

#[given("a built-in tool ran with no permission request")]
#[given("the management panel has a notice")]
async fn notice(w: &mut World) {
    support::signal(
        w,
        acp2api_acceptance::TurnPlan {
            steps: vec![acp2api_acceptance::TurnStep::BuiltInRuns {
                name: "view_file".into(),
            }],
            end: acp2api_acceptance::TurnEndPlan::Never,
        },
    )
    .await;
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().status,
        502,
        "built-in tool reaches the refusal path"
    );
}

#[given("acp2api is in the normal state, and acp2api found a new build")]
async fn newer(w: &mut World) {
    queue::start(w).await;
    w.scenario.stop().await;
    let path = w.scenario.root.path.join("run.toml");
    let mut run: acp2api_acceptance::adapters::RunFile =
        toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let folder = run.environment.exe_dir.clone();
    build_fixture::install(
        &w.scenario.root.path.join("builds/normal"),
        &folder,
        "1.5.0",
        "panel-new-build-75-083f045881784739",
    )
    .unwrap();
    run.release.builds.push(acp2api::ListedBuild {
        version: "1.5.0".into(),
        platform: run.release.platform.clone(),
        agy_acp_server_sha256: acp2api_acceptance::setup::file_hash(
            &folder.join(build_fixture::SERVER),
        ),
        localharness_external_sha256: acp2api_acceptance::setup::file_hash(
            &folder.join(build_fixture::HARNESS),
        ),
    });
    std::fs::write(path, toml::to_string(&run).unwrap()).unwrap();
    w.scenario.restart().await;
    queue::panel(w).await;
    let answer = w.panel.submit("B1", "/manage/search", json!({})).await;
    assert_eq!(answer.status, 200, "the owner discovers the new build");
    let page = queue::view(w).await;
    assert!(
        document::visible(&document::html(&page)).contains("New build found: 1.5.0"),
        "setup discovers a higher accepted build without replacing the copy"
    );
}

#[given("acp2api is in the state \"runtime unavailable\" with no executable")]
async fn missing(w: &mut World) {
    queue::start(w).await;
    w.scenario.stop().await;
    std::fs::remove_dir_all(w.scenario.root.path.join("data/official-runtime")).unwrap();
    w.scenario.restart().await;
}

#[given("acp2api is in the state \"runtime unavailable\" with no accepted version")]
async fn unaccepted(w: &mut World) {
    queue::start(w).await;
    w.scenario.stop().await;
    std::fs::remove_dir_all(w.scenario.root.path.join("data/official-runtime")).unwrap();
    let run: acp2api_acceptance::adapters::RunFile =
        toml::from_str(&std::fs::read_to_string(w.scenario.root.path.join("run.toml")).unwrap())
            .unwrap();
    build_fixture::install(
        &w.scenario.root.path.join("builds/normal"),
        &run.environment.exe_dir,
        "1.6.0",
        "panel-unaccepted-75-24ef78520b4b41b7",
    )
    .unwrap();
    w.scenario.restart().await;
}

#[given("acp2api is in the state \"runtime unavailable\" after 3 failures")]
async fn failures(w: &mut World) {
    crate::unavailable_support::three_failures(w).await;
}

#[given("acp2api is in lockout after the stderr error line 403 \"PERMISSION_DENIED\"")]
async fn evidence(w: &mut World) {
    let description = format!(
        "the stderr error line 403 \"PERMISSION_DENIED\" with the message \"{}\"",
        support::ERROR
    );
    support::signal(w, crate::lockout_support::plan(&description, "")).await;
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().status,
        400,
        "the signal starts a lockout"
    );
}

#[then(expr = "the management panel has the entry {string}")]
fn entry(w: &mut World, expected: String) {
    let html = document::html(w.panel.answer.as_ref().unwrap());
    assert!(
        support::entry(&html, &expected),
        "FR-053 panel needs a usable entry: {expected}"
    );
}

#[when(expr = "the owner runs {string} in another terminal")]
async fn run_command(w: &mut World, command: String) {
    let command = command
        .strip_prefix("acp2api ")
        .expect("named CLI executable");
    assert!(
        matches!(command, "unlock" | "stop"),
        "CLI invocation cap: one known command"
    );
    let (data, run) = if command == "stop" {
        // The reused survival step observes the first account instance.
        w.account.first("D1");
        w.account
            .launch(0, "panel-cli-temp-75-c561a4d30d7449b6")
            .await;
        let root = &w.account.instances[0].root.path;
        (root.join("D1"), root.join("account-run.toml"))
    } else {
        queue::start(w).await;
        (
            w.scenario.root.path.join("data"),
            w.scenario.root.path.join("run.toml"),
        )
    };
    w.startup.initialize().await;
    w.startup.run = Some(toml::from_str(&std::fs::read_to_string(run).unwrap()).unwrap());
    w.startup.data = data;
    w.startup.launch_data(vec![command.into()]).await;
    assert_eq!(
        w.startup.exited,
        Some(false),
        "FR-053 owner actions are not v1 CLI commands"
    );
}

#[then("acp2api is still in the state \"lockout\"")]
async fn still_locked(w: &mut World) {
    assert_eq!(
        w.startup.exited,
        Some(false),
        "FR-053 unlock is not a v1 CLI command"
    );
    let html = document::html(&queue::view(w).await);
    assert!(
        document::visible(support::first_tile(&html)).contains("Lockout"),
        "FR-053 CLI unlock cannot end the lockout"
    );
    assert!(crate::lockout_support::state(w)["lockout"].is_object());
}

#[then("the page has these parts in this order:")]
async fn parts(w: &mut World, step: &cucumber::gherkin::Step) {
    let html = document::html(w.panel.answer.as_ref().unwrap());
    let header = document::elements(&html, "header")
        .next()
        .expect("FR-054 global bar");
    assert!(
        [
            "Exit the panel",
            "Exit panel",
            "Exit",
            "Sign out",
            "Log out"
        ]
        .iter()
        .any(|label| support::entry(header.content, label)),
        "FR-054 global bar needs the exit of the panel"
    );
    assert!(
        document::elements(header.content, "form")
            .any(|form| document::attribute(form.tag, "action") == Some("/manage/language")),
        "FR-054 global bar needs the language switch"
    );
    let before_tile = html.split_once("<section").unwrap().0;
    let bar = before_tile.split_once("</header>").unwrap().1;
    let anchors: Vec<_> = document::elements(bar, "a")
        .filter_map(|a| document::attribute(a.tag, "href"))
        .collect();
    for anchor in ["#runtime", "#quota", "#setup", "#logs", "#settings"] {
        assert!(
            anchors.contains(&anchor),
            "FR-054 frosted bar needs section anchor {anchor}"
        );
    }
    let tile = support::first_tile(&html);
    let state = document::elements(tile, "h1")
        .next()
        .map(|h| document::visible(h.content))
        .unwrap();
    let action = match state.as_str() {
        "Sign-in required" => Some("Sign in with Google"),
        "Runtime unavailable" => Some("Retry"),
        "Lockout" => Some("End the lockout with the management password"),
        "Running" | "Quota wait" => None,
        _ => panic!("FR-054 unexpected state in layout: {state}"),
    };
    if let Some(action) = action {
        assert!(
            support::entry(bar, action),
            "FR-054 frosted bar needs the main state action: {action}"
        );
    }
    let headings: Vec<_> = document::elements(&html, "section")
        .skip(1)
        .filter_map(|s| document::elements(s.content, "h2").next())
        .map(|h| document::visible(h.content).to_ascii_lowercase())
        .collect();
    let expected: Vec<_> = step
        .table
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .filter_map(|row| {
            row[0]
                .strip_prefix("the tile \"")
                .and_then(|s| s.strip_suffix('"'))
        })
        .collect();
    assert_eq!(headings, expected, "FR-054 tiles follow variant A order");
    let footer = document::elements(&html, "footer").next();
    assert!(footer.is_some(), "FR-054 panel needs a footer");
    let footer = footer.unwrap();
    assert!(
        document::visible(footer.content)
            .contains(&format!("acp2api {}", env!("CARGO_PKG_VERSION"))),
        "FR-054 footer shows the acp2api version"
    );
    assert!(
        html.find(footer.content).unwrap() > html.rfind("</section>").unwrap(),
        "FR-054 footer follows the last tile"
    );
    panel_visual::variant_a_bars(support::rendered(w).await).unwrap();
}

#[then(expr = "the first tile shows {string} in large type")]
fn heading(w: &mut World, expected: String) {
    let html = document::html(w.panel.answer.as_ref().unwrap());
    let actual = document::elements(support::first_tile(&html), "h1")
        .next()
        .map(|h| document::visible(h.content));
    assert_eq!(
        actual.as_deref(),
        Some(expected.as_str()),
        "FR-054 first tile state heading"
    );
    // The final shade step checks the rendered 56 px type and the surface together.
}

#[then(expr = "the first tile shows the sentence {string}")]
fn sentence(w: &mut World, expected: String) {
    let html = document::html(w.panel.answer.as_ref().unwrap());
    let expected = expected
        .replace("<reset time>", crate::quota_support::RESET)
        .replace("<time>", crate::lockout_support::SINCE);
    let paragraphs: Vec<_> = document::elements(support::first_tile(&html), "p")
        .map(|p| document::visible(p.content))
        .collect();
    assert!(
        paragraphs.contains(&expected),
        "FR-054 first tile needs the state sentence: {expected}; actual paragraphs: {paragraphs:?}"
    );
}

#[then(regex = r"^the first tile is (light|dark)$")]
async fn state_shade(w: &mut World, shade: String) {
    panel_visual::first_tile_presentation(support::rendered(w).await, &shade).unwrap();
}

#[then(expr = "the first tile has the action {string}")]
fn action(w: &mut World, expected: String) {
    let html = document::html(w.panel.answer.as_ref().unwrap());
    assert!(
        support::entry(support::first_tile(&html), &expected),
        "FR-054 first tile needs the state action: {expected}"
    );
}

#[then("the first tile shows the signal, the time, and the original error text of the lockout")]
fn shown_evidence(w: &mut World) {
    let html = document::html(w.panel.answer.as_ref().unwrap());
    let first = document::visible(support::first_tile(&html));
    for expected in ["403", crate::lockout_support::SINCE, support::ERROR] {
        assert!(
            first.contains(expected),
            "FR-054 first tile needs lockout evidence: {expected}"
        );
    }
}

#[then(regex = r"^the page (has no|has) the band of notices$")]
fn notices(w: &mut World, band: String) {
    let html = document::html(w.panel.answer.as_ref().unwrap());
    assert!(
        !document::visible(support::first_tile(&html)).is_empty(),
        "inspect the authenticated state page"
    );
    let notices: Vec<_> = document::elements(&html, "section")
        .filter(|s| {
            document::elements(s.content, "h2").any(|h| document::visible(h.content) == "Notices")
        })
        .collect();
    assert_eq!(
        notices.len(),
        usize::from(band == "has"),
        "FR-054 notice band exists exactly when a notice exists"
    );
    if band == "has" {
        assert!(
            document::visible(notices[0].content)
                .contains("The built-in tool view_file ran with no permission request."),
            "FR-054 band shows the actual built-in tool notice"
        );
        assert!(
            support::entry(notices[0].content, "Dismiss"),
            "FR-054 notice band offers dismissal"
        );
        assert!(
            html.find(notices[0].content).unwrap()
                < html.find("<h2>Official runtime</h2>").unwrap(),
            "FR-054 notice band precedes official runtime"
        );
    }
}
