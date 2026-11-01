//! FR-021 and ADR-0003: the conversation in the form of the API client, over the real HTTP entry.
use crate::World;
use acp2api_acceptance::{
    TurnPlan, TurnStep, client::ApiCall, content_check, conversation_check as c, endpoint_check,
    journal_check, real_limit, system_check, tool_check,
};
use serde_json::{Value, json};
use std::time::Duration;

const MESSAGES: &str = "POST /v1/messages";
const RESPONSES: &str = "POST /v1/responses";
/// The longest scenario sends 4 requests.
const REQUEST_CAP: usize = 6;
const REQUEST_LIMIT: Duration = Duration::from_secs(15);
const LOG_CAP: u64 = 1_000_000;

#[derive(Debug, Default)]
pub struct HistoryState {
    endpoint: String,
    system: Option<String>,
    tools: Vec<Value>,
    items: Vec<Value>,
    call_id: String,
    tool_name: String,
    arguments: String,
    reply_text: String,
    sent: usize,
    session: Option<String>,
    previous: Option<String>,
    sessions: usize,
}

impl HistoryState {
    fn request(&self) -> ApiCall {
        let mut call = if self.tools.is_empty() {
            endpoint_check::request(&self.endpoint, "", false)
        } else {
            tool_check::request(&self.endpoint, &self.tools, false, "auto")
        };
        if self.endpoint == RESPONSES {
            call.body["input"] = json!(self.items);
            if let Some(system) = &self.system {
                call.body["instructions"] = json!(system);
            }
        } else if self.endpoint == MESSAGES {
            call.body["messages"] = json!(self.items);
            if let Some(system) = &self.system {
                call.body["system"] = json!(system);
            }
        } else {
            let system = self.system.iter().map(|s| c::message("system", None, s));
            call.body["messages"] = system.chain(self.items.iter().cloned()).collect();
        }
        call
    }

    /// The API client keeps each reply in the form of the response.
    fn keep_reply(&mut self, reply: &Value) {
        if self.endpoint == RESPONSES {
            let output = reply["output"].as_array().expect("Responses output");
            self.items.extend(output.iter().cloned());
        } else if self.endpoint == MESSAGES {
            self.items
                .push(json!({"role":"assistant","content":reply["content"]}));
        } else {
            self.items.push(reply["choices"][0]["message"].clone());
        }
    }

    fn call_item(&mut self) -> &mut Value {
        if self.endpoint == RESPONSES {
            self.items
                .iter_mut()
                .rev()
                .find(|item| item["type"] == "function_call")
                .expect("a function_call item")
        } else if self.endpoint == MESSAGES {
            self.items
                .iter_mut()
                .rev()
                .filter_map(|item| item["content"].as_array_mut())
                .flatten()
                .find(|block| block["type"] == "tool_use")
                .expect("a tool_use block")
        } else {
            self.items
                .iter_mut()
                .rev()
                .find(|item| item.get("tool_calls").is_some_and(|calls| !calls.is_null()))
                .map(|item| &mut item["tool_calls"][0])
                .expect("a tool call")
        }
    }

    fn read_call(&mut self) {
        let endpoint = self.endpoint.clone();
        let call = self.call_item().clone();
        (self.call_id, self.tool_name, self.arguments) = if endpoint == RESPONSES {
            (
                text_field(&call["call_id"]),
                text_field(&call["name"]),
                text_field(&call["arguments"]),
            )
        } else if endpoint == MESSAGES {
            (
                text_field(&call["id"]),
                text_field(&call["name"]),
                call["input"].to_string(),
            )
        } else {
            (
                text_field(&call["id"]),
                text_field(&call["function"]["name"]),
                text_field(&call["function"]["arguments"]),
            )
        };
    }

    fn set_arguments(&mut self, arguments: &str) {
        let endpoint = self.endpoint.clone();
        let call = self.call_item();
        if endpoint == MESSAGES {
            call["input"] = serde_json::from_str(arguments).expect("JSON arguments");
        } else if endpoint == RESPONSES {
            call["arguments"] = json!(arguments);
        } else {
            call["function"]["arguments"] = json!(arguments);
        }
        self.arguments = arguments.into();
    }

    fn set_name(&mut self, name: &str) {
        let endpoint = self.endpoint.clone();
        let call = self.call_item();
        if endpoint == MESSAGES || endpoint == RESPONSES {
            call["name"] = json!(name);
        } else {
            call["function"]["name"] = json!(name);
        }
        self.tool_name = name.into();
    }

