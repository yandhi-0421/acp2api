use super::*;

#[test]
fn the_panel_receives_the_effective_quota_reset_until_the_wait_expires() {
    let mut core = Core::fixture(2);
    assert!(core.view.borrow().quota_reset.is_none());
    for (now, reset, message) in [
        (
            10_000,
            "1970-01-01T00:02:00Z",
            "resets 1970-01-01T00:02:00Z",
        ),
        (
            11_000,
            "1970-01-01T00:03:00Z",
            "resets 1970-01-01T00:03:00Z",
        ),
    ] {
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "quota-view-75-4ad26e71a7034b7b".into(),
                error: crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: message.into(),
                },
            }),
            now,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(now),
        );
        let view = core.view.borrow();
        assert_eq!(
            (view.state, serde_json::to_value(&view.quota_reset).unwrap()),
            (State::QuotaWait, serde_json::json!(reset)),
            "FR-054 publishes the current reset time after each quota signal"
        );
    }
    core.handle(
        Event::Deadline,
        180_000,
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(180),
    );
    assert_eq!(
        (
            core.view.borrow().state,
            serde_json::to_value(&core.view.borrow().quota_reset).unwrap()
        ),
        (State::Running, serde_json::Value::Null),
        "FR-054 clears the reset time when the quota wait ends"
    );
}

#[test]
fn the_panel_receives_the_time_signal_and_original_text_of_a_lockout() {
    let mut core = Core::fixture(2);
    core.handle(
        Event::Lockout {
            signal: "403 PERMISSION_DENIED",
            error: crate::messages::Message::UpstreamHttp(crate::messages::HttpError {
                code: Some(403),
                status: "PERMISSION_DENIED".into(),
                message: "Synthetic account suspension 75 b7ed23c745e147d4".into(),
            }),
            signalling_work: None,
        },
        17,
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(123),
    );
    assert_eq!(
        serde_json::to_value(&core.view.borrow().lockout_evidence).unwrap(),
        serde_json::json!({
            "since": "1970-01-01T00:02:03Z",
            "signal": "403 PERMISSION_DENIED",
            "original_error_text": "acp2api: upstream error 403 PERMISSION_DENIED: Synthetic account suspension 75 b7ed23c745e147d4",
        }),
        "FR-054 publishes all lockout evidence to the panel"
    );
}

#[test]
fn the_panel_process_word_changes_when_retirement_starts() {
    let mut core = Core::fixture(2);
    core.handle(
        Event::RuntimeReady {
            sign_in_required: false,
        },
        0,
        std::time::UNIX_EPOCH,
    );
    let ready = core.view.borrow().runtime_process;
    core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
    let retiring = core.view.borrow().runtime_process;
    core.handle(Event::Stop, 2, std::time::UNIX_EPOCH);
    let stopped = core.view.borrow().runtime_process;
    assert_eq!(
        (ready, retiring, stopped),
        (
            RuntimeProcess::Running,
            RuntimeProcess::Stopping,
            RuntimeProcess::Stopped
        ),
        "FR-054 a process being retired must not read running while its replacement is checked"
    );
}

#[test]
fn setup_keeps_the_last_context_size_of_each_model() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-context-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let mut core = Core::fixture(2);
    let mut tickets = Vec::new();
    for model in ["m", "n"] {
        core.sessions.replaced(Some(format!("session-{model}")));
        let body = serde_json::json!({"model":model,"messages":[{"role":"user","content":"Hi"}]});
        let turn = crate::dialect::CHAT
            .parse(&serde_json::to_vec(&body).unwrap())
            .ok()
            .unwrap()
            .turn;
        let (output, ticket) = mpsc::unbounded_channel();
        tickets.push(ticket);
        core.handle(
            Event::Accepted(Box::new(Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: model.into(),
                    level: None,
                    level_from_name: false,
                },
                request: journal.begin(crate::journal::Api::Chat, 0),
                reply: ResponseChannel {
                    output,
                    keep_alive: None,
                },
            })),
            0,
            std::time::UNIX_EPOCH,
        );
    }
    for (model, size) in [("m", 1024), ("n", 4096), ("m", 2048)] {
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: format!("session-{model}"),
                update: TurnUpdate::Usage { used: 13, size },
            }),
            0,
            std::time::UNIX_EPOCH,
        );
    }
    assert_eq!(
        core.view.borrow().context_sizes,
        BTreeMap::from([("m".into(), 2048), ("n".into(), 4096)]),
        "FR-054 the setup window is the last reported size for its own model"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[tokio::test]
async fn a_lockout_publishes_the_stopped_runtime_process() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let (mut core, commands, folder) = error_after_reply(
            "",
            crate::messages::HttpError {
                code: Some(403),
                status: "PERMISSION_DENIED".into(),
                message: "Synthetic account suspension".into(),
            },
        );
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
        Conductor::commands(
            &mut core,
            &mut None,
            &mut false,
            &clock,
            &journal,
            &mut commands.into(),
        )
        .await;
        let view = core.view.borrow().clone();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            (view.state, view.runtime_process),
            (State::Lockout, RuntimeProcess::Stopped),
            "FR-035 and FR-054 publish a stopped process after the lockout stop completes"
        );
    })
    .await
    .expect(
        "lockout view cap: five seconds, no child processes or requests, existing journal caps",
    );
}

#[tokio::test]
async fn a_stored_lockout_starts_with_the_runtime_process_stopped() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-stored-lockout-view-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
        let path = folder.join(crate::store::STATE_FILE);
        let mut state = crate::store::StateFile::read(&path).unwrap();
        state.lockout = Some(crate::store::Lockout {
            since: "2026-10-18T12:00:00Z".into(),
            signal: "403 PERMISSION_DENIED".into(),
            original_error_text: "Synthetic account suspension".into(),
        });
        state.save(&path).unwrap();
        let (conductor, task) = start_without_runtime(&clock, &journal, &folder);
        conductor
            .builds(crate::runtime_builds::Action::Initial, Hold::empty())
            .await
            .unwrap();
        let view = conductor.view();
        drop(conductor);
        task.await.unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            (view.state, view.runtime_process),
            (State::Lockout, RuntimeProcess::Stopped),
            "FR-035 and FR-054 publish no running process at a start in stored lockout"
        );
    })
    .await
    .expect("stored lockout view cap: five seconds, no runtime launches or requests, existing journal caps");
}

#[test]
fn models_are_empty_after_a_runtime_closes() {
    let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
    let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
    let mut core = Core {
        catalog: Some(catalog),
        ..Core::fixture(1)
    };
    assert_eq!(core.model_entries(0).ok().unwrap().len(), 1);
    let missing = crate::dialect::CHAT
        .parse(br#"{"model":"missing","messages":[{"role":"user","content":"Hi"}]}"#)
        .ok()
        .unwrap()
        .turn;
    assert!(core.check_model(&missing).is_err());
    core.handle(
        Event::Runtime(RuntimeEvent::Closed),
        0,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        core.model_entries(0).ok().unwrap().len(),
        0,
        "GET /v1/models"
    );
    assert!(core.check_model(&missing).is_ok());
}
