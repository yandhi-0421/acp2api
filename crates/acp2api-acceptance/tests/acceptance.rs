use acp2api_acceptance::Scenario;
use cucumber::{World as _, event::ScenarioFinished};
mod selection;
#[derive(Debug, Default, cucumber::World)]
struct World {
    scenario: Scenario,
}
#[path = "steps/common.rs"]
mod common;
#[path = "steps/system_prompt_and_role_labels.rs"]
mod system_prompt_and_role_labels;
#[path = "steps/token_usage.rs"]
mod token_usage;
#[tokio::main]
async fn main() {
    let config: toml::Value = toml::from_str(include_str!("runner.toml")).unwrap();
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
            .parse(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../features/acp2api"),
                cucumber::cli::Empty {},
            )
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
        .with_parser(selection::Selected::new(&args, &default_tags))
        .max_concurrent_scenarios(config["concurrent_scenarios"].as_integer().unwrap() as usize)
        .after(|_, _, _, finished, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world
                        .scenario
                        .finish(matches!(finished, ScenarioFinished::StepPassed))
                        .await;
                }
            })
        })
        .filter_run_and_exit(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../features/acp2api"),
            move |_, _, scenario| {
                explicit_tags
                    || default_tags == "not @wip" && !scenario.tags.iter().any(|tag| tag == "wip")
            },
        )
        .await;
}
