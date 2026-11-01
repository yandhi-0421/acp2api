use acp2api_acceptance::{
    Scenario, Setup,
    panel_check::{Panel, text},
    panel_visual as visual, real_limit,
    setup::TestRoot,
};
use std::time::Duration;

#[tokio::test]
async fn the_sideways_scroll_rule_rejects_a_page_wider_than_the_screen() {
    let root = TestRoot::new();
    let page = "<!doctype html><html><body style='margin:0'><div style='width:320px'>Lockout</div></body></html>";
    let audit = visual::render(page, &root.path, false).await.unwrap();
    visual::no_sideways_scroll(&audit).unwrap();
    let wrong = page.replace("320px", "344px");
    let audit = visual::render(&wrong, &root.path, false).await.unwrap();
    let error = visual::no_sideways_scroll(&audit).expect_err("a wide page must fail");
    assert!(error.contains("344 px"), "{error}");
    root.cleanup();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_lockout_page_does_not_scroll_sideways_on_a_narrow_screen() {
    tokio::time::timeout(real_limit(Duration::from_secs(60)), async {
        let mut scenario = Scenario::default();
        scenario.plan(Setup::normal());
        scenario.start().await;
        scenario.stop().await;
        std::fs::write(
            scenario.root.path.join("data/state.json"),
            r#"{"format":1,"lockout":{"since":"2026-10-18T00:00:00Z","signal":"403 PERMISSION_DENIED","original_error_text":"Synthetic account suspension"},"checked_builds":[]}"#,
        )
        .unwrap();
        scenario.restart().await;
        let mut panel = Panel::default();
        panel.scenario(&mut scenario).await;
        for (language, entry) in [
            ("en", "End the lockout with the management password"),
            ("zh-CN", "输入管理密码，结束锁死"),
        ] {
            panel.owner(language).language = language.into();
            let html = text(&panel.view_session(language).await);
            assert!(html.contains(entry), "{language} lockout page");
            let audit = visual::render(&html, &scenario.root.path, false)
                .await
                .expect("Chrome renders the lockout page");
            visual::no_sideways_scroll(&audit).unwrap();
        }
        scenario.finish(true).await;
    })
    .await
    .expect("narrow lockout cap: 60 seconds, two panel views, two Chrome runs");
}

#[tokio::test]
async fn first_tile_rules_read_the_rendered_surface_and_type() {
    let root = TestRoot::new();
    let page = "<!doctype html><html style='background:white'><body><section style='background:#272729'><h1 style='font-size:56px'>Lockout</h1></section></body></html>";
    let audit = visual::render(page, &root.path, false).await.unwrap();
    visual::first_tile_presentation(&audit, "dark").unwrap();
    for (wrong, shade) in [
        (page.replace("#272729", "white"), "dark"),
        (page.to_owned(), "light"),
        (page.replace("56px", "17px"), "dark"),
        (
            page.replace("background:#272729", "display:none;background:#272729"),
            "dark",
        ),
        (
            page.replace(
                "background:#272729",
                "background:linear-gradient(black,black)",
            ),
            "light",
        ),
    ] {
        let audit = visual::render(&wrong, &root.path, false).await.unwrap();
        let error = visual::first_tile_presentation(&audit, shade)
            .expect_err("wrong rendered tile must fail");
        assert!(error.contains("FR-054 first tile"), "{error}");
        eprintln!("Expected wrong-page failure: {error}");
    }
    let light = page.replace("background:#272729", "background:transparent");
    let audit = visual::render(&light, &root.path, false).await.unwrap();
    visual::first_tile_presentation(&audit, "light").unwrap();
    root.cleanup();
}

#[tokio::test]
async fn variant_a_bars_need_black_and_frosted_rendered_surfaces() {
    let root = TestRoot::new();
    let page = "<!doctype html><html><body><header style='background:black'>acp2api</header><nav style='backdrop-filter:blur(20px)'>Sections</nav><section>Running</section></body></html>";
    let audit = visual::render(page, &root.path, false).await.unwrap();
    visual::variant_a_bars(&audit).unwrap();
    for wrong in [
        page.replace("background:black", "background:white"),
        page.replace("backdrop-filter:blur(20px)", "backdrop-filter:none"),
        page.replace("backdrop-filter:blur(20px)", "backdrop-filter:blur(0px)"),
        page.replace(
            "backdrop-filter:blur(20px)",
            "display:none;backdrop-filter:blur(20px)",
        ),
    ] {
        let audit = visual::render(&wrong, &root.path, false).await.unwrap();
        let error = visual::variant_a_bars(&audit).expect_err("wrong rendered bar must fail");
        assert!(error.contains("FR-054"), "{error}");
        eprintln!("Expected wrong-page failure: {error}");
    }
    root.cleanup();
}
