use super::quota::{QuotaPhase, QuotaPurpose, QuotaQuery};
use super::request::Active;
use super::runtime_lifecycle::{RUNTIME_FAILURE_WINDOW, Replacement};
use super::work::ResponseChannel;
use super::*;
use crate::runtime_builds::SelectedBuild;
use crate::{BoxFuture, ProcessTree, conversation::TurnEvent};
use agent_client_protocol_schema::v1::StopReason;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
mod admission_tests;
mod lockout_tests;
mod quota_tests;
mod records_tests;
mod request_tests;
mod retry_tests;
mod runtime_lifecycle_tests;
mod session_limit_tests;
mod sign_in_tests;
mod upstream_order_tests;
mod view_tests;
impl Core {
    fn fixture(limit: u8) -> Self {
        Self {
            quota_result: watch::channel(Arc::new(None)).0,
            quota: quota::QuotaState::default(),
            retry_waits: Vec::new(),
            lifecycle: Lifecycle::new(RuntimeState::Serving),
            lockout: None,
            sign_in: sign_in::SignIn::default(),
            panel_address: "http://127.0.0.1:8329/".into(),
            state_path: PathBuf::new(),
            view: watch::channel(Arc::new(View::default())).0,
            catalog: None,
            model_names: Vec::new(),
            context_sizes: BTreeMap::new(),
            work: WorkTable::default(),
            sessions: records::AcpSessions::default(),
            limit,
            notices: watch::channel(Arc::new(Vec::new())).0,
        }
    }
}

struct StoppedTree(Arc<AtomicUsize>);
impl ProcessTree for StoppedTree {
    fn pid(&self) -> u32 {
        std::process::id()
    }
    fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
        Box::pin(std::future::pending())
    }
    fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }
}
struct Output(std::io::Cursor<Vec<u8>>);
impl tokio::io::AsyncRead for Output {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.0.position() < self.0.get_ref().len() as u64 {
            std::pin::Pin::new(&mut self.0).poll_read(cx, buf)
        } else {
            std::task::Poll::Pending
        }
    }
}

fn quota_error_without_reset_time() -> crate::messages::HttpError {
    crate::messages::HttpError {
        code: Some(429),
        status: "RESOURCE_EXHAUSTED".into(),
        message: "Quota exhausted; no reset time.".into(),
    }
}

fn error_after_reply(
    reply: &str,
    error: crate::messages::HttpError,
) -> (Core, Vec<Command>, PathBuf) {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-lockout-error-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal =
        crate::journal::Journal::open(&folder.join("logs"), std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.state_path = folder.join(crate::store::STATE_FILE);
    core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Text(reply.into()),
        }),
        0,
        std::time::UNIX_EPOCH,
    );
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error,
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    (core, commands, folder)
}

