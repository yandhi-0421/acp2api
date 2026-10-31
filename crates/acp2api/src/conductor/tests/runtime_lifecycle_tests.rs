use super::*;

#[test]
fn stopping_cancels_a_prompt_still_awaiting_its_quota_end() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-stop-quota-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: quota_error_without_reset_time(),
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    let stopped = core.handle(Event::Stop, 2, std::time::UNIX_EPOCH);
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        stopped
            .iter()
            .filter(|command| matches!(command,
                Command::Cancel(session) if session == "running"
            ))
            .count(),
        1,
        "FR-045 cancels a live prompt even after a quota signal"
    );
    assert!(
        stopped.iter().any(|command| matches!(
            command,
            Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::Stopped)))
        )),
        "FR-045 ends the response with M-15"
    );
}

#[test]
fn stopping_cancels_the_running_usage_prompt_and_answers_its_owner() {
    let mut core = Core {
        sessions: records::AcpSessions::fixture(Vec::new(), Some("usage".into()), 0),
        ..Core::fixture(1)
    };
    let (reply, mut answer) = oneshot::channel();
    let started = core.handle(Event::RefreshQuota(reply), 0, std::time::UNIX_EPOCH);
    assert!(
        started.iter().any(|command| matches!(command,
            Command::Usage { session, .. } if session == "usage"
        )),
        "FR-052 starts the owner refresh as a prompt turn"
    );
    let stopped = core.handle(Event::Stop, 1, std::time::UNIX_EPOCH);
    assert_eq!(
        stopped
            .iter()
            .filter(|command| matches!(command,
                Command::Cancel(session) if session == "usage"
            ))
            .count(),
        1,
        "FR-045 cancels every live prompt, including /usage"
    );
    assert_eq!(
        answer
            .try_recv()
            .map(|result| result.map_err(|error| error.to_string())),
        Ok(Err(crate::messages::Message::Stopped.to_string()))
    );
}

#[tokio::test]
async fn stopping_ends_each_running_and_queued_response_once_with_m15() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-stop-responses-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
    let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
    let (mut core, _, tickets) = prompting_with_waiter(&journal);
    let commands = core.handle(Event::Stop, 1, clock.wall());
    Conductor::commands(
        &mut core,
        &mut None,
        &mut false,
        &clock,
        &journal,
        &mut commands.into(),
    )
    .await;
    let responses: Vec<_> = tickets
        .into_iter()
        .map(|mut ticket| {
            std::iter::from_fn(|| ticket.try_recv().ok())
                .map(|(event, _)| match event {
                    TurnEvent::End(Err(error)) => error.to_string(),
                    _ => "unexpected response event".into(),
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let late = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::Cancelled),
            },
        }),
        2,
        clock.wall(),
    );
    std::fs::remove_dir_all(folder).unwrap();
    let stopped = crate::messages::Message::Stopped.to_string();
    assert_eq!(
        responses,
        [vec![stopped.clone()], vec![stopped]],
        "FR-045 and R3 end both responses once; their relative order is not a contract"
    );
    assert!(
        !late
            .iter()
            .any(|command| matches!(command, Command::Emit(_, _))),
        "a late runtime end sends no second response"
    );
    assert!(core.work.is_empty());
}

#[tokio::test]
async fn crashed_checks_restore_the_accepted_version_without_consuming_its_failure_limit() {
    failed_checks(false, false).await;
}

#[tokio::test]
async fn timed_out_checks_restore_the_accepted_version_without_consuming_its_failure_limit() {
    failed_checks(true, false).await;
}

#[tokio::test]
async fn failures_to_restore_the_accepted_version_count_normally() {
    failed_checks(false, true).await;
}

