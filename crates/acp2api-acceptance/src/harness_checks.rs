use crate::real_limit;
use crate::{
    adapters::{GuardedLauncher, HarnessClock},
    client::ApiCall,
    hub::{EVENT_CAP, Hub, HubLink, RECORD_CAP},
    setup::TestRoot,
    *,
};
use acp2api::{Clock, HoldKind, LaunchSpec, ProcessLauncher};
use serde_json::json;
use std::{
    io::Write,
    time::{Duration, Instant},
};
fn world() -> Scenario {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s
}
#[test]
fn runtime_line_without_a_reader_returns_a_hub_failure() {
    use std::io::BufRead;

    let hub = Hub::new("missing-reader-test".into());
    let link = HubLink::new(&hub);
    let mut reader = link
        .connect(json!({"op":"hold","kind":"RuntimeLine","add":true,"reader":"unregistered-task"}))
        .unwrap();
    reader
        .get_mut()
        .set_read_timeout(Some(real_limit(Duration::from_secs(2))))
        .unwrap();
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    assert!(
        !response.is_empty(),
        "a missing reader must return a failure instead of ending the hub connection"
    );
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();
    let message = "RuntimeLine hold without an output read for task unregistered-task";
    assert_eq!(response, json!({"error":message}));
    assert_eq!(hub.failure().as_deref(), Some(message));
    assert_eq!(link.call(json!({"op":"now"})), json!({"now":0}));
    hub.close();
}
#[cfg(unix)]
fn send_signal(pid: i32, signal: i32) {
    assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
}
#[cfg(unix)]
fn process_is_stopped(pid: i32) -> bool {
    let mut status = 0;
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED | libc::WNOHANG) };
    assert!(result >= 0);
    result == pid && libc::WIFSTOPPED(status)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn self_exit_keeps_output_pending_until_read_and_processed() {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut s = world();
    let run = s.root.seed(&s.hub, &Setup::normal());
    let launcher = GuardedLauncher::new(&run.fake_program, run.hub);
    let mut child = launcher
        .launch(LaunchSpec {
            program: s
                .root
                .path
                .join("data/official-runtime")
                .join(if cfg!(windows) {
                    "agy_acp_server.exe"
                } else {
                    "agy_acp_server.par"
                }),
            args: vec![],
            env_set: vec![],
            cwd: s.root.path.clone(),
            own_temp: None,
        })
        .unwrap();
    // One output and one exit command; the fake and hub retain their run and byte caps.
    let line = r#"{"jsonrpc":"2.0","id":1,"result":{"stopReason":"end_turn"}}"#;
    s.hub.command("normal", json!({"op":"Out","line":line}));
    s.hub.command("normal", json!({"op":"Exit","code":0}));
    tokio::time::timeout(real_limit(Duration::from_secs(3)), child.tree.exited())
        .await
        .unwrap();
    wait_for(
        &s.hub,
        Instant::now() + real_limit(Duration::from_secs(3)),
        || {
            s.hub
                .entries()
                .iter()
                .any(|e| e.event["kind"] == "process_end")
        },
    )
    .await;
    let counts = s.hub.counts();
    assert_eq!((counts.ordered, counts.read, counts.processed), (1, 0, 0));
    assert!(
        !counts.balanced(),
        "a self-exited fake's output must still be read: {counts:?}"
    );
    let mut output = BufReader::new(child.stdout);
    let mut received = String::new();
    tokio::time::timeout(
        real_limit(Duration::from_secs(3)),
        output.read_line(&mut received),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(received, format!("{line}\n"));
    let clock = HarnessClock::new(HubLink::new(&s.hub));
    let hold = clock.hold(HoldKind::RuntimeLine);
    assert!(
        !s.hub.counts().balanced(),
        "reading alone must not complete the pending output"
    );
    drop(hold);
    s.time.settle().await;
    let counts = s.hub.counts();
    assert_eq!((counts.ordered, counts.read, counts.processed), (1, 1, 1));
    assert!(counts.balanced());
    s.finish(true).await;
}
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn product_stop_closes_only_its_undelivered_lines() {
    use tokio::io::AsyncWriteExt;

    let mut s = world();
    let run = s.root.seed(&s.hub, &Setup::normal());
    let launcher = GuardedLauncher::new(&run.fake_program, run.hub);
    let folder = s.root.path.join("data/official-runtime");
    let descriptor = folder.join("localharness_external");
    let original = std::fs::read_to_string(&descriptor).unwrap();
    let mut children = Vec::new();
    for label in ["normal", "second"] {
        std::fs::write(
            &descriptor,
            original.replace("label = \"normal\"", &format!("label = \"{label}\"")),
        )
        .unwrap();
        let mut child = launcher
            .launch(LaunchSpec {
                program: folder.join("agy_acp_server.par"),
                args: vec![],
                env_set: vec![],
                cwd: s.root.path.clone(),
                own_temp: None,
            })
            .unwrap();
        let pid = child.tree.pid() as i32;
        let deadline = Instant::now() + real_limit(Duration::from_secs(3));
        wait_for(&s.hub, deadline, || {
            s.hub
                .entries()
                .iter()
                .any(|e| e.event["kind"] == "hello" && e.event["hello"]["pid"] == pid)
        })
        .await;
        send_signal(pid, libc::SIGSTOP);
        wait_for(&s.hub, deadline, || process_is_stopped(pid)).await;
        // Each fake has one input and one output in flight when the process is killed.
        child.stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"session/cancel\",\"params\":{\"sessionId\":\"finished\"}}\n").await.unwrap();
        s.hub.command(label, json!({"op":"Out","line":r#"{"jsonrpc":"2.0","id":136,"result":{"stopReason":"cancelled"}}"#}));
        children.push(child);
    }
    assert!(
        !s.hub.counts().balanced(),
        "live fakes must report and consume their lines"
    );
    for (index, child) in children.into_iter().enumerate() {
        let pid = child.tree.pid() as i32;
        let mut stop = child
            .tree
            .stop(std::sync::Arc::new(acp2api::os::SystemClock::default()));
        assert!(matches!(
            futures_util::poll!(stop.as_mut()),
            std::task::Poll::Pending
        ));
        assert!(
            !s.hub.counts().balanced(),
            "a stop must not close IO before the process connection ends"
        );
        send_signal(pid, libc::SIGKILL);
        tokio::time::timeout(real_limit(Duration::from_secs(3)), stop)
            .await
            .unwrap();
        wait_for(
            &s.hub,
            Instant::now() + real_limit(Duration::from_secs(3)),
            || {
                s.hub
                    .entries()
                    .iter()
                    .filter(|e| e.event["kind"] == "process_end")
                    .count()
                    == index + 1
            },
        )
        .await;
        if index == 0 {
            assert!(
                !s.hub.counts().balanced(),
                "one process exit must not hide a live process's undelivered lines"
            );
        }
    }
    let counts = s.hub.counts();
    assert_eq!(
        (
            counts.stdin_written,
            counts.stdin_got,
            counts.ordered,
            counts.read,
            counts.processed
        ),
        (2, 0, 2, 0, 0)
    );
    assert!(
        counts.balanced(),
        "product-stopped processes cannot deliver more lines: {counts:?}"
    );
    s.time.settle().await;
    s.finish(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settle_drains_real_subprocess_bursts() {
    let mut s = world();
    let mut plan = TurnPlan::new();
    for _ in 0..200 {
        plan = plan.chunk("x");
    }
    s.fake().next_prompt(plan.usage(123, 1_048_576));
    let call = s
        .client()
        .await
        .send(ApiCall::one_user_message("POST /v1/messages", "burst"));
    let answer = call.finish(&mut s.time).await;
    assert_eq!(answer.status, 200);
    assert_eq!(
        answer.json()["content"][0]["text"].as_str().unwrap().len(),
        200
    );
    s.time.settle().await;
    let counts = s.hub.counts();
    assert!(counts.balanced(), "{counts:?}");
    assert_eq!(counts.ordered, counts.processed);
    assert!(counts.ordered >= 204);
    assert_eq!(s.fake().record().offered_tools().len(), 1);
    s.finish(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn virtual_external_inputs_precede_product_timers_and_waits_release_holds() {
    let mut s = world();
    s.fake().next_prompt(
        TurnPlan::new()
            .after(Duration::from_millis(200))
            .chunk("boundary")
            .usage(10, 100),
    );
    let call = s
        .client()
        .await
        .send(ApiCall::one_user_message("POST /v1/messages", "boundary"));
    s.time.settle().await;
    let clock = HarnessClock::new(HubLink::new(&s.hub));
    let sleeper = clock.sleep_until(200);
    let hub = s.hub.clone();
    let task = tokio::spawn(async move {
        let _timer_work = sleeper.await;
        let counts = hub.counts();
        assert_eq!(
            counts.ordered, counts.processed,
            "external lines must finish before timer"
        );
        hub.shared
            .0
            .lock()
            .unwrap()
            .record(json!({"kind":"timer_callback"}));
    });
    s.time.advance_to(200).await;
    task.await.unwrap();
    assert_eq!(s.hub.now(), 200, "the external-input boundary stays exact");
    let answer = call.finish(&mut s.time).await;
    assert_eq!(answer.json()["content"][0]["text"], "boundary");
    assert_eq!(s.hub.now(), 400, "FR-028 adds its 200 ms completion wait");
    let events = s.hub.entries();
    let timer = events
        .iter()
        .position(|e| e.event["kind"] == "timer_callback")
        .unwrap();
    let text = events
        .iter()
        .position(|e| {
            e.event["command"]["line"]
                .as_str()
                .is_some_and(|v| v.contains("agent_message_chunk"))
        })
        .unwrap();
    assert!(text < timer);
    s.finish(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn holds_prevent_clock_advance_and_cancelled_sleepers_do_not_block() {
    let hub = hub::Hub::new(setup::random_hex(24));
    let clock = HarnessClock::new(HubLink::new(&hub));
    let hold = clock.hold(HoldKind::TurnEvent);
    let mut time = timeline::TimeLine::new(hub.clone());
    let timer = clock.sleep_until(50);
    drop(timer);
    let advancing = tokio::spawn(async move {
        time.advance_to(100).await;
    });
    tokio::task::yield_now().await;
    assert_eq!(hub.now(), 0);
    drop(hold);
    advancing.await.unwrap();
    assert_eq!(hub.now(), 100);
    assert!(hub.counts().balanced());
    hub.close();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timer_construction_keeps_virtual_time_at_zero_until_the_future_is_built() {
    let hub = Hub::new("timer-construction-test".into());
    let hub::RpcPause {
        link,
        publication,
        release,
        peer,
    } = hub::pause_rpc_after(&hub, 8, |request, _| request["op"] == "sleep");
    let building = tokio::task::spawn_blocking(move || HarnessClock::new(link).sleep_until(50));
    tokio::time::timeout(real_limit(Duration::from_secs(3)), publication)
        .await
        .unwrap()
        .unwrap();
    let mut time = timeline::TimeLine::new(hub.clone());
    let mut advancing = tokio::spawn(async move { time.advance_to(50).await });
    let blocked = tokio::time::timeout(real_limit(Duration::from_millis(100)), &mut advancing)
        .await
        .is_err();
    let during_construction = hub.now();
    release.send(()).unwrap();
    let timer = building.await.unwrap();
    let completed = tokio::spawn(async move {
        drop(timer.await);
    });
    tokio::time::timeout(real_limit(Duration::from_secs(3)), &mut advancing)
        .await
        .unwrap()
        .unwrap();
    completed.await.unwrap();
    peer.join().unwrap();
    let after_construction = hub.now();
    hub.close();
    // Check after cleanup so a red run retains no socket or timer.
    assert!(blocked, "advance must wait for timer construction");
    assert_eq!(
        during_construction, 0,
        "an incomplete timer is work in progress"
    );
    assert_eq!(after_construction, 50);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guard_refuses_unknown_bytes_and_tracks_fake_descendants() {
    let s = world();
    let run = s.root.seed(&s.hub, &Setup::normal());
    let launcher = GuardedLauncher::new(&run.fake_program, run.hub);
    let launch = |path| LaunchSpec {
        program: path,
        args: vec![],
        env_set: vec![],
        cwd: s.root.path.clone(),
        own_temp: None,
    };
    let denied = s.root.path.join("builds/denied");
    std::fs::write(&denied, b"not an executable").unwrap();
    assert!(matches!(
        launcher.launch(launch(denied)),
        Err(acp2api::LaunchError::Refused { .. })
    ));
    let path = s
        .root
        .path
        .join("data/official-runtime")
        .join(if cfg!(windows) {
            "agy_acp_server.exe"
        } else {
            "agy_acp_server.par"
        });
    let child = launcher.launch(launch(path)).unwrap();
    let deadline = Instant::now() + real_limit(Duration::from_secs(2));
    wait_for(&s.hub, deadline, || {
        s.hub.entries().iter().any(|e| e.event["kind"] == "hello")
    })
    .await;
    s.hub
        .command("normal", json!({"op":"StartHarness","count":2}));
    wait_for(&s.hub, deadline, || {
        s.hub
            .entries()
            .iter()
            .filter(|e| e.event["kind"] == "hello")
            .count()
            == 3
    })
    .await;
    drop(child.stdin);
    #[cfg(not(windows))]
    {
        let mut tree = child.tree;
        tokio::time::timeout(real_limit(Duration::from_secs(2)), tree.exited())
            .await
            .unwrap();
    }
    #[cfg(windows)]
    child
        .tree
        .stop(std::sync::Arc::new(acp2api::os::SystemClock::default()))
        .await;
    wait_for(&s.hub, deadline, || {
        s.hub
            .entries()
            .iter()
            .filter(|e| e.event["kind"] == "process_end")
            .count()
            == 3
    })
    .await;
    let entries = s.hub.entries();
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.event["kind"] == "launch" && e.event["allowed"] == false)
            .count(),
        1
    );
    s.hub.close();
    s.root.cleanup();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fake_enforces_line_and_duration_caps_without_files() {
    for (caps, exit) in [("line_cap = 3\nrun_ms = 2000\n", 70), ("run_ms = 80\n", 71)] {
        let s = world();
        let run = s.root.seed(&s.hub, &Setup::normal());
        let folder = s.root.path.join("data/official-runtime");
        let descriptor = folder.join(if cfg!(windows) {
            "localharness_external.exe"
        } else {
            "localharness_external"
        });
        std::fs::OpenOptions::new()
            .append(true)
            .open(&descriptor)
            .unwrap()
            .write_all(caps.as_bytes())
            .unwrap();
        let launcher = GuardedLauncher::new(&run.fake_program, run.hub);
        let mut child = launcher
            .launch(LaunchSpec {
                program: folder.join(if cfg!(windows) {
                    "agy_acp_server.exe"
                } else {
                    "agy_acp_server.par"
                }),
                args: vec![],
                env_set: vec![],
                cwd: s.root.path.clone(),
                own_temp: None,
            })
            .unwrap();
        if exit == 70 {
            use tokio::io::AsyncWriteExt;
            for n in 0..4 {
                let line = format!(
                    "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/ignored\",\"params\":{{\"n\":{n}}}}}\n"
                );
                child.stdin.write_all(line.as_bytes()).await.unwrap();
            }
        }
        let status = tokio::time::timeout(real_limit(Duration::from_secs(3)), child.tree.exited())
            .await
            .unwrap();
        assert_eq!(status.code(), Some(exit), "entries: {:?}", s.hub.entries());
        assert_eq!(std::fs::read_dir(folder).unwrap().count(), 2);
        s.hub.close();
        s.root.cleanup();
    }
}
#[test]
fn hub_terminal_and_root_caps_are_enforced() {
    let hub = hub::Hub::new(setup::random_hex(24));
    {
        let mut s = hub.shared.0.lock().unwrap();
        for _ in 0..EVENT_CAP {
            s.record(json!({"kind":"cap"}));
        }
        s.record(json!({"kind":"overflow"}));
        assert!(s.failure.as_ref().unwrap().contains("hub record cap"));
        assert!(s.bytes < RECORD_CAP);
    }
    hub.close();
    let hub = hub::Hub::new(setup::random_hex(24));
    hub.terminal(&vec![b'x'; 4 * 1024 * 1024 + 1]);
    assert!(hub.failure().unwrap().contains("terminal capture cap"));
    hub.close();
    let root = TestRoot::new();
    let file = std::fs::File::create(root.path.join("sparse")).unwrap();
    file.set_len(128 * 1024 * 1024 + 1).unwrap();
    assert!(std::panic::catch_unwind(|| root.check_size()).is_err());
    std::fs::remove_dir_all(root.path).unwrap();
}
async fn wait_for(hub: &hub::Hub, deadline: Instant, predicate: impl Fn() -> bool) {
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "harness observation deadline: {:?}",
            hub.entries()
        );
        let hub = hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
    }
}

#[test]
fn normal_setup_has_verifiable_hash_and_isolated_folders() {
    use argon2::{PasswordVerifier, password_hash::phc::PasswordHash};
    let s = world();
    s.root.seed(&s.hub, &Setup::normal());
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(s.root.path.join("data/config.toml")).unwrap())
            .unwrap();
    let hash = PasswordHash::new(config["management_password_hash"].as_str().unwrap()).unwrap();
    assert!(
        argon2::Argon2::default()
            .verify_password(s.root.password.as_bytes(), &hash)
            .is_ok()
    );
    assert_eq!(s.root.password.len(), 23);
    assert_eq!(s.root.key.len(), 51);
    for folder in [
        "account",
        "home",
        "local-app-data",
        "temp",
        "path",
        "exe",
        "data",
        "builds",
    ] {
        assert!(s.root.path.join(folder).is_dir());
    }
    assert!(!s.root.path.join("data/state.json").exists());
    s.hub.close();
    s.root.cleanup();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_fake_plan_resumes_without_holding_virtual_time() {
    let mut s = world();
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::HoldUntil("release".into()));
    plan.steps.push(TurnStep::Chunk("released".into()));
    s.fake().next_prompt(plan);
    let call = s
        .client()
        .await
        .send(ApiCall::one_user_message("POST /v1/messages", "gate"));
    s.time.settle().await;
    assert!(s.hub.counts().balanced());
    s.time.advance_to(4 * 60 * 1000).await;
    assert_eq!(s.hub.now(), 4 * 60 * 1000, "the gate permits time to pass");
    s.fake().open_gate("release");
    let answer = call.finish(&mut s.time).await;
    assert_eq!(answer.json()["content"][0]["text"], "released");
    assert_eq!(s.hub.now(), 4 * 60 * 1000 + 200);
    s.finish(true).await;
}

#[test]
fn setup_extensions_change_release_facts_without_changing_normal() {
    let s = world();
    let changed = s
        .root
        .seed(&s.hub, &Setup::normal().highest_concurrency_limit(2));
    assert_eq!(changed.release.highest, 2);
    let normal = s.root.seed(&s.hub, &Setup::normal());
    assert_eq!(normal.release.highest, 3);
    s.hub.close();
    s.root.cleanup();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_has_a_new_stop_signal_and_answers_a_second_real_request() {
    let mut s = world();
    let first = s
        .client()
        .await
        .send(ApiCall::one_user_message("POST /v1/messages", "first"));
    assert_eq!(first.finish(&mut s.time).await.status, 200);
    s.restart().await;
    assert!(
        !s.hub.shared.0.lock().unwrap().stop,
        "a restarted host must get a fresh stop signal"
    );
    let second = s
        .client()
        .await
        .send(ApiCall::one_user_message("POST /v1/messages", "second"));
    assert_eq!(second.finish(&mut s.time).await.status, 200);
    s.finish(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initialize_reports_each_running_fake_descriptor_version() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let s = world();
    let run = s.root.seed(&s.hub, &Setup::normal());
    let program = crate::build_fixture::SERVER;
    let original = s.root.path.join("data/official-runtime");
    let other = s.root.path.join("builds/version-1.5.0");
    crate::build_fixture::install(&original, &other, "1.5.0", "version-1.5.0").unwrap();
    let launcher = GuardedLauncher::new(&run.fake_program, run.hub);
    let mut children = Vec::new();
    for (folder, label, version) in [
        (original, "normal", "1.4.0"),
        (other, "version-1.5.0", "1.5.0"),
    ] {
        let child = launcher
            .launch(LaunchSpec {
                program: folder.join(program),
                args: vec![],
                env_set: vec![],
                cwd: s.root.path.clone(),
                own_temp: None,
            })
            .unwrap();
        children.push((child, label, version));
    }
    wait_for(
        &s.hub,
        Instant::now() + real_limit(Duration::from_secs(3)),
        || {
            s.hub
                .entries()
                .iter()
                .filter(|e| e.event["hello"]["role"] == "server")
                .count()
                == 2
        },
    )
    .await;
    for (child, _, version) in &mut children {
        child
            .stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n")
            .await
            .unwrap();
        let mut line = String::new();
        tokio::time::timeout(
            real_limit(Duration::from_secs(3)),
            BufReader::new(&mut child.stdout).read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            reply["result"]["agentInfo"]["version"], *version,
            "initialize must report this process's descriptor version"
        );
    }
    for (mut child, label, _) in children {
        s.hub.command(label, json!({"op":"Exit","code":0}));
        tokio::time::timeout(real_limit(Duration::from_secs(3)), child.tree.exited())
            .await
            .unwrap();
    }
    s.hub.close();
    s.root.cleanup();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rearmed_timers_wait_for_other_work_before_advancing_again() {
    let hub = hub::Hub::new(setup::random_hex(24));
    let clock = HarnessClock::new(HubLink::new(&hub));
    let first = clock.sleep_until(50);
    let (entered, started) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let callback = first.await;
        let next = clock.sleep_until(100);
        let other_work = clock.hold(HoldKind::TurnEvent);
        drop(callback);
        let holding = tokio::spawn(async move {
            let _work = other_work;
            released.await.unwrap();
        });
        entered.send(()).unwrap();
        let _callback = next.await;
        holding.await.unwrap();
    });
    let mut time = timeline::TimeLine::new(hub.clone());
    let mut advancing = tokio::spawn(async move { time.advance_to(100).await });
    tokio::time::timeout(real_limit(Duration::from_secs(3)), started)
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(real_limit(Duration::from_millis(100)), &mut advancing)
            .await
            .is_err(),
        "a rearmed task still waits for other work"
    );
    assert_eq!(hub.now(), 50, "the next timer cannot pass the live hold");
    release.send(()).unwrap();
    task.await.unwrap();
    advancing.await.unwrap();
    assert_eq!(hub.now(), 100);
    assert!(hub.counts().balanced());
    hub.close();
}

// Separate timer construction from event holds so each test pauses its own boundary.
struct ClockConnections {
    work: HarnessClock,
    timers: HarnessClock,
}
impl Clock for ClockConnections {
    fn now(&self) -> acp2api::Moment {
        self.work.now()
    }
    fn wall(&self) -> std::time::SystemTime {
        self.work.wall()
    }
    fn sleep_until(&self, at: acp2api::Moment) -> acp2api::BoxFuture<'static, acp2api::Hold> {
        self.timers.sleep_until(at)
    }
    fn hold(&self, kind: HoldKind) -> acp2api::Hold {
        self.work.hold(kind)
    }
}

async fn start_with_clock_link(
    s: &mut Scenario,
    clock: std::sync::Arc<dyn Clock>,
) -> (
    std::thread::JoinHandle<std::process::ExitCode>,
    client::ApiClient,
) {
    let run = s.root.seed(&s.hub, &Setup::normal());
    let mut ports = run.ports();
    ports.clock = clock;
    let args = [
        "acp2api-test-host".into(),
        "--data-dir".into(),
        s.root.path.join("data").into_os_string(),
    ];
    // A child host would discard the proxied connection when it reads the run file.
    let host = std::thread::spawn(move || acp2api::run(ports, args, run.environment));
    wait_for(
        &s.hub,
        Instant::now() + real_limit(Duration::from_secs(3)),
        || s.hub.entries().iter().any(|e| e.event["kind"] == "launch"),
    )
    .await;
    let client = client::ApiClient::new(
        format!("127.0.0.1:{}", s.root.port),
        s.root.key.clone(),
        s.hub.clone(),
    );
    assert_eq!(client.model_names().finish(&mut s.time).await.status, 200);
    s.time.settle().await;
    (host, client)
}

async fn stop_with_clock_link(
    s: &mut Scenario,
    host: std::thread::JoinHandle<std::process::ExitCode>,
) -> std::process::ExitCode {
    s.hub.stop();
    let mut stopped = tokio::task::spawn_blocking(move || host.join().unwrap());
    loop {
        s.time.settle().await;
        if stopped.is_finished() {
            return stopped.await.unwrap();
        }
        if let Some(next) = s.time.next_deadline() {
            s.time.advance_to(next).await;
        } else {
            let hub = s.hub.clone();
            tokio::select! {
                exit = &mut stopped => return exit.unwrap(),
                _ = tokio::task::spawn_blocking(move || {
                    hub.wait_change(Instant::now() + real_limit(Duration::from_secs(3)))
                }) => {}
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_event_is_read_before_settle_allows_the_next_virtual_instant() {
    tokio::time::timeout(real_limit(Duration::from_secs(10)), async {
        let mut s = world();
        let hub::RpcPause {
            link,
            publication,
            release,
            peer,
        } = hub::pause_rpc_after(&s.hub, 512, move |request, reply| {
            request["op"] == "hold"
                && request["kind"] == "TurnEvent"
                && request["add"] == false
                && reply["now"] == 15_000
        });
        let clock = ClockConnections {
            work: HarnessClock::new(link),
            timers: HarnessClock::new(HubLink::new(&s.hub)),
        };
        let (host, client) = start_with_clock_link(&mut s, std::sync::Arc::new(clock)).await;
        let mut plan = TurnPlan::new();
        plan.steps.push(TurnStep::HoldUntil("stream-window".into()));
        plan.steps.push(TurnStep::Chunk("released".into()));
        s.fake().next_prompt(plan);
        let mut request = ApiCall::one_user_message("POST /v1/messages", "stream event arrival");
        request.body["stream"] = json!(true);
        let call = client.send(request);
        s.time.settle().await;
        let mut time = s.time.clone();
        let advancing = tokio::spawn(async move { time.advance_to(15_000).await });
        publication.await.unwrap();
        // Pause a TurnEvent release before its owner can do any more work.
        // Hyper's queue must have written the bytes before it drops their owner.
        let wire = call.wire();
        let deadline = Instant::now() + real_limit(Duration::from_millis(100));
        while !wire.lock().unwrap().blocks.iter().any(|b| b.at == 15_000 && b.event == "ping")
            && Instant::now() < deadline
        {
            let hub = s.hub.clone();
            tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                .await
                .unwrap();
        }
        let before_release = wire.lock().unwrap().blocks.clone();
        release.send(()).unwrap();
        advancing.await.unwrap();
        s.time.advance_to(30_000).await;
        s.fake().open_gate("stream-window");
        let answer = call.finish(&mut s.time).await;
        let exit = stop_with_clock_link(&mut s, host).await;
        peer.join().unwrap();
        s.finish(true).await;
        // Finish both peers before assertions so the red run keeps no process or socket.
        assert_eq!(exit, std::process::ExitCode::SUCCESS);
        assert_eq!(answer.status, 200);
        assert!(
            before_release.iter().any(|b| b.at == 15_000 && b.event == "ping"),
            "the client must read the 15000 keep-alive before its hold ends: {before_release:?}"
        );
    })
    .await
    .expect("one readiness request and one stream request; 10 s run cap; existing hub and 8 MiB answer caps");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restarted_runtime_sends_initialize_before_its_answer_deadline_can_pass() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    tokio::time::timeout(real_limit(Duration::from_secs(10)), async {
        let mut s = world();
        let armed = Arc::new(AtomicBool::new(false));
        let restarting = armed.clone();
        let mut deadline_registered = false;
        let hub::RpcPause {
            link,
            publication,
            release,
            peer,
        } = hub::pause_rpc_after(&s.hub, 512, move |request, _| {
            if restarting.load(Ordering::SeqCst)
                && request["op"] == "sleep"
                && request["at"] == 300_000
            {
                deadline_registered = true;
            }
            deadline_registered
                && request["op"] == "hold"
                && request["kind"] == "TurnEvent"
                && request["add"] == false
        });
        let clock = ClockConnections {
            work: HarnessClock::new(HubLink::new(&s.hub)),
            timers: HarnessClock::new(link),
        };
        let (host, _client) = start_with_clock_link(&mut s, Arc::new(clock)).await;
        armed.store(true, Ordering::SeqCst);
        s.hub.command("normal", json!({"op":"Exit","code":17}));
        publication.await.unwrap();
        // The timer constructor released its own hold; the answer has not had a poll yet.
        let mut time = s.time.clone();
        let mut advancing = tokio::spawn(async move { time.advance_to(300_000).await });
        let blocked = tokio::time::timeout(real_limit(Duration::from_millis(100)), &mut advancing)
            .await
            .is_err();
        let before_first_poll = s.hub.now();
        release.send(()).unwrap();
        if blocked {
            advancing.await.unwrap();
        }
        let initialization_times: Vec<_> = s
            .hub
            .entries()
            .iter()
            .filter_map(|entry| {
                let call: serde_json::Value =
                    serde_json::from_str(entry.event["line"].as_str()?).ok()?;
                (call["method"] == "initialize").then_some(entry.at)
            })
            .collect();
        let exit = stop_with_clock_link(&mut s, host).await;
        peer.join().unwrap();
        s.finish(true).await;
        assert_eq!(exit, std::process::ExitCode::SUCCESS);
        assert!(blocked, "the first answer poll is still work in progress");
        assert_eq!(
            before_first_poll, 0,
            "a registered deadline cannot pass before initialize starts"
        );
        assert_eq!(initialization_times, [0, 0]);
    })
    .await
    .expect("one readiness request and one restart; 10 s run cap; existing fake and hub log caps");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_stream_bodies_are_read_before_settle_allows_the_next_virtual_instant() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let mut observations = Vec::new();
    for (endpoint, authorized, at, status, content) in [
        // Completed JSON after the 200 ms error window.
        ("POST /v1/messages", true, 200, 200, "non-stream body"),
        // Refusal before admission when the local API key is absent.
        ("POST /v1/messages", false, 0, 401, "error"),
        // The cached model list needs no prompt turn.
        ("GET /v1/models", true, 0, 200, "gemini-3.8-flash"),
        // The management panel shows its login page.
        ("GET /", true, 0, 200, "password"),
    ] {
        tokio::time::timeout(real_limit(Duration::from_secs(10)), async {
            let mut s = world();
            let armed = Arc::new(AtomicBool::new(false));
            let observing = armed.clone();
            let observed_hub = s.hub.clone();
            let hub::RpcPause { link, publication, release, peer } =
                hub::pause_rpc_after(&s.hub, 512, move |request, reply| {
                    observing.load(Ordering::SeqCst)
                        && request["op"] == "hold"
                        && request["add"] == false
                        && reply["now"] == at
                        && observed_hub.counts().holds == 0
                });
            let clock = ClockConnections {
                work: HarnessClock::new(link),
                timers: HarnessClock::new(HubLink::new(&s.hub)),
            };
            let (host, client) = start_with_clock_link(&mut s, Arc::new(clock)).await;
            if authorized && endpoint == "POST /v1/messages" {
                s.fake().next_prompt(TurnPlan::new().chunk("non-stream body").usage(10, 100));
            }
            armed.store(true, Ordering::SeqCst);
            let request = ApiCall::one_user_message(endpoint, "body arrival");
            let call = if authorized { client } else { client.without_local_api_key() }.send(request);
            let mut time = s.time.clone();
            let advancing = tokio::spawn(async move { time.advance_to(at + 1).await });
            publication.await.unwrap();
            // The last work release is paused before its owner can return to hyper.
            let wire = call.wire();
            let deadline = Instant::now() + real_limit(Duration::from_millis(100));
            while wire.lock().unwrap().end_at.is_none() && Instant::now() < deadline {
                let hub = s.hub.clone();
                tokio::task::spawn_blocking(move || hub.wait_change(deadline)).await.unwrap();
            }
            let before_release = wire.lock().unwrap().end_at;
            release.send(()).unwrap();
            advancing.await.unwrap();
            let answer = call.finish(&mut s.time).await;
            let exit = stop_with_clock_link(&mut s, host).await;
            peer.join().unwrap();
            s.finish(true).await;
            assert_eq!(exit, std::process::ExitCode::SUCCESS);
            assert_eq!(answer.status, status);
            assert!(String::from_utf8_lossy(&answer.body).contains(content));
            observations.push((endpoint, authorized, at, before_release));
        }).await.expect("one readiness request and one body request; 10 s run cap; existing hub and 8 MiB answer caps");
    }
    // Clean up all four hosts before the assertion, including on a red run.
    assert!(
        observations
            .iter()
            .all(|(_, _, at, read)| *read == Some(*at)),
        "the client must read each non-stream body before the last hold ends: {observations:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_stream_body_preserves_content_length_framing() {
    tokio::time::timeout(real_limit(Duration::from_secs(10)), async {
        let mut s = world();
        let call = s.client().await.model_names();
        let answer = call.finish(&mut s.time).await;
        s.finish(true).await;
        assert_eq!(answer.status, 200);
        let content_length = answer.body.len().to_string();
        assert_eq!(
            (
                answer.headers.get("content-length").map(String::as_str),
                answer.headers.get("transfer-encoding").map(String::as_str),
            ),
            (Some(content_length.as_str()), None),
        );
    })
    .await
    .expect("one models request; 10 s run cap; existing hub and 8 MiB answer caps");
}
