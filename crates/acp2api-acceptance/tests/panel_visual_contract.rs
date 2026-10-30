use acp2api_acceptance::{panel_visual as visual, setup::TestRoot};

const GOOD: &str = r##"<!doctype html><html><head><meta charset="utf-8"><style>
:root {color-scheme:light;font-family:system-ui;color:#1d1d1f;background:#f5f5f7}
button,[role=button] {background:#0066cc;color:white;border:0}
a,summary {color:#0066cc}
input,select {border:1px solid #86868b;color:#1d1d1f;background:white;border-radius:999px;padding:10px}
input:focus-visible,select:focus-visible {outline:3px solid #0066cc}
.dot {display:inline-block;width:9px;height:9px;border-radius:50%;background:#0ca30c}
.unused {box-shadow:0 0 3px red}
</style></head><body><button>Save</button><a href="#section">Section</a><details><summary>Details</summary></details><input aria-label="Name"><select aria-label="Choice"><option>One</option></select><div role="button" tabindex="0">Action</div><p><span class="dot"></span>Running</p></body></html>"##;

#[tokio::test]
async fn actual_rendered_facts_reject_each_deliberately_wrong_page() {
    let root = TestRoot::new();
    let good = visual::render(GOOD, &root.path, false)
        .await
        .expect("Chrome renders the good control page");
    visual::controls(&good).unwrap();
    visual::shadows(&good).unwrap();
    visual::fonts(&good).unwrap();
    visual::status(&good).unwrap();
    let dark = visual::render(GOOD, &root.path, true)
        .await
        .expect("Chrome applies dark preference");
    visual::light_only(&good, &dark).unwrap();

    let cases = [
        (
            "button {background:red}",
            "control",
            visual::controls as fn(&visual::Audit) -> Result<(), String>,
        ),
        (
            "button::before {content:'x';box-shadow:0 0 2px black}",
            "shadow",
            visual::shadows,
        ),
        ("p {background:#fab219}", "status color", visual::status),
        (
            ".dot {display:none}",
            "at least one status-colored 9 px dot",
            visual::status,
        ),
        (
            "@font-face {font-family:AuditWebFont;src:local('Arial')} p {font-family:AuditWebFont}",
            "web font",
            visual::fonts,
        ),
        (
            "input:focus-visible {outline-color:red}",
            "control",
            visual::controls,
        ),
    ];
    for (css, failure, check) in cases {
        let wrong = GOOD.replace("</style>", &format!("{css}</style>"));
        let audit = visual::render(&wrong, &root.path, false)
            .await
            .expect("Chrome renders the wrong page before its assertion");
        let error = check(&audit).expect_err("the rendered defect must fail the Rust rule");
        assert!(error.contains(failure), "wrong failure: {error}");
        eprintln!("Expected wrong-page failure: {error}");
    }
    let wrong = GOOD.replace(
        "</style>",
        "@media(prefers-color-scheme:dark){body{color:red}}</style>",
    );
    let dark = visual::render(&wrong, &root.path, true)
        .await
        .expect("Chrome exercises the deliberately wrong dark rule");
    let error =
        visual::light_only(&good, &dark).expect_err("the dark rule changes actual computed colors");
    assert!(
        error.contains("dark preference changed"),
        "wrong failure: {error}"
    );
    eprintln!("Expected wrong-page failure: {error}");
    root.cleanup();
}

#[tokio::test]
async fn file_audit_rejects_resource_elements_instead_of_ignoring_them() {
    let root = TestRoot::new();
    for resource in [
        "<link rel=stylesheet href=missing.css>",
        "<script src=missing.js></script>",
        "<style>@import url(missing.css);</style>",
    ] {
        let page = GOOD.replace("</head>", &format!("{resource}</head>"));
        let error = visual::render(&page, &root.path, false)
            .await
            .expect_err("file audit cannot prove a page with external files");
        assert!(
            error.contains("<link> or <script src>"),
            "wrong failure: {error}"
        );
    }
    root.cleanup();
}
