use acp2api_acceptance::startup_check::Startup;

#[tokio::test]
#[cfg(unix)]
#[allow(
    clippy::disallowed_methods,
    reason = "This OS-boundary test controls stdout and bounds the test host lifetime."
)]
async fn failed_terminal_delivery_leaves_no_undisclosed_saved_credentials() {
    use std::os::{fd::OwnedFd, unix::net::UnixStream};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut startup = Startup::default();
    startup.initialize().await;
    let run_path = startup.root().path.join("delivery-run.toml");
    std::fs::write(
        &run_path,
        toml::to_string(startup.run.as_ref().unwrap()).unwrap(),
    )
    .unwrap();
    let (terminal, reader) = UnixStream::pair().unwrap();
    drop(reader);
    let mut child = Command::new(env!("CARGO_BIN_EXE_acp2api-test-host"))
        .env_clear()
        .env("ACP2API_TEST_RUN", run_path)
        .current_dir(&startup.root().path)
        .args([
            "--data-dir",
            startup.data.to_str().unwrap(),
            "--port",
            &startup.root().port.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(OwnedFd::from(terminal))
        .stderr(Stdio::null())
        .spawn()
        .expect("test host with a terminal that rejects writes");
    let deadline = Instant::now() + Duration::from_secs(8);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("terminal failure observation deadline (8 s)");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(
        !status.success(),
        "terminal delivery failure must stop startup"
    );
    assert!(
        !startup.config_path().exists(),
        "failed terminal delivery must not save undisclosed credentials"
    );
    assert!(startup.browser_urls().is_empty());
    startup
        .launch_data(vec!["--port".into(), startup.root().port.to_string()])
        .await;
    assert!(startup.exited.is_none());
    startup.remember_secrets();
    assert!(startup.responds(startup.root().port).await);
    startup.stop().await;
}

#[tokio::test]
async fn occupied_first_start_then_retry_delivers_the_saved_credentials() {
    let mut startup = Startup::default();
    startup.initialize().await;
    assert_eq!(
        std::fs::read_dir(&startup.data).unwrap().count(),
        0,
        "first attempt must start from an empty data folder"
    );
    let port = std::env::var("ACP2API_RECOVERY_PROBE_PORT")
        .ok()
        .map(|value| value.parse::<u16>().unwrap())
        .unwrap_or(startup.root().port);
    assert!(port > 0);
    let occupied = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .expect("requested loopback port must be free before the test occupies it");
    let args = vec!["--port".into(), port.to_string()];
    startup.launch_data(args.clone()).await;
    assert!(startup.exited == Some(false), "occupied start must fail");
    let first_terminal = startup.terminal();
    let expected = format!(
        "acp2api: the port {port} is in use. Set another port in the configuration file or with --port."
    );
    assert!(
        first_terminal.lines().any(|line| line == expected),
        "occupied start must report M-25 for the exact requested port"
    );
    let saved_after_failure = std::fs::read(startup.config_path()).unwrap();
    assert!(startup.browser_urls().is_empty());
    drop(occupied);

    // Retry the exact configuration left by the failed start.
    startup.launch_data(args).await;
    assert!(startup.exited.is_none(), "retry must remain available");
    assert!(
        startup.responds(port).await,
        "retry must use the chosen port"
    );
    startup.stop().await;
    let terminal = format!("{first_terminal}\n{}", startup.terminal());
    let saved = startup.saved().unwrap();
    let key_received = terminal.lines().any(|line| {
        line.strip_prefix("acp2api: local API key: ")
            .is_some_and(|key| saved["local_api_key"].as_str() == Some(key))
    });
    use argon2::PasswordVerifier;
    let hash = argon2::password_hash::phc::PasswordHash::new(
        saved["management_password_hash"].as_str().unwrap(),
    )
    .unwrap();
    let password_received = terminal.lines().any(|line| {
        line.strip_prefix("acp2api: management password: ")
            .is_some_and(|password| {
                argon2::Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            })
    });
    assert!(
        key_received && password_received,
        "owner must receive credentials matching the saved configuration across failed start and retry (values withheld)"
    );
    assert!(
        std::fs::read(startup.config_path()).unwrap() == saved_after_failure,
        "retry must preserve valid saved credentials (values withheld)"
    );
    let log = std::fs::read_to_string(startup.data.join("logs/acp2api.log")).unwrap();
    for line in terminal.lines() {
        for prefix in ["acp2api: local API key: ", "acp2api: management password: "] {
            if let Some(value) = line.strip_prefix(prefix) {
                assert!(!log.contains(value), "logs must exclude credentials");
                if prefix.contains("password") {
                    assert!(
                        !String::from_utf8_lossy(&saved_after_failure).contains(value),
                        "configuration must exclude plaintext passwords"
                    );
                }
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(startup.config_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    #[cfg(windows)]
    acp2api_acceptance::startup_permissions::assert_owner_only(&startup.config_path());
}
