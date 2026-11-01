use super::*;

fn full(f: &mut Fixture) -> String {
    let (work, _) = f.request("eighth", true);
    f.prepared(&work, Ok("eighth"));
    work
}

#[test]
fn ending_sign_in_during_a_limit_wait_keeps_the_required_replacement() {
    let mut f = Fixture::new();
    full(&mut f);
    f.event(Event::StartSignIn);
    let (waiting, _) = f.request("waiter", true);
    f.event(Event::EndSignInWait);
    f.event(Event::Disconnected(waiting));
    f.end("eighth");
    assert!(
        matches!(f.core.lifecycle.replacement(), Some(Replacement::SignIn))
            && f.core.replacement_ready(0),
        "M1: a canceled sign-in must replace the process even after the capacity waiter leaves"
    );
}

#[tokio::test]
async fn a_local_prepare_dispatch_failure_releases_its_reservation() {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut f = Fixture::new();
        let (_, commands) = f.request("local failure", true);
        let clock = crate::Ports::production().clock;
        Conductor::commands(
            &mut f.core,
            &mut None,
            &mut false,
            &clock,
            &f.journal,
            &mut commands.into(),
        )
        .await;
        let (_, commands) = f.request("after local failure", true);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, Command::Prepare { .. }))
                && !f.core.lifecycle.replacement_pending(),
            "M2: a local failure must leave room for the next session/new"
        );
    })
    .await
    .expect("local dispatch cap: 3 seconds; two preparations; no process launch");
}

#[test]
fn runtime_unavailable_clears_limit_demand_and_prevents_an_automatic_restart() {
    let mut f = Fixture::new();
    full(&mut f);
    f.request("waiter", true);
    f.end("eighth");
    assert!(f.core.replacement_ready(0) && f.core.lifecycle.replacement_pending());
    f.event(Event::RuntimeUnavailable(UnavailableReason {
        message: crate::messages::Unavailable::no_executable(),
        cause: None,
    }));
    assert!(
        !f.core.lifecycle.replacement_pending(),
        "M3: unavailable must leave no automatic restart demand"
    );
}

#[test]
fn retry_refusals_do_not_start_or_keep_a_limit_replacement() {
    for queued_before_failure in [false, true] {
        let mut f = Fixture::new();
        full(&mut f);
        if queued_before_failure {
            f.request("eighth", true);
        }
        let mut commands = f.event(Event::Runtime(RuntimeEvent::Turn {
            session: "eighth".into(),
            update: TurnUpdate::Failed(crate::messages::Message::Upstream(
                "temporary failure".into(),
            )),
        }));
        commands.extend(f.core.handle(Event::Deadline, 200, std::time::UNIX_EPOCH));
        if !queued_before_failure {
            commands.extend(f.request("eighth", true).1);
        }
        assert!(
            !f.core.lifecycle.replacement_pending()
                && commands.iter().any(|c| matches!(
                    c,
                    Command::Emit(
                        _,
                        TurnEvent::End(Err(crate::messages::Message::RetryWait { .. }))
                    )
                )),
            "F6: a retry refusal needs no new ACP session, queued_before_failure={queued_before_failure}"
        );
    }
}

#[test]
fn a_limit_wait_reports_a_running_process_and_allows_owner_sign_in() {
    let mut f = Fixture::new();
    full(&mut f);
    f.request("waiter", true);
    let settings = crate::store::read(&f.folder.join("config.toml"), 3)
        .unwrap()
        .settings;
    let settings = watch::channel(Arc::new(settings)).0;
    let (reply, mut answer) = oneshot::channel();
    let clock = crate::Ports::production().clock;
    let commands = f.core.owner_action(
        OwnerAction::SignIn,
        reply,
        &settings,
        &f.folder.join("config.toml"),
        clock.as_ref(),
    );
    assert!(
        matches!(answer.try_recv(), Ok(Ok(())))
            && commands.iter().any(|c| matches!(c, Command::Authenticate))
            && matches!(
                f.core.view.borrow().runtime_process,
                RuntimeProcess::Running
            ),
        "F8: a process that serves checks and quota queries is running; its owner can sign in"
    );
    f.end("eighth");
    assert!(
        !f.core.replacement_ready(0),
        "decision 3 also protects a sign-in begun during the limit wait"
    );
    f.event(Event::EndSignInWait);
    assert!(f.core.replacement_ready(0));
}

