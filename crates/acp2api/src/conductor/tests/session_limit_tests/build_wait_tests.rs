use super::*;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

struct Launcher {
    starts: AtomicUsize,
    stopped: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
    holding: Arc<tokio::sync::Notify>,
    crash: Arc<tokio::sync::Notify>,
    initializing: Arc<tokio::sync::Notify>,
    init_gate: Arc<tokio::sync::Notify>,
    gate_second: bool,
    agents: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Launcher {
    fn new(gate_second: bool) -> Arc<Self> {
        Arc::new(Self {
            starts: AtomicUsize::new(0),
            stopped: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(tokio::sync::Notify::new()),
            holding: Arc::new(tokio::sync::Notify::new()),
            crash: Arc::new(tokio::sync::Notify::new()),
            initializing: Arc::new(tokio::sync::Notify::new()),
            init_gate: Arc::new(tokio::sync::Notify::new()),
            gate_second,
            agents: Mutex::new(Vec::new()),
        })
    }
}

impl crate::ProcessLauncher for Launcher {
    fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
        let process = self.starts.fetch_add(1, Ordering::SeqCst);
        assert!(process < 4, "fake process cap: four");
        let (product, agent) = tokio::io::duplex(65536);
        let (stdout, stdin) = tokio::io::split(product);
        let release = self.release.clone();
        let holding = self.holding.clone();
        let crash = self.crash.clone();
        let initializing = self.initializing.clone();
        let gate = self.init_gate.clone();
        let gated = self.gate_second && process > 0;
        self.agents.lock().unwrap().push(tokio::spawn(async move {
            let (input, mut output) = tokio::io::split(agent);
            let mut input = tokio::io::BufReader::new(input);
            let mut held = None;
            let mut sessions = 0;
            for _ in 0..96 {
                let mut bytes = Vec::new();
                let request = tokio::select! {
                    read = async { (&mut input).take(65537).read_until(b'\n', &mut bytes).await } => {
                        if read.unwrap() == 0 { return; }
                        assert!(bytes.len() <= 65536, "ACP frame cap: 64 KiB");
                        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
                    }
                    _ = crash.notified(), if process == 0 => { return; }
                    _ = release.notified(), if held.is_some() => {
                        let id = held.take().unwrap();
                        let reply = serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"stopReason":"end_turn"}});
                        if output.write_all(format!("{reply}\n").as_bytes()).await.is_err() { return; }
                        continue;
                    }
                };
                let options = serde_json::json!([{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]);
                let result = match request["method"].as_str().unwrap() {
                    "initialize" => {
                        if gated { initializing.notify_one(); gate.notified().await; }
                        serde_json::json!({"protocolVersion":1,"agentInfo":{"version":"1.4.0"}})
                    },
                    "session/new" => { sessions += 1; serde_json::json!({"sessionId":format!("{process}-{sessions}"),"configOptions":options}) }
                    "session/set_config_option" => serde_json::json!({"configOptions":options}),
                    "session/prompt" if request.to_string().contains("HOLD") => { held = Some(request["id"].clone()); holding.notify_one(); continue; }
                    "session/prompt" => serde_json::json!({"stopReason":"end_turn"}),
                    method => panic!("unexpected fake ACP call: {method}"),
                };
                let reply = serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result});
                if output.write_all(format!("{reply}\n").as_bytes()).await.is_err() { return; }
            }
            panic!("fake request cap: 96");
        }));
        Ok(crate::Launched {
            stdin: Box::pin(stdin),
            stdout: Box::pin(stdout),
            stderr: Box::pin(tokio::io::empty()),
            tree: Box::new(StoppedTree(self.stopped.clone())),
        })
    }
}

async fn ended(ticket: &mut crate::conversation::Ticket) {
    while let Some((event, _)) = ticket.events.recv().await {
        if let TurnEvent::End(result) = event {
            assert!(result.is_ok());
            return;
        }
    }
    panic!("a connected request must get its end");
}