#[tokio::test]
async fn a_broken_stdin_counts_once_for_a_command_batch_and_later_closed() {
    use crate::{BoxFuture, Launched, ProcessLauncher, ProcessTree};
    use tokio::io::AsyncWriteExt;
    struct Tree;
    impl ProcessTree for Tree {
        fn pid(&self) -> u32 {
            std::process::id()
        }
        fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
            Box::pin(std::future::pending())
        }
        fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
            Box::pin(async {})
        }
    }
    struct Launcher(std::sync::Mutex<Option<Launched>>);
    impl ProcessLauncher for Launcher {
        fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
            Ok(self.0.lock().unwrap().take().expect("one launch cap"))
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let folder = std::env::temp_dir().join(format!("acp2api-cancel-pipe-{}", crate::random_hex::<12>().unwrap()));
        let clock = crate::Ports::production().clock;
        let mut builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
        let (stdin, agent_input) = tokio::io::duplex(65536);
        let (stdout, mut agent_output) = tokio::io::duplex(65536);
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let replies = [serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}}), serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"running","configOptions":options["configOptions"]}})];
        for reply in replies { let line = format!("{reply}\n"); assert!(line.len() <= 65536); agent_output.write_all(line.as_bytes()).await.unwrap(); }
        let permission = serde_json::json!({"jsonrpc":"2.0","id":"permission","method":"session/request_permission","params":{"sessionId":"running","toolCall":{"toolCallId":"call","title":"Run view_file?"},"options":[]}});
        agent_output.write_all(format!("{permission}\n").as_bytes()).await.unwrap();
        builds.start.launcher = Arc::new(Launcher(std::sync::Mutex::new(Some(Launched { stdin:Box::pin(stdin), stdout:Box::pin(stdout), stderr:Box::pin(tokio::io::empty()), tree:Box::new(Tree) }))));
        let (mut runtime, _) = Runtime::start(&builds.start, "1.4.0").await.unwrap();
        let mut agent_input = tokio::io::BufReader::new(agent_input);
        for method in ["initialize", "session/new"] {
            let mut line = String::new();
            tokio::io::AsyncBufReadExt::read_line(&mut agent_input, &mut line).await.unwrap();
            assert_eq!(serde_json::from_str::<serde_json::Value>(&line).unwrap()["method"], method);
        }
        let (RuntimeEvent::Turn { update:TurnUpdate::RefusedPermission { request, .. }, .. }, _hold) = runtime.next().await else { panic!("the fake must send its permission request"); };
        let mut runtime = Some(runtime);
        let mut read_open = true;
        let journal = crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, [ _a, mut b ]) = prompting_with_waiter(&journal);
        let active = core.work[&0].execution().unwrap();
        core.sessions.remember(records::Record { location:records::Location::Unloaded(folder.join("saved-cwd")), session:"saved".into(), record:records::RequestRecord::fixture(crate::conversation::RecordHashes { system:active.record.hashes().system, messages:active.record.hashes().messages.clone() }, active.record.tools(), 0, 86_400_000) });
        core.sessions.replaced(Some("sibling".into()));
        core.handle(Event::Limit(2), 0, std::time::UNIX_EPOCH);
        drop(agent_input);
        let batch = vec![core.sessions.cancel("running".into(), 0), Command::Permission(request)];
        let mut batch: VecDeque<_> = batch.into();
        Conductor::commands(&mut core, &mut runtime, &mut read_open, &clock, &journal, &mut batch).await;
        let (event, _hold) = runtime.as_mut().unwrap().next().await;
        assert!(matches!(event, RuntimeEvent::WriteFailed(_)), "the writer must report the broken pipe even while stdout stays open");
        read_open = false;
        core.apply(Event::Runtime(event), &mut runtime, &mut read_open, &clock, &journal).await;
        assert_eq!(core.failures.len(), 1, "two writes to one closed stdin are one runtime failure");
        let closed = core.handle(Event::Runtime(RuntimeEvent::Closed), 1, std::time::UNIX_EPOCH);
        let deadline = core.handle(Event::Deadline, 30_000, std::time::UNIX_EPOCH);
        assert_eq!(core.failures.len(), 1, "Closed and the old deadline must not count the failed runtime again");
        assert!(closed.is_empty() && deadline.is_empty(), "the failed runtime must emit no second M-20");
        let mut message = None;
        while let Ok((event, _hold)) = b.try_recv() { if let TurnEvent::End(Err(error)) = event { message = Some(error.to_string()); } }
        let retained: Vec<_> = core.sessions.records().iter().map(|record| record.session.as_str()).collect();
        let actual = (message, retained == ["saved"]);
        runtime.take().unwrap().stop().await;
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(actual, (Some("acp2api: the runtime process failed, so acp2api ended the prompt turn.".into()), true), "a closed stdin is a runtime failure before stdout Closed arrives");
    }).await.expect("broken pipe cap: five seconds, one fake launch, three messages of at most 64 KiB");
}

