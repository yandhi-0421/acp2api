use acp2api_acceptance::real_limit;
use acp2api_acceptance::{
    Scenario, Setup, client::ApiCall, content_check as c, fake::ModelSpec,
    journal_check as journal, model_check as check,
};
use serde_json::json;
fn sparse() -> Vec<ModelSpec> {
    vec![
        ModelSpec {
            value: "boreal-9".into(),
            name: "Boreal Nine".into(),
            levels: vec!["low".into(), "high".into()],
        },
        ModelSpec {
            value: "south-2".into(),
            name: "South Two".into(),
            levels: vec!["medium".into()],
        },
    ]
}
#[tokio::test]
async fn sparse_catalog_has_runtime_display_names_and_no_invented_suffixes() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s.fake().catalog(&sparse());
    c::send(&mut s, check::list()).await;
    check::membership(
        s.answer.as_ref().unwrap(),
        &[
            "boreal-9",
            "boreal-9-low",
            "boreal-9-high",
            "south-2",
            "south-2-medium",
        ],
    );
    check::fields(
        s.answer.as_ref().unwrap(),
        1_792_281_600,
        "2026-10-18T00:00:00Z",
    );
    for v in s.answer.as_ref().unwrap().json()["data"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            v["display_name"],
            if v["id"].as_str().unwrap().starts_with("boreal") {
                "Boreal Nine"
            } else {
                "South Two"
            }
        );
    }
    for model in ["boreal-9-medium", "south-2-low", "south-2-high"] {
        let mut call = ApiCall::one_user_message("POST /v1/messages", "private content");
        call.body["model"] = model.into();
        call.body["stream"] = true.into();
        c::send(&mut s, call).await;
        c::refusal(
            &s,
            "POST /v1/messages",
            &format!(
                "acp2api: the model name {model} is not available. The model names are: boreal-9, boreal-9-low, boreal-9-high, south-2, south-2-medium."
            ),
            "model_not_found",
            Some("model"),
        );
    }
    assert_eq!(s.fake().record().session_meta().len(), 1);
    s.finish(true).await;
}
#[tokio::test]
async fn model_lists_use_key_checks_and_one_metadata_record_with_absent_usage() {
    for wrong in [false, true] {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let mut call = check::list();
        if wrong {
            call.headers
                .push(("x-api-key".into(), "deliberately-wrong".into()));
        }
        c::send(&mut s, call).await;
        let answer = s.answer.as_ref().unwrap();
        assert_eq!(answer.status, if wrong { 401 } else { 200 });
        journal::wait_terminal(&s, 1).await;
        let records = journal::request_records(&s);
        assert_eq!(records.len(), 1);
        assert_eq!(journal::terminal(&s), records);
        assert_eq!(records[0]["request_id"], answer.headers["x-request-id"]);
        assert_eq!(records[0]["api"], "models");
        assert_eq!(
            records[0]["result"],
            if wrong { "refused" } else { "success" }
        );
        assert_eq!(
            records[0]["error_code"],
            if wrong {
                json!("invalid_api_key")
            } else {
                json!(null)
            }
        );
        for k in [
            "model",
            "session",
            "session_use",
            "input_tokens",
            "output_tokens",
        ] {
            assert!(records[0][k].is_null(), "absent metadata {k}");
        }
        assert!(s.fake().prompts().is_empty());
        assert_eq!(s.fake().record().session_meta().len(), 1);
        s.finish(true).await;
    }
}

#[tokio::test]
async fn requests_before_startup_result_wait_for_the_same_check_without_http_acceptance() {
    use acp2api_acceptance::client::ApiClient;
    use std::time::Duration;
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s.fake().catalog(&sparse());
    s.fake().hold_next_session_result();
    let fake = s.fake();
    let hub = s.hub.clone();
    let mut time = s.time.clone();
    let client = ApiClient::new(
        format!("127.0.0.1:{}", s.root.port),
        s.root.key.clone(),
        hub.clone(),
    );
    let observer = async move {
        tokio::time::timeout(real_limit(Duration::from_secs(10)), async {
            while fake.record().session_meta().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("startup observation cap: 10 s");
        let mut request = ApiCall::one_user_message("POST /v1/messages", "private content");
        request.body["model"] = "unknown-model".into();
        request.body["stream"] = true.into();
        let refused = client.send(request);
        let listed = client.send(check::list());
        let _ = tokio::time::timeout(Duration::from_millis(200), async {
            while refused.status().is_none() && listed.status().is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert_eq!(
            refused.status(),
            None,
            "required startup check precedes HTTP acceptance"
        );
        assert!(refused.events().is_empty());
        assert_eq!(
            listed.status(),
            None,
            "model list waits for the same startup result"
        );
        assert_eq!(fake.record().session_meta().len(), 1);
        assert!(
            hub.entries()
                .iter()
                .any(|e| e.event["kind"] == "session-result-held")
        );
        fake.release_session_result();
        let refused = refused.finish(&mut time).await;
        let listed = listed.finish(&mut time).await;
        assert_eq!(refused.status, 400);
        assert!(refused.events.is_empty());
        assert_eq!(
            refused.json()["error"]["message"],
            "acp2api: the model name unknown-model is not available. The model names are: boreal-9, boreal-9-low, boreal-9-high, south-2, south-2-medium."
        );
        check::membership(
            &listed,
            &[
                "boreal-9",
                "boreal-9-low",
                "boreal-9-high",
                "south-2",
                "south-2-medium",
            ],
        );
        assert_eq!(fake.record().session_meta().len(), 1);
        assert_eq!(fake.record().client_info().len(), 1);
        assert!(fake.prompts().is_empty());
    };
    tokio::join!(s.start(), observer);
    s.finish(true).await;
}
