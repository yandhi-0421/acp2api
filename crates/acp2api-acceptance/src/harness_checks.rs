use crate::real_limit;
use crate::{
    adapters::{GuardedLauncher, HarnessClock},
    client::ApiCall,
    hub::{EVENT_CAP, HubLink, RECORD_CAP},
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
    s.time.advance_to(60 * 60 * 1000).await;
    assert_eq!(s.hub.now(), 60 * 60 * 1000, "the gate permits time to pass");
    s.fake().open_gate("release");
    let answer = call.finish(&mut s.time).await;
    assert_eq!(answer.json()["content"][0]["text"], "released");
    assert_eq!(s.hub.now(), 60 * 60 * 1000 + 200);
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