    fn set_call_id(&mut self, id: &str) {
        let old = std::mem::replace(&mut self.call_id, id.into());
        let mut items = Value::Array(std::mem::take(&mut self.items));
        assert!(
            replace(&mut items, &old, id) > 0,
            "set-up: the conversation holds the call ID"
        );
        let Value::Array(items) = items else {
            unreachable!()
        };
        self.items = items;
    }

    fn add_result(&mut self, text: &str) {
        let id = &self.call_id;
        self.items.push(if self.endpoint == RESPONSES {
            json!({"type":"function_call_output","call_id":id,"output":text})
        } else if self.endpoint == MESSAGES {
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":text,"is_error":false}]})
        } else {
            json!({"role":"tool","tool_call_id":id,"content":text})
        });
    }

    fn add_user(&mut self, text: &str) {
        self.items.push(c::message("user", None, text));
    }

    /// The one place in the request that holds the text changes.
    fn change(&mut self, old: &str, new: &str) {
        let mut found = 0;
        for item in &mut self.items {
            found += replace(item, old, new);
        }
        for tool in &mut self.tools {
            found += replace(tool, old, new);
        }
        if self.system.as_deref() == Some(old) {
            self.system = Some(new.into());
            found += 1;
        }
        assert_eq!(found, 1, "set-up: one place holds the text {old:?}");
    }
}

fn text_field(value: &Value) -> String {
    value.as_str().expect("a text field").to_owned()
}

fn replace(value: &mut Value, old: &str, new: &str) -> usize {
    match value {
        Value::String(text) if text == old => {
            *text = new.into();
            1
        }
        Value::Array(items) => items.iter_mut().map(|v| replace(v, old, new)).sum(),
        Value::Object(fields) => fields.values_mut().map(|v| replace(v, old, new)).sum(),
        _ => 0,
    }
}

