use super::*;

async fn assert_start_lockout(stdout: &str, stderr: &str, original: &str) {
    use crate::{LaunchError, LaunchSpec, Launched, ProcessLauncher, ReleaseFacts};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Launcher {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        starts: AtomicUsize,
        stops: Arc<AtomicUsize>,
    }
    impl ProcessLauncher for Launcher {
        fn launch(&self, _: LaunchSpec) -> Result<Launched, LaunchError> {
            assert!(
                self.starts.fetch_add(1, Ordering::SeqCst) < 3,
                "three starts cap"
            );
            Ok(Launched {
                stdin: Box::pin(tokio::io::sink()),
                stdout: Box::pin(std::io::Cursor::new(self.stdout.clone())),
                stderr: Box::pin(std::io::Cursor::new(self.stderr.clone())),
                tree: Box::new(StoppedTree(self.stops.clone())),
            })
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        use sha2::{Digest, Sha256};
        assert!(stdout.len() + stderr.len() <= 4096, "four KiB output cap");
        assert!(stdout.lines().count() <= 2, "two ACP answers per start cap");
        let folder = std::env::temp_dir().join(format!(
            "acp2api-start-lockout-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let clock = crate::Ports::production().clock;
        let mut builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
        let launcher = Arc::new(Launcher {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            starts: AtomicUsize::new(0),
            stops: Arc::new(AtomicUsize::new(0)),
        });
        builds.start.launcher = launcher.clone();
        let copy = folder.join("official-runtime");
        std::fs::create_dir_all(&copy).unwrap();
        for name in [
            crate::runtime_builds::SERVER,
            crate::runtime_builds::HARNESS,
        ] {
            std::fs::write(copy.join(name), b"accepted").unwrap();
        }
        let hash = crate::hex(&Sha256::digest(b"accepted"));
        builds.start.release = Arc::new(Facts(vec![crate::ListedBuild {
            version: "1.4.0".into(),
            platform: Facts(Vec::new()).platform().into(),
            agy_acp_server_sha256: hash.clone(),
            localharness_external_sha256: hash,
        }]));
        let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
        let mut core = Core {
            state_path: folder.join(crate::store::STATE_FILE),
            lifecycle: Lifecycle::new(RuntimeState::Restarting),
            ..Core::fixture(1)
        };
        let mut model_replies = Vec::new();
        for _ in 0..2 {
            let (reply, answer) = oneshot::channel();
            core.handle(Event::ReadModelNames(reply), clock.now(), clock.wall());
            model_replies.push(answer);
        }
        let mut runtime = None;
        core.launch(&mut runtime, &mut false, &clock, &journal, &builds)
            .await;
        let refusal = core.refusal(clock.now()).unwrap().to_string();
        for mut answer in model_replies {
            assert_eq!(
                answer
                    .try_recv()
                    .map(|result| result.map_err(|error| error.to_string())),
                Ok(Err(refusal.clone())),
                "startup apply_lockout ends every shared model read with M-12"
            );
        }
        assert_eq!(core.view.borrow().queued, 0);
        let evidence = crate::store::StateFile::read(&core.state_path)
            .unwrap()
            .lockout;
        let actual = (
            core.state(),
            launcher.starts.load(Ordering::SeqCst),
            launcher.stops.load(Ordering::SeqCst),
            evidence.map(|e| (e.signal, e.original_error_text)),
        );
        if let Some(runtime) = runtime {
            runtime.stop().await;
        }
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(
            actual,
            (State::Lockout, 1, 1, Some(("403".into(), original.into()))),
            "FR-034/FR-035: startup lockout stops and persists without a replacement"
        );
    })
    .await
    .expect(
        "startup lockout cap: five seconds, three starts, two ACP answers and four KiB per start",
    );
}

fn assert_state_file_diagnostic(folder: &std::path::Path, expected: &str) {
    let path = folder.join(crate::store::STATE_FILE);
    let clock = crate::Ports::production().clock;
    let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
    let (evidence, cause) = match lockout::load(&path, clock.as_ref(), &journal) {
        lockout::Loaded::Lockout(evidence, cause) => (evidence, cause),
        _ => panic!("expected lockout"),
    };
    let core = Core {
        lockout: Some(lockout::HeldLockout::saved(evidence.clone(), cause)),
        ..Core::fixture(1)
    };
    let response = core.refusal(0).unwrap().to_string();
    let actual = (
        evidence.signal,
        evidence.original_error_text,
        response.contains(expected),
        response.contains(&*path.to_string_lossy()),
    );
    assert_eq!(
        actual,
        (expected.into(), expected.into(), true, false),
        "#161: state-file evidence and the first refusal give the case and OS category without an owner path"
    );
}

async fn assert_compatibility_lockout(
    check_answers: Vec<serde_json::Value>,
    stderr: &str,
    signal: &str,
    original: &str,
) {
    use crate::{Launched, ProcessLauncher};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Launcher {
        check_answers: Vec<serde_json::Value>,
        stderr: Vec<u8>,
        starts: AtomicUsize,
        checks: AtomicUsize,
        check_stops: Arc<AtomicUsize>,
    }
    impl ProcessLauncher for Launcher {
        fn launch(&self, spec: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
            let check = spec
                .program
                .parent()
                .unwrap()
                .ends_with("official-runtime-check");
            let answers = if check {
                assert_eq!(
                    self.checks.fetch_add(1, Ordering::SeqCst),
                    0,
                    "one check start cap"
                );
                self.check_answers.clone()
            } else {
                assert!(
                    self.starts.fetch_add(1, Ordering::SeqCst) < 2,
                    "one initial start and one erroneous restoration cap"
                );
                vec![
                    serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"agentInfo":{"version":"1.4.0"}}}),
                    serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"serving","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
                ]
            };
            assert!(answers.len() <= 4, "four ACP messages per process cap");
            let stdout = answers
                .iter()
                .map(|answer| format!("{answer}\n"))
                .collect::<String>();
            let stderr = if check {
                self.stderr.clone()
            } else {
                Vec::new()
            };
            assert!(stdout.len() + stderr.len() <= 4096, "four KiB output cap");
            Ok(Launched {
                stdin: Box::pin(tokio::io::sink()),
                stdout: Box::pin(Output(std::io::Cursor::new(stdout.into_bytes()))),
                stderr: Box::pin(std::io::Cursor::new(stderr)),
                tree: Box::new(StoppedTree(if check {
                    self.check_stops.clone()
                } else {
                    Arc::new(AtomicUsize::new(0))
                })),
            })
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let launcher = Arc::new(Launcher { check_answers, stderr: stderr.as_bytes().to_vec(), starts: AtomicUsize::new(0), checks: AtomicUsize::new(0), check_stops: Arc::new(AtomicUsize::new(0)) });
        let mut ports = crate::Ports::production();
        ports.launcher = launcher.clone();
        let (stopped, stop) = watch::channel(false);
        let (root, conductor, task) = check_fixture(ports, stop).await;
        let identity = conductor.builds_summary().unwrap().found[0].identity.clone();
        let check_result = conductor.builds(crate::runtime_builds::Action::Check(identity), Hold::empty()).await;
        let owner_lockout = matches!(check_result, Err(LaunchFailure::Owner(OwnerMessage::Api(crate::messages::Message::Lockout(_))))) && check_result.as_ref().is_err_and(|error| { let text = error.to_string(); text.starts_with("acp2api: lockout since ") && !text.contains("compatibility check failed") && !text.contains("Original error:") });
        let evidence = crate::store::StateFile::read(&root.join(crate::store::STATE_FILE)).unwrap().lockout.map(|evidence| (evidence.signal, evidence.original_error_text));
        let refused = conductor.model_names(Hold::empty()).await.err().map(|error| error.message.to_string().starts_with("acp2api: lockout since "));
        let check_removed = !root.join(crate::runtime_builds::CHECK).exists();
        let actual = (conductor.view().state, launcher.starts.load(Ordering::SeqCst), launcher.checks.load(Ordering::SeqCst), launcher.check_stops.load(Ordering::SeqCst), evidence, refused, owner_lockout, check_removed);
        stopped.send(true).unwrap();
        task.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(actual, (State::Lockout, 1, 1, 1, Some((signal.into(), original.into())), Some(true), true, true), "FR-034/FR-035: a check lockout persists redacted evidence, prevents restoration, and ends the owner request with only M-12, and removes the check folder");
    }).await.expect("compatibility lockout cap: five seconds, three processes, four ACP messages and four KiB output per process");
}

