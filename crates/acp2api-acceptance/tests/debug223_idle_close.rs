#![allow(clippy::disallowed_methods)]
use acp2api_acceptance::{
    Scenario, Setup, TurnEndPlan, TurnPlan, TurnStep, client::ApiCall, real_limit,
};
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_idle_peer_cancels_both_response_modes_without_a_virtual_tick() {
    tokio::time::timeout(Duration::from_secs(30), async {
        for stream in [false, true] {
            let mut s = Scenario::default();
            s.hub.enable_debug223();
            s.plan(Setup::normal());
            s.fake().next_prompt(TurnPlan {
                steps: vec![TurnStep::HoldUntil("debug223-idle".into())],
                end: TurnEndPlan::IgnoreCancel,
            });
            let mut request = ApiCall::one_user_message("POST /v1/messages", "debug223 idle close");
            request.body["stream"] = stream.into();
            let live = s.client().await.send(request);
            s.time.settle().await;
            assert_eq!(s.fake().record().prompts().len(), 1);
            if stream {
                assert_eq!(live.status(), Some(200));
            } else {
                assert_eq!(live.status(), None);
            }
            let now = s.hub.now();
            drop(live);
            let deadline = Instant::now() + real_limit(Duration::from_secs(2));
            loop {
                s.time.settle().await;
                if !s.fake().record().cancels().is_empty() || Instant::now() >= deadline {
                    break;
                }
                let hub = s.hub.clone();
                tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                    .await
                    .unwrap();
            }
            for entry in s.hub.entries() {
                if entry.event["kind"] == "terminal"
                    && let Some(line) = entry.event["text"]
                        .as_str()
                        .filter(|s| s.contains("[DEBUG-223]"))
                {
                    eprint!("{line}");
                }
            }
            assert_eq!(
                s.fake().record().cancels().len(),
                1,
                "idle peer close cancels the prompt"
            );
            assert_eq!(s.hub.now(), now, "no keep-alive or deadline was advanced");
            println!("debug223 idle close stream={stream} cancel=1 virtual={now}");
            s.finish(true).await;
        }
    })
    .await
    .expect("debug223 idle close run cap: 30 real seconds; two API requests");
}
