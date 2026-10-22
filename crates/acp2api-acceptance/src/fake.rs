use crate::hub::{Hub, State};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};
#[derive(Clone, Debug)]
pub enum TurnStep {
    Chunk(String),
    Thought(String),
    Usage {
        used: u64,
        size: u64,
    },
    After(Duration),
    ClientToolCall {
        name: String,
        arguments: String,
    },
    BuiltInPermission {
        title: String,
    },
    BuiltInRuns {
        name: String,
    },
    /// Uses captured Error 400 grammar. Other account error classes are synthetic assumptions.
    ErrorLine {
        code: u16,
        status: String,
        message: String,
    },
    Stderr(String),
    HoldUntil(String),
}
#[derive(Clone, Debug)]
pub enum TurnEndPlan {
    Stop(String),
    RpcError { code: i64, message: String },
    IgnoreCancel,
    Exit(i32),
    Never,
}
#[derive(Clone, Debug)]
pub struct TurnPlan {
    pub steps: Vec<TurnStep>,
    pub end: TurnEndPlan,
}
impl Default for TurnPlan {
    fn default() -> Self {
        Self::new()
            .chunk("OK")
            .usage(1000, 1_048_576)
            .end("end_turn")
    }
}
impl TurnPlan {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            end: TurnEndPlan::Stop("end_turn".into()),
        }
    }
    pub fn chunk(mut self, text: &str) -> Self {
        self.steps.push(TurnStep::Chunk(text.into()));
        self
    }
    pub fn usage(mut self, used: u64, size: u64) -> Self {
        self.steps.push(TurnStep::Usage { used, size });
        self
    }
    pub fn after(mut self, duration: Duration) -> Self {
        self.steps.push(TurnStep::After(duration));
        self
    }
    pub fn end(mut self, reason: &str) -> Self {
        self.end = TurnEndPlan::Stop(reason.into());
        self
    }
}
#[derive(Debug)]
struct Paused {
    label: String,
    id: Value,
    session: String,
    plan: TurnPlan,
}
#[derive(Debug)]
struct ActiveTurn {
    id: Value,
    label: String,
    ignore_cancel: bool,
}
#[derive(Debug)]
enum SessionResult {
    Immediate,
    Held,
    Pending { label: String, response: Value },
}
#[derive(Debug)]
pub(crate) struct FakeState {
    session_result: SessionResult,
    catalog: Option<Vec<ModelSpec>>,
    plans: VecDeque<TurnPlan>,
    next_session: u64,
    pub model: String,
    signed_in: bool,
    pending: HashMap<String, ActiveTurn>,
    eager: HashMap<String, HashSet<String>>,
    servers: HashMap<String, Vec<Value>>,
    offered: HashSet<String>,
    mcp_count: usize,
    gates: HashMap<String, Paused>,
}
impl Default for FakeState {
    fn default() -> Self {
        Self {
            plans: VecDeque::new(),
            catalog: None,
            session_result: SessionResult::Immediate,
            next_session: 0,
            model: "gemini-3.8-flash".into(),
            signed_in: true,
            pending: HashMap::new(),
            eager: HashMap::new(),
            servers: HashMap::new(),
            offered: HashSet::new(),
            mcp_count: 0,
            gates: HashMap::new(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct FakeRuntime {
    hub: Hub,
}
#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub value: String,
    pub name: String,
    pub levels: Vec<String>,
}
impl FakeRuntime {
    pub fn hold_next_session_result(&self) {
        let mut state = self.hub.shared.0.lock().unwrap();
        assert!(matches!(
            state.fake.session_result,
            SessionResult::Immediate
        ));
        state.fake.session_result = SessionResult::Held;
    }
    pub fn release_session_result(&self) {
        let mut state = self.hub.shared.0.lock().unwrap();
        let result = std::mem::replace(&mut state.fake.session_result, SessionResult::Immediate);
        let SessionResult::Pending { label, response } = result else {
            panic!("pending session result required")
        };
        let now = state.now;
        emit(&mut state, &label, now, response);
        self.hub.shared.1.notify_all();
    }
    pub fn catalog(&self, models: &[ModelSpec]) {
        assert!(
            !models.is_empty() && models.len() <= 32,
            "fake catalog cap: 1 to 32 models"
        );
        assert!(
            models.iter().all(|m| m.value.len() <= 128
                && m.name.len() <= 128
                && m.levels.len() <= 3
                && m.levels
                    .iter()
                    .all(|v| ["low", "medium", "high"].contains(&v.as_str()))),
            "fake model input caps: names 128 bytes, 3 known thinking levels"
        );
        self.hub.shared.0.lock().unwrap().fake.catalog = Some(models.to_vec());
    }
    pub fn new(hub: Hub) -> Self {
        Self { hub }
    }
    pub fn next_prompt(&self, plan: TurnPlan) {
        self.hub.shared.0.lock().unwrap().fake.plans.push_back(plan);
    }
    pub fn open_gate(&self, gate: &str) {
        let mut s = self.hub.shared.0.lock().unwrap();
        let paused = s.fake.gates.remove(gate).expect("pending fake gate");
        run_plan(
            &mut s,
            &paused.label,
            paused.id,
            paused.session,
            paused.plan,
        );
        self.hub.shared.1.notify_all();
    }
    pub fn model(&self, name: &str) {
        self.hub.shared.0.lock().unwrap().fake.model = name.into();
    }
    pub fn prompts(&self) -> Vec<String> {
        self.hub
            .entries()
            .into_iter()
            .filter_map(|e| {
                let line = e.event["line"].as_str()?;
                let v: Value = serde_json::from_str(line).ok()?;
                (v["method"] == "session/prompt").then(|| {
                    v["params"]["prompt"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|b| b["text"].as_str())
                        .collect::<String>()
                })
            })
            .collect()
    }
    pub fn record(&self) -> Record {
        Record {
            entries: self.hub.entries(),
        }
    }
}
fn result(id: Value, value: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":value})
}
fn emit(s: &mut State, label: &str, at: u64, value: Value) {
    let command = json!({"op":"Out","line":value.to_string()});
    if at <= s.now {
        s.out(label, command);
    } else {
        s.external
            .entry(at)
            .or_default()
            .push(json!({"label":label,"command":command}));
    }
}
pub(crate) fn respond(s: &mut State, label: &str, call: Value) {
    let id = call["id"].clone();
    let method = call["method"].as_str().unwrap_or("");
    match method {
        "initialize" => emit(
            s,
            label,
            s.now,
            result(
                id,
                json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true},"authMethods":[],"agentInfo":{"name":"fake-acp-agent","version":"1.4.0"}}),
            ),
        ),
        "session/new" | "session/load" => {
            if !s.fake.signed_in {
                emit(
                    s,
                    label,
                    s.now,
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"Authentication required"}}),
                );
                return;
            }
            s.fake.next_session += 1;
            let session = format!("session-{}", s.fake.next_session);
            s.fake.eager.insert(
                session.clone(),
                call["params"]["_meta"]["antigravity"]["eagerMcpServers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|name| name.as_str().map(str::to_owned))
                    .collect(),
            );
            s.fake.servers.insert(
                session.clone(),
                call["params"]["mcpServers"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
            if let Some(catalog) = &s.fake.catalog {
                let options: Vec<_> = catalog
                    .iter()
                    .map(|m| json!({"value":m.value,"name":m.name}))
                    .collect();
                let available: Vec<_> = catalog.iter().flat_map(|m| m.levels.iter().map(|level| json!({"modelId":format!("{}-{level}",m.value),"name":format!("{} ({level})",m.name)}))).collect();
                let first = catalog.first().expect("non-empty fake model catalog");
                let level = first.levels.last().map(String::as_str).unwrap_or("high");
                let levels: Vec<_> = first
                    .levels
                    .iter()
                    .map(|v| json!({"value":v,"name":v}))
                    .collect();
                let current = if first.levels.is_empty() {
                    first.value.clone()
                } else {
                    format!("{}-{level}", first.value)
                };
                let metadata = json!({"sessionId":session,"models":{"currentModelId":current,"availableModels":available},"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":first.value,"options":options},{"id":"thinking_level","name":"Thinking Level","type":"select","currentValue":level,"options":levels}]});
                if matches!(s.fake.session_result, SessionResult::Held) {
                    s.fake.session_result = SessionResult::Pending {
                        label: label.into(),
                        response: result(id, metadata),
                    };
                    s.record(json!({"kind":"session-result-held"}));
                    return;
                }
                emit(s, label, s.now, result(id, metadata));
                return;
            }
            emit(
                s,
                label,
                s.now,
                result(
                    id,
                    json!({"sessionId":session,"models":{"currentModelId":s.fake.model,"availableModels":[{"modelId":s.fake.model,"name":"Gemini 3.8 Flash"}]},"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":s.fake.model,"options":[{"value":s.fake.model,"name":"Gemini 3.8 Flash"}]},{"id":"thinking_level","name":"Thinking level","type":"select","currentValue":"high","options":[{"value":"low","name":"Low"},{"value":"medium","name":"Medium"},{"value":"high","name":"High"}]}]}),
                ),
            );
        }
        "session/prompt" => {
            let session = call["params"]["sessionId"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            let plan = s.fake.plans.pop_front().unwrap_or_default();
            run_plan(s, label, id, session, plan);
        }

        "session/cancel" => {
            let session = call["params"]["sessionId"].as_str().unwrap_or("");
            if s.fake
                .pending
                .get(session)
                .is_some_and(|turn| turn.ignore_cancel)
            {
                return;
            }
            if let Some(turn) = s.fake.pending.remove(session) {
                s.fake.gates.retain(|_, paused| paused.session != session);
                for events in s.external.values_mut() {
                    events.retain(|event| event["session"] != session);
                }
                s.external.retain(|_, events| !events.is_empty());
                emit(
                    s,
                    &turn.label,
                    s.now,
                    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"tool_call_update","toolCallId":"cancelled","status":"failed","content":[{"type":"content","content":{"type":"text","text":"The request was cancelled by the client."}}]}}}),
                );
                emit(
                    s,
                    &turn.label,
                    s.now,
                    result(turn.id, json!({"stopReason":"cancelled"})),
                );
            }
        }
        "session/set_config_option" | "authenticate" => {
            emit(s, label, s.now, result(id, json!({})))
        }
        _ if !id.is_null() => emit(
            s,
            label,
            s.now,
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
        ),
        _ => {}
    }
}

