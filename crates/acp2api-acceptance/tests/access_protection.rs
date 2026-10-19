use acp2api_acceptance::{
    Scenario, Setup, TurnPlan,
    access_check::{self as check, Credential},
};
const ENDPOINTS: [&str; 4] = [
    "POST /v1/messages",
    "POST /v1/responses",
    "POST /v1/chat/completions",
    "GET /v1/models",
];
#[tokio::test]
async fn every_api_checks_credentials_before_streaming_and_records_each_request() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut total = 0;
    println!(
        "fresh_selected_port={} credential_values_logged=false",
        s.root.port
    );
    for endpoint in ENDPOINTS {
        for stream in if endpoint.starts_with("POST") {
            vec![false, true]
        } else {
            vec![false]
        } {
            for (credential, wrong_bearer) in [
                (Credential::Absent, false),
                (Credential::Wrong, false),
                (Credential::Key, false),
                (Credential::Bearer, false),
                (Credential::Absent, true),
            ] {
                let before = check::prompt_count(&s);
                s.fake()
                    .next_prompt(TurnPlan::new().chunk("Allowed").end("end_turn"));
                let headers = if wrong_bearer {
                    vec![(
                        "authorization".into(),
                        format!("Bearer {}", acp2api_acceptance::setup::random_hex(24)),
                    )]
                } else {
                    vec![]
                };
                let a = check::send(&mut s, endpoint, credential, headers, stream).await;
                total += 1;
                println!(
                    "endpoint={endpoint} stream={stream} credential={credential:?} wrong_bearer={wrong_bearer} x_api_key_present={} authorization_present={} status={} content_type={:?} request_id_present={} cors_count={} acp_prompt_delta={}",
                    matches!(credential, Credential::Key | Credential::Wrong),
                    matches!(credential, Credential::Bearer) || wrong_bearer,
                    a.status,
                    a.headers.get("content-type"),
                    a.headers.contains_key("x-request-id"),
                    a.headers
                        .keys()
                        .filter(|h| h.starts_with("access-control-"))
                        .count(),
                    check::prompt_count(&s) - before
                );
                if matches!(credential, Credential::Absent | Credential::Wrong) {
                    println!("refusal_envelope={}", a.json());
                    check::key_refusal(&a, endpoint);
                    assert_eq!(check::prompt_count(&s), before);
                } else {
                    assert_eq!(a.status, 200);
                    assert_eq!(
                        check::prompt_count(&s) - before,
                        usize::from(endpoint.starts_with("POST"))
                    );
                    if stream {
                        assert!(!a.events.is_empty());
                    }
                }
                check::no_cors(&a);
                check::journal(&s, &a, total).await;
            }
        }
    }
    s.finish(true).await;
}
#[tokio::test]
async fn all_api_origins_and_foreign_hosts_are_refused_without_cors_or_prompts() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut total = 0;
    println!(
        "fresh_selected_port={} credential_values_logged=false",
        s.root.port
    );
    for endpoint in ENDPOINTS {
        for stream in if endpoint.starts_with("POST") {
            vec![false, true]
        } else {
            vec![false]
        } {
            for origin in [
                "https://example.com".into(),
                String::new(),
                format!("http://127.0.0.1:{}", s.root.port),
            ] {
                let a = check::send(
                    &mut s,
                    endpoint,
                    Credential::Key,
                    vec![("origin".into(), origin)],
                    stream,
                )
                .await;
                total += 1;
                println!(
                    "endpoint={endpoint} stream={stream} origin_present=true status={} cors_count={} acp_prompts={}",
                    a.status,
                    a.headers
                        .keys()
                        .filter(|h| h.starts_with("access-control-"))
                        .count(),
                    check::prompt_count(&s)
                );
                assert!(
                    (400..500).contains(&a.status),
                    "FR-008 every present Origin rejected: {}",
                    a.status
                );
                check::refusal(&a, endpoint);
                assert_eq!(check::prompt_count(&s), 0);
                check::journal(&s, &a, total).await;
            }
            for host in [
                "example.com".into(),
                format!(
                    "localhost:{}",
                    if s.root.port == 65535 {
                        65534
                    } else {
                        s.root.port + 1
                    }
                ),
            ] {
                let a = check::send(
                    &mut s,
                    endpoint,
                    Credential::Key,
                    vec![("host".into(), host.clone())],
                    stream,
                )
                .await;
                total += 1;
                println!(
                    "endpoint={endpoint} stream={stream} host={host} status={} content_type={:?} acp_prompts={}",
                    a.status,
                    a.headers.get("content-type"),
                    check::prompt_count(&s)
                );
                assert!(
                    (400..500).contains(&a.status),
                    "FR-008 exact host and selected port"
                );
                check::refusal(&a, endpoint);
                check::no_cors(&a);
                assert!(a.events.is_empty());
                assert_eq!(check::prompt_count(&s), 0);
                check::journal(&s, &a, total).await;
            }
        }
    }
    for host in ["127.0.0.1", "localhost"] {
        let host_header = format!("{host}:{}", s.root.port);
        let a = check::send(
            &mut s,
            "GET /v1/models",
            Credential::Key,
            vec![("host".into(), host_header)],
            false,
        )
        .await;
        total += 1;
        assert_eq!(a.status, 200);
        check::no_cors(&a);
        check::journal(&s, &a, total).await;
    }
    let a = check::send(
        &mut s,
        "OPTIONS /v1/messages",
        Credential::Absent,
        vec![
            ("origin".into(), "https://example.com".into()),
            ("access-control-request-method".into(), "POST".into()),
        ],
        false,
    )
    .await;
    println!(
        "OPTIONS status={} cors_count={} acp_prompts={}",
        a.status,
        a.headers
            .keys()
            .filter(|h| h.starts_with("access-control-"))
            .count(),
        check::prompt_count(&s)
    );
    check::no_cors(&a);
    assert_eq!(check::prompt_count(&s), 0);
    assert!((400..500).contains(&a.status));
    check::journal(&s, &a, total + 1).await;
    s.finish(true).await;
}
#[tokio::test]
async fn listener_refuses_this_computers_assigned_network_address() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s.start().await;
    assert!(check::connect(std::net::Ipv4Addr::LOCALHOST.into(), s.root.port).await);
    let address = check::network_address().expect("environment needs assigned non-loopback IPv4");
    let connected = check::connect(address, s.root.port).await;
    println!(
        "selected_port={} assigned_address={address} assigned_interface=true loopback_connected=true non_loopback_connected={connected}",
        s.root.port
    );
    assert!(
        !connected,
        "FR-008 refuses actual assigned non-loopback socket"
    );
    s.finish(true).await;
}
