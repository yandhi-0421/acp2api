#[path = "support/network_observer.rs"]
mod network_support;
use acp2api_acceptance::{Scenario, panel_check::Panel};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn network_fixture_reaches_api_sign_in_and_search_with_the_real_product() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let mut scenario = Scenario::default();
        network_support::prepare(&mut scenario);
        scenario.start().await;
        network_support::exercise(&mut scenario, &mut Panel::default()).await;
        assert_eq!(scenario.fake().record().prompts().len(), 3);
        assert!(scenario.stop_network_observed().await.unwrap().success());
        scenario.finish(true).await;
    }).await.expect("network fixture: at most nine HTTP requests, 20 seconds, existing 4 MiB terminal and journal caps");
}
