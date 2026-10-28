#![allow(clippy::disallowed_methods)]
use acp2api_acceptance::{
    Scenario, Setup, real_limit, rpc_script::RpcReply, setup::definitely_exited,
    system_check::requests,
};
use std::time::{Duration, Instant};

async fn initialized(scenario: &mut Scenario, count: usize) {
    let deadline = Instant::now() + real_limit(Duration::from_secs(3));
    loop {
        scenario.time.settle().await;
        if requests(scenario)
            .iter()
            .filter(|request| request["method"] == "initialize")
            .count()
            == count
        {
            return;
        }
        assert!(Instant::now() < deadline, "initialize observation cap: 3 s");
        let hub = scenario.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_waits_for_the_pending_start_host_and_its_replacement_runtime() {
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let mut scenario = Scenario::default();
        scenario.fake().script_call("initialize", RpcReply::Silent);
        let run = scenario.root.seed(&scenario.hub, &Setup::normal());
        let path = scenario.root.path.join("pending-run.toml");
        std::fs::write(&path, toml::to_string(&run).unwrap()).unwrap();
        let host = scenario.launch_host(&[], &path).id();
        initialized(&mut scenario, 1).await;
        scenario.time.advance_to(300_000).await;
        initialized(&mut scenario, 2).await;
        let runtimes: Vec<u32> = scenario
            .hub
            .entries()
            .iter()
            .filter(|entry| {
                entry.event["kind"] == "hello" && entry.event["hello"]["role"] == "server"
            })
            .map(|entry| entry.event["hello"]["pid"].as_u64().unwrap() as u32)
            .collect();
        assert_eq!(runtimes.len(), 2, "the silent start must be replaced");
        assert!(!definitely_exited(host), "the pending host must be live");
        scenario.stop().await;
        assert!(
            definitely_exited(host),
            "pending-start host PID {host} still holds the test root before cleanup"
        );
        assert!(runtimes.into_iter().all(definitely_exited));
        scenario.finish(true).await;
        assert!(!scenario.root.path.exists());
    })
    .await
    .expect("pending-start cleanup run cap: 30 s");
}
