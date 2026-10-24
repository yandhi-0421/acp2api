use acp2api_acceptance::real_limit;
use acp2api_acceptance::{
    Scenario, Setup, TurnPlan, TurnStep, endpoint_check as wire, panel_check::Panel,
};
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panel_limit_opens_a_third_place_without_restarting_and_routes_concurrent_replies() {
    tokio::time::timeout(real_limit(Duration::from_secs(45)),async {
        let mut s=Scenario::default();s.plan(Setup::normal());s.start().await;
        let mut panel=Panel::default();panel.scenario(&mut s).await;panel.login_initial("owner").await;
        let mut calls=Vec::new();
        for n in 0..4 {
            s.fake().next_prompt(TurnPlan {steps:vec![TurnStep::HoldUntil(format!("gate-{n}")),TurnStep::Chunk(format!("reply-{n}")),TurnStep::Usage {used:1000+n,size:1_048_576}],..TurnPlan::new()});
            let mut request=wire::request("POST /v1/messages",&format!("distinct conversation {n}"),true);
            request.body["system"]=json!(format!("system for conversation {n}"));
            calls.push(s.client().await.send(request));s.time.settle().await;
            assert_eq!(s.fake().prompts().len(),(n as usize+1).min(2),"FR-004 concurrency limit two starts two actual prompt turns and queues the rest");
        }
        assert_eq!(panel.submit("owner","/manage/concurrency",json!({"concurrency_limit":"3"})).await.status,200,"FR-004 panel concurrency action");
        s.time.settle().await;
        assert_eq!(s.fake().prompts().len(),3,"FR-004 third prompt starts immediately after panel change while the first two remain held");
        assert_eq!(acp2api_acceptance::fake::Record {entries:s.hub.entries()}.prompts().iter().map(|p|&p.session).collect::<std::collections::HashSet<_>>().len(),3,"each simultaneous turn owns a distinct actual ACP session");
        s.fake().open_gate("gate-1");s.time.settle().await;s.time.advance_to(s.hub.now()+200).await;
        assert_eq!(s.fake().prompts().len(),4,"fourth prompt starts only after a place becomes free");
        for n in [0,2,3] {s.fake().open_gate(&format!("gate-{n}"));s.time.settle().await;}
        for (n,call) in calls.into_iter().enumerate() {
            let answer=call.finish(&mut s.time).await;assert_eq!(answer.status,200);
            wire::verify(&answer,"POST /v1/messages",&format!("reply-{n}"),1000+n as u64,2);
        }
        s.finish(true).await;
    }).await.expect("panel concurrency probe cap: 45 seconds, 4 API requests, existing 8 MiB body and 4 MiB hub log caps");
}
