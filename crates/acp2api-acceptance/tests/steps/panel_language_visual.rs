use crate::{World, panel_document as document, places_support as queue};
use acp2api_acceptance::client::ApiCall;
use cucumber::{given, then, when};
use serde_json::{Map, Value};

#[given(expr = "the browser language is {string}")]
fn browser_language(w: &mut World, language: String) {
    let root = &w.scenario.root;
    w.panel.bind(
        root,
        &root.path.join("data"),
        w.scenario.hub.clone(),
        format!("127.0.0.1:{}", root.port),
    );
    w.panel.owner("B1").language = language;
}

#[given("the owner opened the management panel")]
async fn opened(w: &mut World) {
    w.panel.answer = Some(queue::view(w).await);
}

#[then(regex = r#"^the first tile shows "(运行中|Running)"$"#)]
fn state(w: &mut World, expected: String) {
    let html = document::html(w.panel.answer.as_ref().expect("the panel was opened"));
    let text = document::visible(&html);
    assert!(
        text.split_whitespace().any(|word| word == expected),
        "FR-055 served state text must be {expected:?} for browser language {:?}",
        w.panel.owner("B1").language
    );
}

#[when("the owner selects Chinese with the language switch")]
async fn switch(w: &mut World) -> Result<(), String> {
    let html = document::html(w.panel.answer.as_ref().expect("the panel was opened"));
    let chinese = |label: &str| {
        matches!(
            document::visible(label).as_str(),
            "中文" | "Chinese" | "简体中文"
        )
    };
    let selection = document::elements(&html, "form").find_map(|form| {
        let mut fields = Map::new();
        for input in document::elements(form.content, "input") {
            if let Some(name) = document::attribute(input.tag, "name") {
                fields.insert(
                    name.into(),
                    document::attribute(input.tag, "value").unwrap_or("").into(),
                );
            }
        }
        let mut chosen = false;
        for select in document::elements(form.content, "select") {
            if let Some(option) =
                document::elements(select.content, "option").find(|option| chinese(option.content))
            {
                let name = document::attribute(select.tag, "name")?;
                let value = document::attribute(option.tag, "value")?;
                fields.insert(name.into(), value.into());
                chosen = true;
            }
        }
        for button in document::elements(form.content, "button") {
            if chinese(button.content) {
                if let Some(name) = document::attribute(button.tag, "name") {
                    fields.insert(
                        name.into(),
                        document::attribute(button.tag, "value")
                            .unwrap_or("")
                            .into(),
                    );
                }
                chosen = true;
            }
        }
        chosen.then(|| {
            (
                document::attribute(form.tag, "action")
                    .unwrap_or("/")
                    .to_owned(),
                document::attribute(form.tag, "method")
                    .unwrap_or("get")
                    .to_ascii_lowercase(),
                Value::Object(fields),
            )
        })
    });
    assert!(
        selection.is_some(),
        "FR-055 the served page must offer a Chinese language switch"
    );
    let (action, method, fields) = selection.unwrap();
    if method != "post" {
        return browser_required("the language switch is not a POST form supported by Owner");
    }
    assert!(
        action.starts_with('/') && !action.starts_with("//"),
        "FR-055 the language switch stays on acp2api"
    );
    let mut answer = w.panel.submit("B1", &action, fields).await;
    for _ in 0..4 {
        w.panel.owner("B1").remember(&answer);
        assert!(
            (200..400).contains(&answer.status),
            "FR-055 selecting Chinese succeeds"
        );
        if answer.status < 300 {
            w.panel.answer = Some(answer);
            return Ok(());
        }
        if !matches!(answer.status, 301..=303) {
            return browser_required(
                "the switch uses a redirect that needs browser method preservation",
            );
        }
        let location = answer
            .headers
            .get("location")
            .expect("the redirect has a destination");
        let origin = format!("http://{}", w.panel.address);
        let path = location
            .strip_prefix(&origin)
            .unwrap_or(location)
            .split('#')
            .next()
            .unwrap();
        assert!(
            path.starts_with('/') && !path.starts_with("//"),
            "FR-055 the switch redirect stays on acp2api"
        );
        let owner = w.panel.owner("B1");
        let cookie = owner
            .cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        let call = owner.client.send(ApiCall {
            method: "GET".into(),
            path: path.into(),
            body: Value::Null,
            headers: vec![
                ("cookie".into(), cookie),
                ("accept-language".into(), owner.language.clone()),
            ],
        });
        answer = call.finish_current(&mut w.panel.time()).await;
    }
    browser_required("the switch exceeded its four-response redirect cap")
}

