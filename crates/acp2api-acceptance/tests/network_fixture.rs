#[path = "support/network_observer.rs"]
mod network_support;
use acp2api_acceptance::{Scenario, network_observer::Observer, panel_check::Panel};
use futures_util::FutureExt;
use std::{panic::AssertUnwindSafe, time::Duration};

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires native OS sandbox or audit access; CI runs this explicitly"]
async fn a_planted_attempt_in_the_observed_host_fails_the_destination_check() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let observer = Observer::begin().expect("native network observer");
        let mut scenario = Scenario::default();
        network_support::prepare(&mut scenario);
        let launch = AssertUnwindSafe(
            scenario.start_network_observed(&observer, &[("ACP2API_TEST_OUTBOUND_DIAL", "1")]),
        )
        .catch_unwind()
        .await;
        let error = match launch {
            Ok(pid) => {
                network_support::exercise(&mut scenario, &mut Panel::default()).await;
                let result = match scenario.stop_network_observed().await {
                    Ok(status) => observer.finish(pid, status),
                    Err(error) => Err(error),
                };
                result.expect_err("planted host dial must fail the destination check")
            }
            Err(error) => *error.downcast::<String>().expect("host startup diagnostic"),
        };
        assert!(
            error.contains("FR-046 outbound attempt outside 127.0.0.1:"),
            "{error}"
        );
        scenario.finish(true).await;
    })
    .await
    .expect("one planted dial with a 200 ms timeout, at most nine HTTP requests, 60 seconds, existing 4 MiB terminal and journal caps");
}
