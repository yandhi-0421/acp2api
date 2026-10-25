use acp2api_acceptance::{
    Scenario, TurnPlan, TurnStep,
    client::{Answer, ApiCall},
    endpoint_check, tool_check,
};
use serde_json::{Value, json};

pub const USER_TEXT: &str = "Hello";
pub const USER_TEXT_TOKENS: u64 = 2;
pub const DEFAULT_REPLY: &str = "OK";
const DEFAULT_USED: u64 = 1000;
const CONTEXT_SIZE: u64 = 1_048_576;

#[derive(Debug)]
pub struct RequestCase {
    pub updates: Vec<u64>,
    pub reply: String,
    pub system: Option<String>,
    pub messages: Vec<Value>,
    pub tools: Vec<Value>,
    pub output_call: Option<(String, String)>,
    pub earlier_call: Option<(String, Value)>,
    pub result: String,
    pub next_text: String,
    pub continued: bool,
}

impl Default for RequestCase {
    fn default() -> Self {
        Self {
            updates: vec![DEFAULT_USED],
            reply: DEFAULT_REPLY.into(),
            system: None,
            messages: Vec::new(),
            tools: Vec::new(),
            output_call: None,
            earlier_call: None,
            result: String::new(),
            next_text: String::new(),
            continued: false,
        }
    }
}

impl RequestCase {
    pub fn request(&self, endpoint: &str, stream: bool) -> ApiCall {
        let mut call = if self.tools.is_empty() {
            endpoint_check::request(endpoint, USER_TEXT, stream)
        } else {
            tool_check::request(endpoint, &self.tools, stream, "auto")
        };
        if let Some(system) = &self.system {
            call.body["system"] = json!(system);
        }
        if !self.messages.is_empty() {
            call.body["messages"] = json!(self.messages);
        }
        if let Some((name, arguments)) = &self.earlier_call {
            call.body["messages"] = if endpoint == "POST /v1/messages" {
                json!([
                    {"role":"assistant","content":[{"type":"tool_use","id":"earlier-call","name":name,"input":arguments}]},
                    {"role":"user","content":[{"type":"tool_result","tool_use_id":"earlier-call","content":self.result},{"type":"text","text":self.next_text}]}
                ])
            } else {
                json!([
                    {"role":"assistant","content":null,"tool_calls":[{"id":"earlier-call","type":"function","function":{"name":name,"arguments":arguments}}]},
                    {"role":"tool","tool_call_id":"earlier-call","content":self.result},
                    {"role":"user","content":self.next_text}
                ])
            };
        }
        call
    }

    pub fn next_plan(&mut self) -> TurnPlan {
        let mut plan = TurnPlan::new();
        if let Some((name, arguments)) = &self.output_call {
            plan.steps.push(TurnStep::ClientToolCall {
                name: name.clone(),
                arguments: arguments.clone(),
            });
            plan.end = acp2api_acceptance::fake::TurnEndPlan::Never;
        } else {
            plan = plan.chunk(&std::mem::replace(&mut self.reply, DEFAULT_REPLY.into()));
        }
        self.with_usage(plan)
    }

    pub fn with_usage(&self, mut plan: TurnPlan) -> TurnPlan {
        for used in &self.updates {
            plan = plan.usage(*used, CONTEXT_SIZE);
        }
        plan
    }

    pub fn queue_next(&mut self, scenario: &Scenario) {
        scenario.fake().next_prompt(self.next_plan());
    }

    pub async fn send(&mut self, scenario: &mut Scenario, call: ApiCall) {
        self.queue_next(scenario);
        let answer = scenario
            .client()
            .await
            .send(call)
            .finish(&mut scenario.time)
            .await;
        assert_eq!(answer.status, 200, "request reaches the real HTTP seam");
        assert!(
            !scenario.fake().record().prompts().is_empty(),
            "request reaches the fake ACP runtime through the product"
        );
        if self.continued {
            let prompts = scenario.fake().record().prompts();
            assert_eq!(prompts.len(), 2, "one seed turn and one continued turn");
            assert_eq!(
                prompts[0].session, prompts[1].session,
                "continued ACP session"
            );
            assert_eq!(
                prompts[1].text,
                self.messages.last().unwrap()["content"].as_str().unwrap(),
                "only the new input reaches ACP"
            );
        }
        scenario.answer = Some(answer);
    }
}

pub fn event_usage(answer: &Answer, event: &str) -> Value {
    let value: Value = serde_json::from_str(
        &answer
            .events
            .iter()
            .rev()
            .find(|e| e.event == event)
            .unwrap_or_else(|| panic!("FR-025 missing {event} event"))
            .data,
    )
    .expect("SSE JSON");
    match event {
        "message_start" => value["message"]["usage"].clone(),
        "response.completed" => value["response"]["usage"].clone(),
        _ => value["usage"].clone(),
    }
}

pub fn chat_chunks(answer: &Answer) -> Vec<Value> {
    assert_eq!(
        answer.events.last().map(|e| e.data.as_str()),
        Some("[DONE]"),
        "Chat Completions stream ends with data: [DONE]"
    );
    answer.events[..answer.events.len() - 1]
        .iter()
        .map(|e| serde_json::from_str(&e.data).expect("Chat Completions chunk JSON"))
        .collect()
}

pub fn zero_details(usage: &Value, path: &str) {
    if let Some(fields) = usage.as_object() {
        for (name, value) in fields {
            let path = format!("{path}.{name}");
            if value.is_object() {
                zero_details(value, &path);
            } else if path.contains("cache") || path.contains("reasoning") {
                assert_eq!(value.as_u64(), Some(0), "FR-025 {path} is absent or zero");
            }
        }
    }
}