#[tokio::test]
async fn an_initialize_lockout_stops_without_replacement_and_persists_evidence() {
    assert_start_lockout(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":403,\"message\":\"Account name%40example.com suspended; appeal https://example.com/appeal\"}}\n",
        "",
        "acp2api: upstream ACP error 403: Account <REDACTED> suspended; appeal https://example.com/appeal",
    ).await;
}

#[tokio::test]
async fn a_session_new_lockout_stops_without_replacement_and_persists_evidence() {
    assert_start_lockout(
        concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"agentInfo\":{\"version\":\"1.4.0\"}}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32000,\"message\":\"Authentication required: 403 Terms of Service\"}}\n",
        ),
        "",
        "acp2api: upstream ACP error -32000: Authentication required: 403 Terms of Service",
    ).await;
}

#[tokio::test]
async fn stderr_during_initialize_starts_lockout_without_replacement() {
    assert_start_lockout(
        "",
        concat!(
            "Antigravity SDK error processing prompt for session startup\n",
            "Error 403, Message: Account name%40example.com suspended, Status: PERMISSION_DENIED, Details: []\n",
        ),
        "acp2api: upstream error 403 PERMISSION_DENIED: Account <REDACTED> suspended",
    ).await;
}

#[tokio::test]
async fn stderr_during_session_new_starts_lockout_without_replacement() {
    assert_start_lockout(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"agentInfo\":{\"version\":\"1.4.0\"}}}\n",
        concat!(
            "Antigravity SDK error processing prompt for session startup\n",
            "Error 403, Message: Verify your account, Status: PERMISSION_DENIED, Details: []\n",
        ),
        "acp2api: upstream error 403 PERMISSION_DENIED: Verify your account",
    )
    .await;
}

#[tokio::test]
async fn a_buffered_stderr_lockout_wins_over_a_startup_acp_failure() {
    assert_start_lockout(
        concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"agentInfo\":{\"version\":\"1.4.0\"}}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32603,\"message\":\"Internal error\"}}\n",
        ),
        concat!(
            "Antigravity SDK error processing prompt for session startup\n",
            "Error 403, Message: Verify your account, Status: PERMISSION_DENIED, Details: []\n",
        ),
        "acp2api: upstream error 403 PERMISSION_DENIED: Verify your account",
    ).await;
}

#[test]
fn a_damaged_state_file_lockout_names_the_case_and_category_without_a_path() {
    let (folder, expected) =
        crate::store::fixtures::state_file(crate::store::fixtures::StateFileFault::Damaged);
    assert_state_file_diagnostic(&folder, &expected);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn an_unreadable_state_file_lockout_names_the_case_and_category_without_a_path() {
    let (folder, expected) =
        crate::store::fixtures::state_file(crate::store::fixtures::StateFileFault::Unreadable);
    assert_state_file_diagnostic(&folder, &expected);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_503_with_order_14030_in_the_error_does_not_start_lockout() {
    let (core, _, folder) = error_after_reply(
        "",
        crate::messages::HttpError {
            code: Some(503),
            status: "UNAVAILABLE".into(),
            message: "order 14030".into(),
        },
    );
    let state = core.state();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        state,
        State::Running,
        "FR-034: a longer number in an error is not an HTTP status"
    );
}

#[test]
fn a_429_with_7f403a_in_the_error_starts_quota_wait() {
    let (core, _, folder) = error_after_reply(
        "",
        crate::messages::HttpError {
            code: Some(429),
            status: "RESOURCE_EXHAUSTED".into(),
            message: "request 7f403a; resets 1970-01-01T00:02:00Z".into(),
        },
    );
    let state = core.state();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        state,
        State::QuotaWait,
        "FR-034: an identifier in the error is not an HTTP status"
    );
}

#[tokio::test]
async fn a_failed_reply_classifies_terms_without_copying_private_text() {
    let (mut core, commands, folder) = error_after_reply(
        "Terms of Service: PRIVATE-REPLY my small secret",
        crate::messages::HttpError {
            code: Some(400),
            status: "INVALID_ARGUMENT".into(),
            message: "Account name%40example.com failed".into(),
        },
    );
    let first = commands
        .iter()
        .find_map(|command| match command {
            Command::Emit(_, TurnEvent::End(Err(error)))
                if error.to_string().contains(" Original error: ") =>
            {
                Some(error.to_string())
            }
            _ => None,
        })
        .expect("the signalling response has the Google error");
    let journal =
        crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
    let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
    Conductor::commands(
        &mut core,
        &mut None,
        &mut false,
        &clock,
        &journal,
        &mut commands.into(),
    )
    .await;
    let state_text = std::fs::read_to_string(&core.state_path).unwrap();
    let state: serde_json::Value = serde_json::from_str(&state_text).unwrap();
    let original = state["lockout"]["original_error_text"]
        .as_str()
        .unwrap()
        .to_owned();
    let response_original = first.split_once(" Original error: ").unwrap().1.to_owned();
    std::fs::remove_dir_all(folder).unwrap();
    let expected =
        "acp2api: upstream error 400 INVALID_ARGUMENT: Account <REDACTED> failed".to_owned();
    assert_eq!(
        (original, response_original),
        (expected.clone(), expected),
        "FR-035/FR-056: evidence and the first response contain only the redacted error channel"
    );
    assert!(!state_text.contains("PRIVATE-REPLY"));
}

