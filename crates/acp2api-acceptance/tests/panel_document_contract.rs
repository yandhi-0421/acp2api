#[path = "support/panel_document.rs"]
mod document;

#[test]
fn translation_data_does_not_count_as_served_state_text() {
    let page = acp2api_acceptance::client::Answer {
        status: 200,
        headers: Default::default(),
        body: br#"<html><head><style>.Running {color:blue}</style></head><body><script>const states = ['Running'];</script><template>Running</template><h1>Quota wait</h1></body></html>"#.to_vec(),
        events: Vec::new(),
    };
    let html = document::html(&page);
    assert_eq!(document::visible(&html), "Quota wait");
}

#[test]
fn semantic_form_fields_do_not_depend_on_wrappers_or_attribute_quotes() {
    let html = "<FORM method=post action='/manage/example' data-label=\"x > y\"><div><SELECT name=choice><OPTION value=zh>中文</OPTION></SELECT></div><INPUT disabled name=nonce value='synthetic'></FORM>";
    let form = document::elements(html, "form").next().unwrap();
    assert_eq!(
        document::attribute(form.tag, "action"),
        Some("/manage/example")
    );
    assert_eq!(document::attribute(form.tag, "method"), Some("post"));
    let select = document::elements(form.content, "select").next().unwrap();
    assert_eq!(document::attribute(select.tag, "name"), Some("choice"));
    let option = document::elements(select.content, "option").next().unwrap();
    assert_eq!(document::visible(option.content), "中文");
    assert_eq!(document::attribute(option.tag, "value"), Some("zh"));
    let input = document::elements(form.content, "input").next().unwrap();
    assert_eq!(document::attribute(input.tag, "name"), Some("nonce"));
    assert_eq!(document::attribute(input.tag, "value"), Some("synthetic"));
}

#[test]
fn external_resource_scan_checks_addresses_in_html_css_and_scripts() {
    let local = "<form action='/manage/example'></form><pre>http://127.0.0.1:12345/v1 &quot;http://127.0.0.1:12345&quot;</pre>";
    assert!(document::external_addresses(local, "127.0.0.1:12345").is_empty());
    for resource in [
        "<link rel=stylesheet href='https://example.invalid/style.css'>",
        "<img src=//example.invalid/image.png>",
        "<style>@font-face {src:url(https://example.invalid/font.woff2)}</style>",
        "<script>fetch('https://example.invalid/data')</script>",
        "<link href='http://127.0.0.1:12346/style.css'>",
        "<link href='https://127.0.0.1:12345/style.css'>",
    ] {
        assert_eq!(
            document::external_addresses(resource, "127.0.0.1:12345").len(),
            1,
            "external address in {resource}"
        );
    }
}
