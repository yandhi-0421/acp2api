use acp2api_acceptance::Scenario;
use cucumber::{World as _, event::ScenarioFinished};
#[path = "steps/account_ownership.rs"]
mod account_ownership;
#[path = "steps/acp_call_limit.rs"]
mod acp_call_limit;
#[path = "support/acp_call_limit.rs"]
mod acp_call_limit_support;
#[path = "support/runtime_builds.rs"]
mod build_support;
#[path = "steps/compatibility_check.rs"]
mod compatibility_check;
#[path = "support/compatibility_facts.rs"]
mod compatibility_facts;
#[path = "support/compatibility_check.rs"]
mod compatibility_support;
#[path = "steps/condition_overlap.rs"]
mod condition_overlap;
#[path = "steps/copy_failure.rs"]
mod copy_failure;
#[path = "support/copy_failure.rs"]
mod copy_failure_support;
#[path = "steps/end_lockout.rs"]
mod end_lockout;
#[path = "support/end_lockout.rs"]
mod end_lockout_support;
#[path = "steps/error_statuses.rs"]
mod error_statuses;
#[path = "support/error_cases.rs"]
mod error_support;
#[path = "steps/history_match.rs"]
mod history_match;
#[path = "support/history_match.rs"]
mod history_support;
#[path = "steps/keep_alive.rs"]
mod keep_alive;
#[path = "support/keep_alive.rs"]
mod keep_alive_support;
#[path = "steps/lockout.rs"]
mod lockout;
#[path = "support/lockout.rs"]
mod lockout_support;
#[path = "steps/network_observer.rs"]
mod network_observer;
#[path = "support/network_observer.rs"]
mod network_support;
#[path = "steps/panel.rs"]
mod panel;
#[path = "support/panel_document.rs"]
mod panel_document;
#[path = "steps/panel_language_visual.rs"]
mod panel_language_visual;
#[path = "steps/panel_states.rs"]
mod panel_states;
#[path = "support/panel_states.rs"]
mod panel_states_support;
#[path = "steps/panel_tiles.rs"]
mod panel_tiles;
#[path = "support/panel_tiles.rs"]
mod panel_tiles_support;
#[path = "steps/places_and_queue.rs"]
mod places_and_queue;
#[path = "support/places_and_queue.rs"]
mod places_support;
#[path = "support/prompt_turn.rs"]
mod prompt_turn;
#[path = "support/quota_queries.rs"]
mod query_support;
#[path = "steps/queue_errors.rs"]
mod queue_errors;
#[path = "steps/quota_queries.rs"]
mod quota_queries;
#[path = "support/quota_wait.rs"]
mod quota_support;
#[path = "steps/quota_wait.rs"]
mod quota_wait;
#[path = "steps/response_ends.rs"]
mod response_ends;
#[path = "support/retry_wait.rs"]
mod retry_support;
#[path = "steps/retry_wait.rs"]
mod retry_wait;
#[path = "steps/runtime_builds.rs"]
mod runtime_builds;
#[path = "steps/runtime_continuation.rs"]
mod runtime_continuation;
#[path = "steps/runtime_process.rs"]
mod runtime_process;
#[path = "support/runtime_process.rs"]
mod runtime_support;
#[path = "steps/runtime_unavailable.rs"]
mod runtime_unavailable;
mod selection;
#[path = "steps/session_limit.rs"]
mod session_limit;
#[path = "support/session_limit.rs"]
mod session_limit_support;
#[path = "steps/setup_logs.rs"]
mod setup_logs;
#[path = "support/setup_logs.rs"]
mod setup_logs_support;
#[path = "steps/sign_in.rs"]
mod sign_in;
#[path = "support/sign_in_facts.rs"]
mod sign_in_facts;
#[path = "support/sign_in.rs"]
mod sign_in_support;
#[path = "steps/slash_and_thought.rs"]
mod slash_and_thought;
#[path = "support/slash_and_thought.rs"]
mod slash_and_thought_support;
#[path = "steps/stdin_failures.rs"]
mod stdin_failures;
#[path = "steps/thinking_levels.rs"]
mod thinking_levels;
#[path = "support/thinking_levels.rs"]
mod thinking_support;
#[path = "support/timed_steps.rs"]
mod timed_steps;
#[path = "support/tool_results.rs"]
mod tool_result_support;
#[path = "steps/tool_results.rs"]
mod tool_results;
#[path = "support/runtime_unavailable.rs"]
mod unavailable_support;
#[path = "steps/upstream_error.rs"]
mod upstream_error;
#[path = "support/usage_contract.rs"]
mod usage_contract;
#[path = "steps/usage_contract.rs"]
mod usage_steps;
#[derive(Debug, Default, cucumber::World)]
struct World {
    network_observer: Option<(acp2api_acceptance::network_observer::Observer, u32)>,
    setup_logs: setup_logs_support::SetupLogs,
    errors: error_support::ErrorCases,
    panel_tiles: panel_tiles_support::TileState,
    prompt_sessions: std::collections::HashMap<String, String>,
    lockout: lockout_support::LockoutState,
    end_lockout: end_lockout_support::EndLockoutState,
    keep_alive: keep_alive_support::KeepAliveState,
    sign_in: sign_in_support::SignInState,
    retry: retry_support::RetryState,
    quota: quota_support::QuotaState,
    queries: query_support::QueryState,
    process: runtime_support::ProcessState,
    limit: session_limit_support::LimitState,
    call_limit: acp_call_limit_support::CallLimitState,
    reload: runtime_continuation::ReloadState,
    account: acp2api_acceptance::account_check::AccountState,
    builds: build_support::BuildState,
    compatibility: compatibility_support::CompatibilityState,
    places: places_support::PlacesState,
    ends: response_ends::EndState,
    upstream: upstream_error::ErrorState,
    request: usage_contract::RequestCase,
    conversation: conversation::ConversationState,
    history: history_support::HistoryState,
    fields: request_fields::FieldState,
    safety: runtime_safety::SafetyState,
    server_tools: server_tools::ServerToolState,
    scenario: Scenario,
    panel: acp2api_acceptance::panel_check::Panel,
    panel_dark: bool,
    panel_rendered: Option<acp2api_acceptance::panel_visual::Audit>,
    models: model_names::ModelState,
    logs: logs::LogState,
    copy_failure: Option<copy_failure_support::DeniedCopy>,
    startup: acp2api_acceptance::startup_check::Startup,
    access: access_protection::AccessState,
    endpoint: endpoints_and_streams::EndpointState,
    content: request_content::ContentState,
    tools: client_tools::ToolState,
    custom_tools: custom_tools::CustomToolState,
    tool_results: tool_result_support::ToolResultState,
}
impl World {
    fn record_prompt_session(&mut self, name: &str, before: usize) {
        let prompts = self.scenario.fake().record().prompts();
        if let Some(prompt) = prompts.get(before) {
            assert_eq!(prompts.len(), before + 1, "one prompt starts for {name}");
            self.prompt_sessions
                .insert(name.into(), prompt.session.clone());
        }
    }
}
#[path = "steps/access_protection.rs"]
mod access_protection;
#[path = "steps/client_tools.rs"]
mod client_tools;
#[path = "steps/common.rs"]
mod common;
#[path = "steps/conversation.rs"]
mod conversation;
#[path = "steps/custom_tools.rs"]
mod custom_tools;
#[path = "steps/endpoints_and_streams.rs"]
mod endpoints_and_streams;
#[path = "steps/harness_regression.rs"]
mod harness_regression;
#[path = "steps/logs.rs"]
mod logs;
#[path = "steps/model_names.rs"]
mod model_names;
#[path = "steps/request_content.rs"]
mod request_content;
#[path = "steps/request_fields.rs"]
mod request_fields;
#[path = "steps/runtime_safety.rs"]
mod runtime_safety;
#[path = "steps/server_tools.rs"]
mod server_tools;
#[path = "steps/startup.rs"]
mod startup;
#[path = "steps/state_file_failure.rs"]
mod state_file_failure;
#[path = "steps/system_prompt_and_role_labels.rs"]
mod system_prompt_and_role_labels;
#[path = "steps/token_usage.rs"]
mod token_usage;
fn main() {
    if let Err(payload) = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
    {
        let message = if let Some(message) = payload.downcast_ref::<String>() {
            message.as_str()
        } else if let Some(message) = payload.downcast_ref::<&str>() {
            *message
        } else {
            "non-string panic payload"
        };
        eprintln!("acp2api acceptance runner panicked: {message}");
        std::process::exit(101);
    }
}
#[tokio::main]
async fn run() {
    let config: toml::Value = toml::from_str(include_str!("runner.toml")).unwrap();
    let feature_root = std::env::var("ACP2API_ACCEPTANCE_FEATURES")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../features/acp2api").into());
    let args: Vec<_> = std::env::args().collect();
    let explicit_tags = std::env::var_os("CUCUMBER_FILTER_TAGS").is_some()
        || args
            .iter()
            .any(|arg| arg == "--tags" || arg == "-t" || arg.starts_with("--tags="));
    let default_tags = config["default_tags"].as_str().unwrap().to_owned();
    if std::env::var_os("ACP2API_ACCEPTANCE_LIST").is_some() {
        use cucumber::Parser as _;
        use futures_util::StreamExt as _;
        let parser = selection::Selected::new(&args, &default_tags);
        let features = parser
            .parse(feature_root.as_str(), cucumber::cli::Empty {})
            .collect::<Vec<_>>()
            .await;
        let steps = World::collection();
        let mut count = 0;
        for feature in features {
            let feature = feature.expect("selected Gherkin parses");
            for scenario in &feature.scenarios {
                count += 1;
                println!("{}: {} {:?}", feature.name, scenario.name, scenario.tags);
                for step in feature
                    .background
                    .iter()
                    .flat_map(|b| &b.steps)
                    .chain(&scenario.steps)
                {
                    assert!(
                        steps.find(step).expect("unambiguous step").is_some(),
                        "undefined: {}",
                        step.value
                    );
                }
            }
        }
        println!("{count} scenarios; 0 undefined; 0 ambiguous");
        return;
    }
    World::cucumber::<&str>()
        .steps(timed_steps::collection::<World>())
        .with_parser(selection::Selected::new(&args, &default_tags))
        .max_concurrent_scenarios(config["concurrent_scenarios"].as_integer().unwrap() as usize)
        .after(|_, _, _, finished, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lockout.restore();
                    runtime_continuation::cleanup(world);
                    runtime_support::cleanup(world).await;
                    session_limit_support::cleanup(world).await;
                    acp_call_limit_support::cleanup(world).await;
                    places_support::cleanup(world).await;
                    world
                        .scenario
                        .finish(matches!(finished, ScenarioFinished::StepPassed))
                        .await;
                }
            })
        })
        .filter_run_and_exit(feature_root.as_str(), move |_, _, scenario| {
            explicit_tags
                || default_tags == "not @wip" && !scenario.tags.iter().any(|tag| tag == "wip")
        })
        .await;
}
