#![allow(clippy::disallowed_methods)]
use acp2api_acceptance::{Scenario, TurnPlan, runtime_safety_check as safety};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "usage: runtime-safety-probe TOOL REPLY");
    assert!(
        !args[0].is_empty()
            && args[0].len() <= 96
            && args[0]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    );
    assert!(!args[1].is_empty() && args[1].len() <= 256);
    tokio::time::timeout(Duration::from_secs(120), run(&args[0], &args[1]))
        .await
        .expect("probe total cap: 120 s");
}
async fn run(name: &str, reply: &str) {
    println!(
        "Caps: 120 s; 24 HTTP requests per instance; 300 ACP lines; 180 s fake lifetime; 4 MiB per captured log; guarded fake runtime only."
    );
    let mut denied = Scenario::default();
    denied.plan(safety::isolated());
    denied.fake().next_prompt(safety::denial_plan(name, reply));
    let answer = safety::send(&mut denied, "fresh permission refusal probe").await;
    safety::denied(&denied);
    safety::reply(&answer, reply);
    safety::event_log(&denied, "permission_refused", name);
    safety::home(&denied);
    safety::configuration(&denied);
    safety::capabilities(&denied);
    assert!(denied.hub.counts().http_sent <= 24);
    println!(
        "PASS permission refused without cancellation; actual reply completed; isolated launch, capabilities, tool offer, and safety log verified."
    );
    denied.finish(true).await;
    let mut s = Scenario::default();
    s.plan(safety::isolated());
    s.fake().next_prompt(safety::execution_plan(name));
    let answer = safety::send(&mut s, "fresh unpermitted execution probe").await;
    safety::error(&answer, name);
    acp2api_acceptance::tool_check::cancelled(&s, 1);
    safety::event_log(&s, "built_in_tool_ran", name);
    let mut panel = safety::panel(&mut s).await;
    safety::notice_page(&mut panel, &safety::notice(name)).await;
    safety::notice_page(&mut panel, &safety::notice(name)).await;
    safety::dismiss(&mut panel).await;
    safety::no_notice(&mut panel, &safety::notice(name)).await;
    s.fake().next_prompt(TurnPlan::new().chunk(reply));
    let next = safety::send(&mut s, "fresh next conversation probe").await;
    safety::reply(&next, reply);
    let prompts = s.fake().record().prompts();
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[1].text, "fresh next conversation probe");
    assert_eq!(s.fake().record().cancels().len(), 1);
    assert!(s.hub.counts().http_sent <= 24);
    println!(
        "PASS actual session cancellation; exact M-22 HTTP 502 envelope; one event and request completion; notice reload, form dismissal, and later reload; next prompt and fresh reply completed."
    );
    s.finish(true).await;
}