#[test]
fn quota_expiry_on_an_owner_event_keeps_runtime_unavailable_and_reports_state_changes() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-quota-state-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    let mut commands = core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: crate::messages::HttpError {
                code: Some(429),
                status: "RESOURCE_EXHAUSTED".into(),
                message: "resets 1970-01-01T00:02:00Z".into(),
            },
        }),
        0,
        std::time::UNIX_EPOCH,
    );
    commands.extend(core.handle(
        Event::RuntimeUnavailable(UnavailableReason {
            message: crate::messages::Unavailable::no_executable(),
            cause: None,
        }),
        1,
        std::time::UNIX_EPOCH,
    ));
    commands.extend(core.handle(Event::Limit(2), 120_000, std::time::UNIX_EPOCH));
    assert!(matches!(
        core.refusal(120_000),
        Some(crate::messages::Message::RuntimeUnavailable(_))
    ));
    commands.extend(core.handle(Event::ResumeRuntime, 120_000, std::time::UNIX_EPOCH));
    let states: Vec<_> = commands
        .into_iter()
        .filter_map(|command| match command {
            Command::State(state, ..) => Some(state),
            _ => None,
        })
        .collect();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        states,
        [State::QuotaWait, State::RuntimeUnavailable, State::Running],
        "FR-002/FR-056 each condition keeps its own end rule, including expiry during an owner action"
    );
}
#[test]
fn preparation_rechecks_quota_after_runtime_recovery() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-quota-preparation-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let mut results = Vec::new();
    for quota in [false, true] {
        let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::Limit(2), 0, std::time::UNIX_EPOCH);
        if quota {
            core.handle(
                Event::Runtime(RuntimeEvent::UpstreamError {
                    session: "running".into(),
                    error: crate::messages::HttpError {
                        code: Some(429),
                        status: "RESOURCE_EXHAUSTED".into(),
                        message: "resets 1970-01-01T00:02:00Z".into(),
                    },
                }),
                1,
                std::time::UNIX_EPOCH,
            );
            core.handle(
                Event::RuntimeUnavailable(UnavailableReason {
                    message: crate::messages::Unavailable::no_executable(),
                    cause: None,
                }),
                1,
                std::time::UNIX_EPOCH,
            );
            core.handle(Event::ResumeRuntime, 1, std::time::UNIX_EPOCH);
        }
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: queued,
                result: Ok(("prepared".into(), catalog)),
            }),
            2,
            std::time::UNIX_EPOCH,
        );
        results.push(
            if commands
                .iter()
                .any(|command| matches!(command, Command::Prompt { .. }))
            {
                "prompt"
            } else if commands.iter().any(|command| {
                matches!(
                    command,
                    Command::Emit(
                        _,
                        TurnEvent::End(Err(crate::messages::Message::QuotaWait { .. }))
                    )
                )
            }) {
                "quota"
            } else {
                "no result"
            },
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        results,
        ["prompt", "quota"],
        "FR-036 runtime recovery clears only runtime unavailable, and preparation must check the remaining condition before prompt dispatch"
    );
}
struct Facts(Vec<crate::ListedBuild>);
impl crate::ReleaseFacts for Facts {
    fn version(&self) -> &str {
        "test"
    }
    fn platform(&self) -> &str {
        if cfg!(windows) {
            "windows-x64"
        } else {
            "macos-arm64"
        }
    }
    fn listed_builds(&self) -> &[crate::ListedBuild] {
        &self.0
    }
    fn highest_concurrency_limit(&self) -> u8 {
        1
    }
}
fn prompting_with_waiter(
    journal: &crate::journal::Journal,
) -> (
    Core,
    String,
    [mpsc::UnboundedReceiver<(TurnEvent, Hold)>; 2],
) {
    let mut core = Core {
        sessions: records::AcpSessions::fixture(Vec::new(), Some("running".into()), 0),
        ..Core::fixture(1)
    };
    let mut tickets = Vec::new();
    let mut queued = String::new();
    for text in ["First", "Queued"] {
        let body = serde_json::json!({"model":"m","messages":[{"role":"user","content":text}]});
        let turn = crate::dialect::CHAT
            .parse(&serde_json::to_vec(&body).unwrap())
            .ok()
            .unwrap()
            .turn;
        let request = journal.begin(crate::journal::Api::Chat, 0);
        queued = request.id.clone();
        let (output, ticket) = mpsc::unbounded_channel();
        tickets.push(ticket);
        core.handle(
            Event::Accepted(Box::new(Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: "m".into(),
                    level: None,
                    level_from_name: false,
                },
                request,
                reply: ResponseChannel {
                    output,
                    keep_alive: None,
                },
            })),
            0,
            std::time::UNIX_EPOCH,
        );
    }
    (core, queued, tickets.try_into().ok().unwrap())
}

