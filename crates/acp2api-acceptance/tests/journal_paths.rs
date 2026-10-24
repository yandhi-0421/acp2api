use acp2api_acceptance::real_limit;
use acp2api_acceptance::{
    Scenario, Setup, TurnEndPlan, TurnPlan, TurnStep, client::ApiCall, journal_check as check,
};
use serde_json::{Value, json};
fn one(s: &Scenario, result: &str, code: &str, model: Option<&str>) {
    let r = check::records(s);
    let t = check::terminal(s);
    assert_eq!(r.len(), 1, "one completion record");
    assert_eq!(t, r, "one matching physical terminal line");
    assert_eq!(r[0]["result"], result);
    assert_eq!(r[0]["error_code"], code);
    assert_eq!(r[0]["model"], json!(model));
    assert!(r[0]["input_tokens"].is_null());
    assert!(r[0]["output_tokens"].is_null());
    assert!(
        !serde_json::to_string(&r)
            .unwrap()
            .contains("PRIVATE-PATH-CONTENT")
    );
}
#[tokio::test]
async fn authorization_and_parse_refusals_have_one_metadata_only_record() {
    for unauthorized in [false, true] {
        let mut s = Scenario::default();
        s.plan(Setup::normal());
        let mut call =
            ApiCall::one_user_message("POST /v1/chat/completions", "PRIVATE-PATH-CONTENT");
        if unauthorized {
            call.headers
                .push(("x-api-key".into(), "deliberately-wrong".into()));
        } else {
            call.body["messages"] = json!([]);
        }
        let a = s.client().await.send(call).finish(&mut s.time).await;
        assert_eq!(a.status, if unauthorized { 401 } else { 400 });
        check::wait_terminal(&s, 1).await;
        one(
            &s,
            "refused",
            if unauthorized {
                "invalid_api_key"
            } else {
                "invalid_request_error"
            },
            None,
        );
        let r = check::records(&s);
        assert_eq!(r[0]["request_id"], a.headers["x-request-id"]);
        assert!(r[0]["session"].is_null());
        assert!(r[0]["session_use"].is_null());
        assert_eq!(r[0]["duration_ms"], 0);
        s.finish(true).await;
    }
}
#[tokio::test]
async fn runtime_failure_has_one_final_record_without_error_content() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut plan = TurnPlan::new();
    plan.end = TurnEndPlan::RpcError {
        code: -32000,
        message: "PRIVATE-PATH-CONTENT".into(),
    };
    s.fake().next_prompt(plan);
    let a = s
        .client()
        .await
        .send(ApiCall::one_user_message(
            "POST /v1/chat/completions",
            "PRIVATE-PATH-CONTENT",
        ))
        .finish(&mut s.time)
        .await;
    assert_eq!(a.status, 502);
    check::wait_terminal(&s, 1).await;
    one(&s, "failed", "upstream_error", Some("gemini-3.8-flash"));
    assert!(check::records(&s)[0]["session"].is_string());
    s.finish(true).await;
}
#[tokio::test]
async fn disconnect_is_completed_once_by_the_core() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    let mut plan = TurnPlan::new().chunk("PRIVATE-PATH-CONTENT").chunk("held");
    plan.steps
        .push(TurnStep::HoldUntil("disconnect-release".into()));
    s.fake().next_prompt(plan.end("end_turn"));
    let mut request =
        ApiCall::one_user_message("POST /v1/chat/completions", "PRIVATE-PATH-CONTENT");
    request.body["stream"] = true.into();
    let call = s.client().await.send(request);
    s.time.settle().await;
    assert_eq!(
        call.status(),
        Some(200),
        "real streaming HTTP response started"
    );
    assert_eq!(s.fake().record().prompts().len(), 1);
    drop(call);
    s.fake().open_gate("disconnect-release");
    s.time.advance_to(200).await;
    check::wait_terminal(&s, 1).await;
    one(
        &s,
        "disconnected",
        "client_disconnected",
        Some("gemini-3.8-flash"),
    );
    s.time.advance_to(1000).await;
    assert_eq!(check::records(&s).len(), 1, "no cleanup duplicate");
    s.finish(true).await;
}
#[tokio::test]
async fn metadata_line_escapes_newlines_without_serializing_unknown_values() {
    let mut s = Scenario::default();
    let mut setup = Setup::normal();
    setup.model = "model\nname".into();
    s.plan(setup);
    let expected = check::send(
        &mut s,
        "POST /v1/chat/completions",
        "model\nname",
        "field\nname",
        "PRIVATE-PATH-CONTENT",
        29,
        19,
    )
    .await;
    check::wait_terminal(&s, 1).await;
    check::verify(&s, &[expected]);
    let data: String = check::files(&s.root.path.join("data/logs"))
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    assert_eq!(data.lines().count(), 1, "one physical line");
    let _: Value = serde_json::from_str(data.trim()).unwrap();
    assert!(!data.contains("PRIVATE-PATH-CONTENT"));
    s.finish(true).await;
}