#[then(expr = "the sections are {string}")]
fn sections(w: &mut World, expected: String) {
    let html = document::html(w.panel.answer.as_ref().expect("the panel was opened"));
    let text = document::visible(&html);
    for label in expected.split('、') {
        assert!(
            text.split_whitespace().any(|actual| actual == label),
            "FR-055 missing Chinese section label: {label}"
        );
    }
}

#[when(expr = "the owner opens the management panel on the port {int}")]
async fn open_port(w: &mut World, port: u16) {
    assert_eq!(
        port, 8329,
        "the design maps this port to the scenario's leased loopback port"
    );
    w.panel.answer = Some(queue::view(w).await);
}

#[then(expr = "each request of the browser goes to {string}")]
fn local(w: &mut World, address: String) {
    assert_eq!(address, "127.0.0.1:8329");
    let html = document::html(w.panel.answer.as_ref().expect("the panel was opened"));
    assert!(
        html.to_ascii_lowercase().contains("<html"),
        "inspect a served HTML document"
    );
    assert!(
        document::external_addresses(&html, &w.panel.address).is_empty(),
        "FR-055 the served document must contain no external address"
    );
}

#[then("the page loads no font from an external address")]
fn no_external_font(w: &mut World) {
    let html = document::html(w.panel.answer.as_ref().expect("the panel was opened"));
    assert!(
        document::external_addresses(&html, &w.panel.address).is_empty(),
        "FR-055 the served document must contain no external font address"
    );
}

#[then(expr = "each control of the page has the color {string}")]
async fn control_color(w: &mut World, color: String) {
    assert_eq!(color, "#0066cc");
    acp2api_acceptance::panel_visual::controls(rendered(w).await).unwrap();
}

#[then("no element of the page has a shadow")]
async fn no_shadow(w: &mut World) {
    acp2api_acceptance::panel_visual::shadows(rendered(w).await).unwrap();
}

#[then("the page uses only the system fonts")]
async fn system_fonts(w: &mut World) {
    acp2api_acceptance::panel_visual::fonts(rendered(w).await).unwrap();
}

#[then(
    expr = "each element with one of the status colors {string}, {string}, {string}, and {string} is a dot of 9 px with text next to it"
)]
async fn status_dots(
    w: &mut World,
    good: String,
    warning: String,
    serious: String,
    critical: String,
) {
    assert_eq!(
        [
            good.as_str(),
            warning.as_str(),
            serious.as_str(),
            critical.as_str()
        ],
        ["#0ca30c", "#fab219", "#ec835a", "#d03b3b"]
    );
    acp2api_acceptance::panel_visual::status(rendered(w).await).unwrap();
}

#[then("no control and no background has a status color")]
async fn no_status_background(w: &mut World) {
    acp2api_acceptance::panel_visual::status(rendered(w).await).unwrap();
}

#[given("the browser prefers the dark color scheme")]
fn dark(w: &mut World) {
    w.panel_dark = true;
}

#[then("the page has the same colors as with the light color scheme")]
async fn light(w: &mut World) {
    assert!(
        w.panel_dark,
        "the Given requests the dark browser preference"
    );
    let html = document::html(w.panel.answer.as_ref().expect("the panel was opened"));
    let light = acp2api_acceptance::panel_visual::render(&html, &w.scenario.root.path, false)
        .await
        .expect("Chrome renders the light preference");
    let dark = rendered(w).await;
    acp2api_acceptance::panel_visual::light_only(&light, dark).unwrap();
}

async fn rendered(w: &mut World) -> &acp2api_acceptance::panel_visual::Audit {
    if w.panel_rendered.is_none() {
        let html = document::html(w.panel.answer.as_ref().expect("the panel was opened"));
        w.panel_rendered = Some(
            acp2api_acceptance::panel_visual::render(&html, &w.scenario.root.path, w.panel_dark)
                .await
                .expect("Chrome renders the authenticated Owner document"),
        );
    }
    w.panel_rendered.as_ref().unwrap()
}

fn browser_required(rule: &str) -> Result<(), String> {
    Err(format!(
        "Owner form adapter cannot execute this interaction: {rule}"
    ))
}
