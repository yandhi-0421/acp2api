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
    for endpoint in ENDPOINTS {
        for stream in if endpoint.starts_with("POST") {
            vec![false, true]
        } else {
            vec![false]
        } {
            for credential in [
                Credential::Absent,
                Credential::Wrong,
                Credential::Key,
                Credential::Bearer,
                Credential::WrongBearer,
            ] {
                let before = check::prompt_count(&s);
                s.fake()
                    .next_prompt(TurnPlan::new().chunk("Allowed").end("end_turn"));
                let a = check::send(&mut s, endpoint, credential, vec![], stream).await;
                total += 1;
                if matches!(
                    credential,
                    Credential::Absent | Credential::Wrong | Credential::WrongBearer
                ) {
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
                check::refusal(&a, endpoint);
                assert_eq!(check::prompt_count(&s), 0);
                check::journal(&s, &a, total).await;
            }
            for host in [
                "example.com".into(),
                format!("localhost:{}", check::other_port(s.root.port)),
            ] {
                let a = check::send(
                    &mut s,
                    endpoint,
                    Credential::Key,
                    vec![("host".into(), host)],
                    stream,
                )
                .await;
                total += 1;
                check::refusal(&a, endpoint);
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
    check::no_cors(&a);
    assert_eq!(check::prompt_count(&s), 0);
    assert!((400..500).contains(&a.status));
    check::journal(&s, &a, total + 1).await;
    s.finish(true).await;
}
