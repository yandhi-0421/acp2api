#[path = "support/compatibility_facts.rs"]
pub mod compatibility_facts;
use acp2api_acceptance::{
    Scenario, Setup, TurnPlan, build_fixture, client::ApiCall, panel_check::Panel, real_limit,
    setup::file_hash,
};
use serde_json::json;
use std::{fs, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_prompt_after_compatibility_check_uses_its_next_prompt_plan() {
    tokio::time::timeout(real_limit(Duration::from_secs(20)), async {
        let mut scenario = Scenario::default();
        scenario.plan(Setup::normal().configure(|root, _, _| {
            build_fixture::install(
                &root.path.join("builds/normal"),
                &root.path.join("path"),
                "1.6.0",
                "build-1.6.0",
            )
            .unwrap();
        }));
        let mut panel = Panel::default();
        panel.scenario(&mut scenario).await;
        panel.login_initial("check-owner").await;
        assert_eq!(panel.login_answer.as_ref().unwrap().status, 303);
        compatibility_facts::configure(&scenario.fake(), compatibility_facts::echo);
        let identity = [build_fixture::SERVER, build_fixture::HARNESS]
            .map(|name| file_hash(&scenario.root.path.join("path").join(name)))
            .join(":");
        let checked = panel
            .submit(
                "check-owner",
                "/manage/compatibility-check",
                json!({"build":identity}),
            )
            .await;
        assert_eq!(checked.status, 200);
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(scenario.root.path.join("data/state.json")).unwrap())
                .unwrap();
        assert_eq!(state["checked_builds"][0]["version"], "1.6.0");

        scenario
            .fake()
            .next_prompt(TurnPlan::new().chunk("reply for the API request"));
        let mut call = ApiCall::one_user_message(
            "POST /v1/messages",
            "The request after the compatibility check.",
        );
        call.body["system"] = json!("An ordinary API conversation.");
        let answer = scenario
            .client()
            .await
            .send(call)
            .finish(&mut scenario.time)
            .await;
        assert_eq!(answer.status, 200);
        assert_eq!(
            answer.json()["content"][0]["text"],
            "reply for the API request",
            "the API prompt after the check must use its own next_prompt plan"
        );
        scenario.finish(true).await;
    })
    .await
    .expect("compatibility check and subsequent API request cap: 20 s");
}