#[test]
fn a_stderr_failed_reply_uses_all_lockout_signals_but_only_error_channel_evidence() {
    for (reply, signal) in [
        (
            "PERMISSION_DENIED: Verify your account PRIVATE-REPLY",
            Some("PERMISSION_DENIED"),
        ),
        ("Error 403. PRIVATE-REPLY", Some("403")),
        ("Terms of Service PRIVATE-REPLY", Some("Terms of Service")),
        ("backend latency 1.403 s", None),
        ("v2.403", None),
        ("req-403", None),
        ("order 14030", None),
    ] {
        let (core, commands, folder) = error_after_reply(
            reply,
            crate::messages::HttpError {
                code: Some(400),
                status: "INVALID_ARGUMENT".into(),
                message: "Account name%40example.com failed".into(),
            },
        );
        let evidence = core.lockout.as_ref().map(|held| {
            (
                held.evidence().signal.clone(),
                held.evidence().original_error_text.clone(),
            )
        });
        let response = commands.iter().find_map(|command| match command {
            Command::Emit(_, TurnEvent::End(Err(error))) => error
                .to_string()
                .split_once(" Original error: ")
                .map(|(_, original)| original.to_owned()),
            _ => None,
        });
        std::fs::remove_dir_all(folder).unwrap();
        let original = "acp2api: upstream error 400 INVALID_ARGUMENT: Account <REDACTED> failed";
        assert_eq!(
            (evidence, response),
            (
                signal.map(|signal| (signal.to_owned(), original.to_owned())),
                signal.map(|_| original.to_owned())
            ),
            "FR-034 classifies the failed reply; FR-035/FR-056 keep only the error channel: {reply}"
        );
    }
}

#[tokio::test]
async fn a_lockout_answers_the_waiting_and_queued_build_changes() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-lockout-owner-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
    let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.state_path = folder.join(crate::store::STATE_FILE);
    let (waiting, mut waiting_reply) = oneshot::channel();
    let (queued, mut queued_reply) = oneshot::channel();
    core.lifecycle.set_state(RuntimeState::Retiring {
        sign_in_wait_ended: false,
    });
    core.lifecycle.plan(Replacement::Build {
        reply: waiting,
        source: None,
        initial: false,
    });
    core.lifecycle
        .queue(crate::runtime_builds::Action::Search, queued);
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: crate::messages::HttpError {
                code: Some(403),
                status: "PERMISSION_DENIED".into(),
                message: "Account suspended".into(),
            },
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    Conductor::commands(
        &mut core,
        &mut None,
        &mut false,
        &clock,
        &journal,
        &mut commands.into(),
    )
    .await;
    let responses = [waiting_reply.try_recv(), queued_reply.try_recv()];
    std::fs::remove_dir_all(folder).unwrap();
    for response in responses {
        assert!(
            matches!(response, Ok(Err(ref error)) if error.to_string().starts_with("acp2api: lockout since ") && !error.to_string().contains("Original error:")),
            "a waiting owner request receives M-12: {response:?}"
        );
    }
}

#[tokio::test]
async fn a_failed_lockout_save_is_journaled_and_keeps_lockout_in_memory() {
    let (mut core, commands, folder) = error_after_reply(
        "",
        crate::messages::HttpError {
            code: Some(403),
            status: "PERMISSION_DENIED".into(),
            message: "Account suspended".into(),
        },
    );
    std::fs::create_dir(&core.state_path).unwrap();
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
    let records: Vec<serde_json::Value> = std::fs::read_dir(folder.join("logs"))
        .unwrap()
        .flat_map(|entry| {
            std::fs::read_to_string(entry.unwrap().path())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect::<Vec<_>>()
        })
        .collect();
    let state = core.state();
    let later = core.refusal(1).unwrap().to_string();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(state, State::Lockout);
    assert!(later.starts_with("acp2api: lockout since "));
    assert!(
        records
            .iter()
            .any(|record| record["event"] == "state_file_read_failed"
                && record["operation"] == "lockout"),
        "a failed lockout persistence is recorded in the journal: {records:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn state_file_failures_log_only_the_os_category_and_the_failed_access() {
    use std::os::unix::fs::PermissionsExt;
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for access in ["read", "write"] {
        let (mut core, commands, folder) = error_after_reply(
            "",
            crate::messages::HttpError {
                code: Some(403),
                status: "PERMISSION_DENIED".into(),
                message: "Account suspended".into(),
            },
        );
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
        let category = if access == "read" {
            core.state_path = folder.join(r"C:\Users\PRIVATE-OWNER\state.json");
            std::fs::create_dir(&core.state_path).unwrap();
            std::fs::read(&core.state_path).unwrap_err().kind()
        } else {
            std::fs::write(
                &core.state_path,
                br#"{"format":1,"lockout":null,"checked_builds":[]}"#,
            )
            .unwrap();
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
            std::io::ErrorKind::PermissionDenied
        };
        Conductor::commands(
            &mut core,
            &mut None,
            &mut false,
            &clock,
            &journal,
            &mut commands.into(),
        )
        .await;
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        for entry in std::fs::read_dir(folder.join("logs")).unwrap() {
            for line in std::fs::read_to_string(entry.unwrap().path())
                .unwrap()
                .lines()
            {
                let mut record: serde_json::Value = serde_json::from_str(line).unwrap();
                if record["event"]
                    .as_str()
                    .is_some_and(|event| event.starts_with("state_file_"))
                {
                    record.as_object_mut().unwrap().remove("time");
                    actual.push(record);
                }
            }
        }
        std::fs::remove_dir_all(folder).unwrap();
        expected.push(serde_json::json!({
            "event": format!("state_file_{access}_failed"),
            "operation": "lockout", "cause": format!("{category:?}"),
        }));
    }
    assert_eq!(
        actual, expected,
        "FR-056 logs the OS category without an owner path"
    );
}

#[test]
fn lockout_records_state_and_cancels_and_persists_before_any_response_ends() {
    let (_, commands, folder) = error_after_reply(
        "",
        crate::messages::HttpError {
            code: Some(403),
            status: "PERMISSION_DENIED".into(),
            message: "Account suspended".into(),
        },
    );
    let state = commands
        .iter()
        .position(|command| matches!(command, Command::State(State::Lockout, ..)))
        .unwrap();
    let cancel = commands
        .iter()
        .position(|command| matches!(command, Command::Cancel(_)))
        .unwrap();
    let persist_and_stop = commands
        .iter()
        .position(|command| matches!(command, Command::Lockout(_)))
        .unwrap();
    let first_end = commands
        .iter()
        .position(|command| matches!(command, Command::Emit(_, TurnEvent::End(_))))
        .unwrap();
    let state_notices = commands
        .iter()
        .filter(|command| matches!(command, Command::State(State::Lockout, ..)))
        .count();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(state_notices, 1);
    assert!(
        state < first_end && cancel < persist_and_stop && persist_and_stop < first_end,
        "FR-035 and R1 require the state, cancellation, persistence, and runtime stop before a response ends"
    );
}

#[tokio::test]
async fn a_lockout_while_restoring_after_a_check_returns_only_m12() {
    struct Launcher {
        starts: AtomicUsize,
        checks: AtomicUsize,
        stops: Arc<AtomicUsize>,
    }
    impl crate::ProcessLauncher for Launcher {
        fn launch(&self, spec: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
            let stdout: crate::ProcessRead = if spec
                .program
                .parent()
                .unwrap()
                .ends_with(crate::runtime_builds::CHECK)
            {
                assert_eq!(
                    self.checks.fetch_add(1, Ordering::SeqCst),
                    0,
                    "one check start cap"
                );
                Box::pin(tokio::io::empty())
            } else {
                let start = self.starts.fetch_add(1, Ordering::SeqCst);
                assert!(start < 2, "one initial start and one restoration cap");
                let answers = if start == 0 {
                    vec![
                        serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"agentInfo":{"version":"1.4.0"}}}),
                        serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"serving","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
                    ]
                } else {
                    vec![
                        serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":403,"message":"Verify your account"}}),
                    ]
                };
                let output = answers
                    .iter()
                    .map(|answer| format!("{answer}\n"))
                    .collect::<String>();
                assert!(output.len() <= 1024, "one KiB output cap");
                Box::pin(Output(std::io::Cursor::new(output.into_bytes())))
            };
            Ok(crate::Launched {
                stdin: Box::pin(tokio::io::sink()),
                stdout,
                stderr: Box::pin(tokio::io::empty()),
                tree: Box::new(StoppedTree(self.stops.clone())),
            })
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let launcher = Arc::new(Launcher { starts: AtomicUsize::new(0), checks: AtomicUsize::new(0), stops: Arc::new(AtomicUsize::new(0)) });
        let mut ports = crate::Ports::production();
        ports.launcher = launcher.clone();
        let (stop, stopped) = watch::channel(false);
        let (root, conductor, task) = check_fixture(ports, stopped).await;
        let identity = conductor.builds_summary().unwrap().found[0].identity.clone();
        let owner = conductor.builds(crate::runtime_builds::Action::Check(identity), Hold::empty()).await;
        let owner_lockout = matches!(owner, Err(LaunchFailure::Owner(OwnerMessage::Api(crate::messages::Message::Lockout(_))))) && owner.as_ref().is_err_and(|error| { let text = error.to_string(); text.starts_with("acp2api: lockout since ") && !text.contains("compatibility check") && !text.contains("Original error:") });
        let evidence = crate::store::StateFile::read(&root.join(crate::store::STATE_FILE)).unwrap().lockout.map(|lockout| lockout.signal);
        let actual = (conductor.view().state, owner_lockout, launcher.starts.load(Ordering::SeqCst), launcher.checks.load(Ordering::SeqCst), launcher.stops.load(Ordering::SeqCst), evidence);
        stop.send(true).unwrap();
        task.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(actual, (State::Lockout, true, 2, 1, 3, Some("403".into())), "FR-035: lockout during restoration replaces the check notice with only M-12 for its waiting owner");
    }).await.expect("check restoration lockout cap: five seconds, three processes, at most two ACP replies and one KiB output per process");
}

