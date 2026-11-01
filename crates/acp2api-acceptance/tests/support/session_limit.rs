use crate::{World, runtime_support as process};
use acp2api_acceptance::{
    TurnPlan, TurnStep,
    client::{Answer, Call},
    conversation_check as conversation,
};
use serde_json::{Value, json};

/// FR-041: the temporary value of N in the first release.
pub const N: usize = 8;
const REQUEST_CAP: usize = 48;

#[derive(Clone, Debug)]
pub struct Conversation {
    pub messages: Vec<Value>,
    pub session: String,
}
pub struct Live {
    pub conversation: Conversation,
    pub text: String,
    pub reply: String,
    pub gate: String,
    pub call: Option<Call>,
    pub arrival: u64,
    pub process: usize,
    pub sessions: usize,
    pub loads: bool,
}
#[derive(Default)]
pub struct LimitState {
    pub conversations: Vec<Conversation>,
    pub earlier: Vec<Conversation>,
    pub running: Vec<Live>,
    pub waiting: Vec<Live>,
    pub finished: Vec<Live>,
    pub sent: usize,
    pub process_before_fill: usize,
    pub continued: String,
}
impl std::fmt::Debug for LimitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LimitState")
            .field("sent", &self.sent)
            .finish_non_exhaustive()
    }
}

/// Each ACP call that reached the fake, with the number of its runtime process.
/// A replacement stops one runtime process before the next one says hello.
pub fn calls(w: &World) -> Vec<(usize, Value)> {
    let mut process = 0;
    let mut calls = Vec::new();
    for entry in w.scenario.hub.entries() {
        if entry.event["kind"] == "hello" && entry.event["hello"]["role"] == "server" {
            process += 1;
        } else if entry.event["kind"] == "got"
            && let Some(line) = entry.event["line"].as_str()
            && let Ok(call) = serde_json::from_str(line)
        {
            calls.push((process, call));
        }
    }
    calls
}
pub fn current(w: &World) -> usize {
    process::servers(w).len()
}
/// The ACP sessions of the current runtime process, as the fake got them.
pub fn sessions(w: &World) -> usize {
    let process = current(w);
    calls(w)
        .iter()
        .filter(|(at, call)| {
            *at == process && (call["method"] == "session/new" || call["method"] == "session/load")
        })
        .count()
}
pub fn prompt(w: &World, text: &str) -> Option<(usize, String)> {
    calls(w).into_iter().find_map(|(process, call)| {
        (call["method"] == "session/prompt" && call.to_string().contains(text)).then(|| {
            (
                process,
                call["params"]["sessionId"].as_str().unwrap().to_owned(),
            )
        })
    })
}
/// The time of the hello of the current runtime process.
pub fn last_start(w: &World) -> u64 {
    w.scenario
        .hub
        .entries()
        .into_iter()
        .rfind(|e| e.event["kind"] == "hello" && e.event["hello"]["role"] == "server")
        .expect("a runtime process started")
        .at
}
pub fn loads(w: &World, session: &str) -> Vec<usize> {
    calls(w)
        .into_iter()
        .filter(|(_, call)| {
            call["method"] == "session/load" && call["params"]["sessionId"] == session
        })
        .map(|(process, _)| process)
        .collect()
}
fn prompt_position(w: &World, text: &str) -> Option<usize> {
    calls(w)
        .iter()
        .position(|(_, call)| call["method"] == "session/prompt" && call.to_string().contains(text))
}
pub fn arrived_in_order(w: &World, first: &str, second: &str) -> bool {
    matches!(
        (prompt_position(w, first), prompt_position(w, second)),
        (Some(first), Some(second)) if first < second
    )
}
fn plan(gate: Option<&str>, reply: &str) -> TurnPlan {
    let mut plan = TurnPlan::new();
    if let Some(gate) = gate {
        plan.steps.push(TurnStep::HoldUntil(gate.into()));
    }
    plan.chunk(reply).usage(1000, 1_048_576)
}
async fn send(w: &mut World, messages: &[Value], turn: TurnPlan) -> Call {
    assert!(
        w.limit.sent < REQUEST_CAP,
        "session limit scenario API request cap: {REQUEST_CAP}"
    );
    w.limit.sent += 1;
    w.scenario.fake().next_prompt(turn);
    let call = conversation::call(json!(messages), None, Value::Null);
    let live = w.scenario.client().await.send(call);
    w.scenario.time.settle().await;
    live
}
fn reply_text(answer: &Answer) -> Value {
    answer.json()["choices"][0]["message"]["content"].clone()
}