#[test]
#[allow(clippy::disallowed_methods)]
fn retention_uses_real_modification_order_and_leaves_unrelated_paths() {
    use acp2api::journal::{Api, RequestRecord, ResultKind, Writer};
    let root = acp2api_acceptance::setup::TestRoot::new();
    let folder = root.path.join("writer-order");
    std::fs::create_dir(&folder).unwrap();
    let unrelated = folder.join("notes.log");
    std::fs::write(&unrelated, "unrelated file").unwrap();
    let outside = root.path.join("outside.log");
    std::fs::write(&outside, "outside target").unwrap();
    let old = check::wall(0) - std::time::Duration::from_secs(31 * 24 * 60 * 60);
    for p in [&unrelated, &outside] {
        std::fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
    }
    #[cfg(unix)]
    let link = {
        let p = folder.join("acp2api-2020-01-01-1.log");
        std::os::unix::fs::symlink(&outside, &p).unwrap();
        p
    };
    let mut writer = Writer::open(&folder, check::wall(0)).unwrap();
    let writer_files = || {
        let mut p: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("acp2api-2026-10-18-")
            })
            .collect();
        p.sort();
        p
    };
    let mut first = None;
    let mut oldest = None;
    let mut written = 0;
    let end = std::time::Instant::now() + real_limit(std::time::Duration::from_secs(12));
    for n in 0..7 {
        assert!(
            std::time::Instant::now() < end,
            "writer order run cap: 12 s; 7 records; 64 MB input; no terminal output"
        );
        let record = RequestRecord {
            time: "2026-10-18T00:00:00Z".into(),
            request_id: format!("order-{n}"),
            api: Api::Chat,
            model: None,
            session: None,
            session_use: None,
            duration_ms: 0,
            input_tokens: None,
            output_tokens: None,
            result: ResultKind::Success,
            error_code: None,
            unknown_fields: vec!["x".repeat(9_000_000)],
        };
        written += writer.write(&record).unwrap();
        if n == 0 {
            let p = writer_files()[0].clone();
            let future = std::fs::metadata(&p).unwrap().modified().unwrap()
                + std::time::Duration::from_secs(365 * 24 * 60 * 60);
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(future))
                .unwrap();
            assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), future);
            first = Some(p);
        }
        if n == 1 {
            oldest = Some(writer_files()[1].clone());
            assert!(
                std::fs::metadata(first.as_ref().unwrap())
                    .unwrap()
                    .modified()
                    .unwrap()
                    > std::fs::metadata(oldest.as_ref().unwrap())
                        .unwrap()
                        .modified()
                        .unwrap()
            );
        }
    }
    assert!((63_000_000..=64_000_000).contains(&written));
    assert!(
        first.unwrap().is_file(),
        "newest modification survives even on the first writer file"
    );
    assert!(
        !oldest.unwrap().exists(),
        "actual oldest modification is deleted first"
    );
    assert_eq!(
        std::fs::read_to_string(unrelated).unwrap(),
        "unrelated file"
    );
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "outside target");
    #[cfg(unix)]
    assert!(
        link.is_symlink(),
        "owned-looking symlink remains outside cleanup authority"
    );
    root.check_size();
    drop(writer);
    root.cleanup();
}