#[tokio::test]
async fn a_failed_initial_copy_succeeds_with_a_prd_unavailable_reason() {
    use crate::{ListedBuild, ProcessLauncher, ReleaseFacts};
    use sha2::{Digest, Sha256};
    struct NoLaunch;
    impl ProcessLauncher for NoLaunch {
        fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
            panic!("a failed first copy must launch no process")
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let folder = std::env::temp_dir().join(format!("acp2api-initial-copy-{}", crate::random_hex::<12>().unwrap()));
        let clock = crate::Ports::production().clock;
        let mut builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
        builds.start.launcher = Arc::new(NoLaunch);
        let hash = crate::hex(&Sha256::digest(b"accepted source"));
        builds.start.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:Facts(Vec::new()).platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
        let source = folder.join("source");
        std::fs::create_dir_all(&source).unwrap();
        for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(source.join(name), b"accepted source").unwrap(); }
        let config = folder.join("config.toml");
        let mut settings = crate::store::read(&config, 1).unwrap().settings;
        settings.agy_acp_server_path = Some(source.join(crate::runtime_builds::SERVER));
        let crate::runtime_builds::Selection::Start(selected) = builds.select(crate::runtime_builds::Action::Initial, &mut settings, &config).await.unwrap() else { panic!("the accepted source must be selected"); };
        std::fs::remove_file(source.join(crate::runtime_builds::HARNESS)).unwrap();
        std::fs::create_dir(source.join(crate::runtime_builds::HARNESS)).unwrap();
        let journal = crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
        let mut core = Core::fixture(1);
        core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
        let (reply, result) = oneshot::channel();
        core.replacement = Some(Replacement::Build { reply, source:selected, initial:true });
        let settings = watch::channel(Arc::new(settings)).0;
        let mut runtime = None;
        let mut read_open = false;
        core.replace(&mut runtime, &mut read_open, &clock, &journal, &builds, &settings).await;
        let reason = match core.runtime_state { RuntimeState::Unavailable(reason) => Some(reason.message.to_string()), _ => None };
        let actual = (result.await.unwrap().map(|notice| notice.map(|notice| notice.to_string())).map_err(|error| error.to_string()), reason, runtime.is_none() && !read_open);
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(actual, (Ok(None), Some("acp2api: the official runtime is unavailable: the copy of the accepted build 1.4.0 failed. Accepted versions: 1.4.0. See http://127.0.0.1:0/.".into()), true), "a failed automatic copy must let startup keep the management panel available with FR-040 M-14");
    }).await.expect("initial-copy test cap: five seconds, one copy attempt, zero process launches");
}

#[tokio::test]
async fn a_failed_owner_copy_preserves_the_existing_unavailable_reason() {
    use crate::{ListedBuild, ProcessLauncher, ReleaseFacts};
    use sha2::{Digest, Sha256};
    struct NoLaunch;
    impl ProcessLauncher for NoLaunch {
        fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
            panic!("a copy failure must launch no process")
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let folder = std::env::temp_dir().join(format!("acp2api-unavailable-copy-{}", crate::random_hex::<12>().unwrap()));
        let source = folder.join("source");
        let mut ports = crate::Ports::production();
        let hash = crate::hex(&Sha256::digest(b"accepted source"));
        ports.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:Facts(Vec::new()).platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
        ports.launcher = Arc::new(NoLaunch);
        let builds = crate::runtime_builds::Builds::new(&ports, crate::Environment { account_root:folder.join("account"), temp:folder.join("temp"), home:None, local_app_data:None, path:vec![], exe_dir:source.clone() }, folder.clone(), crate::account::Temporary::fixture(&folder), crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
        let config = folder.join("config.toml");
        let settings = crate::store::read(&config, 1).unwrap().settings;
        let journal = crate::journal::Journal::open(&folder.join("logs"), ports.clock.wall()).unwrap();
        let (conductor, task) = Conductor::start(builds.clone(), ports.clock.clone(), journal, settings, config, 1);
        conductor.builds(crate::runtime_builds::Action::Initial, Hold::empty()).await.unwrap();
        let before = conductor.model_names(Hold::empty()).await.err().map(|error| error.message.to_string());
        std::fs::create_dir_all(&source).unwrap();
        for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(source.join(name), b"accepted source").unwrap(); }
        builds.discover(None).await.unwrap();
        let choice = crate::runtime_builds::identity(&ports.release.listed_builds()[0]);
        std::fs::remove_file(source.join(crate::runtime_builds::HARNESS)).unwrap();
        std::fs::create_dir(source.join(crate::runtime_builds::HARNESS)).unwrap();
        let copy_error = std::fs::copy(source.join(crate::runtime_builds::HARNESS), folder.join("probe")).unwrap_err().to_string();
        let owner = conductor.builds(crate::runtime_builds::Action::Build(choice.to_string()), Hold::empty()).await.map(|notice| notice.map(|notice| notice.to_string())).map_err(|error| error.to_string());
        let after = conductor.model_names(Hold::empty()).await.err().map(|error| error.message.to_string());
        drop(conductor);
        task.await.unwrap();
        std::fs::remove_dir_all(folder).unwrap();
        let expected = "acp2api: the official runtime is unavailable: acp2api found no executable of the official runtime. Accepted versions: 1.4.0. See http://127.0.0.1:0/.".to_string();
        assert_eq!((owner, before, after), (Err(copy_error), Some(expected.clone()), Some(expected)), "an owner's failed copy must preserve the earlier FR-040 state and M-14 reason");
    }).await.expect("unavailable-copy test cap: five seconds, four conductor requests, zero process launches");
}

