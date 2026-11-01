use crate::{World, panel_document as document, places_support as queue};
use acp2api_acceptance::{TurnPlan, endpoint_check};

pub const MARKER: &str = "panel-state-request-75-6b46cf1e2be548f8";
pub const ERROR: &str = "Synthetic account suspension panel-evidence-75-84de23c7bb24";

pub fn first_tile(html: &str) -> &str {
    document::elements(html, "section")
        .next()
        .expect("FR-054 the panel has a first tile")
        .content
}

pub fn entry(html: &str, label: &str) -> bool {
    document::elements(html, "form").any(|form| {
        document::attribute(form.tag, "method").is_some_and(|m| m.eq_ignore_ascii_case("post"))
            && document::attribute(form.tag, "action")
                .is_some_and(|a| a.starts_with('/') && !a.starts_with("//"))
            && document::elements(form.content, "button").any(|button| {
                document::visible(button.content) == label
                    && !button
                        .tag
                        .split_whitespace()
                        .any(|a| a.starts_with("disabled"))
                    && document::attribute(button.tag, "aria-disabled") != Some("true")
            })
    }) || document::elements(html, "a").any(|link| {
        document::visible(link.content) == label
            && document::attribute(link.tag, "href").is_some_and(|href| {
                href.starts_with('/') && !href.starts_with("//")
                    || href.starts_with('#') && href.len() > 1
            })
    })
}

pub async fn signal(w: &mut World, plan: TurnPlan) {
    queue::start(w).await;
    let before = w.scenario.fake().record().prompts().len();
    w.request.plan = Some(plan);
    let live = queue::send(
        w,
        endpoint_check::request("POST /v1/messages", MARKER, false),
    )
    .await;
    w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
    let prompts = w.scenario.fake().record().prompts();
    assert_eq!(
        prompts.len(),
        before + 1,
        "state signal reaches ACP exactly once"
    );
    assert!(prompts.last().unwrap().text.contains(MARKER));
}
