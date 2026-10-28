use acp2api_acceptance::{Scenario, Setup, client::ApiCall};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_new_names_the_eager_mcp_server() {
    let mut scenario = Scenario::default();
    scenario.plan(Setup::normal());
    let call = scenario.client().await.send(ApiCall::one_user_message(
        "POST /v1/messages",
        "protocol option check",
    ));
    assert_eq!(call.finish(&mut scenario.time).await.status, 200);
    // Captured session/new in locked-meta-echo.transcript.ndjson uses a name list.
    let meta = scenario.fake().record().session_meta();
    assert_eq!(
        meta[0]["antigravity"]["eagerMcpServers"],
        json!(["client-tools"])
    );
    scenario.finish(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_preserves_saved_configuration_and_runtime_copy() {
    let mut scenario = Scenario::default();
    scenario.plan(Setup::normal());
    scenario.start().await;
    scenario.stop().await;
    let config_path = scenario.root.path.join("data/config.toml");
    let config = std::fs::read_to_string(&config_path)
        .unwrap()
        .replace("concurrency_limit = 2", "concurrency_limit = 1");
    std::fs::write(&config_path, &config).unwrap();
    let runtime = scenario.root.path.join("data/official-runtime");
    let marker = runtime.join("owner-marker");
    let saved = b"owner annotation";
    std::fs::write(&marker, saved).unwrap();
    #[cfg(unix)]
    let executable = runtime.join("agy_acp_server.par");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    scenario.restart().await;
    let loaded: toml::Value =
        toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(loaded["concurrency_limit"].as_integer(), Some(1));
    assert!(
        std::fs::read_to_string(&config_path).unwrap() == config,
        "saved config bytes changed"
    );
    assert!(
        std::fs::read(&marker).unwrap() == saved,
        "runtime marker changed"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(executable).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    let call = scenario.client().await.send(ApiCall::one_user_message(
        "POST /v1/messages",
        "after owner edit",
    ));
    assert_eq!(call.finish(&mut scenario.time).await.status, 200);
    scenario.stop().await;
    let program = runtime.join(if cfg!(windows) {
        "agy_acp_server.exe"
    } else {
        "agy_acp_server.par"
    });
    let mut edited_program = std::fs::read(&program).unwrap();
    edited_program.extend_from_slice(b"owner edit retained across restart");
    std::fs::write(&program, &edited_program).unwrap();
    let launches = scenario
        .hub
        .entries()
        .iter()
        .filter(|entry| entry.event["kind"] == "launch")
        .count();
    scenario.restart().await;
    let answer = scenario
        .client()
        .await
        .send(ApiCall::one_user_message(
            "POST /v1/messages",
            "changed executable",
        ))
        .finish(&mut scenario.time)
        .await;
    assert_eq!(answer.status, 400);
    assert!(
        answer.json()["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("acp2api: the official runtime is unavailable:")
    );
    assert_eq!(
        scenario
            .hub
            .entries()
            .iter()
            .filter(|entry| entry.event["kind"] == "launch")
            .count(),
        launches,
        "a changed copy must be rejected before launch"
    );
    assert!(
        std::fs::read(&program).unwrap() == edited_program,
        "runtime executable bytes changed"
    );
    scenario.finish(true).await;
}
