use super::*;

#[test]
fn queued_model_reads_end_when_the_runtime_stops_requests() {
    for (event, expected) in [
        (
            Event::RuntimeUnavailable(UnavailableReason {
                message: crate::messages::Unavailable::no_executable(),
                cause: None,
            }),
            crate::messages::Message::RuntimeUnavailable(
                crate::messages::Unavailable::no_executable(),
            ),
        ),
        (Event::Stop, crate::messages::Message::Stopped),
        (
            Event::Runtime(RuntimeEvent::Closed),
            crate::messages::Message::RuntimeFailed,
        ),
        (
            Event::Lockout {
                signal: "403 PERMISSION_DENIED",
                error: crate::messages::Message::UpstreamHttp(crate::messages::HttpError {
                    code: Some(403),
                    status: "PERMISSION_DENIED".into(),
                    message: "Account suspended".into(),
                }),
                signalling_work: None,
            },
            crate::messages::Message::lockout(
                &crate::store::Lockout {
                    since: "1970-01-01T00:00:00Z".into(),
                    signal: "403 PERMISSION_DENIED".into(),
                    original_error_text:
                        "acp2api: upstream error 403 PERMISSION_DENIED: Account suspended".into(),
                },
                "http://127.0.0.1:8329/",
                false,
            ),
        ),
    ] {
        let mut core = Core::fixture(1);
        core.lifecycle.set_state(RuntimeState::Restarting);
        let (reply, _result) = oneshot::channel();
        core.handle(Event::ReadModelNames(reply), 0, std::time::UNIX_EPOCH);
        assert_eq!(core.view.borrow().queued, 1);
        let commands = core.handle(event, 1, std::time::UNIX_EPOCH);
        let answer = commands.into_iter().find_map(|command| match command {
            Command::ModelsRead { result, .. } => Some(result),
            _ => None,
        });
        assert_eq!(
            answer.map(|result| result.map_err(|error| error.to_string())),
            Some(Err(expected.to_string()))
        );
        assert_eq!(core.view.borrow().queued, 0);
    }
}

#[test]
fn model_name_callers_share_one_read_and_then_use_the_cached_names() {
    let mut core = Core::fixture(2);
    core.lifecycle.set_state(RuntimeState::Restarting);
    let mut answers = Vec::new();
    for _ in 0..2 {
        let (reply, answer) = oneshot::channel();
        answers.push(answer);
        core.handle(Event::ReadModelNames(reply), 0, std::time::UNIX_EPOCH);
    }
    assert_eq!(core.view.borrow().queued, 1, "one shared queued read");
    let commands = core.handle(
        Event::RuntimeReady {
            sign_in_required: false,
        },
        1,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        commands
            .iter()
            .filter(|c| matches!(c, Command::Prepare { .. }))
            .count(),
        1
    );
    let (reply, answer) = oneshot::channel();
    answers.push(answer);
    let commands = core.handle(Event::ReadModelNames(reply), 2, std::time::UNIX_EPOCH);
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::Prepare { .. }))
    );
    assert_eq!(core.view.borrow().places, 1, "one shared running read");

    core.sessions.replaced(Some("previous-session".into()));
    let metadata = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::Prepared {
            work: "models-0".into(),
            result: Ok((
                "models-session".into(),
                crate::catalog::Catalog::decode(&metadata, std::time::UNIX_EPOCH).unwrap(),
            )),
        }),
        3,
        std::time::UNIX_EPOCH,
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::Forget(session) if session == "previous-session"))
    );
    for command in commands {
        if let Command::ModelsRead { reply, result } = command {
            reply.send(result).unwrap();
        }
    }
    assert!(
        answers
            .iter_mut()
            .all(|answer| matches!(answer.try_recv(), Ok(Ok(())))),
        "every waiter gets the shared result"
    );
    let (reply, mut answer) = oneshot::channel();
    let commands = core.handle(Event::ReadModelNames(reply), 4, std::time::UNIX_EPOCH);
    assert!(
        matches!(answer.try_recv(), Ok(Ok(()))),
        "cached names answer immediately"
    );
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::Prepare { .. }))
    );
    assert_eq!(core.sessions.startup(), Some("models-session"));
    assert_eq!(core.view.borrow().model_names, ["m"]);
    assert_eq!(
        (core.view.borrow().places, core.view.borrow().queued),
        (0, 0)
    );
}

