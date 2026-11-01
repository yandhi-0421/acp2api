use super::*;
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

struct Launcher {
    starts: AtomicUsize,
    stopped: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
    agents: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl crate::ProcessLauncher for Launcher {
    fn launch(&self, _: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
        let process = self.starts.fetch_add(1, Ordering::SeqCst);
        assert!(process < 4, "fake process cap: four");
        let (product, agent) = tokio::io::duplex(65536);
        let (stdout, stdin) = tokio::io::split(product);
        let release = self.release.clone();
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
                    _ = release.notified(), if held.is_some() => {
                        let id = held.take().unwrap();
                        let reply = serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"stopReason":"end_turn"}});
                        if output.write_all(format!("{reply}\n").as_bytes()).await.is_err() { return; }
                        continue;
                    }
                };
                let options = serde_json::json!([{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]);
                let result = match request["method"].as_str().unwrap() {
                    "initialize" => serde_json::json!({"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}),
                    "session/new" => { sessions += 1; serde_json::json!({"sessionId":format!("{process}-{sessions}"),"configOptions":options}) }
                    "session/set_config_option" => serde_json::json!({"configOptions":options}),
                    "session/prompt" if request.to_string().contains("HOLD") => { held = Some(request["id"].clone()); continue; }
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
            let launcher = Arc::new(Launcher { starts: AtomicUsize::new(0), stopped: Arc::new(AtomicUsize::new(0)), release: Arc::new(tokio::sync::Notify::new()), agents: Mutex::new(Vec::new()) });
            let mut ports = crate::Ports::production();
            ports.launcher = launcher.clone();
            let (root, conductor, task) = check_fixture(ports, watch::channel(false).1).await;
            for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] {
                std::fs::write(root.join("exe").join(name), b"accepted").unwrap();
            }
            conductor.builds(crate::runtime_builds::Action::Search, Hold::empty()).await.unwrap();
            conductor.owner(OwnerAction::SetConcurrencyLimit(2), Hold::empty()).await.unwrap();
            let turn = |text: &str| crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"system","content":"build wait"},{"role":"user","content":text}]})).unwrap()).ok().unwrap().turn;
            let mut holding = None;
            for index in 0..7 {
                let text = if index == 6 { "HOLD".into() } else { format!("fill-{index}") };
                let mut ticket = conductor.admit(turn(&text), Hold::empty(), conductor.journal.begin(crate::journal::Api::Chat, 0), ResponseMode::Body).await.ok().unwrap();
                if index == 6 { holding = Some(ticket); } else { ended(&mut ticket).await; }
            }
            let mut waiting = conductor.admit(turn("waiter"), Hold::empty(), conductor.journal.begin(crate::journal::Api::Chat, 0), ResponseMode::Body).await.ok().unwrap();
            conductor.owner(OwnerAction::ReadModelNames, Hold::empty()).await.unwrap();
            let action = match kind {
                "keep" => crate::runtime_builds::Action::Search,
                "notice" => crate::runtime_builds::Action::Path(root.join("no-harness/server")),
                "replace" => crate::runtime_builds::Action::Build(conductor.builds_summary().unwrap().found[0].identity.clone()),
                _ => unreachable!(),
            };
            let (answered_before_release, notice) = {
            let owner = conductor.builds(action, Hold::empty());
            tokio::pin!(owner);
            let early = tokio::time::timeout(std::time::Duration::from_millis(150), &mut owner).await;
            let answered_before_release = early.is_ok();
            let mut owner_result = early.ok();
            launcher.release.notify_one();
            ended(holding.as_mut().unwrap()).await;
            ended(&mut waiting).await;
            if owner_result.is_none() { owner_result = Some(owner.await); }
            let result = owner_result.unwrap().unwrap();
            let notice = result.is_some();
            (answered_before_release, notice)
            };
            drop(conductor);
            task.await.unwrap();
            let agents = std::mem::take(&mut *launcher.agents.lock().unwrap());
            for agent in agents { agent.await.unwrap(); }
            let starts = launcher.starts.load(Ordering::SeqCst);
            std::fs::remove_dir_all(root).unwrap();
            assert_eq!((answered_before_release, notice, starts), (kind != "replace", kind == "notice", 2),
                "F7: {kind} must answer promptly or replace the pending limit demand");
        }).await.unwrap_or_else(|_| panic!("bounded owner build wait: {kind}"));
    }
}
