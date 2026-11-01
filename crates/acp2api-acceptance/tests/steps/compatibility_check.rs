use crate::{World, build_support as build, compatibility_support as check};
use crate::{panel_document as document, panel_states_support as panel};
use acp2api_acceptance::{
    client::{ApiCall, ApiClient},
    setup::alive,
};
use cucumber::{given, then, when};
use serde_json::Value;

#[given("acp2api found the build 1.6.0, which is not an accepted version")]
async fn unaccepted(w: &mut World) {
    check::found(w).await;
}

#[given(
    "acp2api found the build 1.6.0, and the official runtime of that build gives each fact that acp2api needs"
)]
async fn all_facts(w: &mut World) {
    check::found(w).await;
    crate::compatibility_facts::configure(&w.scenario.fake(), crate::compatibility_facts::echo);
}

#[given("the official runtime of the found build 1.6.0 runs a built-in tool in the check")]
async fn built_in(w: &mut World) {
    check::found(w).await;
    crate::compatibility_facts::failure(&w.scenario.fake());
}

#[when("the owner opens the compatibility check for 1.6.0 in the management panel")]
async fn open(w: &mut World) {
    w.compatibility.page = build::page(w).await;
}

#[then(
    "the management panel shows the cost of 3 to 4 model requests through the Google account of the owner"
)]
fn cost(w: &mut World) {
    let page = &w.compatibility.page;
    assert!(
        page.contains("3 to 4")
            && page.contains("model requests")
            && page.contains("Google account"),
        "FR-044 the authenticated panel must show the cost of 3 to 4 model requests through the Google account before a check"
    );
}

#[then("the management panel shows these risks that the check does not check:")]
fn risks(w: &mut World, step: &cucumber::gherkin::Step) {
    for row in step.table.as_ref().unwrap().rows.iter().skip(1) {
        assert!(
            w.compatibility.page.contains(&row[0]),
            "FR-044 the panel must show unchecked risk: {}",
            row[0]
        );
    }
}

#[when("the owner starts the compatibility check for the build 1.6.0")]
async fn start(w: &mut World) {
    crate::compatibility_facts::configure(&w.scenario.fake(), crate::compatibility_facts::held);
    check::begin(w).await;
}

#[when("an API client sends a request during the check")]
async fn during(w: &mut World) {
    crate::runtime_support::observe(w, |w| {
        w.scenario.hub.entries().iter().any(|entry| {
            entry.event["kind"] == "gate" && entry.event["name"] == "compatibility-check"
        })
    })
    .await;
    assert!(
        w.scenario.hub.entries().iter().any(|entry| {
            entry.event["kind"] == "gate" && entry.event["name"] == "compatibility-check"
        }),
        "BLOCKED: fake must hold an actual compatibility-check prompt before the API request"
    );
    assert!(w.compatibility.request.is_none(), "one API request cap");
    w.request.queue_next(&w.scenario);
    w.compatibility.request = Some(
        ApiClient::new(
            format!("127.0.0.1:{}", w.startup.root().port),
            w.startup.root().key.clone(),
            w.scenario.hub.clone(),
        )
        .send(ApiCall::one_user_message(
            "POST /v1/messages",
            "request during compatibility check",
        )),
    );
    w.scenario.time.settle().await;
}

#[then("the request waits in the queue until the end of the check")]
async fn queued(w: &mut World) {
    let entries = w.scenario.hub.entries();
    let servers: Vec<_> = entries
        .iter()
        .filter(|entry| entry.event["hello"]["role"] == "server")
        .collect();
    assert_eq!(
        servers.len(),
        2,
        "FR-044 the check replaces the earlier runtime process"
    );
    assert_eq!(servers[1].event["hello"]["label"], "build-1.6.0");
    assert_eq!(
        servers[1].event["hello"]["argv"][0].as_str(),
        w.startup
            .data
            .join("official-runtime-check")
            .join(build::program())
            .to_str(),
        "ADR-0006 the check uses its own copy"
    );
    assert_ne!(
        servers[0].event["hello"]["pid"],
        servers[1].event["hello"]["pid"]
    );
    assert!(!alive(
        servers[0].event["hello"]["pid"].as_u64().unwrap() as u32
    ));
    assert!(alive(
        servers[1].event["hello"]["pid"].as_u64().unwrap() as u32
    ));
    let prompts = w.scenario.fake().record().prompts();
    assert!(
        !prompts.is_empty()
            && !prompts
                .iter()
                .any(|prompt| prompt.text.contains("request during compatibility check")),
        "FR-044 the API request must send no ACP prompt while the check runs"
    );
    assert_eq!(
        w.compatibility.request.as_ref().unwrap().status(),
        None,
        "FR-044 the queued API request must not finish during the check"
    );
    w.scenario.fake().open_gate("compatibility-check");
    check::finish(w).await;
    let request = w.compatibility.request.take().unwrap();
    let answer = request.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, 200,
        "FR-044 queued request resumes after the check"
    );
    assert!(
        w.scenario
            .fake()
            .record()
            .prompts()
            .iter()
            .any(|prompt| { prompt.text.contains("request during compatibility check") }),
        "FR-044 the queued request reaches ACP after the check"
    );
}