async fn check_fixture(
    mut ports: crate::Ports,
    stop: watch::Receiver<bool>,
) -> (PathBuf, Conductor, tokio::task::JoinHandle<()>) {
    use sha2::{Digest, Sha256};
    let root = std::env::temp_dir().join(format!(
        "acp2api-check-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let source = root.join("exe");
    for (folder, bytes) in [
        (&source, b"candidate".as_slice()),
        (&root.join("official-runtime"), b"accepted".as_slice()),
    ] {
        std::fs::create_dir_all(folder).unwrap();
        for name in [
            crate::runtime_builds::SERVER,
            crate::runtime_builds::HARNESS,
        ] {
            std::fs::write(folder.join(name), bytes).unwrap();
        }
    }
    let hash = crate::hex(&Sha256::digest(b"accepted"));
    ports.release = Arc::new(Facts(vec![crate::ListedBuild {
        version: "1.4.0".into(),
        platform: ports.release.platform().into(),
        agy_acp_server_sha256: hash.clone(),
        localharness_external_sha256: hash,
    }]));
    let builds = crate::runtime_builds::Builds::new(
        &ports,
        crate::Environment {
            account_root: root.join("account"),
            temp: root.join("temp"),
            home: Some(root.join("home")),
            local_app_data: None,
            path: Vec::new(),
            exe_dir: source,
        },
        root.clone(),
        crate::account::Temporary::fixture(&root),
        crate::runtime_driver::mcp::ToolSets::new(0),
        stop,
        0,
    );
    let configuration = root.join("config.toml");
    let settings = crate::store::read(&configuration, 1).unwrap().settings;
    let journal = crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
    let (conductor, task) = Conductor::start(
        builds,
        ports.clock,
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
    (root, conductor, task)
}

#[tokio::test]
async fn a_planned_build_change_replaces_the_actor_runtime_after_the_running_turn() {
    use crate::{
        LaunchError, LaunchSpec, Launched, ListedBuild, ProcessLauncher, ProcessTree, ReleaseFacts,
    };
    use std::sync::Mutex;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
    fn record(trace: &Mutex<Vec<String>>, event: String) {
        let mut trace = trace.lock().unwrap();
        assert!(trace.len() < 32, "fake trace cap: 32 events");
        trace.push(event);
    }
    struct Tree(&'static str, Arc<Mutex<Vec<String>>>);
    impl ProcessTree for Tree {
        fn pid(&self) -> u32 {
            std::process::id()
        }
        fn exited(&mut self) -> crate::BoxFuture<'_, std::process::ExitStatus> {
            Box::pin(std::future::pending())
        }
        fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> crate::BoxFuture<'static, ()> {
            Box::pin(async move {
                record(&self.1, format!("stop:{}", self.0));
            })
        }
    }
    struct Launcher(
        Mutex<VecDeque<(&'static str, Launched)>>,
        Arc<Mutex<Vec<String>>>,
    );
    impl ProcessLauncher for Launcher {
        fn launch(&self, _: LaunchSpec) -> Result<Launched, LaunchError> {
            let (label, child) = self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("two launches only");
            record(&self.1, format!("launch:{label}"));
            Ok(child)
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        use sha2::{Digest, Sha256};
        let root = std::env::temp_dir().join(format!("acp2api-build-replacement-{}", crate::random_hex::<12>().unwrap()));
        let temporary = crate::account::Temporary::fixture(&root);
        let trace = Arc::new(Mutex::new(Vec::new()));
        let release_a = Arc::new(tokio::sync::Notify::new());
        let prompting_a = Arc::new(tokio::sync::Notify::new());
        let mut children = VecDeque::new();
        let mut agents = Vec::new();
        let mut facts = Vec::new();
        for (label, version, folder) in [("old", "1.4.0", root.join("official-runtime")), ("new", "1.5.0", root.join("new-build"))] {
            std::fs::create_dir_all(&folder).unwrap();
            for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] { std::fs::write(folder.join(name), label.as_bytes()).unwrap(); }
            let hash = crate::hex(&Sha256::digest(label.as_bytes()));
            facts.push(ListedBuild { version: version.into(), platform: Facts(Vec::new()).platform().into(), agy_acp_server_sha256: hash.clone(), localharness_external_sha256: hash });
            let (product, agent) = tokio::io::duplex(65536);
            let (stdout, stdin) = tokio::io::split(product);
            children.push_back((label, Launched { stdin: Box::pin(stdin), stdout: Box::pin(stdout), stderr: Box::pin(tokio::io::empty()), tree: Box::new(Tree(label, trace.clone())) }));
            let trace = trace.clone();
            let release_a = release_a.clone();
            let prompting_a = prompting_a.clone();
            agents.push(tokio::spawn(async move {
                let (input, mut output) = tokio::io::split(agent);
                let mut input = tokio::io::BufReader::new(input);
                let mut sessions = 0;
                for _ in 0..8 {
                    let mut bytes = Vec::new();
                    let read = (&mut input).take(65537).read_until(b'\n', &mut bytes).await.unwrap();
                    if read == 0 { return; }
                    assert!(bytes.len() <= 65536, "fake ACP line cap: 64 KiB");
                    let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    let method = request["method"].as_str().unwrap();
                    let options = serde_json::json!([{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]);
                    let result = match method {
                        "initialize" => serde_json::json!({"protocolVersion":1,"agentInfo":{"version":version}}),
                        "session/new" => {
                            sessions += 1;
                            record(&trace, format!("prepare:{label}:{sessions}"));
                            serde_json::json!({"sessionId":format!("{label}-{sessions}"),"configOptions":options})
                        }
                        "session/set_config_option" => serde_json::json!({"configOptions":options}),
                        "session/prompt" => {
                            let session = request["params"]["sessionId"].as_str().unwrap();
                            assert!(session.starts_with(label), "no ACP session from the old process may reach the new process");
                            record(&trace, format!("prompt:{label}"));
                            if label == "old" { prompting_a.notify_one(); release_a.notified().await; }
                            record(&trace, format!("reply:{label}"));
                            serde_json::json!({"stopReason":"end_turn"})
                        }
                        _ => panic!("unexpected ACP method: {method}"),
                    };
                    let reply = serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result});
                    if output.write_all(format!("{reply}\n").as_bytes()).await.is_err() { return; }
                }
                panic!("fake ACP request cap: 8");
            }));
        }
        let mut ports = crate::Ports::production();
        ports.release = Arc::new(Facts(facts));
        ports.launcher = Arc::new(Launcher(Mutex::new(children), trace.clone()));
        let environment = crate::Environment { account_root: root.join("account"), temp: root.join("temp"), home: None, local_app_data: None, path: vec![], exe_dir: root.join("new-build") };
        let builds = crate::runtime_builds::Builds::new(&ports, environment, root.clone(), temporary, crate::runtime_driver::mcp::ToolSets::new(0), watch::channel(false).1, 0);
        let config = root.join("config.toml");
        let settings = crate::store::read(&config, 1).unwrap().settings;
        let choice = crate::runtime_builds::identity(&ports.release.listed_builds()[1]);
        let journal = crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
        let (conductor, task) = Conductor::start(builds, ports.clock.clone(), journal.clone(), format!("http://127.0.0.1:{}/", settings.port), settings, config, 1);
        conductor.builds(crate::runtime_builds::Action::Initial, Hold::empty()).await.unwrap();
        let parse = |system: &str| crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"system","content":system},{"role":"user","content":"request"}]})).unwrap()).ok().unwrap().turn;
        let mut a = conductor.admit(parse("A"), Hold::empty(), journal.begin(crate::journal::Api::Chat, 0), ResponseMode::Body).await.ok().unwrap();
        prompting_a.notified().await;
        let (reply, owner) = oneshot::channel();
        conductor.send.send(Input::Builds { action: crate::runtime_builds::Action::Build(choice.to_string()), reply, hold: Hold::empty() }).await.unwrap();
        let mut b = conductor.admit(parse("B"), Hold::empty(), journal.begin(crate::journal::Api::Chat, 0), ResponseMode::Body).await.ok().unwrap();
        let waiting = trace.lock().unwrap().clone();
        let deferred = !waiting.iter().any(|event| event == "stop:old" || event == "launch:new");
        if !deferred {
            task.abort(); for agent in agents { agent.abort(); }
            std::fs::remove_dir_all(&root).unwrap();
            assert!(deferred, "the actor replaced the runtime before A ended: {waiting:?}");
            return;
        }
        release_a.notify_one();
        let mut normal = Vec::new();
        for ticket in [&mut a, &mut b] {
            while let Some((event, _hold)) = ticket.events.recv().await {
                if let TurnEvent::End(result) = event { normal.push(result.is_ok()); break; }
            }
        }
        owner.await.unwrap().unwrap();
        drop(conductor);
        task.await.unwrap();
        for agent in agents { agent.await.unwrap(); }
        let actual = trace.lock().unwrap().clone();
        let index = |event: &str| actual.iter().position(|entry| entry == event).unwrap();
        let ordered = index("reply:old") < index("stop:old") && index("stop:old") < index("launch:new") && index("launch:new") < index("prepare:new:2") && index("prepare:new:2") < index("prompt:new");
        std::fs::remove_dir_all(root).unwrap();
        assert!(normal == [true, true] && ordered, "A and B must end normally around the planned replacement: {actual:?}");
    }).await.expect("planned build replacement cap: 5 seconds; two fake agents, 8 requests and 64 KiB per line each, 32 trace events");
}