#[tokio::test]
async fn panel_build_actions_answer_or_take_over_a_limit_wait_without_two_restarts() {
    // Per case: 8 API requests, at most 4 processes, 96 ACP frames of 64 KiB per process, 8 seconds.
    for kind in ["keep", "notice", "replace"] {
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            let launcher = Launcher::new(false);
            let (root, conductor, task) = setup(&launcher).await;
            let (mut holding, mut waiting) = limit_wait(&launcher, &conductor).await;
            let action = match kind {
                "keep" => crate::runtime_builds::Action::Search,
                "notice" => crate::runtime_builds::Action::Path(root.join("no-harness/server")),
                "replace" => crate::runtime_builds::Action::Build(
                    conductor.builds_summary().unwrap().found[0]
                        .identity
                        .clone(),
                ),
                _ => unreachable!(),
            };
            let notice = if kind == "replace" {
                let owner = conductor.builds(action, Hold::empty());
                tokio::pin!(owner);
                assert!(futures_util::poll!(owner.as_mut()).is_pending());
                barrier(&conductor).await;
                assert_eq!(conductor.view().runtime_process, RuntimeProcess::Stopping);
                assert!(futures_util::poll!(owner.as_mut()).is_pending());
                launcher.release.notify_one();
                owner.await.unwrap().is_some()
            } else {
                let result = conductor.builds(action, Hold::empty()).await.unwrap();
                assert_eq!(launcher.starts.load(Ordering::SeqCst), 1);
                launcher.release.notify_one();
                result.is_some()
            };
            ended(&mut holding).await;
            ended(&mut waiting).await;
            let starts = finish(&launcher, root, conductor, task).await;
            assert_eq!(
                (notice, starts),
                (kind == "notice", 2),
                "F7: {kind} must answer or replace the pending limit demand"
            );
        })
        .await
        .unwrap_or_else(|_| panic!("bounded owner build wait: {kind}"));
    }
}

async fn end_of(ticket: &mut crate::conversation::Ticket) -> Result<(), String> {
    while let Some((event, _)) = ticket.events.recv().await {
        if let TurnEvent::End(result) = event {
            return result.map(|_| ()).map_err(|error| error.to_string());
        }
    }
    Err("a connected request has no end".into())
}

async fn ask(conductor: &Conductor, text: &str) -> crate::conversation::Ticket {
    let turn = crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"system","content":"build wait"},{"role":"user","content":text}]})).unwrap()).ok().unwrap().turn;
    conductor
        .admit(
            turn,
            Hold::empty(),
            conductor.journal.begin(crate::journal::Api::Chat, 0),
            ResponseMode::Body,
        )
        .await
        .ok()
        .unwrap()
}

async fn setup(launcher: &Arc<Launcher>) -> (PathBuf, Conductor, tokio::task::JoinHandle<()>) {
    let mut ports = crate::Ports::production();
    ports.launcher = launcher.clone();
    let (root, conductor, task) = check_fixture(ports, watch::channel(false).1).await;
    for name in [
        crate::runtime_builds::SERVER,
        crate::runtime_builds::HARNESS,
    ] {
        std::fs::write(root.join("exe").join(name), b"accepted").unwrap();
    }
    conductor
        .builds(crate::runtime_builds::Action::Search, Hold::empty())
        .await
        .unwrap();
    barrier(&conductor).await;
    (root, conductor, task)
}

async fn barrier(conductor: &Conductor) {
    // This reply follows each earlier input, even while a build action waits for a prompt turn.
    conductor
        .owner(OwnerAction::SetConcurrencyLimit(2), Hold::empty())
        .await
        .unwrap();
}

async fn limit_wait(
    launcher: &Launcher,
    conductor: &Conductor,
) -> (crate::conversation::Ticket, crate::conversation::Ticket) {
    for index in 0..6 {
        ended(&mut ask(conductor, &format!("fill-{index}")).await).await;
    }
    let holding = ask(conductor, "HOLD").await;
    launcher.holding.notified().await;
    let waiting = ask(conductor, "waiter").await;
    barrier(conductor).await;
    (holding, waiting)
}

fn break_copy(root: &std::path::Path) {
    let harness = root.join("exe").join(crate::runtime_builds::HARNESS);
    std::fs::remove_file(&harness).unwrap();
    std::fs::create_dir(&harness).unwrap();
}

