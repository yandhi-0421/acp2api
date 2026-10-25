use crate::World;
use acp2api_acceptance::{
    client::ApiCall,
    panel_check::{self as check, text},
};
use cucumber::{given, then, when};
use serde_json::json;

async fn prepare(w: &mut World) {
    if w.startup.root.is_some() {
        w.panel.startup(&mut w.startup).await;
    } else {
        w.panel.scenario(&mut w.scenario).await;
    }
}
#[given("the owner has a panel session")]
#[given("the owner opened a panel session")]
async fn session(w: &mut World) {
    prepare(w).await;
    w.panel.login_initial("B1").await;
    w.panel.answer = Some(
        w.panel
            .submit(
                "B1",
                "/manage/concurrency",
                json!({"concurrency_limit":"2"}),
            )
            .await,
    );
}
#[given(
    expr = "the owner has a panel session in the browser {string} and a panel session in the browser {string}"
)]
async fn sessions(w: &mut World, a: String, b: String) {
    prepare(w).await;
    w.panel.login_initial(&a).await;
    prepare(w).await;
    w.panel.login_initial(&b).await;
    for name in [&a, &b] {
        let a = w
            .panel
            .submit(
                name,
                "/manage/concurrency",
                json!({"concurrency_limit":"2"}),
            )
            .await;
        w.panel.answer = Some(a);
    }
}
#[given("a browser with no panel session")]
#[given("a request with no panel session")]
async fn anonymous(w: &mut World) {
    prepare(w).await;
}
#[given(expr = "acp2api runs with the concurrency limit {int}")]
async fn limit(w: &mut World, n: i64) {
    w.startup.initialize().await;
    w.startup
        .set("concurrency_limit", Some(toml::Value::Integer(n)));
}
#[when("the browser opens the management panel")]
async fn open(w: &mut World) {
    prepare(w).await;
    w.panel.answer = Some(w.panel.page("anonymous").await);
}
#[then("the page holds the entry for the management password")]
fn entry(w: &mut World) {
    let a = w.panel.answer.as_ref().unwrap();
    assert_eq!(a.status, 200, "FR-009 unauthenticated panel page");
    assert!(
        text(a).contains("autocomplete=\"current-password\""),
        "FR-009 password entry must be visible"
    );
}
#[then("the page holds no local API key and no state of acp2api")]
fn private(w: &mut World) {
    let html = text(w.panel.answer.as_ref().unwrap());
    assert!(
        !html.contains(&w.panel.old_key)
            && !html.contains("id=\"local-api-key\"")
            && !html.contains("Running")
            && !html.contains("Runtime unavailable"),
        "FR-009 anonymous page must reveal no key or state"
    );
}
#[when("the request asks a management endpoint for a new local API key")]
async fn unauthed_key(w: &mut World) {
    w.scenario.answer = Some(w.panel.submit("anonymous", "/manage/key", json!({})).await);
}
#[then(expr = "the local API key is still {string}")]
async fn unchanged_key(w: &mut World, alias: String) {
    assert_eq!(alias, "K1");
    assert_eq!(
        w.scenario.answer.as_ref().unwrap().status,
        401,
        "FR-009 actual management action rejects anonymous session rather than missing route"
    );
    let key = w.panel.old_key.clone();
    let a = w.panel.api(&key).await;
    assert_eq!(
        a.status, 200,
        "FR-009 refused management action must leave old API key usable"
    );
}
#[when("the owner enters the correct management password")]
async fn correct(w: &mut World) {
    prepare(w).await;
    w.panel.login_initial("B1").await;
}
#[when("the owner enters a wrong management password")]
async fn wrong(w: &mut World) {
    prepare(w).await;
    let wrong = acp2api_acceptance::setup::random_hex(20);
    w.panel.login("wrong", &wrong).await;
}
#[then(expr = "acp2api gives a panel session cookie with the attributes {string} and {string}")]
fn cookie(w: &mut World, first: String, second: String) {
    let a = w.panel.login_answer.as_ref().unwrap();
    assert_eq!(
        a.status, 303,
        "FR-009 correct password must redirect after form login"
    );
    let cookie = a.headers.get("set-cookie");
    assert!(
        cookie.is_some(),
        "FR-009 correct password must set a panel session cookie"
    );
    for attribute in [first, second, "Path=/".into()] {
        assert!(
            cookie.unwrap().split(';').any(|s| s.trim() == attribute),
            "FR-009 required session cookie attribute missing"
        );
    }
}
#[then("acp2api gives no panel session cookie")]
fn no_cookie(w: &mut World) {
    assert_eq!(
        w.panel.login_answer.as_ref().unwrap().status,
        401,
        "FR-009 wrong management password is refused by the actual login endpoint"
    );
    assert!(
        !w.panel
            .login_answer
            .as_ref()
            .unwrap()
            .headers
            .contains_key("set-cookie"),
        "FR-009 wrong password must not create a panel session"
    );
}
#[when(regex = r"^(11 hours 59 minutes|12 hours) pass with no restart of acp2api$")]
async fn advance_age(w: &mut World, age: String) {
    let target = if age == "12 hours" {
        43_200_000
    } else {
        43_140_000
    };
    let mut time = w.panel.time();
    time.advance_to(target).await;
}
#[then(regex = r"^the panel session is (still valid|no longer valid)$")]
async fn validity(w: &mut World, state: String) {
    w.panel.valid("B1", state == "still valid").await;
}
#[then(expr = "the panel session in the browser {string} is no longer valid")]
async fn invalid(w: &mut World, browser: String) {
    w.panel.valid(&browser, false).await;
}
#[then("the panel session of the owner is still valid")]
async fn still_valid(w: &mut World) {
    w.panel.valid("B1", true).await;
    let page = w.panel.page("B1").await;
    assert!(
        text(&page).contains("value=\"3\" selected"),
        "FR-004 changed concurrency limit is live without restart"
    );
    restart_host(w).await;
    w.panel.login_initial("after-limit-restart").await;
    let page = w.panel.page("after-limit-restart").await;
    assert!(
        text(&page).contains("value=\"3\" selected"),
        "FR-004 changed concurrency limit survives restart"
    );
}
async fn restart_host(w: &mut World) {
    if w.startup.root.is_some() {
        w.startup.stop().await;
        w.startup.launch_data(vec![]).await;
    } else {
        w.scenario.restart().await;
    }
}
#[when(expr = "the owner changes the management password in the browser {string}")]
async fn password(w: &mut World, browser: String) {
    w.panel.change_password(&browser).await;
}
#[when(expr = "the owner changes the management password to {string}")]
async fn set_password(w: &mut World, alias: String) {
    assert_eq!(alias, "P2");
    prepare(w).await;
    w.panel.login_initial("B1").await;
    w.panel.change_password("B1").await;
}
#[then(expr = "the configuration file does not hold {string}")]
fn no_plaintext(w: &mut World, alias: String) {
    assert_eq!(alias, "P2");
    assert!(
        !toml::to_string(&w.panel.saved())
            .unwrap()
            .contains(w.panel.new_password.as_ref().unwrap()),
        "FR-009 configured management password is hash-only"
    );
}
#[then("the configuration file holds a hash of the management password")]
fn hash(w: &mut World) {
    w.panel.saved_hash(w.panel.new_password.as_ref().unwrap());
}
#[then(expr = "{string} opens the management panel")]
async fn password_opens(w: &mut World, alias: String) {
    assert_eq!(alias, "P2");
    let p = w.panel.new_password.clone().unwrap();
    w.panel.login("changed", &p).await;
    w.panel.valid("changed", true).await;
    restart_host(w).await;
    w.panel.login("after-restart", &p).await;
    w.panel.valid("after-restart", true).await;
}
#[when(expr = "the owner sets the concurrency limit {int} in the management panel")]
async fn change_limit(w: &mut World, n: u8) {
    w.panel.answer = Some(
        w.panel
            .submit(
                "B1",
                "/manage/concurrency",
                json!({"concurrency_limit":n.to_string()}),
            )
            .await,
    );
}
#[when("the owner makes a new local API key in the management panel")]
async fn rotate(w: &mut World) {
    prepare(w).await;
    w.panel.login_initial("B1").await;
    w.panel.answer = Some(w.panel.submit("B1", "/manage/key", json!({})).await);
    w.panel.new_key = Some(w.panel.saved()["local_api_key"].as_str().unwrap().into());
}
#[then(expr = "a request with the local API key {string} gets HTTP 401")]
async fn old_key(w: &mut World, alias: String) {
    assert_eq!(alias, "K1");
    let key = w.panel.old_key.clone();
    assert_eq!(
        w.panel.api(&key).await.status,
        401,
        "FR-007 old key must stop at once"
    );
}
#[then("a request with the new local API key gets HTTP 200")]
async fn new_key(w: &mut World) {
    let key = w.panel.new_key.clone().unwrap();
    assert_eq!(
        w.panel.api(&key).await.status,
        200,
        "FR-007 new key must work immediately"
    );
    restart_host(w).await;
    assert_eq!(
        w.panel.api(&key).await.status,
        200,
        "FR-004 rotated key must survive restart"
    );
    let old = w.panel.old_key.clone();
    assert_eq!(
        w.panel.api(&old).await.status,
        401,
        "FR-007 old key stays invalid after restart"
    );
}
#[when("a request with the Origin of the management panel goes to a management endpoint")]
async fn origin(w: &mut World) {
    prepare(w).await;
    let address = w.panel.address.clone();
    let owner = w.panel.owner("B1");
    let cookie = owner
        .cookies
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    let call = owner.client.send(ApiCall {
        method: "POST".into(),
        path: "/manage/concurrency".into(),
        body: json!({"concurrency_limit":"3"}),
        headers: vec![
            ("cookie".into(), cookie),
            ("origin".into(), format!("http://{address}")),
            (
                "content-type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
        ],
    });
    w.panel.answer = Some(call.finish(&mut w.panel.time()).await);
}
#[then("acp2api answers with an HTTP status from 200 to 299")]
fn accepted(w: &mut World) {
    assert!(
        (200..300).contains(&w.panel.answer.as_ref().unwrap().status),
        "FR-008 same-origin management request must be accepted"
    );
    assert_eq!(
        w.panel.saved()["concurrency_limit"].as_integer(),
        Some(3),
        "FR-008 accepted same-origin form must change the actual setting"
    );
}
#[given(expr = "acp2api started one time before with the data folder {string}")]
async fn previous(w: &mut World, name: String) {
    w.startup.initialize().await;
    w.startup.data = w.startup.root().path.join(name);
    std::fs::create_dir_all(&w.startup.data).unwrap();
    w.startup.launch_data(vec![]).await;
    w.startup.stop().await;
}
#[when(expr = "the owner starts acp2api with the data folder {string} and the port {int}")]
async fn address_start(w: &mut World, name: String, port: u16) {
    assert_eq!(w.startup.data, w.startup.root().path.join(name));
    w.startup
        .launch_data(vec!["--port".into(), port.to_string()])
        .await;
}
#[then(expr = "the terminal shows the address of the management panel on the port {int}")]
fn terminal_address(w: &mut World, port: u16) {
    assert!(
        w.startup
            .terminal()
            .contains(&format!("management panel at http://127.0.0.1:{port}/")),
        "FR-002 every start must show panel address"
    );
}
#[then("the browser can open the management panel at that address")]
async fn page_at_address(w: &mut World) {
    prepare(w).await;
    let a = w.panel.page("address").await;
    assert_eq!(
        a.status, 200,
        "FR-002 published panel address must serve the real page"
    );
}
#[then(expr = "acp2api makes the configuration file in {string}")]
fn made(w: &mut World, name: String) {
    assert!(
        w.startup
            .root()
            .path
            .join(name)
            .join("config.toml")
            .is_file()
    );
}
#[then(
    "the terminal shows the address of the management panel, the local API key, and the management password"
)]
fn credentials(w: &mut World) {
    assert!(
        w.startup
            .terminal()
            .contains("management panel at http://127.0.0.1:")
    );
    assert!(
        !check::generated(&w.startup, "local API key").is_empty(),
        "FR-005 generated key delivery"
    );
    assert!(
        !check::generated(&w.startup, "management password").is_empty(),
        "FR-005 generated password delivery"
    );
}
#[then("a request with the local API key from the terminal gets no HTTP 401")]
#[then("a request with that new key gets no HTTP 401")]
async fn generated_key(w: &mut World) {
    prepare(w).await;
    let key = check::generated(&w.startup, "local API key");
    assert_ne!(
        w.panel.api(&key).await.status,
        401,
        "FR-004 generated delivered key must authenticate actual API"
    );
}
#[then("the management password from the terminal opens the management panel")]
#[then("the new management password opens the management panel")]
async fn generated_password(w: &mut World) {
    prepare(w).await;
    let password = check::generated(&w.startup, "management password");
    w.panel.login("generated", &password).await;
    w.panel.valid("generated", true).await;
}
#[then("the configuration file holds only the hash of that management password")]
#[then("the configuration file holds only the hash of the new management password")]
fn generated_hash(w: &mut World) {
    check::saved_hash_path(
        &w.startup.config_path(),
        &check::generated(&w.startup, "management password"),
    );
}
#[given(expr = "the owner removed the local API key {string} from the configuration file")]
async fn remove_key(w: &mut World, alias: String) {
    assert_eq!(alias, "K1");
    w.startup.set("local_api_key", None);
}
#[given(expr = "the configuration file still has the hash of the management password {string}")]
fn still_hash(w: &mut World, alias: String) {
    assert_eq!(alias, "P1");
    check::saved_hash_seed(&w.startup);
}
#[given("the owner removed the field of the management password from the configuration file")]
async fn remove_password(w: &mut World) {
    w.startup.set("management_password_hash", None);
}
#[then("the terminal shows a new random local API key")]
fn new_random_key(w: &mut World) {
    let key = check::generated(&w.startup, "local API key");
    assert!(
        key.starts_with("acp2api-") && key.len() == 51 && key != w.startup.root().key,
        "FR-004 absent key must generate a new random key (withheld)"
    );
}
#[then("the configuration file holds that new key")]
fn saved_key(w: &mut World) {
    assert!(
        w.startup.saved().unwrap()["local_api_key"].as_str()
            == Some(&check::generated(&w.startup, "local API key")),
        "FR-004 generated key must be saved (withheld)"
    );
}
#[then(expr = "the terminal does not show {string}")]
fn no_password_repeat(w: &mut World, alias: String) {
    assert_eq!(alias, "P1");
    assert!(
        !w.startup.terminal().contains(&w.startup.root().password)
            && check::generated(&w.startup, "management password").is_empty(),
        "FR-004 valid stored password must not be delivered again"
    );
}
#[then("the terminal shows a new random management password")]
fn new_random_password(w: &mut World) {
    let password = check::generated(&w.startup, "management password");
    assert!(
        password.len() == 23 && password != w.startup.root().password,
        "FR-004 absent password must generate a new password (withheld)"
    );
    assert!(
        check::generated(&w.startup, "local API key").is_empty(),
        "FR-004 valid stored key must not be delivered again"
    );
}
#[then("the earlier management password does not open the management panel")]
async fn old_password(w: &mut World) {
    let password = w.startup.root().password.clone();
    w.panel.login("old", &password).await;
    assert!(
        !w.panel
            .login_answer
            .as_ref()
            .unwrap()
            .headers
            .contains_key("set-cookie"),
        "FR-009 earlier password must not open panel"
    );
}