#[test]
fn a_build_change_finishes_a_before_preparing_b_on_the_new_runtime() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-core-build-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, queued, tickets) = prompting_with_waiter(&journal);
    let id = core.work.by_request(&queued).unwrap();
    let mut b = core.work.remove(&id).unwrap().into_queued().unwrap();
    b.turn = crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({
        "model":"m", "messages":[{"role":"system","content":"B's instructions"},{"role":"user","content":"B"}]
    })).unwrap()).ok().unwrap().turn;
    let changed = core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
    let arrival = core.handle(Event::Accepted(Box::new(b)), 2, std::time::UNIX_EPOCH);
    let ended = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(agent_client_protocol_schema::v1::StopReason::EndTurn),
            },
        }),
        3,
        std::time::UNIX_EPOCH,
    );
    let completed = core.handle(Event::Deadline, 203, std::time::UNIX_EPOCH);
    let no_dispatch = changed
        .iter()
        .chain(&arrival)
        .chain(&ended)
        .chain(&completed)
        .all(|command| {
            !matches!(
                command,
                Command::Prepare { .. } | Command::Prompt { .. } | Command::Cancel(_)
            )
        });
    let ended_normally = completed
        .iter()
        .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Ok(_)))));
    assert!(
        no_dispatch && ended_normally && core.replacement_ready(0),
        "B must wait for A to end normally before runtime replacement"
    );
    core.replaced(Some("new-runtime-startup".into()), None);
    let dispatched = core.handle(Event::ResumeRuntime, 204, std::time::UNIX_EPOCH);
    assert!(
        core.sessions.records().len() == 1
            && dispatched
                .iter()
                .any(|command| matches!(command, Command::Prepare {work,..} if work == &queued)),
        "ADR-0003 retains A's record while the unrelated B gets a new ACP session"
    );
    std::fs::remove_dir_all(folder).unwrap();
    drop(tickets);
}

