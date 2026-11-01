use acp2api_acceptance::{network_observer::Observer, setup::binary};
use std::{process::Stdio, time::Duration};

async fn run_control(mode: &str) -> Result<(), String> {
    let observer = Observer::begin()?;
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
        .expect("single-attempt control cap: 5 seconds")
        .unwrap();
    observer.finish(pid, status)
}

#[tokio::test]
#[ignore = "Requires native OS sandbox or audit access; CI runs this explicitly"]
async fn short_and_failed_testnet_attempts_fail_the_destination_check() {
    run_control("loopback")
        .await
        .expect("literal 127.0.0.1 control is allowed");
    for mode in [
        "short",
        "failed",
        #[cfg(target_os = "macos")]
        "other-loopback",
    ] {
        let error = run_control(mode)
            .await
            .expect_err("forbidden control must fail the destination assertion");
        assert!(
            error.starts_with("FR-046 outbound attempt outside 127.0.0.1:"),
            "{mode}: {error}"
        );
    }
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "Requires native Windows audit access; CI runs this explicitly"]
async fn ipv6_localhost_fails_the_windows_destination_check() {
    let error = run_control("ipv6")
        .await
        .expect_err("planted ::1 control must fail the destination assertion");
    assert!(
        error.starts_with("FR-046 outbound attempt outside 127.0.0.1:"),
        "{error}"
    );
}
