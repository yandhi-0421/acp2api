use crate::World;
use acp2api_acceptance::access_check::{self as check, Credential};
use cucumber::{given, then, when};
#[derive(Default, Debug)]
pub struct AccessState {
    endpoint: String,
    address: Option<std::net::IpAddr>,
    connected: Option<bool>,
}
#[given(expr = "acp2api is in the normal state on the port {int}")]
fn normal(w: &mut World, port: u16) {
    assert_eq!(port, 8329);
    // The scenario port is an alias. TestRoot selects a fresh loopback port.
    w.scenario.plan(acp2api_acceptance::Setup::normal());
}
#[given(expr = "the local API key is {string}")]
fn key(_w: &mut World, alias: String) {
    assert_eq!(alias, "K1");
}
async fn send(w: &mut World, endpoint: String, key: Credential, headers: Vec<(String, String)>) {
    w.access.endpoint = endpoint.clone();
    w.scenario.answer = Some(check::send(&mut w.scenario, &endpoint, key, headers, false).await);
}
#[when(
    regex = r#"^an API client sends "([^"]+)" with (no local API key|the wrong local API key "K9")$"#
)]
async fn missing(w: &mut World, endpoint: String, key: String) {
    send(
        w,
        endpoint,
        if key == "no local API key" {
            Credential::Absent
        } else {
            Credential::Wrong
        },
        vec![],
    )
    .await;
}
#[when(
    expr = "an API client sends {string} with the local API key {string} in the header {string}"
)]
async fn header_key(w: &mut World, endpoint: String, alias: String, header: String) {
    assert_eq!(alias, "K1");
    send(
        w,
        endpoint,
        match header.as_str() {
            "Authorization: Bearer" => Credential::Bearer,
            "x-api-key" => Credential::Key,
            _ => panic!("key header"),
        },
        vec![],
    )
    .await;
}
#[when(expr = "an API client sends {string} with the header {string}")]
async fn custom_header(w: &mut World, endpoint: String, header: String) {
    let (name, value) = header.split_once(": ").unwrap();
    let value = value
        .replace(":8329", &format!(":{}", w.scenario.root.port))
        .replace(
            ":8330",
            &format!(":{}", check::other_port(w.scenario.root.port)),
        );
    send(w, endpoint, Credential::Key, vec![(name.into(), value)]).await;
}
#[when(expr = "a request with the Origin of the management panel goes to {string}")]
async fn panel(w: &mut World, endpoint: String) {
    send(
        w,
        endpoint,
        Credential::Key,
        vec![(
            "Origin".into(),
            format!("http://127.0.0.1:{}", w.scenario.root.port),
        )],
    )
    .await;
}
#[when(expr = "a program sends {string} with the headers {string} and {string}")]
async fn options(w: &mut World, endpoint: String, first: String, last: String) {
    let headers = [first, last]
        .into_iter()
        .map(|h| {
            let (n, v) = h.split_once(": ").unwrap();
            (n.into(), v.into())
        })
        .collect();
    send(w, endpoint, Credential::Absent, headers).await;
}
#[then(expr = "acp2api refuses the request with HTTP 401 and the message {string}")]
async fn refused(w: &mut World, message: String) {
    assert_eq!(message, check::KEY_MESSAGE);
    check::key_refusal(w.scenario.answer.as_ref().unwrap(), &w.access.endpoint);
    check::journal(&w.scenario, w.scenario.answer.as_ref().unwrap(), 1).await;
}
#[then("acp2api answers with an HTTP status from 400 to 499")]
fn rejected(w: &mut World) {
    let answer = w.scenario.answer.as_ref().unwrap();
    assert!(
        (400..500).contains(&answer.status),
        "FR-008 browser/Host refusal: {}",
        answer.status
    );
    check::no_cors(answer);
}
#[then("no prompt turn starts")]
fn no_prompt(w: &mut World) {
    assert_eq!(
        check::prompt_count(&w.scenario),
        0,
        "no ACP prompt for a guard refusal"
    );
}
#[then("the answer has no header whose name starts with \"Access-Control-\"")]
fn no_cors(w: &mut World) {
    check::no_cors(w.scenario.answer.as_ref().unwrap());
}
#[given("the computer has a network address other than 127.0.0.1")]
fn network(w: &mut World) {
    w.access.address = check::network_address();
    assert!(
        w.access.address.is_some(),
        "no assigned non-loopback IPv4 interface: environmental limitation"
    );
}
#[when(expr = "a program connects to the port {int} on that network address")]
async fn connection(w: &mut World, port: u16) {
    assert_eq!(port, 8329);
    w.scenario.start().await;
    assert!(
        check::connect(std::net::Ipv4Addr::LOCALHOST.into(), w.scenario.root.port).await,
        "listener is live on loopback"
    );
    w.access.connected =
        Some(check::connect(w.access.address.unwrap(), w.scenario.root.port).await);
}
#[then("acp2api accepts no connection")]
fn no_connection(w: &mut World) {
    assert_eq!(
        w.access.connected,
        Some(false),
        "FR-008 refuses actual assigned non-loopback socket"
    );
}
#[when(
    expr = "an API client sends a valid request with a stream to {string} with a wrong local API key"
)]
async fn streamed_wrong_key(w: &mut World, endpoint: String) {
    w.access.endpoint = endpoint.clone();
    w.scenario.answer =
        Some(check::send(&mut w.scenario, &endpoint, Credential::Wrong, vec![], true).await);
}
#[then(
    "acp2api refuses the request with HTTP 401 in the error envelope of the Anthropic Messages API"
)]
fn streamed_key_refusal(w: &mut World) {
    check::key_refusal(w.scenario.answer.as_ref().unwrap(), &w.access.endpoint);
}