fn reply_text(endpoint: &str, reply: &Value) -> String {
    if endpoint == RESPONSES {
        reply["output"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["type"] == "message")
            .flat_map(|item| item["content"].as_array().into_iter().flatten())
            .filter_map(|part| part["text"].as_str())
            .collect()
    } else if endpoint == MESSAGES {
        reply["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| block["text"].as_str())
            .collect()
    } else {
        reply["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

/// The ACP sessions of FR-041 that this run added: each `session/new` and `session/load`.
fn session_calls(w: &World) -> usize {
    system_check::requests(&w.scenario)
        .iter()
        .filter(|r| matches!(r["method"].as_str(), Some("session/new" | "session/load")))
        .count()
}

fn log_bytes(w: &World) -> u64 {
    journal_check::files(&w.scenario.root.path.join("data/logs"))
        .iter()
        .map(|file| std::fs::metadata(file).expect("log file").len())
        .sum()
}

async fn send(w: &mut World, plan: TurnPlan) -> Value {
    w.history.sent += 1;
    assert!(w.history.sent <= REQUEST_CAP, "request cap of the scenario");
    w.scenario.start().await;
    w.history.sessions = session_calls(w);
    w.history.previous = w.history.session.take();
    let completed = journal_check::request_records(&w.scenario).len();
    let call = w.history.request();
    w.scenario.fake().next_prompt(plan);
    tokio::time::timeout(
        real_limit(REQUEST_LIMIT),
        content_check::send(&mut w.scenario, call),
    )
    .await
    .expect("time limit of one request");
    let answer = w.scenario.answer.as_ref().expect("an HTTP answer");
    assert_eq!(
        answer.status, 200,
        "set-up: the request reaches the product"
    );
    let reply = answer.json();
    assert_eq!(
        journal_check::request_records(&w.scenario).len(),
        completed + 1,
        "set-up: one log record for the request"
    );
    assert!(log_bytes(w) <= LOG_CAP, "log size cap of the scenario");
    w.history.session = Some(c::session(&w.scenario));
    w.history.keep_reply(&reply);
    reply
}

fn plan_text(text: &str) -> TurnPlan {
    TurnPlan::new().chunk(text).end("end_turn")
}

fn plan_call(text: &str, name: &str, arguments: &str) -> TurnPlan {
    let mut plan = TurnPlan::new();
    if !text.is_empty() {
        plan = plan.chunk(text);
    }
    plan.steps.push(TurnStep::ClientToolCall {
        name: name.into(),
        arguments: arguments.into(),
    });
    plan.end("end_turn")
}

async fn send_text(w: &mut World, text: &str) {
    let reply = send(w, plan_text(text)).await;
    expect_reply_text(w, &reply, text);
}

fn expect_reply_text(w: &World, reply: &Value, expected: &str) {
    assert_eq!(
        reply_text(&w.history.endpoint, reply),
        expected,
        "set-up: the reply text"
    );
}

async fn answer(w: &mut World) {
    let text = format!("Reply {}.", w.history.sent + 1);
    send_text(w, &text).await;
}

pub fn uses(w: &mut World, endpoint: String) {
    w.history.endpoint = endpoint;
}

pub fn offers(w: &mut World, name: &str, description: &str) {
    w.history.tools.push(json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object"},
    }));
}

pub fn system(w: &mut World, text: String) {
    w.history.system = Some(text);
}

pub async fn sent_user(w: &mut World, text: &str, reply: &str) {
    w.history.add_user(text);
    send_text(w, reply).await;
}

pub async fn called(w: &mut World, text: &str, name: &str, arguments: &str) {
    w.history.add_user("Use an offered tool.");
    let reply = send(w, plan_call(text, name, arguments)).await;
    expect_reply_text(w, &reply, text);
    w.history.reply_text = text.into();
    w.history.read_call();
    assert_eq!(w.history.tool_name, name, "set-up: the tool name");
    assert_eq!(w.history.arguments, arguments, "set-up: the arguments");
}

pub async fn sent_result(w: &mut World, text: &str, reply: &str) {
    w.history.add_result(text);
    send_text(w, reply).await;
}

pub async fn return_call(w: &mut World, arguments: &str, result: &str) {
    w.history.set_arguments(arguments);
    w.history.add_result(result);
    answer(w).await;
}

pub async fn return_id(w: &mut World, id: &str, result: &str) {
    w.history.set_call_id(id);
    w.history.add_result(result);
    answer(w).await;
}

pub async fn return_name(w: &mut World, name: &str, result: &str) {
    w.history.set_name(name);
    w.history.add_result(result);
    answer(w).await;
}

pub async fn return_text_and_call(w: &mut World, text: &str, arguments: &str, result: &str) {
    let old = w.history.reply_text.clone();
    w.history.change(&old, text);
    return_call(w, arguments, result).await;
}

pub async fn next_message(w: &mut World, text: &str) {
    w.history.add_user(text);
    answer(w).await;
}

pub async fn changed_text(w: &mut World, old: &str, new: &str, next: &str) {
    w.history.change(old, new);
    next_message(w, next).await;
}

pub async fn changed_arguments(w: &mut World, arguments: &str, next: &str) {
    w.history.set_arguments(arguments);
    next_message(w, next).await;
}

pub async fn changed_id(w: &mut World, id: &str, next: &str) {
    w.history.set_call_id(id);
    next_message(w, next).await;
}

fn previous(w: &World) -> String {
    w.history
        .previous
        .clone()
        .expect("set-up: an earlier request")
}

fn session_use(w: &World, expected: &str) {
    assert_eq!(
        c::kind(&w.scenario),
        expected,
        "FR-021 ADR-0003 session use of the request; prompt: {:?}",
        c::text(&w.scenario)
    );
}

pub fn continued_with_prompt(w: &World, text: &str) {
    session_use(w, "continue");
    c::continuation(&w.scenario, &previous(w), text);
}

pub fn continued_with_result(w: &World, text: &str) {
    let state = &w.history;
    let prompt = format!(
        "Result of your tool call {} {}:\n{text}",
        state.tool_name, state.arguments
    );
    continued_with_prompt(w, &prompt);
}

pub fn same_session_count(w: &World) {
    assert_eq!(
        session_calls(w),
        w.history.sessions,
        "FR-021 FR-041 the request adds no ACP session"
    );
}

pub fn rebuilt(w: &World) {
    session_use(w, "rebuild");
    assert_ne!(
        c::session(&w.scenario),
        previous(w),
        "FR-021 a rebuild uses a new ACP session"
    );
    assert!(
        c::text(&w.scenario).starts_with(c::NEUTRAL),
        "FR-021 the first prompt starts with the neutral line"
    );
}