#[tokio::test]
async fn a_lockout_during_an_owner_build_restart_returns_only_m12() {
    struct Launcher {
        starts: AtomicUsize,
        stops: Arc<AtomicUsize>,
    }
    impl crate::ProcessLauncher for Launcher {
        fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
            let start = self.starts.fetch_add(1, Ordering::SeqCst);
            assert!(start < 2, "one initial start and one owner restart cap");
            let answers = if start == 0 {
                vec![
                    serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"agentInfo":{"version":"1.4.0"}}}),
                    serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"serving","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
                ]
            } else {
                vec![
                    serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":403,"message":"Verify your account"}}),
                ]
            };
            let stdout = answers
                .iter()
                .map(|answer| format!("{answer}\n"))
                .collect::<String>();
            assert!(stdout.len() <= 1024, "one KiB output cap");
            Ok(crate::Launched {
                stdin: Box::pin(tokio::io::sink()),
                stdout: Box::pin(Output(std::io::Cursor::new(stdout.into_bytes()))),
                stderr: Box::pin(tokio::io::empty()),
                tree: Box::new(StoppedTree(self.stops.clone())),
            })
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let launcher = Arc::new(Launcher { starts: AtomicUsize::new(0), stops: Arc::new(AtomicUsize::new(0)) });
        let mut ports = crate::Ports::production();
        ports.launcher = launcher.clone();
        let (stop, stopped) = watch::channel(false);
        let (root, conductor, task) = check_fixture(ports, stopped).await;
        let choice = conductor.builds_summary().unwrap().choices[0].0.clone();
        let owner = conductor.builds(crate::runtime_builds::Action::Build(choice), Hold::empty()).await;
        let owner_lockout = matches!(owner, Err(LaunchFailure::Owner(OwnerMessage::Api(crate::messages::Message::Lockout(_))))) && owner.as_ref().is_err_and(|error| { let text = error.to_string(); text.starts_with("acp2api: lockout since ") && !text.contains("Original error:") });
        let actual = (conductor.view().state, owner_lockout, launcher.starts.load(Ordering::SeqCst), launcher.stops.load(Ordering::SeqCst));
        stop.send(true).unwrap();
        task.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(actual, (State::Lockout, true, 2, 2), "FR-035/#70: lockout while restarting an owner-selected build must end that owner request with only M-12");
    }).await.expect("owner restart lockout cap: five seconds, two starts, at most two ACP answers and one KiB output per start");
}

#[tokio::test]
async fn a_compatibility_prompt_acp_lockout_prevents_restoration() {
    assert_compatibility_lockout(
        vec![
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"mcpCapabilities":{"http":true},"_meta":{"antigravity":{"systemPrompt":{"append":true}}}},"authMethods":[{"id":"oauth-personal","name":"Google"}],"agentInfo":{"version":"1.6.0"}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"checked","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
            serde_json::json!({"jsonrpc":"2.0","id":3,"error":{"code":-32603,"message":"PERMISSION_DENIED: Account name%40example.com; Bearer abcdefghijklmnopqrstuvwxyz012345; appeal https://example.com/appeal"}}),
        ],
        "",
        "PERMISSION_DENIED",
        "acp2api: upstream ACP error -32603: PERMISSION_DENIED: Account <REDACTED>; Bearer <REDACTED>; appeal https://example.com/appeal",
    ).await;
}

#[tokio::test]
async fn a_compatibility_prompt_stderr_lockout_prevents_restoration() {
    assert_compatibility_lockout(
        vec![
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"mcpCapabilities":{"http":true},"_meta":{"antigravity":{"systemPrompt":{"append":true}}}},"authMethods":[{"id":"oauth-personal","name":"Google"}],"agentInfo":{"version":"1.6.0"}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"checked","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
        ],
        concat!(
            "Antigravity SDK error processing prompt for session checked\n",
            "Error 403, Message: Account name%40example.com; Bearer abcdefghijklmnopqrstuvwxyz012345; appeal https://example.com/appeal, Status: PERMISSION_DENIED, Details: []\n",
        ),
        "403",
        "acp2api: upstream error 403 PERMISSION_DENIED: Account <REDACTED>; Bearer <REDACTED>; appeal https://example.com/appeal",
    ).await;
}

