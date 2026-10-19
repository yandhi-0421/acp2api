use crate::World;
use acp2api_acceptance::journal_check as check;
use cucumber::{given, then, when};
use std::{path::PathBuf, time::Duration};
#[derive(Debug, Default)]
pub struct LogState {
    expected: Vec<serde_json::Value>,
    stress: Option<check::Retention>,
    age: Option<PathBuf>,
}
#[when(
    expr = "an API client sends a valid request to {string} with the model {string} and the unknown field {string}"
)]
async fn request(w: &mut World, path: String, model: String, field: String) {
    let expected = check::send(
        &mut w.scenario,
        &path,
        &model,
        &field,
        "PRIVATE-CONTENT-MARKER",
        137,
        17,
    )
    .await;
    w.logs.expected.push(expected);
    check::wait_terminal(&w.scenario, 1).await;
}
#[then("the log has one line for the request with these fields:")]
fn fields(w: &mut World, #[step] step: &cucumber::gherkin::Step) {
    assert_eq!(step.table.as_ref().unwrap().rows.len(), 12);
    check::verify(&w.scenario, &w.logs.expected);
    for file in check::files(&w.scenario.root.path.join("data/logs")) {
        assert!(
            !std::fs::read_to_string(file)
                .unwrap()
                .contains("PRIVATE-CONTENT-MARKER")
        );
    }
}
#[then(expr = "the unknown field names of the line hold {string}")]
fn unknown(w: &mut World, field: String) {
    assert_eq!(
        check::records(&w.scenario)[0]["unknown_fields"],
        serde_json::json!([field])
    );
}
#[when(expr = "an API client sends 3 requests with the model name {string}")]
async fn three(w: &mut World, model: String) {
    for (n, path) in [
        "POST /v1/messages",
        "POST /v1/responses",
        "POST /v1/chat/completions",
    ]
    .iter()
    .enumerate()
    {
        w.logs.expected.push(
            check::send(
                &mut w.scenario,
                path,
                &model,
                &format!("unknown_{n}"),
                "PRIVATE-THREE",
                137 + n as u64,
                17 + n as u64,
            )
            .await,
        );
    }
    check::wait_terminal(&w.scenario, 3).await;
}
#[then("the terminal shows 3 request lines")]
fn count(w: &mut World) {
    assert_eq!(
        check::terminal(&w.scenario).len(),
        3,
        "FR-002 three terminal request lines"
    );
}
#[then("each request line holds the request ID, the API, and the model name of its request")]
fn correlation(w: &mut World) {
    check::verify(&w.scenario, &w.logs.expected);
}
#[when("acp2api writes 60 MB of log lines")]
fn stress(w: &mut World) {
    w.logs.stress = Some(check::stress(
        &w.scenario.root.path.join("writer-logs"),
        "acceptance",
    ));
}
#[then("the logs take no more than 50 MB")]
fn total(w: &mut World) {
    check::verify_stress(w.logs.stress.as_ref().unwrap(), "acceptance");
    w.scenario.root.check_size();
}
#[then("no log file is larger than 10 MB")]
fn individual(w: &mut World) {
    assert!(
        w.logs
            .stress
            .as_ref()
            .unwrap()
            .retained
            .iter()
            .all(|(_, n)| *n <= 10_000_000)
    );
}
#[then("the log file that acp2api deleted is the oldest log file")]
fn oldest(w: &mut World) {
    assert!(!w.logs.stress.as_ref().unwrap().first.exists());
}
#[given(regex = r"^the log folder has a log file whose modification time is (.+) ago$")]
async fn age_fixture(w: &mut World, age: String) {
    w.scenario.start().await;
    let offset = match age.as_str() {
        "30 days plus 1 ms" => 1,
        "30 days" => 0,
        "30 days minus 1 ms" => -1,
        _ => panic!("age"),
    };
    let at = check::wall(w.scenario.hub.now())
        - Duration::from_millis((30_i64 * 24 * 60 * 60 * 1000 + offset) as u64);
    let file = w
        .scenario
        .root
        .path
        .join("data/logs/acp2api-2026-09-17-1.log");
    std::fs::write(&file, "owned log age fixture\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(at))
        .unwrap();
    assert_eq!(
        std::fs::metadata(&file).unwrap().modified().unwrap(),
        at,
        "real modification timestamp"
    );
    let unrelated = w.scenario.root.path.join("data/logs/unrelated.txt");
    std::fs::write(&unrelated, "outside cleanup authority").unwrap();
    std::fs::File::options()
        .write(true)
        .open(unrelated)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(at))
        .unwrap();
    w.logs.age = Some(file);
}
#[when("the owner restarts acp2api")]
async fn restart(w: &mut World) {
    w.scenario.restart().await;
}
#[then(regex = r"^that log file is (gone|still there)$")]
fn age_result(w: &mut World, result: String) {
    assert_eq!(
        w.logs.age.as_ref().unwrap().exists(),
        result == "still there",
        "FR-057 strict modification-age boundary"
    );
    assert!(
        w.scenario
            .root
            .path
            .join("data/logs/unrelated.txt")
            .is_file(),
        "unrelated file survives cleanup"
    );
}