#[tokio::test]
async fn a_failed_owner_copy_keeps_the_running_runtime_and_counts_no_failure() {
    use crate::{BoxFuture, Launched, ListedBuild, ProcessLauncher, ProcessTree, ReleaseFacts};
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::io::AsyncWriteExt;
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
    struct Launcher(std::sync::Mutex<Option<Launched>>, AtomicUsize);
    impl ProcessLauncher for Launcher {
        fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
            assert!(self.1.fetch_add(1, Ordering::SeqCst) < 4, "launch cap: 4");
            self.0.lock().unwrap().take().ok_or_else(|| {
                crate::LaunchError::Io(std::io::Error::other("unexpected replacement start"))
            })
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let folder = std::env::temp_dir().join(format!("acp2api-owner-copy-{}", crate::random_hex::<12>().unwrap()));
        let mut ports = crate::Ports::production();
        let copy = folder.join(crate::runtime_builds::COPY);
        std::fs::create_dir_all(&copy).unwrap();
        for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(copy.join(name), b"accepted").unwrap(); }
        let hash = crate::hex(&Sha256::digest(b"accepted"));
        ports.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:Facts(Vec::new()).platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
        let (stdin, _agent_input) = tokio::io::duplex(65536);
        let (stdout, mut agent_output) = tokio::io::duplex(65536);
        let replies = [serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}}), serde_json::json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"running","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}})];
        for reply in replies { let line = format!("{reply}\n"); assert!(line.len() <= 65536); agent_output.write_all(line.as_bytes()).await.unwrap(); }
        let stopped = Arc::new(AtomicBool::new(false));
        ports.launcher = Arc::new(Launcher(std::sync::Mutex::new(Some(Launched { stdin:Box::pin(stdin), stdout:Box::pin(stdout), stderr:Box::pin(tokio::io::empty()), tree:Box::new(Tree(stopped.clone())) })), AtomicUsize::new(0)));
        let temporary = crate::account::Temporary::fixture(&folder);
        let builds = crate::runtime_builds::Builds::new(&ports, crate::Environment { account_root:folder.join("account"), temp:folder.join("temp"), home:None, local_app_data:None, path:vec![], exe_dir:folder.join("exe") }, folder.clone(), temporary, crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
        let (running, _) = Runtime::start(&builds.start, "1.4.0").await.unwrap();
        let mut runtime = Some(running);
        let mut read_open = true;
        let source = folder.join("new-build");
        std::fs::create_dir_all(source.join(crate::runtime_builds::HARNESS)).unwrap();
        std::fs::write(source.join(crate::runtime_builds::SERVER), b"accepted").unwrap();
        let copy_error = std::fs::copy(source.join(crate::runtime_builds::HARNESS), folder.join("probe")).unwrap_err().to_string();
        let journal = crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
        let (build_reply, build_result) = oneshot::channel();
        let (check_reply, check_result) = oneshot::channel();
        for (replacement, result) in [
            (Replacement::Build { reply:build_reply, source:Some(SelectedBuild { folder:source.clone(), version:"1.4.0".into() }), initial:false }, build_result),
            (Replacement::Check { reply:check_reply, folder:source, identity:crate::runtime_builds::identity(&ports.release.listed_builds()[0]) }, check_result),
        ] {
            let (mut core, _, _tickets) = prompting_with_waiter(&journal);
            core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
            core.handle(Event::Runtime(RuntimeEvent::Turn { session:"running".into(), update:TurnUpdate::PromptEnded { stop_reason:Some(agent_client_protocol_schema::v1::StopReason::EndTurn) } }), 1, std::time::UNIX_EPOCH);
            core.handle(Event::Deadline, 201, std::time::UNIX_EPOCH);
            core.cleanup_before_start = true;
            core.replacement = Some(replacement);
            let config = folder.join("config.toml");
            let settings = watch::channel(Arc::new(crate::store::read(&config, 1).unwrap().settings)).0;
            core.replace(&mut runtime, &mut read_open, &ports.clock, &journal, &builds, &settings).await;
            let actual = (result.await.unwrap().map(|notice| notice.map(|notice| notice.to_string())).map_err(|error| error.to_string()), core.failures.len(), stopped.load(Ordering::SeqCst), runtime.is_some() && read_open, matches!(core.runtime_state, RuntimeState::Serving));
            assert_eq!(actual, (Err(copy_error.clone()), 0, false, true, true), "owner copy errors must return to the owner while the existing runtime stays available");
        }
        if let Some(runtime) = runtime { runtime.stop().await; }
        std::fs::remove_dir_all(folder).unwrap();
    }).await.expect("owner copy cap: five seconds, four launch attempts, two replies of at most 64 KiB");
}

