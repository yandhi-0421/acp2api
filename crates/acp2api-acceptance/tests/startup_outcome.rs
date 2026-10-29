use acp2api_acceptance::{Scenario, Setup};

#[tokio::test]
async fn a_ready_restart_returns_an_available_api_client() {
    let mut scenario = Scenario::default();
    scenario.plan(Setup::normal());
    scenario.start().await;
    assert!(
        scenario.restart_outcome().await.is_ok(),
        "observer reports a ready restart"
    );
    let live = scenario.client().await.model_names();
    let answer = live.finish(&mut scenario.time).await;
    assert_eq!(answer.status, 200, "observed restart serves real HTTP");
    assert!(
        answer.json()["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["id"] == "gemini-3.8-flash")
    );
    scenario.finish(true).await;
}

#[tokio::test]
async fn an_exit_at_startup_returns_its_status_without_a_plumbing_panic() {
    let mut scenario = Scenario::default();
    scenario.plan(Setup::normal());
    scenario.start().await;
    scenario.stop().await;
    // Invalid settings cause a real startup exit, independent of the lockout policy.
    let path = scenario.root.path.join("data/config.toml");
    let mut config: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    config["port"] = toml::Value::Integer(-1);
    std::fs::write(path, toml::to_string(&config).unwrap()).unwrap();
    let outcome = scenario.restart_outcome().await;
    assert!(outcome.is_err(), "observer reports the real startup exit");
    assert_eq!(outcome.unwrap_err().code(), Some(1));
    scenario.finish(true).await;
}