#[tokio::test]
async fn a_prompt_end_before_stderr_lockout_prevents_check_restoration() {
    assert_compatibility_lockout(
        vec![
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"mcpCapabilities":{"http":true},"_meta":{"antigravity":{"systemPrompt":{"append":true}}}},"authMethods":[{"id":"oauth-personal","name":"Google"}],"agentInfo":{"version":"1.6.0"}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"checked","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
            serde_json::json!({"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}),
        ],
        concat!(
            "Antigravity SDK error processing prompt for session checked\n",
            "Error 403, Message: Account name%40example.com; Bearer abcdefghijklmnopqrstuvwxyz012345; appeal https://example.com/appeal, Status: PERMISSION_DENIED, Details: []\n",
        ),
        "403",
        "acp2api: upstream error 403 PERMISSION_DENIED: Account <REDACTED>; Bearer <REDACTED>; appeal https://example.com/appeal",
    ).await;
}

#[tokio::test]
async fn a_failed_check_reply_signals_lockout_without_persisting_private_text() {
    assert_compatibility_lockout(
        vec![
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"mcpCapabilities":{"http":true},"_meta":{"antigravity":{"systemPrompt":{"append":true}}}},"authMethods":[{"id":"oauth-personal","name":"Google"}],"agentInfo":{"version":"1.6.0"}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"checked","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
            serde_json::json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"checked","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"PRIVATE-REPLY Terms of Service"}}}}),
            serde_json::json!({"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}),
        ],
        concat!(
            "Antigravity SDK error processing prompt for session checked\n",
            "Error 503, Message: Service unavailable, Status: UNAVAILABLE, Details: []\n",
        ),
        "Terms of Service",
        "acp2api: upstream error 503 UNAVAILABLE: Service unavailable",
    ).await;
}

#[test]
fn the_end_of_a_lockout_restarts_the_runtime_and_clears_the_file() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-end-lockout-{}",
        crate::random_hex::<12>().unwrap()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let state_path = folder.join(crate::store::STATE_FILE);
    let clock = crate::Ports::production().clock;
    let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();

    let hash = "a".repeat(64);
    let saved = serde_json::json!({
        "format": 1,
        "lockout": {
            "since": "2026-10-18T12:00:00Z",
            "signal": "403",
            "original_error_text": "Account suspended",
        },
        "checked_builds": [{
            "version": "1.6.0",
            "platform": "macos-arm64",
            "agy_acp_server_sha256": hash,
            "localharness_external_sha256": hash,
            "checked_at": "2026-10-18T12:00:00Z",
        }]
    });
    std::fs::write(&state_path, saved.to_string()).unwrap();

    let (evidence, cause) = match lockout::load(&state_path, clock.as_ref(), &journal) {
        lockout::Loaded::Lockout(evidence, cause) => (evidence, cause),
        _ => panic!("expected lockout"),
    };
    let mut core = Core {
        state_path: state_path.clone(),
        lockout: Some(lockout::HeldLockout::saved(evidence, cause)),
        lifecycle: Lifecycle::new(RuntimeState::Stopped),
        ..Core::fixture(1)
    };

    let commands = core.handle(Event::EndLockout, 0, std::time::UNIX_EPOCH);
    assert_eq!(core.state(), State::Running);
    assert!(matches!(core.lifecycle.state(), RuntimeState::Restarting));
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::State(State::Running, ..)))
    );

    lockout::clear(&state_path).unwrap();
    let state = crate::store::StateFile::read(&state_path).unwrap();
    assert!(state.lockout.is_none());
    assert_eq!(state.checked_builds.len(), 1);
    assert_eq!(state.checked_builds[0].build.version, "1.6.0");

    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn old_runtime_failures_do_not_stop_the_start_after_a_lockout() {
    let evidence = crate::store::Lockout {
        since: "2026-10-18T12:00:00Z".into(),
        signal: "403".into(),
        original_error_text: "Account suspended".into(),
    };

    let mut core = Core::fixture(1);
    core.handle(Event::RuntimeFailure, 0, std::time::UNIX_EPOCH);
    core.handle(Event::RuntimeFailure, 1, std::time::UNIX_EPOCH);
    core.handle(Event::RuntimeFailure, 2, std::time::UNIX_EPOCH);
    core.lockout = Some(lockout::HeldLockout::saved(
        evidence.clone(),
        LockoutCause::Signal,
    ));
    core.handle(
        Event::EndLockout,
        RUNTIME_FAILURE_WINDOW + 3,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(core.lifecycle.failure_count(), 0);

    let mut core = Core::fixture(1);
    core.handle(Event::RuntimeFailure, 0, std::time::UNIX_EPOCH);
    core.handle(Event::RuntimeFailure, 1, std::time::UNIX_EPOCH);
    core.handle(
        Event::RuntimeFailure,
        RUNTIME_FAILURE_WINDOW + 1,
        std::time::UNIX_EPOCH,
    );
    core.lockout = Some(lockout::HeldLockout::saved(evidence, LockoutCause::Signal));
    core.handle(
        Event::EndLockout,
        RUNTIME_FAILURE_WINDOW + 3,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(core.lifecycle.failure_count(), 1);
}

#[tokio::test]
async fn a_lockout_stays_when_its_end_cannot_be_written() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-lockout-stays-{}",
        crate::random_hex::<12>().unwrap()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let clock = crate::Ports::production().clock;
    let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();

    let evidence = crate::store::Lockout {
        since: "2026-10-18T12:00:00Z".into(),
        signal: "403".into(),
        original_error_text: "Account suspended".into(),
    };

    // Row 1: state_path is a folder, so that the save fails
    let folder_path = folder.join("state-is-a-folder");
    std::fs::create_dir_all(&folder_path).unwrap();
    let mut core = Core {
        state_path: folder_path,
        lockout: Some(lockout::HeldLockout::saved(
            evidence.clone(),
            LockoutCause::Signal,
        )),
        ..Core::fixture(1)
    };
    let (reply, answer) = oneshot::channel();
    let mut batch = VecDeque::from([Command::ClearLockout(reply)]);
    Conductor::commands(
        &mut core, &mut None, &mut false, &clock, &journal, &mut batch,
    )
    .await;
    assert_eq!(core.state(), State::Lockout);
    assert!(matches!(
        answer.await.unwrap(),
        Err(OwnerMessage::LockoutEndNotSaved)
    ));

    // Row 2: writable path: State::Running, and Ok(())
    let file_path = folder.join(crate::store::STATE_FILE);
    let mut core = Core {
        state_path: file_path,
        lockout: Some(lockout::HeldLockout::saved(evidence, LockoutCause::Signal)),
        ..Core::fixture(1)
    };
    let (reply, answer) = oneshot::channel();
    let mut batch = VecDeque::from([Command::ClearLockout(reply)]);
    Conductor::commands(
        &mut core, &mut None, &mut false, &clock, &journal, &mut batch,
    )
    .await;
    assert_eq!(core.state(), State::Running);
    assert!(matches!(answer.await.unwrap(), Ok(())));

    std::fs::remove_dir_all(folder).unwrap();
}

fn test_environment(prefix: &str) -> (PathBuf, PathBuf, Arc<dyn Clock>, crate::journal::Journal) {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-{prefix}-{}",
        crate::random_hex::<12>().unwrap()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let state_path = folder.join(crate::store::STATE_FILE);
    let clock = crate::Ports::production().clock;
    let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
    (folder, state_path, clock, journal)
}

fn unavailable_state_file_lifecycle() -> Lifecycle {
    let mut lifecycle = Lifecycle::new(RuntimeState::Unavailable(UnavailableReason {
        message: crate::messages::Unavailable {
            fault: RuntimeFault::StateFileNotWritable,
            versions: "1.4.0".into(),
            port: 8329,
        },
        cause: Some(std::io::ErrorKind::PermissionDenied),
    }));
    lifecycle.defer_start();
    lifecycle
}

#[tokio::test]
async fn next_build_change_returns_the_queued_change_after_a_failed_start() {
    let (folder, state_path, clock, journal) = test_environment("deferred-start-failed");
    crate::store::StateFile::normal().save(&state_path).unwrap();

    // Make official-runtime/server a directory so hashes fails on initial start
    std::fs::create_dir_all(
        folder
            .join(crate::runtime_builds::COPY)
            .join(crate::runtime_builds::SERVER),
    )
    .unwrap();

    let mut core = Core {
        state_path,
        lifecycle: unavailable_state_file_lifecycle(),
        ..Core::fixture(1)
    };

    let target_path = folder.join("custom-runtime");
    let (reply, rx) = oneshot::channel();
    let commands = core.build_input(
        crate::runtime_builds::Action::Path(target_path),
        reply,
        clock.as_ref(),
        &journal,
    );
    assert!(commands.is_empty());

    let builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
    let (publish, _) = tokio::sync::watch::channel(Arc::new(crate::store::Settings {
        port: 8329,
        concurrency_limit: 1,
        local_api_key: String::new(),
        management_password_hash: String::new(),
        agy_acp_server_path: None,
    }));
    let configuration = folder.join("config.toml");

    let first = core
        .next_build_change(&builds, &publish, &configuration)
        .await;
    let Some(BuildSelection::StartFailed(error)) = first else {
        panic!("expected StartFailed");
    };

    let mut runtime = None;
    let mut read_open = false;
    core.deferred_start_failed(error, &mut runtime, &mut read_open, &journal, &builds)
        .await;

    let second = core
        .next_build_change(&builds, &publish, &configuration)
        .await;
    assert!(
        second.is_none(),
        "next_build_change processes the queued change without conductor action"
    );

    let reply_result = rx.await.unwrap();
    assert!(
        matches!(
            reply_result,
            Ok(Some(crate::messages::OwnerMessage::PathWithoutHarness))
        ),
        "the queued action receives its reply"
    );

    std::fs::remove_dir_all(folder).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_queued_search_replies_before_the_deadline_after_a_failed_deferred_start() {
    use std::os::unix::fs::PermissionsExt;
    let (folder, state_path, clock, journal) = test_environment("deferred-start-failed-loop");
    let builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
    let configuration = folder.join("config.toml");
    let settings = crate::store::read(&configuration, 1).unwrap().settings;
    assert!(!state_path.exists());

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
    let (conductor, task) = Conductor::start(
        builds,
        clock.clone(),
        journal,
        format!("http://127.0.0.1:{}/", settings.port),
        settings,
        configuration,
        1,
    );
    conductor
        .builds(crate::runtime_builds::Action::Initial, Hold::empty())
        .await
        .unwrap();

    assert_eq!(conductor.view().state, State::RuntimeUnavailable);

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::create_dir_all(
        folder
            .join(crate::runtime_builds::COPY)
            .join(crate::runtime_builds::SERVER),
    )
    .unwrap();

    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        conductor.builds(crate::runtime_builds::Action::Search, Hold::empty()),
    )
    .await;

    assert!(
        reply.is_ok(),
        "FR-040: the search reply arrives before the deadline after a deferred initial start fails"
    );

    let unavailable = conductor
        .model_names(Hold::empty())
        .await
        .err()
        .map(|error| error.message.to_string())
        .unwrap_or_default();
    assert!(
        unavailable.contains("acp2api: the official runtime is unavailable:"),
        "FR-040: the runtime remains unavailable with an FR-040 message"
    );

    drop(conductor);
    task.await.unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn after_a_retry_that_finds_the_state_file_normal_state_is_still_runtime_unavailable_until_runtime_ready()
 {
    let (folder, state_path, clock, journal) = test_environment("retry-normal");

    crate::store::StateFile::normal().save(&state_path).unwrap();

    let mut core = Core {
        state_path,
        lifecycle: unavailable_state_file_lifecycle(),
        ..Core::fixture(1)
    };

    let (reply, _rx) = oneshot::channel();
    let commands = core.build_input(
        crate::runtime_builds::Action::Search,
        reply,
        clock.as_ref(),
        &journal,
    );
    assert!(commands.is_empty());
    assert_eq!(
        core.state(),
        State::RuntimeUnavailable,
        "FR-040: after a retry that finds the state file normal, state() is still RuntimeUnavailable until runtime_ready"
    );

    core.handle(
        Event::RuntimeReady {
            sign_in_required: false,
        },
        clock.now(),
        clock.wall(),
    );
    assert_eq!(
        core.state(),
        State::Running,
        "FR-040: the condition ends when a start succeeds"
    );

    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_retry_leaves_a_state_file_with_other_content_byte_for_byte_unchanged() {
    let (folder, state_path, clock, journal) = test_environment("retry-unchanged");

    let hash = "c".repeat(64);
    let saved = serde_json::json!({
        "format": 1,
        "lockout": null,
        "checked_builds": [{
            "version": "1.7.0",
            "platform": "macos-arm64",
            "agy_acp_server_sha256": hash,
            "localharness_external_sha256": hash,
            "checked_at": "2026-10-18T12:00:00Z"
        }]
    });
    let initial_bytes = serde_json::to_vec(&saved).unwrap();
    std::fs::write(&state_path, &initial_bytes).unwrap();

    let mut core = Core {
        state_path: state_path.clone(),
        lifecycle: unavailable_state_file_lifecycle(),
        ..Core::fixture(1)
    };

    let (reply, _rx) = oneshot::channel();
    core.build_input(
        crate::runtime_builds::Action::Search,
        reply,
        clock.as_ref(),
        &journal,
    );

    let actual_bytes = std::fs::read(&state_path).unwrap();
    assert_eq!(
        actual_bytes, initial_bytes,
        "FR-035: a retry leaves a state file with other content byte for byte unchanged"
    );

    std::fs::remove_dir_all(folder).unwrap();
}

#[tokio::test]
async fn a_state_file_lockout_stays_unsaved_after_the_file_is_repaired() {
    let (folder, state_path, clock, journal) = test_environment("state-file-cause-unsaved");
    std::fs::write(&state_path, b"damaged state file").unwrap();
    let mut core = Core {
        state_path: state_path.clone(),
        lifecycle: unavailable_state_file_lifecycle(),
        ..Core::fixture(1)
    };
    let (reply, _rx) = oneshot::channel();
    core.build_input(
        crate::runtime_builds::Action::Search,
        reply,
        clock.as_ref(),
        &journal,
    );
    crate::store::StateFile::normal().save(&state_path).unwrap();
    let repaired = std::fs::read(&state_path).unwrap();
    core.retry_lockout_save();
    let view = core.view.borrow().clone();
    let unchanged = std::fs::read(&state_path).unwrap() == repaired;
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        (
            view.state,
            view.lockout_cause,
            view.lockout_unsaved,
            unchanged
        ),
        (State::Lockout, Some(LockoutCause::StateFile), false, true),
        "#195: a state-file lockout stays in memory and never overwrites the repaired file"
    );
}

#[tokio::test]
async fn a_runtime_signal_publishes_the_ordinary_cause_after_absence_or_an_owner_end() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        for after_state_file in [false, true] {
            let (folder, state_path, clock, journal) = test_environment("signal-cause");
            let mut core = Core {
                state_path: state_path.clone(),
                lifecycle: unavailable_state_file_lifecycle(),
                ..Core::fixture(1)
            };
            let initial = core.view.borrow().lockout_cause;
            let mut previous = None;
            if after_state_file {
                std::fs::write(&state_path, b"damaged state file").unwrap();
                let (reply, _rx) = oneshot::channel();
                core.build_input(crate::runtime_builds::Action::Search, reply, clock.as_ref(), &journal);
                previous = core.view.borrow().lockout_cause;
                let (reply, answer) = oneshot::channel();
                Conductor::commands(&mut core, &mut None, &mut false, &clock, &journal, &mut VecDeque::from([Command::ClearLockout(reply)])).await;
                answer.await.unwrap().unwrap();
            }
            let before_signal = core.view.borrow().lockout_cause;
            core.apply(
                Event::Runtime(RuntimeEvent::UpstreamError {
                    session: "synthetic-session".into(),
                    error: crate::messages::HttpError {
                        code: Some(403),
                        status: "PERMISSION_DENIED".into(),
                        message: "Synthetic account suspension".into(),
                    },
                }),
                &mut None, &mut false, &clock, &journal,
            ).await;
            let view = core.view.borrow().clone();
            let saved = crate::store::StateFile::read(&state_path).unwrap().lockout.is_some();
            std::fs::remove_dir_all(folder).unwrap();
            assert_eq!(
                (initial, previous, before_signal, view.state, view.lockout_cause, saved),
                (None, after_state_file.then_some(LockoutCause::StateFile), None, State::Lockout, Some(LockoutCause::Signal), true),
                "#195: an owner end clears the cause, and a later runtime signal publishes the ordinary cause"
            );
        }
    }).await.expect("signal cause cap: five seconds, two runtime events, no child processes or requests, existing journal caps");
}

