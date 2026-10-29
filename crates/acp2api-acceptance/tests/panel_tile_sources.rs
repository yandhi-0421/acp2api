#[path = "support/panel_source.rs"]
mod panel_source;
use acp2api_acceptance::{
    build_fixture,
    panel_check::{Panel, text},
    real_limit,
    startup_check::Startup,
};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restart_recovers_the_build_source_from_zed() {
    tokio::time::timeout(real_limit(Duration::from_secs(45)), async {
        let mut startup = Startup::default();
        startup.initialize().await;
        startup.seed_config();
        let environment = &startup.run.as_ref().unwrap().environment;
        let zed = if cfg!(windows) {
            environment.local_app_data.as_ref().unwrap().join("Zed")
        } else {
            environment
                .home
                .as_ref()
                .unwrap()
                .join("Library/Application Support/Zed")
        };
        let source = zed.join("1.4.0");
        build_fixture::install(
            &startup.root().path.join("builds/normal"),
            &source,
            "1.4.0",
            "normal",
        )
        .unwrap();
        build_fixture::install(
            &startup.root().path.join("builds/normal"),
            &zed.join("0-decoy"),
            "1.4.0",
            "different-harness",
        )
        .unwrap();
        startup.launch_data(vec![]).await;
        startup.stop().await;
        startup.launch_data(vec![]).await;
        let mut panel = Panel::default();
        panel.bind(
            startup.root(),
            &startup.data,
            startup.panel_hub(),
            format!("127.0.0.1:{}", startup.root().port),
        );
        let page = text(&panel.view_session("owner").await);
        let actual = panel_source::source_line(&page).map(str::to_owned);
        let expected = format!(
            "<code>{}</code>",
            source.join(build_fixture::SERVER).display()
        );
        startup.stop().await;
        assert_eq!(
            actual.as_deref(),
            Some(expected.as_str()),
            "FR-054 the source line of the build in use survives a restart"
        );
        std::fs::remove_dir_all(&source).unwrap();
        startup.launch_data(vec![]).await;
        let page = text(&panel.view_session("owner").await);
        let missing = panel_source::source_line(&page).map(str::to_owned);
        startup.stop().await;
        assert_eq!(missing.as_deref(), Some("Not found."),
            "FR-054 a copy and a same-version build with a different harness are not the missing source");
    })
    .await
    .expect(
        "source restart cap: 45 s, three starts, four HTTP requests, existing hub and capture caps",
    );
}
