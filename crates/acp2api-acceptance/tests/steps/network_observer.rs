use crate::World;
use acp2api_acceptance::network_observer::Observer;
use cucumber::{given, then, when};

#[given("acp2api runs with a fake ACP agent in place of the official runtime")]
async fn observed(w: &mut World) {
    crate::network_support::prepare(&mut w.scenario);
    let observer = Observer::begin().expect("OS network observer and initial positive control");
    let pid = w.scenario.start_network_observed(&observer).await;
    w.network_observer = Some((observer, pid));
}

#[when("an API client sends requests, the owner signs in, and the owner starts a new search")]
async fn exercise(w: &mut World) {
    crate::network_support::exercise(&mut w.scenario, &mut w.panel).await;
}

#[then("each network connection that acp2api opens goes to 127.0.0.1")]
async fn loopback(w: &mut World) {
    let (observer, pid) = w.network_observer.take().expect("observed host");
    w.scenario.stop().await;
    observer
        .finish(pid)
        .expect("complete OS network record")
        .only_loopback()
        .expect("FR-046 destination assertion");
}
