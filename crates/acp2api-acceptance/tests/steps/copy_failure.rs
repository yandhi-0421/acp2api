use crate::{World, copy_failure_support::DeniedCopy};
use acp2api_acceptance::journal_check;
use cucumber::{given, then};
use serde_json::{Value, json};
use std::fs;

#[given("the data folder denies creation of the automatic copy")]
fn denied_copy(w: &mut World) {
    fs::create_dir_all(w.startup.data.join("logs")).unwrap();
    w.copy_failure = Some(DeniedCopy::new(w.startup.data.clone()));
}

#[then(
    expr = "the log has one line for the change to \"runtime unavailable\" with the cause {string} and no path or other new field"
)]
fn copy_cause(w: &mut World, cause: String) {
    let records: Vec<Value> = journal_check::files(&w.startup.data.join("logs"))
        .into_iter()
        .flat_map(|path| {
            let log = fs::read_to_string(path).unwrap();
            log.lines()
                .map(|line| serde_json::from_str::<Value>(line).expect("production JSON log"))
                .filter(|record| {
                    record["event"] == "state_change" && record["state"] == "Runtime unavailable"
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(records.len(), 1, "FR-056 one change to runtime unavailable");
    let line = &records[0];
    assert_eq!(
        line,
        &json!({
            "time": line["time"],
            "event": "state_change",
            "state": "Runtime unavailable",
            "cause": cause,
        }),
        "FR-056 failed automatic copy must log only its OS error category with the state metadata"
    );
    assert!(
        !line
            .to_string()
            .contains(w.startup.root().path.to_str().unwrap()),
        "FR-056 state-change line must exclude paths of the owner"
    );
}
