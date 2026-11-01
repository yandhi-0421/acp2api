use crate::World;
use acp2api_acceptance::{Scenario, network_observer::Observer};
use cucumber::{given, then, when};
use futures_util::FutureExt;
use std::panic::{AssertUnwindSafe, resume_unwind};

fn checked_host_result<T>(scenario: &mut Scenario, result: std::thread::Result<T>) -> T {
    scenario
        .check_network_host()
        .expect("observed host must stay alive until harness stop");
    match result {
        Ok(value) => value,
        Err(error) => resume_unwind(error),
    }
}

#[given("acp2api runs with a fake ACP agent in place of the official runtime")]
async fn observed(w: &mut World) {
    crate::network_support::prepare(&mut w.scenario);
    let observer = Observer::begin().expect("OS network observer and initial control");
    let result = AssertUnwindSafe(w.scenario.start_network_observed(&observer, &[]))
        .catch_unwind()
        .await;
    let pid = checked_host_result(&mut w.scenario, result);
    w.network_observer = Some((observer, pid));
}

#[when("an API client sends requests, the owner signs in, and the owner starts a new search")]
async fn exercise(w: &mut World) {
    w.scenario
        .check_network_host()
        .expect("observed host before requests");
    let result = AssertUnwindSafe(crate::network_support::exercise(
        &mut w.scenario,
        &mut w.panel,
    ))
    .catch_unwind()
    .await;
    checked_host_result(&mut w.scenario, result);
}

#[then("each network connection that acp2api opens goes to 127.0.0.1")]
async fn loopback(w: &mut World) {
    let (observer, pid) = w.network_observer.take().expect("observed host");
    let status = w
        .scenario
        .stop_network_observed()
        .await
        .expect("FR-046 observed shutdown");
    observer
        .finish(pid, status)
        .expect("FR-046 destination assertion");
}