#[when("the owner runs the compatibility check for 1.6.0")]
async fn run(w: &mut World) {
    check::begin(w).await;
    check::finish(w).await;
}

#[then("the check passes")]
fn passes(w: &mut World) {
    assert!(
        check::checked(w.compatibility.candidate.as_ref().unwrap(), &w.startup.data).is_some(),
        "FR-044 a passed check must make the candidate an accepted version in the actual state file"
    );
}

#[then("the state file holds the two hashes and the version of the build 1.6.0")]
fn saved(w: &mut World) {
    let entry = check::checked(w.compatibility.candidate.as_ref().unwrap(), &w.startup.data)
        .expect("FR-044 both candidate hashes in state file");
    assert_eq!(entry["version"], "1.6.0", "FR-044 saved version");
    assert_eq!(
        entry["platform"],
        w.startup.run.as_ref().unwrap().release.platform,
        "FR-044 checked build belongs to this platform"
    );
}

#[then(expr = "the management panel shows the build 1.6.0 with the label {string}")]
async fn checked_label(w: &mut World, label: String) {
    check::labelled(&build::page(w).await, "1.6.0", &label);
}

#[then(
    expr = "the management panel shows each build of the list of the release with the label {string}"
)]
async fn release_labels(w: &mut World, label: String) {
    let html = build::page(w).await;
    for listed in &w.startup.run.as_ref().unwrap().release.builds {
        check::labelled(&html, &listed.version, &label);
    }
}

#[then("the check fails")]
fn fails(w: &mut World) {
    let entries = w.scenario.hub.entries();
    assert!(
        entries.iter().any(|entry| {
            entry.event["label"] == "build-1.6.0"
                && entry.event["command"]["line"].as_str().is_some_and(|line| {
                    let message: Value = serde_json::from_str(line).unwrap();
                    message["params"]["update"]["sessionUpdate"] == "tool_call"
                        && message["params"]["update"]["title"] == "view_file"
                })
        }),
        "FR-044 failure must follow a built-in tool that actually runs in the candidate process"
    );
    assert!(
        check::checked(w.compatibility.candidate.as_ref().unwrap(), &w.startup.data).is_none(),
        "FR-044 failed check cannot accept the candidate"
    );
}

#[then("the runtime process runs from the build 1.4.0 again")]
async fn restored(w: &mut World) {
    build::copied(w, "1.4.0");
    let entries = w.scenario.hub.entries();
    let servers: Vec<_> = entries
        .iter()
        .filter(|entry| entry.event["hello"]["role"] == "server")
        .collect();
    assert_eq!(
        servers.len(),
        3,
        "FR-044 earlier build, check, restored build"
    );
    assert_eq!(servers[1].event["hello"]["label"], "build-1.6.0");
    assert_eq!(servers[2].event["hello"]["label"], "normal");
    assert_ne!(
        servers[0].event["hello"]["pid"],
        servers[2].event["hello"]["pid"]
    );
    assert!(alive(
        servers[2].event["hello"]["pid"].as_u64().unwrap() as u32
    ));
    assert!(!alive(
        servers[1].event["hello"]["pid"].as_u64().unwrap() as u32
    ));
    assert_eq!(
        build::request(w).await.status,
        200,
        "FR-044 restored build serves requests"
    );
}

#[then(expr = "the row with the full path of the found build has the entry {string}")]
fn found_build_entry(w: &mut World, entry: String) {
    let folder = w.compatibility.candidate.as_ref().expect("a found build");
    let [server, _] = check::identity(folder);
    let expected = entry.replace("<short hash>", &server[..12]);
    let path = folder.join(build::program());
    let path = path.to_str().unwrap();
    let html = document::html(w.panel.answer.as_ref().unwrap());
    let row = document::elements(&html, "li")
        .find(|row| document::visible(row.content).contains(path))
        .unwrap_or_else(|| panic!("FR-042 the found build keeps its full path: {path}"));
    let buttons: Vec<_> = document::elements(row.content, "button")
        .map(|button| document::visible(button.content))
        .collect();
    assert!(
        panel::entry(row.content, &expected),
        "FR-053 the row of the found build needs the entry: {expected}; actual: {buttons:?}"
    );
}

#[then("the build 1.6.0 is not an accepted version")]
async fn rejected(w: &mut World) {
    assert!(
        check::checked(w.compatibility.candidate.as_ref().unwrap(), &w.startup.data).is_none(),
        "FR-044 failed build stays unaccepted"
    );
    let html = build::page(w).await;
    let path = w
        .compatibility
        .candidate
        .as_ref()
        .unwrap()
        .join(build::program());
    check::labelled(&html, path.to_str().unwrap(), "Not an accepted version");
}