#[tokio::test]
async fn a_retry_that_finds_a_damaged_state_file_publishes_the_lockout_cause() {
    let (folder, state_path, clock, journal) = test_environment("retry-state-file-cause");
    std::fs::write(&state_path, b"damaged state file").unwrap();
    let mut core = Core {
        state_path: state_path.clone(),
        lifecycle: unavailable_state_file_lifecycle(),
        ..Core::fixture(1)
    };
    let (reply, rx) = oneshot::channel();
    core.build_input(
        crate::runtime_builds::Action::Search,
        reply,
        clock.as_ref(),
        &journal,
    );
    let refused = matches!(
        rx.await.unwrap(),
        Err(LaunchFailure::Owner(OwnerMessage::Api(
            crate::messages::Message::Lockout(_)
        )))
    );
    let view = core.view.borrow().clone();
    let bytes = std::fs::read(&state_path).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        (
            view.state,
            view.lockout_cause,
            view.lockout_unsaved,
            refused,
            bytes
        ),
        (
            State::Lockout,
            Some(LockoutCause::StateFile),
            false,
            true,
            b"damaged state file".to_vec()
        ),
        "#195: the retry publishes the state-file cause and preserves the damaged file"
    );
}

#[tokio::test]
async fn a_retry_that_finds_a_stored_lockout_gives_state_lockout_and_queues_initial_on_end() {
    let (folder, state_path, clock, journal) = test_environment("retry-lockout");

    let evidence = crate::store::Lockout {
        since: "2026-10-18T12:00:00Z".into(),
        signal: "403".into(),
        original_error_text: "Account suspended".into(),
    };
    let saved = serde_json::json!({
        "format": 1,
        "lockout": evidence,
        "checked_builds": []
    });
    std::fs::write(&state_path, saved.to_string()).unwrap();

    let mut core = Core {
        state_path: state_path.clone(),
        lifecycle: unavailable_state_file_lifecycle(),
        ..Core::fixture(1)
    };

    let (reply, rx) = oneshot::channel();
    let _commands = core.build_input(
        crate::runtime_builds::Action::Search,
        reply,
        clock.as_ref(),
        &journal,
    );

    assert_eq!(
        (core.state(), core.view.borrow().lockout_cause),
        (State::Lockout, Some(LockoutCause::Signal)),
        "FR-035: a retry that finds a stored lockout gives State::Lockout"
    );
    let error = rx.await.unwrap().unwrap_err();
    assert!(
        matches!(
            &error,
            LaunchFailure::Owner(OwnerMessage::Api(crate::messages::Message::Lockout(msg)))
                if msg.signal == "403"
        ),
        "FR-035: a retry that finds a stored lockout gives a lockout error in the reply"
    );

    core.handle(Event::EndLockout, clock.now(), clock.wall());
    let queued = core.lifecycle.pop_change();
    assert!(
        matches!(queued, Some((crate::runtime_builds::Action::Initial, _))),
        "FR-035: a later end queues Action::Initial"
    );

    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_retry_with_action_path_queues_action_initial_then_that_path() {
    let (folder, state_path, clock, journal) = test_environment("retry-path");

    crate::store::StateFile::normal().save(&state_path).unwrap();

    let mut core = Core {
        state_path,
        lifecycle: unavailable_state_file_lifecycle(),
        ..Core::fixture(1)
    };

    let target_path = folder.join("custom-runtime");
    let (reply, _rx) = oneshot::channel();
    let commands = core.build_input(
        crate::runtime_builds::Action::Path(target_path.clone()),
        reply,
        clock.as_ref(),
        &journal,
    );
    assert!(commands.is_empty());

    let first = core.lifecycle.pop_change();
    let second = core.lifecycle.pop_change();
    assert!(
        matches!(first, Some((crate::runtime_builds::Action::Initial, _))),
        "FR-040: a retry with Action::Path queues Action::Initial first"
    );
    assert!(
        matches!(second, Some((crate::runtime_builds::Action::Path(p), _)) if p == target_path),
        "FR-040: a retry with Action::Path queues that Path second"
    );

    std::fs::remove_dir_all(folder).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_retry_with_an_unwritable_file_replies_with_the_full_m14_text() {
    use std::os::unix::fs::PermissionsExt;
    let (folder, state_path, clock, journal) = test_environment("retry-unwritable");

    let lifecycle = unavailable_state_file_lifecycle();
    let expected_m14 = crate::messages::Message::RuntimeUnavailable(
        lifecycle.state_file_reason().unwrap().clone(),
    )
    .to_string();
    assert!(
        expected_m14.contains("acp2api cannot write its state file state.json"),
        "M-14 text contains 'acp2api cannot write its state file state.json'"
    );

    let mut core = Core {
        state_path: state_path.clone(),
        lifecycle,
        ..Core::fixture(1)
    };

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();

    let (reply, rx) = oneshot::channel();
    let commands = core.build_input(
        crate::runtime_builds::Action::Search,
        reply,
        clock.as_ref(),
        &journal,
    );
    assert!(commands.is_empty());

    let error = rx.await.unwrap().unwrap_err();
    let error_text = match error {
        LaunchFailure::Owner(OwnerMessage::Api(message)) => message.to_string(),
        other => panic!("expected Owner Api message, got {other:?}"),
    };
    assert_eq!(
        error_text, expected_m14,
        "FR-035: a retry with an unwritable file replies with the full M-14 text that has \"acp2api cannot write its state file state.json\""
    );

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[tokio::test]
async fn a_read_failure_at_the_lockout_start_gives_no_notice() {
    let (folder, state_path, clock, journal) = test_environment("read-failure-no-notice");

    std::fs::write(&state_path, b"invalid json content").unwrap();
    let Err(read_error) = crate::store::StateFile::read(&state_path) else {
        panic!("expected read error");
    };
    assert_eq!(read_error.kind(), std::io::ErrorKind::InvalidData);

    let mut core = Core {
        state_path: state_path.clone(),
        ..Core::fixture(1)
    };

    let evidence = crate::store::Lockout {
        since: "2026-10-18T12:00:00Z".into(),
        signal: "403".into(),
        original_error_text: "Account suspended".into(),
    };

    let mut commands = VecDeque::from([Command::Lockout(evidence)]);
    Conductor::commands(
        &mut core,
        &mut None,
        &mut false,
        &clock,
        &journal,
        &mut commands,
    )
    .await;

    assert_eq!(core.state(), State::Lockout);
    assert!(
        !core.view.borrow().lockout_unsaved,
        "FR-035: a read failure at the lockout start gives no notice"
    );

    std::fs::remove_dir_all(folder).unwrap();
}

#[tokio::test]
async fn an_other_read_failure_gives_the_notice_and_a_later_retry_saves_the_lockout() {
    let (folder, state_path, clock, journal) = test_environment("read-failure-directory");

    std::fs::create_dir(&state_path).unwrap();

    let mut core = Core {
        state_path: state_path.clone(),
        ..Core::fixture(1)
    };

    let evidence = crate::store::Lockout {
        since: "2026-10-18T12:00:00Z".into(),
        signal: "403".into(),
        original_error_text: "Account suspended".into(),
    };

    let mut commands = VecDeque::from([Command::Lockout(evidence)]);
    Conductor::commands(
        &mut core,
        &mut None,
        &mut false,
        &clock,
        &journal,
        &mut commands,
    )
    .await;

    assert_eq!(core.state(), State::Lockout);
    assert!(
        core.view.borrow().lockout_unsaved,
        "FR-035: an other read failure gives the notice"
    );

    std::fs::remove_dir(&state_path).unwrap();
    crate::store::StateFile::normal().save(&state_path).unwrap();

    core.retry_lockout_save();

    assert!(
        !core.view.borrow().lockout_unsaved,
        "FR-035: after replacing the directory with a normal file, the retry saves the lockout"
    );
    assert!(
        crate::store::StateFile::read(&state_path)
            .unwrap()
            .lockout
            .is_some(),
        "FR-035: the lockout is saved in the state file"
    );

    std::fs::remove_dir_all(folder).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_write_failure_gives_the_notice_and_a_later_save_clears_it() {
    use std::os::unix::fs::PermissionsExt;
    let (folder, state_path, clock, journal) = test_environment("write-failure-notice");

    crate::store::StateFile::normal().save(&state_path).unwrap();

    let mut core = Core {
        state_path: state_path.clone(),
        ..Core::fixture(1)
    };

    let evidence = crate::store::Lockout {
        since: "2026-10-18T12:00:00Z".into(),
        signal: "403".into(),
        original_error_text: "Account suspended".into(),
    };

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();

    let mut commands = VecDeque::from([Command::Lockout(evidence)]);
    Conductor::commands(
        &mut core,
        &mut None,
        &mut false,
        &clock,
        &journal,
        &mut commands,
    )
    .await;

    assert_eq!(core.state(), State::Lockout);
    assert!(
        core.view.borrow().lockout_unsaved,
        "FR-035: a write failure gives the notice"
    );

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();

    core.retry_lockout_save();

    assert!(
        !core.view.borrow().lockout_unsaved,
        "FR-035: a write failure gives the notice, and a later save clears it"
    );

    std::fs::remove_dir_all(folder).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn an_end_of_an_unsaved_lockout_succeeds_while_the_file_still_cannot_be_written() {
    use std::os::unix::fs::PermissionsExt;
    let (folder, state_path, clock, journal) = test_environment("unsaved-end-succeeds");

    crate::store::StateFile::normal().save(&state_path).unwrap();

    let mut core = Core {
        state_path: state_path.clone(),
        ..Core::fixture(1)
    };

    let evidence = crate::store::Lockout {
        since: "2026-10-18T12:00:00Z".into(),
        signal: "403".into(),
        original_error_text: "Account suspended".into(),
    };

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();

    let mut commands = VecDeque::from([Command::Lockout(evidence)]);
    Conductor::commands(
        &mut core,
        &mut None,
        &mut false,
        &clock,
        &journal,
        &mut commands,
    )
    .await;

    assert_eq!(core.state(), State::Lockout);
    assert!(core.view.borrow().lockout_unsaved);

    let (reply, rx) = oneshot::channel();
    let mut commands = VecDeque::from([Command::ClearLockout(reply)]);
    Conductor::commands(
        &mut core,
        &mut None,
        &mut false,
        &clock,
        &journal,
        &mut commands,
    )
    .await;

    let result = rx.await.unwrap();
    assert!(
        result.is_ok(),
        "FR-035: an end of an unsaved lockout succeeds while the file still cannot be written"
    );
    assert_ne!(
        core.state(),
        State::Lockout,
        "FR-035: an end of an unsaved lockout succeeds while the file still cannot be written"
    );

    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[tokio::test]
async fn a_lockout_can_reach_the_deadline_of_a_stalled_cancel() {
    use crate::runtime_driver::write_tests::{Launcher, Pipe, Recording, TestClock};
    use tokio::io::AsyncWriteExt;
    // One process, four ACP frames, two stderr lines, 16 KiB capture, five real seconds.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let (clock, mut deadlines) = TestClock::new();
        let recording = Recording::new(16 * 1024);
        let (stderr, mut errors) = tokio::io::duplex(1024);
        let stops = Arc::new(AtomicUsize::new(0));
        let replies = [
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"running","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}}),
        ].iter().map(|reply| format!("{reply}\n")).collect::<String>();
        let mut ports = crate::Ports::production();
        ports.clock = clock.clone();
        ports.launcher = Arc::new(Launcher(std::sync::Mutex::new(Some(crate::Launched {
            stdin: Box::pin(Pipe(recording.clone())),
            stdout: Box::pin(Output(std::io::Cursor::new(replies.into_bytes()))),
            stderr: Box::pin(stderr),
            tree: Box::new(StoppedTree(stops.clone())),
        }))));
        let (root, conductor, task) = check_fixture(ports, watch::channel(false).1).await;
        assert!(conductor.model_names(Hold::empty()).await.is_ok());
        let owner = conductor.clone();
        let refreshing = tokio::spawn(async move { owner.owner(OwnerAction::RefreshQuota, Hold::empty()).await });
        loop {
            if String::from_utf8_lossy(&recording.bytes.lock().unwrap()).contains("session/prompt") { break; }
            tokio::task::yield_now().await;
        }
        recording.allow(recording.bytes.lock().unwrap().len());
        while deadlines.try_recv().is_ok() {}
        errors.write_all(b"Antigravity SDK error processing prompt for session running\nError 403, Message: Synthetic account suspension, Status: PERMISSION_DENIED, Details: none\n").await.unwrap();
        assert_eq!(deadlines.recv().await, Some(30_000));
        clock.advance_to(30_000).await;
        assert!(refreshing.await.unwrap().is_err());
        assert!(conductor.model_names(Hold::empty()).await.is_err());
        assert_eq!((conductor.view().state, stops.load(Ordering::SeqCst)), (State::Lockout, 1),
            "lockout must finish a stalled cancel at its write deadline without an incoming hold freezing time");
        drop(conductor);
        task.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }).await.expect("lockout stalled cancel clock cap: 5 s");
}