#[tokio::test]
async fn a_cleanup_error_during_replacement_counts_failures_and_ends_the_queue() {
    use crate::{ListedBuild, ProcessLauncher, ReleaseFacts};
    use sha2::{Digest, Sha256};
    struct NoLaunch;
    impl ProcessLauncher for NoLaunch {
        fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
            panic!("cleanup failure must start no process")
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let folder = std::env::temp_dir().join(format!("acp2api-cleanup-replacement-{}", crate::random_hex::<12>().unwrap()));
        let mut ports = crate::Ports::production();
        let copy = folder.join(crate::runtime_builds::COPY);
        std::fs::create_dir_all(&copy).unwrap();
        for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(copy.join(name), b"accepted").unwrap(); }
        let hash = crate::hex(&Sha256::digest(b"accepted"));
        ports.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:Facts(Vec::new()).platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
        ports.launcher = Arc::new(NoLaunch);
        let temporary = crate::account::Temporary::fixture(&folder);
        let cwd = temporary.sessions().join("kept");
        crate::account::private_folder(&cwd).unwrap();
        std::fs::write(temporary.sessions().join("not-a-folder"), b"cleanup obstruction").unwrap();
        let builds = crate::runtime_builds::Builds::new(&ports, crate::Environment { account_root:folder.join("account"), temp:folder.join("temp"), home:None, local_app_data:None, path:vec![], exe_dir:folder.join("exe") }, folder.clone(), temporary, crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
        let journal = crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
        let (mut core, _, [_a, mut queued]) = prompting_with_waiter(&journal);
        core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
        core.handle(Event::Runtime(RuntimeEvent::Turn { session:"running".into(), update:TurnUpdate::PromptEnded { stop_reason:Some(agent_client_protocol_schema::v1::StopReason::EndTurn) } }), 1, std::time::UNIX_EPOCH);
        core.handle(Event::Deadline, 201, std::time::UNIX_EPOCH);
        assert_eq!(core.sessions.records().len(), 1);
        core.sessions.unload(|_| cwd.clone());
        core.cleanup_before_start = true;
        let (reply, result) = oneshot::channel();
        core.replacement = Some(Replacement::Build { reply, source:None, initial:false });
        let config = folder.join("config.toml");
        let settings = watch::channel(Arc::new(crate::store::read(&config, 1).unwrap().settings)).0;
        let mut runtime = None; let mut read_open = false;
        core.replace(&mut runtime, &mut read_open, &ports.clock, &journal, &builds, &settings).await;
        let owner = result.await.unwrap();
        let mut unavailable = false;
        while let Ok((event, _hold)) = queued.try_recv() {
            if let TurnEvent::End(Err(crate::messages::Message::RuntimeUnavailable(message))) = event { unavailable = message.to_string().contains("the runtime process failed 3 times in 10 minutes"); }
        }
        let actual = (owner.is_ok(), core.failures.len(), unavailable);
        std::fs::remove_dir_all(folder).unwrap();
        assert_eq!(actual, (true, 3, true), "a verified copy whose preparation fails must not leave a serving conductor without a runtime");
    }).await.expect("cleanup failure cap: five seconds, one local obstruction, no process launch or HTTP request");
}

