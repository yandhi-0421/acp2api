use acp2api_acceptance::Scenario;
use serde_json::Value;
use std::collections::HashMap;

pub fn level_at_prompt(s: &Scenario, expected: &str) {
    let mut options: HashMap<String, HashMap<String, Value>> = HashMap::new();
    let mut selected = None;
    for entry in s.hub.entries() {
        let event = &entry.event;
        let line = if event["kind"] == "command" && event["command"]["op"] == "Out" {
            event["command"]["line"].as_str()
        } else if event["kind"] == "got" {
            event["line"].as_str()
        } else {
            None
        };
        let Some(line) = line else { continue };
        let value: Value = serde_json::from_str(line).expect("captured ACP JSON");
        if let Some(session) = value["result"]["sessionId"].as_str() {
            let config = value["result"]["configOptions"]
                .as_array()
                .expect("session config options");
            options.insert(
                session.into(),
                config
                    .iter()
                    .map(|option| {
                        (
                            option["id"].as_str().unwrap().into(),
                            option["currentValue"].clone(),
                        )
                    })
                    .collect(),
            );
        }
        if value["method"] == "session/set_config_option" {
            let params = &value["params"];
            options
                .get_mut(params["sessionId"].as_str().unwrap())
                .expect("configured ACP session")
                .insert(
                    params["configId"].as_str().unwrap().into(),
                    params["value"].clone(),
                );
        }
        if value["method"] == "session/prompt" {
            selected = Some(options[value["params"]["sessionId"].as_str().unwrap()].clone());
        }
    }
    assert_eq!(
        s.answer.as_ref().unwrap().status,
        200,
        "FR-013 request succeeds"
    );
    let selected = selected.expect("request reached ACP prompt");
    assert_eq!(selected["model"], "gemini-3.8-flash", "FR-013 base model");
    assert_eq!(
        selected["thinking_level"], expected,
        "FR-013 thinking level at prompt"
    );
}
