use acp2api_acceptance::{panel_visual as visual, setup::TestRoot};

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
