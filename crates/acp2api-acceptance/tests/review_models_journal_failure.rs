#![cfg(unix)]

use acp2api_acceptance::{Scenario, Setup, model_check};
use std::time::{Duration, Instant};

fn failure_lines(s: &Scenario) -> usize {
    s.hub
        .entries()
        .iter()
        .filter(|e| {
            e.event["text"]
                .as_str()
                .is_some_and(|t| t.contains("cannot write request metadata"))
        })
        .count()
}

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn a_failed_log_write_keeps_the_model_list() {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    s.start().await;
    std::fs::remove_dir_all(s.root.path.join("data/logs")).unwrap();
    let a = s
        .client()
        .await
        .send(model_check::list())
        .finish(&mut s.time)
        .await;
    assert_eq!(a.status, 200, "model list after a failed log write");
    assert!(
        a.json()["data"].as_array().is_some_and(|d| !d.is_empty()),
        "model names: {}",
        a.json()
    );
    let end = Instant::now() + Duration::from_secs(5);
    while failure_lines(&s) < 1 && Instant::now() < end {
        let hub = s.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(end))
            .await
            .unwrap();
    }
    assert_eq!(failure_lines(&s), 1, "one stderr line for the dropped line");
    s.finish(true).await;
}
