#[path = "support/sign_in_facts.rs"]
#[allow(
    dead_code,
    reason = "These tests use only the captured initialize reply of the shared facts"
)]
mod sign_in_facts;
use acp2api_acceptance::{
    Scenario, Setup, panel_check::Panel, panel_check::text, real_limit, rpc_script::RpcReply,
};
use serde_json::json;
use std::time::Duration;

const M_13_ENGLISH: &str = "the official runtime is not signed in";
const M_13_CHINESE: &str = "官方运行时没有有效的登录";

async fn chinese_owner_without_sign_in() -> (Scenario, Panel) {
    let mut scenario = Scenario::default();
    scenario.plan(Setup::normal());
    scenario.fake().set_signed_in(false);
    for _ in 0..5 {
        scenario.fake().script_call(
            "initialize",
            RpcReply::Reply {
                updates: vec![],
                result: sign_in_facts::captured("initialize"),
            },
        );
    }
    scenario.start().await;
    let mut panel = Panel::default();
    panel.scenario(&mut scenario).await;
    panel.owner("owner").language = "zh-CN".into();
    panel.login_initial("owner").await;
    (scenario, panel)
}

async fn chinese_page_of(action: &'static str) -> String {
    tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let (mut scenario, mut panel) = chinese_owner_without_sign_in().await;
        let answer = panel.submit("owner", action, json!({})).await;
        scenario.finish(true).await;
        text(&answer)
    })
    .await
    .expect("panel language cap: 30 seconds, three panel requests, existing hub and capture caps")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_quota_refresh_shows_the_sign_in_message_in_chinese() {
    let page = chinese_page_of("/manage/refresh-quota").await;
    assert!(
        page.contains(M_13_CHINESE) && !page.contains(M_13_ENGLISH),
        "FR-055 a Chinese page has the Chinese M-13"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_read_of_the_model_names_shows_the_sign_in_message_in_chinese() {
    let page = chinese_page_of("/manage/models").await;
    assert!(
        page.contains(M_13_CHINESE) && !page.contains(M_13_ENGLISH),
        "FR-055 the error branch of the model read has the Chinese M-13"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_model_read_with_no_session_id_shows_a_chinese_failure() {
    let page = tokio::time::timeout(real_limit(Duration::from_secs(30)), async {
        let (mut scenario, mut panel) = chinese_owner_without_sign_in().await;
        scenario.fake().set_signed_in(true);
        scenario.fake().script_call(
            "session/new",
            RpcReply::Reply {
                updates: vec![],
                result: json!({}),
            },
        );
        let answer = panel.submit("owner", "/manage/models", json!({})).await;
        scenario.finish(true).await;
        text(&answer)
    })
    .await
    .expect("panel language cap: 30 seconds, three panel requests, existing hub and capture caps");
    assert!(
        page.contains("官方运行时的回复里没有 sessionId") && !page.contains("missing sessionId"),
        "FR-055 acp2api's own failure text has Chinese"
    );
}
