use acp2api_acceptance::{
    Scenario, Setup, TurnPlan, endpoint_check, panel_check::Panel, rpc_script::RpcReply,
};
use serde_json::{Value, json};

pub fn prepare(scenario: &mut Scenario) {
    scenario.plan(Setup::normal());
    scenario.fake().set_signed_in(false);
    let facts: Value = serde_json::from_str(include_str!("../fixtures/sign-in.json")).unwrap();
    for _ in 0..5 {
        scenario.fake().script_call(
            "initialize",
            RpcReply::Reply {
                updates: vec![],
                result: facts["initialize"].clone(),
            },
        );
    }
}
pub async fn exercise(scenario: &mut Scenario, panel: &mut Panel) {
    let facts: Value = serde_json::from_str(include_str!("../fixtures/sign-in.json")).unwrap();
    for route in ["POST /v1/messages", "POST /v1/chat/completions"] {
        let call =
            scenario
                .client()
                .await
                .send(endpoint_check::request(route, "before sign-in", false));
        assert_eq!(call.finish(&mut scenario.time).await.status, 400);
    }
    panel.scenario(scenario).await;
    panel.login_initial("network-owner").await;
    assert_eq!(panel.login_answer.as_ref().unwrap().status, 303);
    scenario.fake().script_call(
        "authenticate",
        RpcReply::Authentication {
            address: "https://accounts.example.com/signin".into(),
            token: None,
            result: Some(facts["authenticated"].clone()),
        },
    );
    assert_eq!(
        panel
            .submit("network-owner", "/manage/sign-in", json!({}))
            .await
            .status,
        200
    );
    scenario.time.settle().await;
    for route in [
        "POST /v1/messages",
        "POST /v1/chat/completions",
        "POST /v1/responses",
    ] {
        scenario
            .fake()
            .next_prompt(TurnPlan::new().chunk("network observation reply"));
        let call =
            scenario
                .client()
                .await
                .send(endpoint_check::request(route, "after sign-in", false));
        assert_eq!(call.finish(&mut scenario.time).await.status, 200);
    }
    assert_eq!(
        panel
            .submit("network-owner", "/manage/search", json!({}))
            .await
            .status,
        200
    );
    scenario.time.settle().await;
}
