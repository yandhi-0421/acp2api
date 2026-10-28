#[path = "selection.rs"]
mod selection;

use acp2api_acceptance::setup::TestRoot;
use cucumber::Parser as _;
use futures_util::StreamExt as _;

#[tokio::test]
async fn outline_keeps_a_step_placeholder_that_is_not_an_examples_column() {
    let root = TestRoot::new();
    let folder = root.path.join("features");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("literal.feature"), "Feature: Publication\n @FR-006\n Scenario Outline: Duplicate\n  When start uses <folder>\n  Then message is \"panel at <address>\"\n  Examples:\n   | folder |\n   | D1     |\n   | D2     |\n").unwrap();
    let features = selection::Selected::new(&[], "not @wip")
        .parse(&folder, cucumber::cli::Empty {})
        .collect::<Vec<_>>()
        .await;
    let feature = features
        .into_iter()
        .next()
        .unwrap()
        .expect("literal message placeholders must parse");
    assert_eq!(feature.scenarios.len(), 2);
    for (scenario, folder) in feature.scenarios.iter().zip(["D1", "D2"]) {
        assert_eq!(scenario.steps[0].value, format!("start uses {folder}"));
        assert_eq!(scenario.steps[1].value, "message is \"panel at <address>\"");
    }
    root.cleanup();
}

#[tokio::test]
async fn outline_replaces_a_step_placeholder_named_by_an_examples_column() {
    let root = TestRoot::new();
    let folder = root.path.join("features");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("columns.feature"), "Feature: Publication\n Rule: Address\n  @FR-006\n  Scenario Outline: Duplicate\n   Then message is \"panel at <address>\"\n   Examples:\n    | address               |\n    | http://127.0.0.1:8329/ |\n    | http://127.0.0.1:8330/ |\n").unwrap();
    let features = selection::Selected::new(&[], "not @wip")
        .parse(&folder, cucumber::cli::Empty {})
        .collect::<Vec<_>>()
        .await;
    let feature = features
        .into_iter()
        .next()
        .unwrap()
        .expect("Examples columns must expand");
    let scenarios = &feature.rules[0].scenarios;
    assert_eq!(scenarios.len(), 2);
    for (scenario, port) in scenarios.iter().zip([8329, 8330]) {
        assert_eq!(
            scenario.steps[0].value,
            format!("message is \"panel at http://127.0.0.1:{port}/\"")
        );
    }
    root.cleanup();
}
