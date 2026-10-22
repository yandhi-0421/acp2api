use acp2api_acceptance::{
    client::ApiCall,
    panel_check::{Panel, management_actions, text},
    startup_check::Startup,
};
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_management_action_pairs_denial_with_authenticated_success_and_origin_checks() {
    let mut startup = Startup::default();
    let mut panel = Panel::default();
    panel.startup(&mut startup).await;
    let old_key = panel.old_key.clone();
    for (path, body) in management_actions() {
        let before = toml::to_string(&panel.saved()).unwrap();
        let denied = panel.submit("anonymous", path, body.clone()).await;
        assert_eq!(
            denied.status, 401,
            "FR-009 actual management action requires a session: {path}"
        );
        assert!(
            toml::to_string(&panel.saved()).unwrap() == before,
            "FR-009 anonymous action changes no setting"
        );
        panel.login_initial("owner").await;
        panel.valid("owner", true).await;
        let origin = format!("http://{}", panel.address);
        let cookie = panel
            .owner("owner")
            .cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        for (name, value) in [
            ("origin", "https://example.com"),
            ("origin", "null"),
            ("host", "example.com"),
        ] {
            let a = panel
                .owner("owner")
                .client
                .send(ApiCall {
                    method: "POST".into(),
                    path: path.into(),
                    body: body.clone(),
                    headers: vec![
                        ("cookie".into(), cookie.clone()),
                        (name.into(), value.into()),
                        (
                            "content-type".into(),
                            "application/x-www-form-urlencoded".into(),
                        ),
                    ],
                })
                .finish(&mut panel.time())
                .await;
            assert_eq!(
                a.status, 400,
                "FR-008 Host/Origin guard on every actual management action: {path}"
            );
            assert!(
                !a.headers
                    .keys()
                    .any(|key| key.starts_with("access-control-"))
            );
            assert!(
                toml::to_string(&panel.saved()).unwrap() == before,
                "FR-008 rejected Origin changes no setting"
            );
        }
        let a = panel
            .owner("owner")
            .client
            .send(ApiCall {
                method: "POST".into(),
                path: path.into(),
                body: body.clone(),
                headers: vec![
                    ("cookie".into(), cookie),
                    ("origin".into(), origin),
                    (
                        "content-type".into(),
                        "application/x-www-form-urlencoded".into(),
                    ),
                ],
            })
            .finish(&mut panel.time())
            .await;
        assert_eq!(
            a.status,
            if path == "/manage/concurrency" {
                200
            } else {
                303
            },
            "FR-008 actual same-origin form action succeeds: {path}"
        );
        match path {
            "/manage/key" => {
                let key = panel.saved()["local_api_key"].as_str().unwrap().to_owned();
                assert_eq!(panel.api(&old_key).await.status, 401);
                assert_eq!(panel.api(&key).await.status, 200);
            }
            "/manage/password" => {
                let password = body["password"].as_str().unwrap();
                panel.saved_hash(password);
                panel.login("new-password", password).await;
                panel.valid("new-password", true).await;
                panel.initial_password = password.into();
            }
            "/manage/concurrency" => {
                assert_eq!(panel.saved()["concurrency_limit"].as_integer(), Some(3))
            }
            _ => unreachable!(),
        }
    }
    let page = panel.page("owner").await;
    assert_eq!(
        page.headers.get("cache-control").map(String::as_str),
        Some("no-store")
    );
    assert_eq!(
        page.headers.get("x-frame-options").map(String::as_str),
        Some("DENY")
    );
    assert!(text(&page).contains("Local API key"));
    for (name, value) in [("origin", "https://example.com"), ("host", "example.com")] {
        let p = panel.initial_password.clone();
        let a = panel
            .owner("bad-login")
            .client
            .send(ApiCall {
                method: "POST".into(),
                path: "/manage/login".into(),
                body: json!({"password":p}),
                headers: vec![
                    (name.into(), value.into()),
                    (
                        "content-type".into(),
                        "application/x-www-form-urlencoded".into(),
                    ),
                ],
            })
            .finish(&mut panel.time())
            .await;
        assert_eq!(a.status, 400);
        assert!(!a.headers.contains_key("set-cookie"));
    }
    startup.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_settings_and_persistence_failure_leave_live_settings_unchanged() {
    let mut startup = Startup::default();
    let mut panel = Panel::default();
    panel.startup(&mut startup).await;
    panel.login_initial("B1").await;
    panel.valid("B1", true).await;
    let before = std::fs::read(startup.config_path()).unwrap();
    for (path, body) in [
        ("/manage/concurrency", json!({"concurrency_limit":"0"})),
        ("/manage/concurrency", json!({"concurrency_limit":"4"})),
        ("/manage/concurrency", json!({"concurrency_limit":"wrong"})),
        ("/manage/password", json!({"password":""})),
    ] {
        let a = panel.submit("B1", path, body).await;
        assert_eq!(a.status, 422, "FR-004 unusable setting must fail");
        assert!(
            std::fs::read(startup.config_path()).unwrap() == before,
            "FR-004 invalid input leaves settings intact"
        );
    }
    let backup = startup.data.join("configuration-before-failure");
    std::fs::rename(startup.config_path(), &backup).unwrap();
    std::fs::create_dir(startup.config_path()).unwrap();
    let call = panel.owner("B1").submit("/manage/key", json!({}));
    let a = call.finish(&mut panel.time()).await;
    assert_eq!(a.status, 500, "FR-004 persistence failure is reported");
    std::fs::remove_dir(startup.config_path()).unwrap();
    std::fs::rename(backup, startup.config_path()).unwrap();
    let key = panel.old_key.clone();
    assert_eq!(
        panel.api(&key).await.status,
        200,
        "FR-004 persistence failure leaves live key intact"
    );
    assert!(std::fs::read(startup.config_path()).unwrap() == before);
    panel.valid("B1", true).await;
    startup.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panel_fixture_uses_the_published_port_override_after_a_settings_write() {
    let mut startup = Startup::default();
    startup.initialize().await;
    startup.seed_config();
    startup.panel_runtime();
    let saved = startup.root().port;
    let _saved_port_reservation =
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, saved)).unwrap();
    let override_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert_ne!(saved, override_port);
    startup
        .launch_data(vec!["--port".into(), override_port.to_string()])
        .await;
    let mut panel = Panel::default();
    panel.startup(&mut startup).await;
    assert_eq!(panel.address, format!("127.0.0.1:{override_port}"));
    panel.login_initial("override").await;
    panel.valid("override", true).await;
    assert_eq!(
        panel
            .submit(
                "override",
                "/manage/concurrency",
                json!({"concurrency_limit":"3"})
            )
            .await
            .status,
        200
    );
    panel.startup(&mut startup).await;
    panel.valid("override", true).await;
    assert_eq!(panel.saved()["port"].as_integer(), Some(i64::from(saved)));
    startup.stop().await;
}
