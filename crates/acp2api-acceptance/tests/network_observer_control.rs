use acp2api_acceptance::{network_observer::Observer, setup::binary};
use std::{process::Stdio, time::Duration};

#[tokio::test]
#[ignore = "Requires native OS audit access; CI and the orchestrator run this explicitly"]
async fn short_and_failed_testnet_attempts_fail_the_destination_check() {
    for mode in ["loopback", "short", "failed"] {
        let observer = Observer::begin().expect("OS network observer");
        let mut command =
            tokio::process::Command::from(observer.command(&binary("network-observer-control")));
        let mut child = command
            .arg(mode)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("single-attempt target cap: 5 seconds")
            .unwrap();
        assert!(
            status.success(),
            "bounded target completed its actual attempt"
        );
        let observation = observer
            .finish(pid)
            .expect("complete OS evidence, including both canaries");
        if mode == "loopback" {
            observation
                .only_loopback()
                .expect("literal 127.0.0.1 is allowed");
        } else {
            let error = observation
                .only_loopback()
                .expect_err("planted TEST-NET attempt must fail the destination assertion");
            assert!(error.starts_with("FR-046 outbound attempt outside 127.0.0.1:"));
        }
    }
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore = "Requires sandbox-exec and unified log access outside Codex"]
async fn ipv6_localhost_is_denied_by_the_literal_ipv4_profile() {
    let observer = Observer::begin().unwrap();
    let mut command =
        tokio::process::Command::from(observer.command(&binary("network-observer-control")));
    let mut child = command
        .arg("ipv6")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    observer
        .finish(pid)
        .expect("complete log window")
        .only_loopback()
        .expect_err("::1 must produce a sandbox deny");
}
