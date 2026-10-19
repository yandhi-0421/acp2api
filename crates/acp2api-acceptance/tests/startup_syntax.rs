mod selection;
use cucumber::Parser;
use futures_util::StreamExt;
#[tokio::test]
async fn startup_syntax_retains_paths_and_message_templates() {
    let parser = selection::Selected::new(&["--tags".into(), "@issue-37".into()], "not @wip");
    let features = parser
        .parse(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../features/acp2api"),
            cucumber::cli::Empty {},
        )
        .collect::<Vec<_>>()
        .await;
    let scenarios = features
        .into_iter()
        .flat_map(|f| f.expect("startup syntax").scenarios)
        .collect::<Vec<_>>();
    assert_eq!(scenarios.len(), 21);
    let windows = scenarios
        .iter()
        .find(|s| {
            s.steps
                .iter()
                .any(|step| step.value == "a computer with Windows and no data folder of acp2api")
        })
        .unwrap();
    assert_eq!(
        windows.steps.last().unwrap().value,
        "the configuration file \"config.toml\" and the logs are in the folder \"%LOCALAPPDATA%\\acp2api\\\""
    );
    let syntax = scenarios
        .iter()
        .filter(|s| s.name == "A TOML syntax error stops the start and identifies its location")
        .collect::<Vec<_>>();
    assert_eq!(syntax.len(), 2);
    for (scenario, location) in syntax.into_iter().zip(["port", "at line 2"]) {
        assert!(scenario.steps.iter().any(|s|s.value==format!("the start fails with the message \"acp2api: the setting {location} in <path of the configuration file> has a value that acp2api cannot use.\"")));
    }
}
