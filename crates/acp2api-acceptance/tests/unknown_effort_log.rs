use acp2api_acceptance::{
    Scenario, Setup, endpoint_check, fake::ModelSpec, field_check, journal_check,
};
use serde_json::{Value, json};

#[tokio::test]
async fn unknown_effort_log_uses_the_256_byte_string_boundary() {
    let boundary = "é".repeat(128);
    for (value, expected) in [
        (json!(&boundary), json!(&boundary)),
        (json!(format!("{boundary}x")), Value::Null),
    ] {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let mut call = endpoint_check::request("POST /v1/chat/completions", "Hello", false);
        call.body["reasoning_effort"] = value;
        let answer = s.client().await.send(call).finish(&mut s.time).await;
        assert_eq!(answer.status, 200);
        let diagnostics: Vec<_> = journal_check::records(&s)
            .into_iter()
            .filter(|record| record["event"] == "unknown_effort")
            .collect();
        assert_eq!(diagnostics.len(), 1, "one unknown-effort diagnostic");
        assert_eq!(
            diagnostics[0]["value"], expected,
            "FR-013 256-byte log limit"
        );
        s.finish(true).await;
    }
}

#[tokio::test]
async fn unknown_effort_log_records_only_the_json_type() {
    for (value, expected) in [
        (json!({"private": "EFFORT-CONTENT"}), "object"),
        (json!(["EFFORT-CONTENT"]), "array"),
        (json!(123), "number"),
        (json!(true), "boolean"),
    ] {
        for (endpoint, field) in [
            ("POST /v1/chat/completions", "reasoning_effort"),
            ("POST /v1/responses", "reasoning.effort"),
            ("POST /v1/messages", "output_config.effort"),
        ] {
            let mut s = Scenario::default();
            s.plan(Setup::normal());
            let mut call = endpoint_check::request(endpoint, "Hello", false);
            field_check::set(&mut call, field, value.clone());
            let answer = s.client().await.send(call).finish(&mut s.time).await;
            assert_eq!(answer.status, 200);
            let diagnostics: Vec<_> = journal_check::records(&s)
                .into_iter()
                .filter(|record| record["event"] == "unknown_effort")
                .collect();
            assert_eq!(diagnostics.len(), 1, "one diagnostic for {field}={value}");
            assert_eq!(diagnostics[0]["value"], expected, "FR-013 JSON type only");
            assert!(
                !serde_json::to_string(&diagnostics)
                    .unwrap()
                    .contains("EFFORT-CONTENT")
            );
            s.finish(true).await;
        }
    }
}

#[tokio::test]
async fn null_effort_is_absent_on_each_api() {
    for (endpoint, field, thinking, level) in [
        (
            "POST /v1/chat/completions",
            "reasoning_effort",
            false,
            "high",
        ),
        ("POST /v1/responses", "reasoning.effort", false, "high"),
        ("POST /v1/messages", "output_config.effort", false, "high"),
        ("POST /v1/messages", "output_config.effort", true, "low"),
    ] {
        let mut s = Scenario::default();
        let setup = Setup::normal();
        let model = setup.model.clone();
        s.plan(setup);
        s.fake().catalog(&[ModelSpec {
            value: model,
            name: "Test model".into(),
            levels: vec!["low".into(), "medium".into(), "high".into()],
        }]);
        let mut call = endpoint_check::request(endpoint, "Hello", false);
        field_check::set(&mut call, field, Value::Null);
        if thinking {
            call.body["thinking"] = json!({"type":"disabled"});
        }
        let answer = s.client().await.send(call).finish(&mut s.time).await;
        assert_eq!(answer.status, 200);
        let records = journal_check::records(&s);
        assert!(
            records.iter().any(|record| record["result"] == "success"),
            "completion log control"
        );
        assert!(
            records
                .iter()
                .all(|record| record["event"] != "unknown_effort"),
            "FR-014 null is absent on {endpoint}"
        );
        let sets: Vec<_> = s
            .hub
            .entries()
            .iter()
            .filter_map(|entry| {
                let value: Value = serde_json::from_str(entry.event["line"].as_str()?).ok()?;
                (value["method"] == "session/set_config_option"
                    && value["params"]["configId"] == "thinking_level")
                    .then(|| value["params"]["value"].clone())
            })
            .collect();
        assert_eq!(
            sets,
            if level == "low" {
                vec![json!("low")]
            } else {
                vec![]
            },
            "FR-013 null uses the absent-field level"
        );
        s.finish(true).await;
    }
}