#[test]
fn a_broken_pipe_during_exit_keeps_the_conductor_stopping() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-exit-pipe-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(Event::Stop, 1, std::time::UNIX_EPOCH);
    core.command_failed(
        None,
        crate::runtime_driver::CommandError::Io(crate::messages::Message::Upstream(
            std::io::Error::from(std::io::ErrorKind::BrokenPipe).to_string(),
        )),
        2,
        std::time::UNIX_EPOCH,
    );
    let exit_without_restart =
        matches!(core.runtime_state, RuntimeState::Stopping) && core.failures.is_empty();
    std::fs::remove_dir_all(folder).unwrap();
    assert!(
        exit_without_restart,
        "FR-045 an exit cancel that finds a closed pipe must not start a replacement"
    );
}

#[tokio::test]
async fn closed_build_channels_report_stopped() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-closed-build-channel-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let clock = crate::Ports::production().clock;
        for close_before_send in [true, false] {
            let (send, mut recv) = mpsc::channel(1);
            if close_before_send {
                recv.close();
            }
            let receiver = tokio::spawn(async move {
                if let Some(Input::Builds { reply, .. }) = recv.recv().await {
                    drop(reply);
                }
            });
            let settings = crate::store::read(&folder.join("config.toml"), 1)
                .unwrap()
                .settings;
            let conductor = Conductor {
                journal: crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap(),
                view: watch::channel(Arc::new(View::default())).1,
                quota_result: watch::channel(Arc::new(None)).1,
                send,
                builds: crate::runtime_builds::Builds::fixture(&folder, clock.clone()),
                settings: watch::channel(Arc::new(settings)).1,
                notices: watch::channel(Arc::new(Vec::new())).1,
                highest: 1,
            };
            let result = conductor
                .builds(crate::runtime_builds::Action::Search, Hold::empty())
                .await;
            receiver.await.unwrap();
            assert!(
                matches!(result, Err(LaunchFailure::Stopped)),
                "a closed conductor channel must report Stopped: {result:?}"
            );
        }
        std::fs::remove_dir_all(folder).unwrap();
    })
    .await
    .expect("closed-channel test cap: five seconds, two owner requests, no process launch");
}

#[test]
fn a_failed_replacement_ends_queued_requests_with_runtime_unavailable() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-core-build-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, tickets) = prompting_with_waiter(&journal);
    core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
    let message = crate::messages::Unavailable::no_executable();
    let commands = core.handle(
        Event::RuntimeUnavailable(UnavailableReason {
            message: message.clone(),
            cause: None,
        }),
        2,
        std::time::UNIX_EPOCH,
    );
    let ends: Vec<_> = commands
        .iter()
        .filter_map(|command| match command {
            Command::Emit(
                _,
                TurnEvent::End(Err(crate::messages::Message::RuntimeUnavailable(text))),
            ) => Some(text.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        [message.to_string()],
        "queued requests must receive M-14 when a planned replacement fails"
    );
    std::fs::remove_dir_all(folder).unwrap();
    drop(tickets);
}

#[cfg(unix)]
#[tokio::test]
async fn a_start_without_state_survives_an_unwritable_data_folder() {
    use std::os::unix::fs::PermissionsExt;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-state-create-failure-{}",
            crate::random_hex::<12>().unwrap()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
        let builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
        let configuration = folder.join("config.toml");
        let settings = crate::store::read(&configuration, 3).unwrap().settings;
        let state_path = folder.join(crate::store::STATE_FILE);
        assert!(!state_path.exists());
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
        let (conductor, task) =
            Conductor::start(builds, clock.clone(), journal, settings, configuration, 3);
        let result = conductor
            .builds(crate::runtime_builds::Action::Initial, Hold::empty())
            .await;
        let state = conductor.view().state;
        let file_exists = state_path.exists();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        drop(conductor);
        task.await.unwrap();
        let logged = std::fs::read_dir(folder.join("logs"))
            .unwrap()
            .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect::<String>();
        std::fs::remove_dir_all(folder).unwrap();
        assert!(
            result.is_ok(),
            "the start keeps serving after a state-file write failure: {result:?}"
        );
        assert_eq!(
            state,
            State::RuntimeUnavailable,
            "the missing executable, rather than lockout, stops runtime work"
        );
        assert!(!file_exists);
        assert!(
            logged.contains("state_file_write_failed"),
            "the journal reports the failed state-file creation"
        );
    })
    .await
    .expect("state-file startup test cap: five seconds, no runtime launches");
}