fn accepted(f: &mut Fixture, turn: crate::conversation::TurnRequest) -> Accepted {
    let request = f.journal.begin(crate::journal::Api::Chat, 0);
    let (output, ticket) = mpsc::unbounded_channel();
    f.tickets.push(ticket);
    Accepted {
        turn,
        request,
        model: crate::catalog::SelectedModel {
            model: "m".into(),
            level: None,
            level_from_name: false,
        },
        reply: ResponseChannel {
            output,
            keep_alive: None,
        },
    }
}

#[test]
fn a_checked_request_cannot_load_a_ninth_acp_session() {
    let mut f = Fixture::new();
    f.core.sessions.unload(|_| f.folder.join("retained"));
    f.core.replaced(Some("second-startup".into()), None);
    for index in 0..6 {
        let (work, _) = f.request(&format!("second-{index}"), true);
        let session = format!("second-{index}");
        f.prepared(&work, Ok(&session));
        f.end(&session);
    }
    let turn = crate::dialect::CHAT.parse(br#"{"model":"m","messages":[{"role":"system","content":"session limit test"},{"role":"user","content":"fill-0"},{"role":"assistant","content":""},{"role":"user","content":"continue"}]}"#).ok().unwrap().turn;
    let request = accepted(&mut f, turn);
    let work = request.request.id.clone();
    // This is the successful check's already-created ACP session, not the old record's session/load.
    let prepare = f.core.sessions.prepare(
        work.clone(),
        work::SessionSettings::for_turn(&request.turn),
        None,
    );
    drop(prepare);
    let commands = f.event(Event::Checked {
        id: 100,
        accepted: Some(Box::new(request)),
        session: "check".into(),
    });
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::Prepare { .. }))
            && f.core.work.queued() == 1
            && f.core.lifecycle.replacement_pending(),
        "F5: the check exception cannot authorize a ninth session/load"
    );
    f.core.sessions.unload(|_| f.folder.join("retained"));
    f.core.replaced(Some("third-startup".into()), None);
    let commands = f.event(Event::RuntimeReady {
        sign_in_required: false,
    });
    assert!(commands.iter().any(|c| matches!(c, Command::Prepare { work: id, load: Some((session, _)), .. } if id == &work && session == "session-0")),
        "the checked request loads its matched record after replacement");
}

#[tokio::test]
async fn successful_wire_results_with_unusable_metadata_keep_the_acp_session_count() {
    // Three requests and replies per fake: initialize, startup session/new, then one session/new.
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        for (result, load) in [
            (serde_json::json!({"configOptions":[]}), false),
            (serde_json::json!({"sessionId":"created", "configOptions":[]}), false),
            (serde_json::json!({"configOptions":[]}), true),
        ] {
            let mut f = Fixture::new();
            let (work, _) = f.request("bad metadata", true);
            let clock = crate::Ports::production().clock;
            let mut builds = crate::runtime_builds::Builds::fixture(&f.folder, clock);
            let options = serde_json::json!([{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]);
            let replies = [
                serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}}),
                serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"startup","configOptions":options}}),
                serde_json::json!({"jsonrpc":"2.0","id":3,"result":result}),
            ].iter().map(|value| format!("{value}\n")).collect::<String>();
            assert!(replies.len() < 16 * 1024);
            builds.start.launcher = Arc::new(crate::runtime_driver::write_tests::Launcher(std::sync::Mutex::new(Some(crate::Launched {
                stdin: Box::pin(tokio::io::sink()), stdout: Box::pin(Output(std::io::Cursor::new(replies.into_bytes()))),
                stderr: Box::pin(tokio::io::empty()), tree: Box::new(StoppedTree(Arc::new(AtomicUsize::new(0)))),
            }))));
            let (mut runtime, _) = Runtime::start(&builds.start, "1.4.0").await.unwrap();
            let load = load.then(|| ("retained".into(), builds.start.temporary.sessions().join("retained")));
            runtime.request_session(work, &[], None, load).await.unwrap();
            let (event, _hold) = runtime.next().await;
            f.event(Event::Runtime(event));
            let (_, commands) = f.request("after bad metadata", true);
            let correct = f.core.lifecycle.replacement_pending()
                && !commands.iter().any(|c| matches!(c, Command::Prepare { .. }));
            runtime.stop().await;
            assert!(correct, "F4: a successful wire result already created its ACP session, even if acp2api cannot use its metadata");
        }
    }).await.expect("bad metadata cap: 3 seconds, three fake runtimes, three frames and 16 KiB each");
}
