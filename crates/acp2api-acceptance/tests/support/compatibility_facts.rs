use acp2api_acceptance::{TurnPlan, TurnStep, fake::FakeRuntime, hub::Entry, rpc_script::RpcReply};
use serde_json::{Value, json};

pub fn configure(fake: &FakeRuntime, first: fn(&[Entry], &Value) -> TurnPlan) {
    fake.script_call("", RpcReply::Silent);
    metadata(fake);
    for make in [first, permission, echo, echo] {
        fake.script_call("session/prompt", RpcReply::Prompt(make));
    }
}

pub fn failure(fake: &FakeRuntime) {
    metadata(fake);
    fake.script_call("session/prompt", RpcReply::Prompt(built_in));
}

fn metadata(fake: &FakeRuntime) {
    for _ in 0..2 {
        fake.script_call("initialize", RpcReply::FromRequest(initialize));
    }
    fake.script_call("session/load", RpcReply::FromRequest(load));
}

fn initialize(entries: &[Entry], _: &Value) -> Value {
    let version = entries
        .iter()
        .rev()
        .find(|entry| entry.event["hello"]["role"] == "server")
        .unwrap()
        .event["hello"]["version"]
        .clone();
    json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true},"authMethods":[{"id":"oauth-personal","name":"Google"}],"agentInfo":{"name":"fake-acp-agent","version":version}})
}

fn messages(entries: &[Entry]) -> impl DoubleEndedIterator<Item = Value> + '_ {
    entries.iter().filter_map(|entry| {
        entry.event["line"]
            .as_str()
            .or_else(|| entry.event["command"]["line"].as_str())
            .and_then(|line| serde_json::from_str(line).ok())
    })
}

fn load(entries: &[Entry], call: &Value) -> Value {
    messages(entries)
        .rev()
        .find(|message| message["result"]["sessionId"] == call["params"]["sessionId"])
        .expect("script loads the actual earlier ACP session")["result"]
        .clone()
}

pub fn echo(entries: &[Entry], call: &Value) -> TurnPlan {
    let setup = messages(entries)
        .rev()
        .find(|message| {
            message["method"] == "session/new"
                && message["params"]["_meta"]["antigravity"]["systemPrompt"]["append"].is_string()
        })
        .expect("check session has its system prompt addition");
    let system = setup["params"]["_meta"]["antigravity"]["systemPrompt"]["append"]
        .as_str()
        .unwrap();
    let text = call["params"]["prompt"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    TurnPlan::new().chunk(&format!("{system}\n{text}"))
}

pub fn held(entries: &[Entry], call: &Value) -> TurnPlan {
    let mut plan = echo(entries, call);
    plan.steps
        .insert(0, TurnStep::HoldUntil("compatibility-check".into()));
    plan
}

fn built_in(_: &[Entry], _: &Value) -> TurnPlan {
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::BuiltInRuns {
        name: "view_file".into(),
    });
    plan
}

fn permission(entries: &[Entry], _: &Value) -> TurnPlan {
    let name = entries
        .iter()
        .rev()
        .filter(|entry| {
            entry.event["kind"] == "mcp" && entry.event["request"]["method"] == "tools/list"
        })
        .find_map(|entry| {
            entry.event["response"]["result"]["tools"]
                .as_array()?
                .first()?["name"]
                .as_str()
        })
        .expect("check offers a client tool over MCP HTTP");
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::ClientToolCall {
        name: name.into(),
        arguments: r#"{"value":"checked text"}"#.into(),
    });
    plan.end = acp2api_acceptance::TurnEndPlan::Never;
    plan
}
