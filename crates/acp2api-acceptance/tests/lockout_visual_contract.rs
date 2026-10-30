use acp2api_acceptance::{panel_visual as visual, setup::TestRoot};

#[tokio::test]
async fn the_lockout_tile_audit_rejects_light_hidden_and_later_dark_tiles() {
    let root = TestRoot::new();
    let good = "<!doctype html><html><body><section style='background:#272729;color:white'><h1>Lockout</h1></section><section>Quota</section></body></html>";
    let audit = visual::render(good, &root.path, false).await.unwrap();
    visual::dark_first_tile(&audit).unwrap();
    for wrong in [
        good.replace("background:#272729", "background:white"),
        good.replace("background:#272729", "display:none;background:#272729"),
        good.replace("<body>", "<body><section>Running</section>"),
    ] {
        let audit = visual::render(&wrong, &root.path, false).await.unwrap();
        let error = visual::dark_first_tile(&audit).expect_err("wrong rendered tile must fail");
        assert!(error.contains("FR-035 first tile must render"), "{error}");
        eprintln!("Expected wrong-page failure: {error}");
    }
    root.cleanup();
}
