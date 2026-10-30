use crate::{
    World, end_lockout_support as support, lockout_support as lockout, panel_document as document,
};
use acp2api_acceptance::{Setup, endpoint_check};
use cucumber::{given, then, when};
use serde_json::json;

#[when("a lockout signal starts a lockout")]
#[when("a lockout signal then starts a lockout")]
async fn signal(w: &mut World) {
    lockout::send_signal(w, lockout::SIGNAL).await;
}

#[then("the terminal shows one line with the lockout notice")]
async fn terminal_notice(w: &mut World) {
    w.scenario.time.settle().await;
    let terminal = support::terminal(w);
    let lines: Vec<_> = terminal
        .lines()
        .filter(|line| line.contains(" state Lockout"))
        .collect();
    assert_eq!(lines.len(), 1, "FR-035 exactly one lockout terminal line");
    let notice = format!(
        "acp2api: lockout since {} after 403; acp2api sends no request to the official runtime until the owner ends the lockout at http://127.0.0.1:{}/.",
        lockout::SINCE,
        w.scenario.root.port
    );
    assert!(
        lines[0].ends_with(&notice),
        "FR-035 terminal line must contain M-12: {}",
        lines[0]
    );
}

#[then(expr = "the first tile of the management panel is dark and shows the state {string}")]
async fn dark_tile(w: &mut World, state: String) {
    let html = support::page(w).await;
    let first = document::elements(&html, "section")
        .next()
        .expect("first tile");
    let heading = document::elements(first.content, "h1")
        .next()
        .expect("state heading");
    assert_eq!(
        document::visible(heading.content),
        state,
        "FR-035 first tile state"
    );
    let audit = acp2api_acceptance::panel_visual::render(&html, &w.scenario.root.path, false)
        .await
        .expect("Chrome renders the locked management panel");
    acp2api_acceptance::panel_visual::dark_first_tile(&audit).unwrap();
}

#[when("the owner restarts acp2api and asks for a refresh of the quota in the management panel")]
async fn restarted_refresh(w: &mut World) {
    assert!(
        !w.scenario.fake().record().prompts().is_empty(),
        "the signal reached ACP before restart"
    );
    w.scenario.restart().await;
    support::page(w).await;
    let call = w
        .panel
        .owner(support::OWNER)
        .submit("/manage/refresh-quota", json!({}));
    w.panel.answer = Some(call.finish(&mut w.scenario.time).await);
}

#[when("the owner opens the management panel")]
async fn opened(w: &mut World) {
    support::page(w).await;
}

#[then("the management panel has no sign-in entry")]
fn no_sign_in(w: &mut World) {
    let html = document::html(w.panel.answer.as_ref().unwrap());
    assert!(
        document::visible(&html).contains("Lockout"),
        "inspect the authenticated lockout page"
    );
    assert!(
        document::elements(&html, "form")
            .all(|form| document::attribute(form.tag, "action") != Some("/manage/sign-in")),
        "FR-035/FR-039 locked panel must omit the sign-in entry"
    );
}

#[given("the management panel shows the evidence of the lockout")]
async fn evidence(w: &mut World) {
    let html = support::page(w).await;
    let visible = document::visible(&html);
    let stored = lockout::state(w);
    for field in ["since", "signal", "original_error_text"] {
        assert!(
            visible.contains(stored["lockout"][field].as_str().unwrap()),
            "FR-035 the owner can read lockout evidence {field}"
        );
    }
}

#[when("the owner ends the lockout with the management password")]
async fn end(w: &mut World) {
    w.panel.answer = Some(support::end(w).await);
}

#[then("acp2api is in the normal state at once")]
async fn normal(w: &mut World) {
    let at = w.scenario.hub.now();
    let status = w.panel.answer.as_ref().unwrap().status;
    let html = support::page(w).await;
    assert!(
        (200..400).contains(&status)
            && html.contains(">Running<")
            && lockout::state(w)["lockout"].is_null(),
        "FR-035 ending lockout must immediately restore Running and clear persisted lockout; HTTP {status}"
    );
    assert_eq!(w.scenario.hub.now(), at, "FR-035 no timer ends the lockout");
}

#[when("the owner asks for the end of the lockout with no management password")]
async fn without_password(w: &mut World) {
    assert_eq!(
        w.panel.answer.as_ref().unwrap().status,
        200,
        "open session has performed a management action"
    );
    assert!(
        !w.panel.owner("B1").cookies.is_empty(),
        "a real session exists"
    );
    // Use the same session as the Given, so rejection cannot come from the session guard.
    w.panel.answer = Some(w.panel.submit("B1", support::END, json!({})).await);
    let status = w.panel.answer.as_ref().unwrap().status;
    assert!(
        status < 500 && !matches!(status, 404 | 405),
        "BLOCKED: the password check needs an available end-lockout endpoint; HTTP {status}"
    );
}

#[given("the owner ended a lockout")]
async fn ended(w: &mut World) {
    support::ended(w).await;
}