async fn finish(
    launcher: &Launcher,
    root: PathBuf,
    conductor: Conductor,
    task: tokio::task::JoinHandle<()>,
) -> usize {
    drop(conductor);
    task.await.unwrap();
    let agents = std::mem::take(&mut *launcher.agents.lock().unwrap());
    for agent in agents {
        agent.await.unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
    launcher.starts.load(Ordering::SeqCst)
}

#[tokio::test]
async fn limit_replacement_publishes_stopping_before_the_next_initialize_completes() {
    // 8 API requests, 4 processes, 96 ACP frames of 64 KiB per process, 8 seconds.
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        let launcher = Launcher::new(true);
        let (root, conductor, task) = setup(&launcher).await;
        let (mut holding, mut waiting) = limit_wait(&launcher, &conductor).await;
        launcher.release.notify_one();
        ended(&mut holding).await;
        launcher.initializing.notified().await;
        let during = conductor.view().runtime_process;
        assert_eq!(launcher.stopped.load(Ordering::SeqCst), 1);
        launcher.init_gate.notify_one();
        ended(&mut waiting).await;
        let after = conductor.view().runtime_process;
        finish(&launcher, root, conductor, task).await;
        assert_eq!(
            (during, after),
            (RuntimeProcess::Stopping, RuntimeProcess::Running),
            "F2: the panel must leave Running while the replacement cannot serve"
        );
    })
    .await
    .expect("replacement view cap: 8 seconds");
}

// The walk uses the real actor because a Core-only walk cannot test actor event order.
pub(super) async fn failed_copies_after_crash(seed: u64) -> usize {
    // 3 API requests, 2 or 3 build actions, 4 processes, 96 frames of 64 KiB, 8 seconds.
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        let launcher = Launcher::new(false);
        let (root, conductor, task) = setup(&launcher).await;
        let id = conductor.builds_summary().unwrap().found[0]
            .identity
            .clone();
        let mut holding = ask(&conductor, "HOLD").await;
        launcher.holding.notified().await;
        break_copy(&root);
        let copies = 2 + (seed % 2) as usize;
        let mut actions = Vec::new();
        for _ in 0..copies {
            let mut action = Box::pin(conductor.builds(
                crate::runtime_builds::Action::Build(id.clone()),
                Hold::empty(),
            ));
            assert!(futures_util::poll!(action.as_mut()).is_pending());
            actions.push(action);
        }
        barrier(&conductor).await;
        launcher.crash.notify_one();
        for action in actions {
            assert!(action.await.is_err(), "the broken build copy must fail");
        }
        assert!(end_of(&mut holding).await.is_err());
        let later = end_of(&mut ask(&conductor, "after").await).await;
        let again = end_of(&mut ask(&conductor, "after again").await).await;
        let view = conductor.view().runtime_process;
        let starts = finish(&launcher, root, conductor, task).await;
        assert_eq!(
            (later, again, view, starts),
            (Ok(()), Ok(()), RuntimeProcess::Running, 2),
            "F1: a crash must restart before later build actions can resume a process; seed={seed}"
        );
        copies
    })
    .await
    .expect("crash with queued build actions cap: 8 seconds")
}

#[tokio::test]
async fn failed_build_copies_after_a_crash_do_not_resume_the_dead_process() {
    failed_copies_after_crash(0).await;
}

#[tokio::test]
async fn failed_build_takeover_keeps_the_limit_waiter_and_replaces_once() {
    // 8 API requests, 1 build action, 4 processes, 96 frames of 64 KiB, 8 seconds.
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        let launcher = Launcher::new(false);
        let (root, conductor, task) = setup(&launcher).await;
        let id = conductor.builds_summary().unwrap().found[0]
            .identity
            .clone();
        let (mut holding, mut waiting) = limit_wait(&launcher, &conductor).await;
        break_copy(&root);
        let failed = {
            let owner = conductor.builds(crate::runtime_builds::Action::Build(id), Hold::empty());
            tokio::pin!(owner);
            assert!(futures_util::poll!(owner.as_mut()).is_pending());
            barrier(&conductor).await;
            assert_eq!(conductor.view().runtime_process, RuntimeProcess::Stopping);
            assert!(futures_util::poll!(owner.as_mut()).is_pending());
            launcher.release.notify_one();
            owner.await.is_err()
        };
        ended(&mut holding).await;
        ended(&mut waiting).await;
        let starts = finish(&launcher, root, conductor, task).await;
        assert_eq!(
            (failed, starts),
            (true, 2),
            "a failed take-over must keep the limit waiter and replace once"
        );
    })
    .await
    .expect("failed build take-over cap: 8 seconds");
}