#[tokio::test]
async fn a_check_queues_requests_when_no_accepted_runtime_existed() {
    use crate::{BoxFuture, Launched, ProcessLauncher, ProcessTree};
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    struct Tree(Arc<AtomicBool>);
    impl ProcessTree for Tree {
        fn pid(&self) -> u32 {
            std::process::id()
        }
        fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
            Box::pin(std::future::pending())
        }
        fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
            Box::pin(async move {
                self.0.store(true, Ordering::SeqCst);
            })
        }
    }
    struct Launcher(Mutex<Option<Launched>>, mpsc::UnboundedSender<()>);
    impl ProcessLauncher for Launcher {
        fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
            self.1.send(()).unwrap();
            Ok(self
                .0
                .lock()
                .unwrap()
                .take()
                .expect("one check process cap"))
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let root = std::env::temp_dir().join(format!(
            "acp2api-check-unavailable-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let source = root.join("exe");
        std::fs::create_dir_all(&source).unwrap();
        for name in [
            crate::runtime_builds::SERVER,
            crate::runtime_builds::HARNESS,
        ] {
            std::fs::write(source.join(name), b"unlisted candidate").unwrap();
        }
        let (stdout, _peer) = tokio::io::duplex(8192);
        let stopped = Arc::new(AtomicBool::new(false));
        let (launched, mut launch) = mpsc::unbounded_channel();
        let mut ports = crate::Ports::production();
        ports.launcher = Arc::new(Launcher(
            Mutex::new(Some(Launched {
                stdin: Box::pin(tokio::io::sink()),
                stdout: Box::pin(stdout),
                stderr: Box::pin(tokio::io::empty()),
                tree: Box::new(Tree(stopped.clone())),
            })),
            launched,
        ));
        let (stop, stopping) = watch::channel(false);
        let builds = crate::runtime_builds::Builds::new(
            &ports,
            crate::Environment {
                account_root: root.join("account"),
                temp: root.join("temp"),
                home: Some(root.join("home")),
                local_app_data: None,
                path: vec![],
                exe_dir: source,
            },
            root.clone(),
            crate::account::Temporary::fixture(&root),
            crate::runtime_driver::mcp::ToolSets::new(0),
            stopping,
            0,
        );
        let configuration = root.join("config.toml");
        let settings = crate::store::read(&configuration, 1).unwrap().settings;
        let journal =
            crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
        let request = journal.begin(crate::journal::Api::Chat, ports.clock.now());
        let (conductor, task) = Conductor::start(
            builds,
            ports.clock.clone(),
            journal,
            settings,
            configuration,
            1,
        );
        conductor
            .builds(crate::runtime_builds::Action::Initial, Hold::empty())
            .await
            .unwrap();
        assert!(conductor.model_names(Hold::empty()).await.is_err());
        let identity = conductor.builds_summary().unwrap().found[0]
            .identity
            .clone();
        let owner = conductor.clone();
        let check = tokio::spawn(async move {
            owner
                .builds(
                    crate::runtime_builds::Action::Check(identity),
                    Hold::empty(),
                )
                .await
        });
        launch.recv().await.unwrap();
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"m","messages":[{"role":"user","content":"queued"}]}"#)
            .ok()
            .unwrap()
            .turn;
        let admission = conductor
            .admit(turn, Hold::empty(), request, ResponseMode::Body)
            .await;
        assert!(
            admission.is_ok(),
            "a request during a check must queue even when no earlier accepted runtime existed"
        );
        let mut ticket = admission.ok().unwrap();
        assert!(
            matches!(
                ticket.events.try_recv(),
                Err(mpsc::error::TryRecvError::Empty)
            ),
            "a check sends no API prompt"
        );
        stop.send(true).unwrap();
        task.await.unwrap();
        assert!(matches!(check.await.unwrap(), Err(LaunchFailure::Stopped)));
        assert!(
            stopped.load(Ordering::SeqCst),
            "stop must end the check process"
        );
        std::fs::remove_dir_all(root).unwrap();
    })
    .await
    .expect(
        "check cap: 5 seconds, one process, one initialize write, 8 KiB input, one API request",
    );
}

