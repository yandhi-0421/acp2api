use acp2api_acceptance::{client::ApiCall, conversation_check, tool_check};
use serde_json::{Value, json};

#[derive(Debug, Default)]
pub struct ToolResultState {
    pub endpoint: String,
    pub messages: Vec<Value>,
    pub tools: Vec<Value>,
    pub id: String,
    pub arguments: String,
    pub prompt_count: usize,
}

impl ToolResultState {
    pub fn request(&self) -> ApiCall {
        let mut call = tool_check::request(&self.endpoint, &self.tools, false, "auto");
        call.body["messages"] = json!(self.messages);
        call
    }

    pub fn result(&mut self, content: Value, error: bool) {
        let message = if self.endpoint == "POST /v1/messages" {
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":self.id,"content":content,"is_error":error}]})
        } else {
            assert!(!error, "Chat Completions has no is_error field");
            json!({"role":"tool","tool_call_id":self.id,"content":content})
        };
        self.messages.push(message);
    }

    pub fn prompt(&self, scenario: &acp2api_acceptance::Scenario) -> Option<Value> {
        conversation_check::prompts(scenario)
            .get(self.prompt_count)
            .map(|prompt| prompt["prompt"].clone())
    }

    pub fn text(&self, scenario: &acp2api_acceptance::Scenario) -> Option<String> {
        self.prompt(scenario).map(|blocks| {
            blocks
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|block| block["text"].as_str())
                .collect()
        })
    }

    pub fn first_line(&self, scenario: &acp2api_acceptance::Scenario) -> Option<String> {
        self.text(scenario).and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("Result of "))
                .map(str::to_owned)
        })
    }
}

pub fn long_arguments(length: usize) -> String {
    let arguments = format!(r#"{{"path":"{}"}}"#, "é".repeat(length - 11));
    assert_eq!(arguments.chars().count(), length);
    arguments
}
