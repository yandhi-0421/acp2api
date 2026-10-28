use acp2api_acceptance::account_check::AccountState;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_test_host_owns_both_locks_then_go_runs_acp2api_in_that_process() {
    let mut account = AccountState::default();
    account.first("D1");
    account.hold_first().await;
    let pid = account.instances[0].pid();
    assert!(!account.instances[0].published());
    account.first_files_unchanged();
    account.release_first().await;
    assert_eq!(
        account.instances[0].pid(),
        pid,
        "go enters acp2api::run in the lock holder process"
    );
    assert!(account.instances[0].published());
    account.instances[0].stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(
    clippy::disallowed_methods,
    reason = "Bounded OS lock-release observation in the test harness."
)]
async fn killing_the_held_host_releases_both_locks_without_publication() {
    use acp2api_acceptance::real_limit;
    use std::time::{Duration, Instant};
    let mut account = AccountState::default();
    account.first("D1");
    account.hold_first().await;
    account.instances[0].kill();
    let deadline = Instant::now() + real_limit(Duration::from_secs(3));
    for name in ["startup.lock", "instance.lock"] {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(account.instances[0].root.path.join("account").join(name))
            .unwrap();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    assert!(
                        Instant::now() < deadline,
                        "OS release of killed fixture locks: infrastructure cap"
                    );
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                Err(error) => panic!("read killed fixture lock: {error}"),
            }
        }
    }
    assert!(
        !account.instances[0].published(),
        "a crash never calls acp2api::run or publishes an address"
    );
    account.first_files_unchanged();
}