#[test]
fn an_unpermitted_tool_keeps_its_place_until_the_prompt_ends() {
    let folder =
        std::env::temp_dir().join(format!("acp2api-tool-retirement-{}", std::process::id()));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, queued, tickets) = prompting_with_waiter(&journal);
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::UnpermittedTool("run_command".into()),
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::Cancel(session) if session == "running"))
    );
    assert!(commands.iter().any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::BuiltInToolRan(name)))) if name == "run_command")));
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Prepare { .. } | Command::Prompt { .. })),
        "FR-049 the cancelled prompt still occupies the only place"
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Forget(_))),
        "the runtime must retain the outstanding prompt until its result"
    );
    drop(commands);
    assert!(
        tickets[0].is_closed(),
        "the ended request retains no reply route"
    );
    let late = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Text("Late text".into()),
        }),
        2,
        std::time::UNIX_EPOCH,
    );
    assert!(late.is_empty(), "no output follows the request's error");
    let ended = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded { stop_reason: None },
        }),
        3,
        std::time::UNIX_EPOCH,
    );
    assert!(
        ended
            .iter()
            .any(|command| matches!(command, Command::Prepare { work, .. } if work == &queued)),
        "FR-049 the prompt result releases its place for the queued request"
    );
    assert!(
        !ended
            .iter()
            .any(|command| matches!(command, Command::Emit(_, _) | Command::Complete(_))),
        "retirement sends no second end or request record"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn an_upstream_error_keeps_its_place_until_the_prompt_fails() {
    let folder =
        std::env::temp_dir().join(format!("acp2api-http-retirement-{}", std::process::id()));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, queued, tickets) = prompting_with_waiter(&journal);
    let error = || {
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: crate::messages::HttpError {
                code: Some(503),
                status: "UNAVAILABLE".into(),
                message: "HTTP failure".into(),
            },
        })
    };
    let commands = core.handle(error(), 1, std::time::UNIX_EPOCH);
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Prepare { .. } | Command::Prompt { .. })),
        "FR-049 an upstream error does not end the outstanding prompt"
    );
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::Cancel(session) if session == "running"))
    );
    assert!(commands.iter().any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::UpstreamHttp(error)))) if error.code == Some(503) && error.message == "HTTP failure")));
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Forget(_)))
    );
    drop(commands);
    assert!(
        tickets[0].is_closed(),
        "the ended request retains no reply route"
    );
    let repeated = core.handle(error(), 2, std::time::UNIX_EPOCH);
    assert!(
        matches!(&repeated[..], [Command::UpstreamLog { .. }]),
        "a repeated error keeps the occupied entry and sends no second end"
    );
    let ended = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Failed(crate::messages::Message::Upstream("RPC failure".into())),
        }),
        3,
        std::time::UNIX_EPOCH,
    );
    assert!(
        ended
            .iter()
            .any(|command| matches!(command, Command::Prepare { work, .. } if work == &queued)),
        "FR-049 the failed prompt releases its place for the queued request"
    );
    assert!(
        !ended
            .iter()
            .any(|command| matches!(command, Command::Emit(_, _) | Command::Complete(_))),
        "retirement sends no second end or request record"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_retired_configuration_call_dispatches_the_next_request() {
    let folder =
        std::env::temp_dir().join(format!("acp2api-config-retirement-{}", std::process::id()));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
    let request = core.work[&core.work.by_session("running").unwrap()]
        .key()
        .to_owned();
    let disconnected = core.handle(Event::Disconnected(request), 1, std::time::UNIX_EPOCH);
    assert!(
        !disconnected
            .iter()
            .any(|command| matches!(command, Command::Prepare { .. }))
    );
    let retired = core.handle(
        Event::Runtime(RuntimeEvent::Retired {
            session: "running".into(),
        }),
        2,
        std::time::UNIX_EPOCH,
    );
    assert!(
        retired
            .iter()
            .any(|command| matches!(command, Command::Prepare { work, .. } if work == &queued)),
        "FR-049 the retired ACP call releases its place for the queued request"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[tokio::test]
async fn without_a_runtime_a_request_gets_no_model_name_error() {
    let folder = std::env::temp_dir().join(format!("acp2api-no-runtime-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
    let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
    let (conductor, _task) = start_without_runtime(&clock, &journal, &folder);
    let body = br#"{"model":"gemini-3.8-flash","reasoning_effort":"extreme","messages":[{"role":"user","content":"Hi"}]}"#;
    let turn = crate::dialect::CHAT.parse(body).ok().unwrap().turn;
    let request = journal.begin(crate::journal::Api::Chat, clock.now());
    let admitted = conductor
        .admit(
            turn,
            clock.hold(HoldKind::HttpRequest),
            request,
            ResponseMode::Body,
        )
        .await;
    let end = match admitted {
        Ok(Ticket { mut events, hold }) => {
            drop(hold);
            loop {
                if let (TurnEvent::End(end), _) = events.recv().await.unwrap() {
                    break end.err().map(|message| message.to_string());
                }
            }
        }
        Err(refusal) => Some(refusal.message.to_string()),
    };
    let listed = conductor
        .model_names(clock.hold(HoldKind::HttpRequest))
        .await
        .ok()
        .map(|entries| entries.len());
    let lines: String = std::fs::read_dir(folder.join("logs"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    std::fs::remove_dir_all(&folder).unwrap();
    let unavailable = Some("official runtime is not available".to_owned());
    assert_eq!(end, unavailable, "answer to a request");
    assert_eq!(listed, Some(0), "GET /v1/models lists no model");
    assert!(
        lines.contains(r#""result":"failed""#),
        "request log control: {lines}"
    );
    assert!(
        !lines.contains(r#""event":"unknown_effort""#),
        "61ee383 emits no effort diagnostic without a catalog: {lines}"
    );
}

#[tokio::test]
async fn a_client_gone_before_admission_still_has_its_request_line() {
    let folder = std::env::temp_dir().join(format!("acp2api-conductor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
    let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
    let (conductor, _task) = start_without_runtime(&clock, &journal, &folder);
    let body = br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#;
    let turn = || crate::dialect::CHAT.parse(body).ok().unwrap().turn;
    let request = || journal.begin(crate::journal::Api::Chat, clock.now());
    let (admission, reply) = oneshot::channel();
    drop(reply);
    conductor
        .send
        .send(Input::Turn(Box::new(TurnWork {
            pending: PendingRequest {
                mode: ResponseMode::Body,
                turn: turn(),
                request: request(),
                admission,
                connection: mpsc::unbounded_channel().0,
            },
            hold: clock.hold(HoldKind::HttpRequest),
        })))
        .await
        .ok()
        .unwrap();
    let next = conductor.admit(
        turn(),
        clock.hold(HoldKind::HttpRequest),
        request(),
        ResponseMode::Body,
    );
    assert!(next.await.is_ok(), "the conductor serves the next request");
    let lines: String = std::fs::read_dir(folder.join("logs"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    std::fs::remove_dir_all(&folder).unwrap();
    assert!(
        lines.contains(r#""result":"disconnected""#),
        "request line of the gone client: {lines}"
    );
}