pub async fn start(w: &mut World) {
    process::start(w).await;
}
/// Sends one conversation to its end, so that its ACP session stays open with no prompt turn.
async fn finished_conversation(w: &mut World) {
    let number = w.limit.sent;
    let text = format!("[session limit conversation {number}]");
    let reply = format!("session limit reply {number}");
    let messages = vec![json!({"role":"user","content":text})];
    let live = send(w, &messages, plan(None, &reply)).await;
    let answer = live.finish(&mut w.scenario.time).await;
    assert_eq!(
        answer.status, 200,
        "setup request reaches the fake ACP agent"
    );
    assert_eq!(reply_text(&answer), json!(reply));
    let (_, session) = prompt(w, &text).expect("setup prompt reaches ACP");
    let mut messages = messages;
    messages.push(json!({"role":"assistant","content":reply}));
    w.limit
        .conversations
        .push(Conversation { messages, session });
}
/// Fills the current runtime process until the fake holds `target` ACP sessions.
pub async fn fill(w: &mut World, target: usize) {
    start(w).await;
    w.limit.process_before_fill = current(w);
    w.limit.earlier.append(&mut w.limit.conversations);
    while sessions(w) < target {
        finished_conversation(w).await;
        assert_eq!(
            current(w),
            w.limit.process_before_fill,
            "FR-041 no replacement before the runtime process holds N ACP sessions"
        );
    }
    assert_eq!(
        (current(w), sessions(w)),
        (w.limit.process_before_fill, target),
        "FR-041 setup: one runtime process holds {target} ACP sessions"
    );
    w.sign_in.process_before = process::process_ids(w);
}
fn take_conversation(w: &mut World, earlier: bool) -> Conversation {
    let pool = if earlier {
        &mut w.limit.earlier
    } else {
        &mut w.limit.conversations
    };
    assert!(!pool.is_empty(), "a conversation to continue exists");
    pool.remove(0)
}
async fn live(w: &mut World, conversation: Conversation, continues: bool) -> Live {
    let number = w.limit.sent;
    let gate = format!("session-limit-{number}");
    let text = format!("[session limit request {number}]");
    let reply = format!("session limit answer {number}");
    let mut conversation = conversation;
    conversation
        .messages
        .push(json!({"role":"user","content":text}));
    let process = current(w);
    let sessions = sessions(w);
    let arrival = w.scenario.hub.now();
    let call = send(w, &conversation.messages, plan(Some(&gate), &reply)).await;
    Live {
        conversation,
        text,
        reply,
        gate,
        call: Some(call),
        arrival,
        process,
        sessions,
        loads: continues,
    }
}
pub async fn new_request(w: &mut World) {
    w.sign_in.process_before = process::process_ids(w);
    let conversation = Conversation {
        messages: Vec::new(),
        session: String::new(),
    };
    let request = live(w, conversation, false).await;
    w.limit.waiting.push(request);
}
pub async fn continuing_request(w: &mut World, earlier: bool) {
    w.sign_in.process_before = process::process_ids(w);
    let conversation = take_conversation(w, earlier);
    w.limit.continued = conversation.session.clone();
    let request = live(w, conversation, earlier).await;
    w.limit.waiting.push(request);
}
pub async fn running_turn(w: &mut World) {
    let conversation = take_conversation(w, false);
    let session = conversation.session.clone();
    let request = live(w, conversation, false).await;
    assert_eq!(
        prompt(w, &request.text),
        Some((current(w), session)),
        "FR-041 setup: the prompt turn runs in one of the ACP sessions"
    );
    w.limit.running.push(request);
}
pub fn has_prompt(w: &World, request: &Live) -> bool {
    prompt(w, &request.text).is_some()
}
/// Opens the gate of a prompt turn and reads its complete response.
pub async fn complete(w: &mut World, request: &mut Live) {
    w.scenario.fake().open_gate(&request.gate);
    let answer = request
        .call
        .take()
        .unwrap()
        .finish(&mut w.scenario.time)
        .await;
    assert_eq!(
        answer.status, 200,
        "FR-041 the response comes with HTTP 200"
    );
    assert_eq!(
        reply_text(&answer),
        json!(request.reply),
        "FR-041 the response comes from the prompt turn of the request"
    );
    let content = request.reply.clone();
    request
        .conversation
        .messages
        .push(json!({"role":"assistant","content":content}));
}
pub async fn end_running(w: &mut World) {
    let mut request = w.limit.running.remove(0);
    complete(w, &mut request).await;
    w.limit.finished.push(request);
}
/// Waits for the prompt of the oldest waiting request, then completes it.
pub async fn response_from_new_process(w: &mut World) {
    assert!(
        !w.limit.waiting.is_empty(),
        "a request waits for a response"
    );
    let mut request = w.limit.waiting.remove(0);
    let text = request.text.clone();
    process::observe(w, |w| prompt(w, &text).is_some()).await;
    let (at, session) = prompt(w, &text).unwrap_or_else(|| {
        panic!("FR-041 the prompt of the waiting request reaches the new runtime process")
    });
    assert_eq!(
        at,
        current(w),
        "FR-041 the newest runtime process runs the prompt turn"
    );
    assert!(
        at > request.process,
        "FR-041 the response comes from a runtime process after the replacement"
    );
    complete(w, &mut request).await;
    request.conversation.session = session;
    let conversation = request.conversation.clone();
    w.limit.conversations.push(conversation);
    w.limit.finished.push(request);
}
pub async fn at_once_in_same_process(w: &mut World) {
    let mut request = w.limit.waiting.remove(0);
    let (at, session) = prompt(w, &request.text)
        .expect("FR-041 the prompt turn starts with no replacement and no wait");
    assert_eq!(
        (at, current(w)),
        (request.process, request.process),
        "FR-041 the prompt turn stays in the same runtime process"
    );
    assert_eq!(
        w.scenario.hub.now(),
        request.arrival,
        "FR-041 the prompt turn starts with no virtual wait"
    );
    let new_sessions = sessions(w) - request.sessions;
    if request.conversation.session.is_empty() || request.loads {
        assert_eq!(new_sessions, 1, "FR-041 the request adds one ACP session");
    } else {
        assert_eq!(
            session, request.conversation.session,
            "FR-041 the request continues its ACP session"
        );
        assert_eq!(
            new_sessions, 0,
            "FR-041 a continued ACP session adds no ACP session"
        );
    }
    complete(w, &mut request).await;
    request.conversation.session = session;
    let conversation = request.conversation.clone();
    w.limit.conversations.push(conversation);
    w.limit.finished.push(request);
}
pub fn assert_no_response(w: &World) {
    assert!(!w.limit.waiting.is_empty(), "a request waits");
    for request in &w.limit.waiting {
        assert_eq!(
            request.call.as_ref().unwrap().status(),
            None,
            "FR-041 the request stays in the queue with no response"
        );
    }
}
pub fn assert_all_wait(w: &World) {
    assert!(!w.limit.waiting.is_empty(), "a request waits");
    for request in &w.limit.waiting {
        assert!(
            !has_prompt(w, request),
            "FR-041 the waiting request gets no prompt turn while the runtime process holds N ACP sessions"
        );
        assert_eq!(
            (sessions(w), current(w)),
            (request.sessions, request.process),
            "FR-041 the waiting request gets no ACP session"
        );
        assert_eq!(
            request.call.as_ref().unwrap().status(),
            None,
            "FR-041 the waiting request has no response"
        );
    }
    assert_eq!(
        w.scenario.fake().record().cancels().len(),
        0,
        "FR-041 the replacement cancels no prompt turn"
    );
}

pub async fn cleanup(w: &mut World) {
    let live = w.limit.running.len() + w.limit.waiting.len();
    w.limit.running.clear();
    w.limit.waiting.clear();
    if live > 0 {
        w.scenario.time.settle().await;
    }
}
