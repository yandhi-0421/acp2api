use crate::{World, lockout_support as lockout};
use acp2api_acceptance::{client::Answer, journal_check, panel_check};
use serde_json::json;

use std::path::PathBuf;

pub const OWNER: &str = "lockout-owner";
pub const END: &str = "/manage/end-lockout";
pub const SIGN_IN_FILES: [(&str, &[u8]); 2] = [
    ("settings.json", b"synthetic-settings-73-7ff7ce4fd4b73fdd"),
    ("acp_token.json", b"synthetic-token-73-bf10eac6f5327658"),
];

#[derive(Debug, Default)]
pub struct EndLockoutState {
    pub marker: String,
    pub denied: Option<PathBuf>,
}

pub async fn page(w: &mut World) -> String {
    w.panel.scenario(&mut w.scenario).await;
    let answer = w.panel.view_session(OWNER).await;
    let html = panel_check::text(&answer);
    w.panel.answer = Some(answer);
    html
}

pub async fn end(w: &mut World) -> Answer {
    let body = json!({"password": w.scenario.root.password});
    w.panel.submit(OWNER, END, body).await
}

pub async fn ended(w: &mut World) {
    lockout::send_signal(w, lockout::SIGNAL).await;
    let html = page(w).await;
    assert!(html.contains(">Lockout<"), "setup reaches a real lockout");
    let answer = end(w).await;
    assert!(
        (200..400).contains(&answer.status) && lockout::state(w)["lockout"].is_null(),
        "BLOCKED setup: the owner must end the lockout before this scenario; HTTP {}",
        answer.status
    );
}

pub fn terminal(w: &World) -> String {
    w.scenario
        .hub
        .entries()
        .iter()
        .filter(|entry| entry.event["kind"] == "terminal")
        .filter_map(|entry| entry.event["text"].as_str())
        .collect()
}

pub fn private_outputs(w: &World, values: &[&str]) {
    let logs = journal_check::lines(&w.scenario);
    assert!(!logs.is_empty(), "positive control: product logs exist");
    let terminal = terminal(w);
    assert!(
        terminal.contains(" state Lockout"),
        "terminal captured the lockout"
    );
    let state = std::fs::read_to_string(lockout::state_path(w)).expect("product state file");
    assert!(
        serde_json::from_str::<serde_json::Value>(&state).unwrap()["lockout"].is_object(),
        "positive control: the lockout evidence was persisted"
    );
    for text in logs
        .iter()
        .map(String::as_str)
        .chain([terminal.as_str(), state.as_str()])
    {
        for value in values {
            assert!(
                !text.contains(value),
                "FR-056 request content and credentials must be absent from logs, terminal and state (value withheld)"
            );
        }
    }
}