fn start_without_runtime(
    clock: &Arc<dyn Clock>,
    journal: &crate::journal::Journal,
    folder: &std::path::Path,
) -> (Conductor, tokio::task::JoinHandle<()>) {
    let settings = crate::store::Settings {
        port: 8329,
        concurrency_limit: 1,
        local_api_key: String::new(),
        management_password_hash: String::new(),
        agy_acp_server_path: None,
    };
    let configuration = folder.join("config.toml");
    Conductor::start(
        crate::runtime_builds::Builds::fixture(folder, clock.clone()),
        clock.clone(),
        journal.clone(),
        format!("http://127.0.0.1:{}/", settings.port),
        settings,
        configuration,
        3,
    )
}

#[test]
fn a_successful_sign_in_ends_only_its_own_condition() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-conditions-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.work.clear();
    core.start_quota(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(120),
        0,
        std::time::UNIX_EPOCH,
        &mut Vec::new(),
    );
    core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
    core.handle(
        Event::RuntimeReady {
            sign_in_required: false,
        },
        30_000,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        core.state(),
        State::QuotaWait,
        "FR-038 a successful check clears sign-in required during the owner's wait"
    );
    assert_eq!(core.sign_in.wait().unwrap().deadline, 600_000);
    core.handle(
        Event::Runtime(RuntimeEvent::Authenticated(Ok(()))),
        30_000,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        core.state(),
        State::QuotaWait,
        "FR-038 sign-in preserves the unexpired quota wait"
    );
    assert_eq!(core.next_deadline(), Some(120_000));
    core.handle(Event::StartSignIn, 40_000, std::time::UNIX_EPOCH);
    core.handle(
        Event::RuntimeUnavailable(UnavailableReason {
            message: crate::messages::Unavailable::no_executable(),
            cause: None,
        }),
        40_000,
        std::time::UNIX_EPOCH,
    );
    core.handle(
        Event::Runtime(RuntimeEvent::Authenticated(Ok(()))),
        40_000,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        core.state(),
        State::RuntimeUnavailable,
        "sign-in cannot clear runtime unavailability"
    );
    assert_eq!(core.quota.wait().unwrap().deadline, 120_000);
    std::fs::remove_dir_all(folder).unwrap();
}