pub(crate) fn mcp_requests(s: &mut State, call: &Value) -> Vec<(String, Value)> {
    if call["method"] != "session/prompt" {
        return Vec::new();
    }
    let session = call["params"]["sessionId"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    if !s.fake.offered.insert(session.clone()) {
        return Vec::new();
    }
    let eager = s.fake.eager.get(&session);
    let servers: Vec<_> = s
        .fake
        .servers
        .get(&session)
        .into_iter()
        .flatten()
        .filter(|server| {
            eager.is_some_and(|names| {
                server["name"]
                    .as_str()
                    .is_some_and(|name| names.contains(name))
            })
        })
        .cloned()
        .collect();
    s.fake.mcp_count += servers.len() * 3;
    if s.fake.mcp_count > 300 {
        s.failure = Some("fake MCP request cap: 300".into());
        return Vec::new();
    }
    servers.into_iter().flat_map(|server|{
        let url=server["url"].as_str().unwrap_or("").to_owned();
        vec![(url.clone(),json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"fake-acp-agent","version":"1.4.0"}}})),(url.clone(),json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}})),(url,json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}))]
    }).collect()
}
pub(crate) fn mcp_exchange(url: &str, body: &Value) -> Result<Value, String> {
    use std::io::{Read, Write};
    let address = url.strip_prefix("http://").ok_or("MCP URL scheme")?;
    let (host, path) = address.split_once('/').ok_or("MCP URL path")?;
    let mut stream = std::net::TcpStream::connect(host).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let body = body.to_string();
    write!(stream,"POST /{path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).map_err(|e|e.to_string())?;
    let mut data = Vec::new();
    stream
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 8 * 1024 * 1024 {
        return Err("MCP answer cap: 8 MiB".into());
    }
    let offset = data
        .windows(4)
        .position(|p| p == b"\r\n\r\n")
        .ok_or("MCP response head")?
        + 4;
    serde_json::from_slice(&data[offset..]).map_err(|e| e.to_string())
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub at: u64,
    pub session: String,
    pub text: String,
}
impl Prompt {
    pub fn text(&self) -> &str {
        &self.text
    }
}
#[derive(Clone, Debug)]
pub struct Record {
    pub entries: Vec<crate::hub::Entry>,
}
impl Record {
    fn calls(&self, method: &str) -> Vec<(u64, Value)> {
        self.entries
            .iter()
            .filter_map(|e| {
                let line = e.event["line"].as_str()?;
                let call: Value = serde_json::from_str(line).ok()?;
                (call["method"] == method).then_some((e.at, call))
            })
            .collect()
    }
    pub fn prompts(&self) -> Vec<Prompt> {
        self.calls("session/prompt")
            .into_iter()
            .map(|(at, v)| Prompt {
                at,
                session: v["params"]["sessionId"].as_str().unwrap().into(),
                text: v["params"]["prompt"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect(),
            })
            .collect()
    }
    pub fn cancels(&self) -> Vec<Value> {
        self.calls("session/cancel")
            .into_iter()
            .map(|(_, v)| v)
            .collect()
    }
    pub fn client_info(&self) -> Vec<Value> {
        self.calls("initialize")
            .into_iter()
            .map(|(_, v)| v["params"]["clientInfo"].clone())
            .collect()
    }
    pub fn session_meta(&self) -> Vec<Value> {
        self.calls("session/new")
            .into_iter()
            .map(|(_, v)| v["params"]["_meta"].clone())
            .collect()
    }
    pub fn offered_tools(&self) -> Vec<Value> {
        self.entries
            .iter()
            .filter(|e| e.event["kind"] == "mcp" && e.event["request"]["method"] == "tools/list")
            .map(|e| e.event["response"].clone())
            .collect()
    }
    pub fn usage_queries(&self) -> Vec<Prompt> {
        self.prompts()
            .into_iter()
            .filter(|p| p.text == "/usage")
            .collect()
    }
    pub fn processes_alive(&self) -> usize {
        let started = self
            .entries
            .iter()
            .filter(|e| e.event["kind"] == "hello")
            .count();
        let ended = self
            .entries
            .iter()
            .filter(|e| e.event["kind"] == "process_end")
            .count();
        started.saturating_sub(ended)
    }
}

fn run_plan(s: &mut State, label: &str, id: Value, session: String, plan: TurnPlan) {
    let mut at = s.now;
    s.fake.pending.insert(
        session.clone(),
        ActiveTurn {
            id: id.clone(),
            label: label.into(),
            ignore_cancel: matches!(plan.end, TurnEndPlan::IgnoreCancel),
        },
    );
    let mut remaining: VecDeque<_> = plan.steps.into();
    while let Some(step) = remaining.pop_front() {
        let update = match step {
            TurnStep::Chunk(text) => {
                json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}})
            }
            TurnStep::Thought(text) => {
                json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":text}})
            }
            TurnStep::Usage { used, size } => {
                json!({"sessionUpdate":"usage_update","used":used,"size":size})
            }
            TurnStep::After(duration) => {
                at += duration.as_millis() as u64;
                continue;
            }
            TurnStep::Stderr(line) => {
                let c = json!({"op":"Err","line":line});
                turn_schedule(s, label, at, (&session, &id), c, false);
                continue;
            }
            TurnStep::ErrorLine {
                code,
                status,
                message,
            } => {
                turn_schedule(
                    s,
                    label,
                    at,
                    (&session, &id),
                    json!({"op":"Err","line":format!("Error {code}, Message: {message}, Status: {status}, Details: []")}),
                    false,
                );
                continue;
            }
            TurnStep::ClientToolCall { name, arguments } => {
                turn_emit(
                    s,
                    label,
                    at,
                    (&session, &id),
                    json!({"jsonrpc":"2.0","id":9000,"method":"session/request_permission","params":{"sessionId":session,"toolCall":{"toolCallId":"tool-1","kind":"other","status":"pending","title":format!("Run {name}?"),"rawInput":serde_json::from_str::<Value>(&arguments).expect("client tool arguments must be JSON")},"options":[{"optionId":"allow_always","name":"Allow Always","kind":"allow_always"},{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"deny","name":"Deny","kind":"reject_once"}]}}),
                );
                continue;
            }
            TurnStep::BuiltInPermission { title } => {
                turn_emit(
                    s,
                    label,
                    at,
                    (&session, &id),
                    json!({"jsonrpc":"2.0","id":9001,"method":"session/request_permission","params":{"sessionId":session,"toolCall":{"toolCallId":"builtin-1","title":title},"options":[]}}),
                );
                continue;
            }
            TurnStep::BuiltInRuns { name } => {
                json!({"sessionUpdate":"tool_call","toolCallId":"builtin-1","title":name,"status":"completed"})
            }
            TurnStep::HoldUntil(gate) => {
                s.record(json!({"kind":"gate","name":gate}));
                s.fake.gates.insert(
                    gate,
                    Paused {
                        label: label.into(),
                        id: id.clone(),
                        session: session.clone(),
                        plan: TurnPlan {
                            steps: remaining.into(),
                            end: plan.end,
                        },
                    },
                );
                return;
            }
        };
        turn_emit(
            s,
            label,
            at,
            (&session, &id),
            json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":update}}),
        );
    }
    let command = match plan.end {
        TurnEndPlan::Stop(stop) => {
            json!({"op":"Out","line":result(id.clone(), json!({"stopReason":stop})).to_string()})
        }
        TurnEndPlan::RpcError { code, message } => {
            json!({"op":"Out","line":json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}).to_string()})
        }
        TurnEndPlan::Exit(code) => json!({"op":"Exit","code":code}),
        TurnEndPlan::Never | TurnEndPlan::IgnoreCancel => return,
    };
    turn_schedule(s, label, at, (&session, &id), command, true);
}

fn turn_emit(s: &mut State, label: &str, at: u64, owner: (&str, &Value), value: Value) {
    turn_schedule(
        s,
        label,
        at,
        owner,
        json!({"op":"Out","line":value.to_string()}),
        false,
    );
}
fn turn_schedule(
    s: &mut State,
    label: &str,
    at: u64,
    (session, id): (&str, &Value),
    command: Value,
    terminal: bool,
) {
    let event = json!({"label":label,"command":command,"session":session,"prompt_id":id,"terminal":terminal});
    if at <= s.now {
        dispatch(s, event);
    } else {
        s.external.entry(at).or_default().push(event);
    }
}
pub(crate) fn dispatch(s: &mut State, event: Value) {
    if let Some(session) = event["session"].as_str() {
        if !s
            .fake
            .pending
            .get(session)
            .is_some_and(|turn| turn.id == event["prompt_id"])
        {
            return;
        }
        if event["terminal"] == true {
            s.fake.pending.remove(session);
        }
    }
    s.out(event["label"].as_str().unwrap(), event["command"].clone());
}