#[when("no API request comes for 10 minutes")]
async fn idle(w: &mut World) {
    w.scenario
        .time
        .advance_to(w.scenario.hub.now() + 600_000)
        .await;
}

#[given("the runtime home holds the sign-in files of the official runtime")]
async fn sign_in_files(w: &mut World) {
    w.scenario.plan(Setup::normal().configure(|root, _, _| {
        let home = root.path.join("data/runtime-home");
        std::fs::create_dir_all(&home).unwrap();
        for (name, bytes) in support::SIGN_IN_FILES {
            std::fs::write(home.join(name), bytes).unwrap();
        }
    }));
    w.scenario.start().await;
    let call = w.scenario.client().await.model_names();
    let answer = call.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, 200,
        "the runtime is ready before the sign-in file event"
    );
    for (name, bytes) in support::SIGN_IN_FILES {
        assert_eq!(
            std::fs::read(w.scenario.root.path.join("data/runtime-home").join(name)).unwrap(),
            bytes,
            "sign-in fixture exists before the event"
        );
    }
}

#[when("the owner stops acp2api")]
async fn stop(w: &mut World) {
    w.scenario.stop().await;
}

#[then("the sign-in files of the official runtime are unchanged")]
fn unchanged(w: &mut World) {
    for (name, bytes) in support::SIGN_IN_FILES {
        let actual = std::fs::read(w.scenario.root.path.join("data/runtime-home").join(name));
        assert_eq!(
            actual.ok().as_deref(),
            Some(bytes),
            "FR-046 sign-in file {name} must remain byte-for-byte unchanged"
        );
    }
}

#[when(
    expr = "an API client sends a request with the marker {string} in the message text, the system prompt, a tool argument, and a tool result"
)]
async fn private_content(w: &mut World, alias: String) {
    assert_eq!(alias, "MARKER-7Q");
    let marker = format!(
        "{alias}-privacy-73-{}",
        acp2api_acceptance::setup::random_hex(24)
    );
    let mut call = endpoint_check::request("POST /v1/messages", &marker, false);
    call.body["system"] = json!(marker);
    call.body["tools"] = json!([{"name":"privacy_tool", "description":"Synthetic tool", "input_schema":{"type":"object", "properties":{"value":{"type":"string"}}}}]);
    call.body["messages"] = json!([
        {"role":"user","content":marker},
        {"role":"assistant","content":[{"type":"tool_use","id":"privacy-call","name":"privacy_tool","input":{"value":marker}}]},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"privacy-call","content":marker}]}
    ]);
    w.request.send(&mut w.scenario, call).await;
    let record = w.scenario.fake().record();
    assert!(
        record
            .session_meta()
            .iter()
            .any(|meta| meta.to_string().contains(&marker)),
        "system prompt marker reaches ACP"
    );
    assert!(
        record
            .prompts()
            .last()
            .unwrap()
            .text
            .matches(&marker)
            .count()
            >= 3,
        "message, argument and result markers reach ACP"
    );
    w.end_lockout.marker = marker;
}

#[then(expr = "the logs, the terminal, and the state file do not hold {string}")]
async fn content_absent(w: &mut World, alias: String) {
    assert_eq!(alias, "MARKER-7Q");
    w.scenario.stop().await;
    support::private_outputs(w, &[&w.end_lockout.marker]);
}

#[given(expr = "the local API key is {string} and the management password is {string}")]
fn credentials(_w: &mut World, key: String, password: String) {
    assert_eq!((key.as_str(), password.as_str()), ("K1", "P1"));
}

#[given("the owner restarted acp2api, so the start made no new key and no new password")]
async fn restarted(w: &mut World) {
    w.scenario.start().await;
    w.scenario.restart().await;
    let terminal = support::terminal(w);
    assert!(
        !terminal.contains("acp2api: local API key:")
            && !terminal.contains("acp2api: management password:"),
        "stored credentials cause no new-credential delivery"
    );
}

#[when(
    expr = "an API client sends requests with {string}, and the owner opens the management panel with {string}"
)]
async fn use_credentials(w: &mut World, key: String, password: String) {
    assert_eq!((key.as_str(), password.as_str()), ("K1", "P1"));
    for endpoint in [
        "POST /v1/messages",
        "POST /v1/chat/completions",
        "POST /v1/responses",
    ] {
        w.request
            .send(
                &mut w.scenario,
                endpoint_check::request(endpoint, "credential-privacy-73-6fea20bf4e66555d", false),
            )
            .await;
    }
    support::page(w).await;
}

#[then(expr = "the logs, the terminal, and the state file do not hold {string} or {string}")]
async fn credentials_absent(w: &mut World, key: String, password: String) {
    assert_eq!((key.as_str(), password.as_str()), ("K1", "P1"));
    w.scenario.stop().await;
    support::private_outputs(w, &[&w.scenario.root.key, &w.scenario.root.password]);
}
