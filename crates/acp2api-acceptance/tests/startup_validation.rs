use acp2api_acceptance::startup_check::Startup;
#[tokio::test]
async fn present_unusable_values_never_take_defaults() {
    use toml::Value as V;
    let mut s = Startup::default();
    s.initialize().await;
    s.seed_config();
    let original = std::fs::read_to_string(s.config_path()).unwrap();
    let mut cases = Vec::new();
    for key in [
        "port",
        "concurrency_limit",
        "local_api_key",
        "management_password_hash",
        "agy_acp_server_path",
    ] {
        cases.push((key, V::Boolean(false)));
        cases.push((key, V::Array(vec![])));
    }
    for n in [-1, 0, 65536] {
        cases.push(("port", V::Integer(n)));
    }
    for n in [-1, 0] {
        cases.push(("concurrency_limit", V::Integer(n)));
    }
    for text in [
        "",
        "has space",
        "has\ttab",
        "has\nnewline",
        "nonascii-密",
        "has\u{7f}delete",
    ] {
        cases.push(("local_api_key", V::String(text.into())));
    }
    for text in [
        "",
        "$argon2id$",
        "$argon2id$v=19$m=19456,t=2,p=1",
        "$argon2id$v=99$m=19456,t=2,p=1$c2FsdHNhbHQ$YWJjZA",
        "$argon2id$v=19$m=0,t=2,p=1$c2FsdHNhbHQ$YWJjZA",
        "$argon2i$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$YWJjZA",
    ] {
        cases.push(("management_password_hash", V::String(text.into())));
    }
    cases.push(("agy_acp_server_path", V::String("relative/agent".into())));
    for (key, value) in cases {
        std::fs::write(s.config_path(), &original).unwrap();
        s.set(key, Some(value));
        s.launch_data(vec!["--port".into(), s.root().port.to_string()])
            .await;
        let expected = format!(
            "acp2api: the setting {key} in {} has a value that acp2api cannot use.",
            s.config_path().display()
        );
        assert!(
            s.exited == Some(false),
            "unusable present {key} must fail even with CLI override"
        );
        assert!(
            s.terminal().lines().any(|line| line == expected),
            "unusable {key} must identify that key without printing its value"
        );
    }
}
#[tokio::test]
async fn version_and_invalid_options_exit_without_writes() {
    let mut s = Startup::default();
    s.initialize().await;
    s.launch(vec!["--version".into()]).await;
    assert!(s.exited == Some(true));
    assert_eq!(
        s.terminal().trim(),
        concat!("acp2api ", env!("CARGO_PKG_VERSION"))
    );
    for args in [
        vec!["--other"],
        vec!["status"],
        vec!["--port"],
        vec!["--port", "0"],
        vec!["--port", "65536"],
        vec!["--data-dir"],
        vec!["--port", "1234", "--port", "1235"],
    ] {
        s.launch(args.into_iter().map(str::to_owned).collect())
            .await;
        assert!(
            s.exited == Some(false),
            "unknown commands or unusable options must fail"
        );
        assert!(
            !s.config_path().exists(),
            "CLI display/errors must not create configuration"
        );
    }
}
#[tokio::test]
async fn startup_round_trip_with_fresh_inputs() {
    use toml::Value as V;
    let mut s = Startup::default();
    s.initialize().await;
    let port = std::env::var("ACP2API_STARTUP_PROBE_PORT")
        .ok()
        .map(|p| p.parse::<u16>().unwrap())
        .unwrap_or(s.root().port);
    assert!(port > 0);
    let concurrency = std::env::var("ACP2API_STARTUP_PROBE_CONCURRENCY")
        .ok()
        .map(|v| v.parse::<i64>().unwrap())
        .unwrap_or(3);
    assert!((1..=3).contains(&concurrency));
    s.data = s.root().path.join("relative-settings");
    std::fs::create_dir(&s.data).unwrap();
    assert_eq!(
        std::fs::read_dir(&s.data).unwrap().count(),
        0,
        "first-start folder must be completely empty"
    );
    s.launch(vec![
        "--data-dir".into(),
        "relative-settings".into(),
        "--port".into(),
        port.to_string(),
    ])
    .await;
    assert!(
        s.exited.is_none(),
        "valid first start without runtime must remain available"
    );
    assert!(s.responds(port).await);
    s.remember_secrets();
    let key = &s.keys[0];
    assert!(
        key.starts_with("acp2api-")
            && key.len() == 51
            && key[8..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "generated key format (value withheld)"
    );
    let password = &s.passwords[0];
    assert!(
        password.len() == 23
            && password.split('-').count() == 4
            && password.split('-').all(|g| g.len() == 5
                && g.bytes()
                    .all(|b| b.is_ascii_lowercase() || (b'2'..=b'9').contains(&b))),
        "generated password format (value withheld)"
    );
    let before = s.saved().unwrap();
    assert_eq!(
        before["port"].as_integer(),
        Some(8329),
        "CLI port override must not replace saved default"
    );
    assert!(
        before["management_password_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$")),
        "new password must use the specified Argon2id parameters (value withheld)"
    );
    let urls = s.browser_urls();
    assert_eq!(urls, vec![format!("http://127.0.0.1:{port}/")]);
    let log_files: Vec<_> = std::fs::read_dir(s.data.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .collect();
    assert!(!log_files.is_empty(), "startup opens the production log");
    let log: String = log_files
        .into_iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    assert!(
        !log.contains(key) && !log.contains(password),
        "log must exclude generated secrets"
    );
    #[cfg(windows)]
    acp2api_acceptance::startup_permissions::assert_owner_only(&s.config_path());
    s.stop().await;
    s.set("port", Some(V::Integer(port.into())));
    s.set("concurrency_limit", Some(V::Integer(concurrency)));
    s.set(
        "local_api_key",
        Some(V::String(format!("{}:!~", s.root().key))),
    );
    let path = s.root().path.join("path/not-yet-installed-agent");
    s.set(
        "agy_acp_server_path",
        Some(V::String(path.to_str().unwrap().into())),
    );
    let edited = std::fs::read(s.config_path()).unwrap();
    s.launch_data(vec![]).await;
    assert!(
        s.exited.is_none(),
        "stopped settings edit must survive the next start"
    );
    assert!(s.responds(port).await);
    assert!(
        std::fs::read(s.config_path()).unwrap() == edited,
        "complete saved settings must not be reseeded or rewritten"
    );
    assert!(
        s.saved().unwrap()["management_password_hash"] == before["management_password_hash"],
        "restart must preserve saved hash (withheld)"
    );
    assert!(
        !s.terminal().contains("local API key:") && !s.terminal().contains("management password:"),
        "restart must not display existing secrets"
    );
    assert_eq!(
        s.browser_urls().len(),
        1,
        "subsequent start must not reopen browser"
    );
    s.stop().await;
}
#[tokio::test]
async fn missing_platform_folder_variable_fails_without_fallback() {
    let mut s = Startup::default();
    s.initialize().await;
    for platform in ["macos-arm64", "windows-x64"] {
        let run = s.run.as_mut().unwrap();
        run.release.platform = platform.into();
        if platform.starts_with("windows") {
            run.environment.local_app_data = None;
        } else {
            run.environment.home = None;
        }
        s.launch(vec![]).await;
        assert!(
            s.exited == Some(false),
            "missing platform variable must fail startup"
        );
        assert!(!s.config_path().exists());
    }
}
