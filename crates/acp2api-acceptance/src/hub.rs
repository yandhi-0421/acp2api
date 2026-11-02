use crate::fake::FakeState;
use crate::real_limit;
use crate::time_limits::SETTLE_CAP;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
pub const EVENT_CAP: usize = 20_000;
pub const RECORD_CAP: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Entry {
    pub at: u64,
    pub event: Value,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub holds: u64,
    pub stdin_written: u64,
    pub stdin_got: u64,
    pub ordered: u64,
    pub read: u64,
    pub processed: u64,
    /// Derived stdin lines closed after a product stop and control connection end.
    pub derived_stdin_closed: u64,
    /// Derived ordered output lines that no longer need to be read after that stop.
    pub derived_read_closed: u64,
    /// Derived ordered output lines that no longer need completion after that stop.
    pub derived_processed_closed: u64,
    /// Derived number of processes whose deliverable IO remains unbalanced.
    pub derived_open_process_io: u64,
    pub http_sent: u64,
    pub http_started: u64,
    pub mcp_started: u64,
    pub mcp_finished: u64,
    pub fake_mcp_inflight: u64,
    pub fake_mcp_sent: u64,
    pub client_read: u64,
    pub client_consumed: u64,
    pub client_available: bool,
    pub wake_pending: u64,
    pub revision: u64,
}
impl Counts {
    pub fn balanced(self) -> bool {
        self.holds == 0
            && self.stdin_written == self.stdin_got + self.derived_stdin_closed
            && self.ordered == self.read + self.derived_read_closed
            && self.ordered == self.processed + self.derived_processed_closed
            && self.derived_open_process_io == 0
            && self.http_sent + self.fake_mcp_sent == self.http_started + self.mcp_started
            && self.client_read == self.client_consumed
            && !self.client_available
            && self.mcp_started == self.mcp_finished
            && self.fake_mcp_inflight == 0
            && self.wake_pending == 0
    }
}
pub(crate) struct Sleeper {
    pub at: u64,
    pub task: Option<String>,
    pub writer: Option<Arc<Mutex<TcpStream>>>,
}
pub(crate) struct ClientSocket {
    pub socket: TcpStream,
    pub read: u64,
    pub consumed: u64,
}
#[derive(Default)]
struct ProcessIo {
    stdin_written: u64,
    stdin_got: u64,
    ordered: u64,
    read: u64,
    processed: u64,
    ended: bool,
    stop_requested: bool,
}
pub(crate) struct State {
    pub active_request: Option<(usize, Value)>,
    pub client_sockets: HashMap<u64, ClientSocket>,
    pub next_socket: u64,
    pub now: u64,
    pub counts: Counts,
    process_io: Vec<ProcessIo>,
    pending_processes: HashMap<u64, usize>,
    server_processes: HashMap<String, usize>,
    // The reading task creates each RuntimeLine hold before it sends the line to the conductor.
    last_read_process_by_task: HashMap<String, usize>,
    pub entries: Vec<Entry>,
    pub bytes: usize,
    pub failure: Option<String>,
    pub sleepers: HashMap<u64, Sleeper>,
    pub wake_pending: Vec<u64>,
    pub completed_timers: HashMap<u64, String>,
    pub external: BTreeMap<u64, Vec<Value>>,
    pub servers: HashMap<String, Arc<Mutex<TcpStream>>>,
    pub connections: Vec<TcpStream>,
    #[cfg(windows)]
    pub process_root: Option<std::path::PathBuf>,
    pub fake: FakeState,
    pub stop: bool,
    pub ready: bool,
    pub terminal_bytes: usize,
    pub next_sleep: u64,
    trace_connections: HashMap<String, u64>,
    trace_ops: HashMap<String, u64>,
    trace_events: std::collections::VecDeque<Value>,
    trace_sequence: u64,
    trace_started: Instant,
    trace_holds: HashMap<String, Value>,
}
impl State {
    pub(crate) fn trace_230_event(&mut self, event: Value) {
        if std::env::var_os("ACP2API_TRACE_230").is_none() {
            return;
        }
        self.trace_sequence += 1;
        if self.trace_events.len() == 1024 {
            self.trace_events.pop_front();
        }
        self.trace_events.push_back(json!({"seq":self.trace_sequence,"real_us":self.trace_started.elapsed().as_micros(),"now":self.now,"event":event,"holds":self.counts.holds,"processes":self.process_io.len(),"sleepers":self.sleepers.len(),"external":self.external.keys().collect::<Vec<_>>()}));
    }

    pub(crate) fn record(&mut self, event: Value) {
        let bytes = event.to_string().len();
        if self.entries.len() >= EVENT_CAP || self.bytes + bytes > RECORD_CAP {
            self.failure = Some("hub record cap: 20000 events or 16 MiB".into());
            return;
        }
        self.bytes += bytes;
        self.entries.push(Entry {
            at: self.now,
            event,
        });
        self.counts.revision += 1;
    }
    /// Replies can outlive the command channel of a process that the product stopped.
    fn stopped_by_product(&self, label: &str) -> bool {
        self.server_processes
            .get(label)
            .is_some_and(|process| self.process_io[*process].stop_requested)
    }
    pub(crate) fn out(&mut self, label: &str, command: Value) {
        if command["op"] == "Out" || command["op"] == "Err" {
            let lines = command["line"]
                .as_str()
                .unwrap_or("")
                .bytes()
                .filter(|b| *b == b'\n')
                .count() as u64
                + 1;
            if let Some(process) = self.server_processes.get(label) {
                self.process_io[*process].ordered += lines;
            }
        }
        self.record(json!({"kind":"command", "label":label, "command":command}));
        if self.failure.is_some() {
            return;
        }
        if let Some(writer) = self.servers.get(label) {
            if let Err(e) = send(&mut writer.lock().unwrap(), &command)
                && !self.stopped_by_product(label)
            {
                self.failure = Some(format!("fake command channel: {e}"));
            }
        } else if !self.stopped_by_product(label) {
            self.failure = Some(format!("missing fake channel {label}"));
        }
    }
}
#[derive(Clone)]
pub struct Hub {
    pub address: String,
    pub(crate) trace_settle_stage: Arc<std::sync::atomic::AtomicU8>,
    pub token: String,
    pub(crate) shared: Arc<(Mutex<State>, Condvar)>,
    alive: Arc<AtomicBool>,
}
impl std::fmt::Debug for Hub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hub")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}
impl Hub {
    pub fn new(token: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("hub listener");
        let address = listener.local_addr().unwrap().to_string();
        listener.set_nonblocking(true).unwrap();
        let shared = Arc::new((
            Mutex::new(State {
                client_sockets: HashMap::new(),
                next_socket: 0,
                now: 0,
                counts: Counts::default(),
                process_io: Vec::new(),
                pending_processes: HashMap::new(),
                server_processes: HashMap::new(),
                last_read_process_by_task: HashMap::new(),
                entries: Vec::new(),
                bytes: 0,
                failure: None,
                active_request: None,
                sleepers: HashMap::new(),
                wake_pending: Vec::new(),
                completed_timers: HashMap::new(),
                external: BTreeMap::new(),
                servers: HashMap::new(),
                connections: Vec::new(),
                #[cfg(windows)]
                process_root: None,
                fake: FakeState::default(),
                stop: false,
                ready: false,
                terminal_bytes: 0,
                next_sleep: 0,
                trace_connections: HashMap::new(),
                trace_ops: HashMap::new(),
                trace_events: std::collections::VecDeque::new(),
                trace_sequence: 0,
                trace_started: Instant::now(),
                trace_holds: HashMap::new(),
            }),
            Condvar::new(),
        ));
        let hub = Self {
            trace_settle_stage: Arc::new(std::sync::atomic::AtomicU8::new(0)),
            address,
            token,
            shared,
            alive: Arc::new(AtomicBool::new(true)),
        };
        let worker = hub.clone();
        thread::spawn(move || {
            while worker.alive.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let hub = worker.clone();
                        thread::spawn(move || hub.connection(stream));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        let state = worker.shared.0.lock().unwrap();
                        let _ = worker
                            .shared
                            .1
                            .wait_timeout(state, Duration::from_millis(1));
                    }
                    Err(_) => break,
                }
            }
        });
        hub
    }
    fn connection(&self, stream: TcpStream) {
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(real_limit(Duration::from_secs(300))))
            .unwrap();
        let writer = Arc::new(Mutex::new(
            crate::client::duplicate_stream(&stream).unwrap(),
        ));

        let mut reader = BufReader::new(stream);
        let mut role = String::new();
        let mut label = String::new();
        let mut pid = 0;
        let mut process = None;
        #[cfg(windows)]
        let mut process_recorded = false;
        loop {
            let mut line = String::new();
            let n =
                match reader.by_ref().take(1024 * 1024 + 1).read_line(&mut line) {
                    Ok(n) => n,
                    Err(e) => {
                        self.shared.0.lock().unwrap().record(
                            json!({"kind":"channel_error","role":role,"error":e.to_string()}),
                        );
                        break;
                    }
                };
            if n == 0 || line.len() > 1024 * 1024 {
                break;
            }
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                break;
            };
            if role.is_empty() {
                if message["token"] != self.token {
                    break;
                }
                if message["op"] == "Hello" {
                    pid = message["pid"].as_u64().expect("fake Hello PID");
                }
                role = message["role"].as_str().unwrap_or("rpc").to_owned();
                if std::env::var_os("ACP2API_TRACE_230").is_some() {
                    let class = format!("{}:{}", role, message["op"].as_str().unwrap_or("?"));
                    *self
                        .shared
                        .0
                        .lock()
                        .unwrap()
                        .trace_connections
                        .entry(class)
                        .or_default() += 1;
                }
                if role == "server" {
                    let mut state = self.shared.0.lock().unwrap();
                    let id = state.process_io.len();
                    state.process_io.push(ProcessIo::default());
                    state.pending_processes.insert(pid, id);
                    process = Some(id);
                }
                if role != "rpc" {
                    self.shared
                        .0
                        .lock()
                        .unwrap()
                        .connections
                        .push(crate::client::duplicate_stream(reader.get_ref()).unwrap());
                }
                label = message["label"].as_str().unwrap_or("normal").to_owned();
            }
            let mut state = self.shared.0.lock().unwrap();
            if std::env::var_os("ACP2API_TRACE_230").is_some() {
                *state
                    .trace_ops
                    .entry(message["op"].as_str().unwrap_or("?").to_owned())
                    .or_default() += 1;
            }
            let result = match message["op"].as_str().unwrap_or("") {
                "Hello" => {
                    #[cfg(windows)]
                    if !process_recorded && (role == "server" || role == "harness") {
                        let pid = u32::try_from(pid).expect("fake Hello PID");
                        if let Some((_process, created)) = crate::setup::open_fake_process(pid) {
                            let record = format!("{pid} {created}\n");
                            let cap = (EVENT_CAP * 32) as u64; // u32 + space + u64 + newline.
                            let mut file = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(
                                    state
                                        .process_root
                                        .as_ref()
                                        .expect("fake process root")
                                        .join(".acp2api-pids"),
                                )
                                .expect("open fake PID record");
                            assert!(
                                file.metadata().unwrap().len() + record.len() as u64 <= cap,
                                "fake PID record cap: {cap} bytes"
                            );
                            file.write_all(record.as_bytes()).expect("record fake PID");
                        }
                        process_recorded = true;
                    }
                    if role == "server" {
                        state.servers.insert(label.clone(), writer.clone());
                        state
                            .server_processes
                            .insert(label.clone(), process.unwrap());
                    }
                    let mut redacted = message.clone();
                    redacted.as_object_mut().unwrap().remove("token");
                    state.trace_230_event(json!({"op":"Hello","role":role,"process":process}));
                    state.record(json!({"kind":"hello","hello":redacted}));
                    json!({"ok":true})
                }
                "Got" => {
                    if let Some(line) = message["line"].as_str() {
                        match serde_json::from_str::<Value>(line) {
                            Ok(value) => {
                                state.trace_230_event(json!({"op":"Got","method":value["method"].as_str().map(|s|s.chars().take(48).collect::<String>()),"id":value["id"].as_u64(),"process":process}));
                                let mut redacted = value.clone();
                                redact(&mut redacted);
                                state.record(json!({"kind":"got","line":redacted.to_string()}));
                                let requests = crate::fake::mcp_requests(&mut state, &value);
                                state.counts.fake_mcp_inflight += requests.len() as u64;
                                state.counts.fake_mcp_sent += requests.len() as u64;
                                drop(state);
                                for (url, request) in requests {
                                    let response = crate::fake::mcp_exchange(&url, &request);
                                    let mut state = self.shared.0.lock().unwrap();
                                    state.counts.fake_mcp_inflight -= 1;
                                    match response {Ok(response)=>state.record(json!({"kind":"mcp","request":request,"response":response})),Err(error)=>state.failure=Some(format!("fake MCP: {error}"))}
                                }
                                state = self.shared.0.lock().unwrap();
                                crate::fake::respond(&mut state, &label, value);
                            }
                            Err(_) => state.failure = Some("invalid ACP from product".into()),
                        }
                    }
                    // Keep reply preparation outstanding until its output is queued.
                    state.process_io[process.expect("fake Hello precedes Got")].stdin_got += 1;
                    self.shared.1.notify_all();
                    continue;
                }
                "StdinEndArmed" => {
                    state.record(json!({"kind":"stdin-end-armed"}));
                    self.shared.1.notify_all();
                    continue;
                }
                // The fake writes this line by itself, so the count of output grows here.
                "StdinEndAnswer" => {
                    state.process_io[process.expect("fake Hello precedes its answer")].ordered += 1;
                    state.record(json!({"kind":"stdin-end-answer"}));
                    self.shared.1.notify_all();
                    continue;
                }
                "hold" => {
                    let kind = message["kind"].as_str().unwrap_or("");
                    let add = message["add"].as_bool().unwrap_or(false);
                    let reader = message["reader"].as_str().unwrap_or("<missing>");
                    let line_process = if add && kind == "RuntimeLine" {
                        state.last_read_process_by_task.get(reader).copied()
                    } else {
                        None
                    };
                    if add && kind == "RuntimeLine" && line_process.is_none() {
                        let message =
                            format!("RuntimeLine hold without an output read for task {reader}");
                        state.failure = Some(message.clone());
                        json!({"error":message})
                    } else {
                        if add {
                            state.counts.holds += 1;
                            if kind == "HttpRequest" {
                                state.counts.http_started += 1;
                            }
                            if kind == "McpRequest" {
                                state.counts.mcp_started += 1;
                            }
                        } else {
                            state.counts.holds =
                                state.counts.holds.checked_sub(1).expect("balanced holds");
                            if kind == "RuntimeLine" {
                                let process = message["process"].as_u64().unwrap() as usize;
                                state.process_io[process].processed += 1;
                            }
                            if kind == "McpRequest" {
                                state.counts.mcp_finished += 1;
                            }
                        }
                        if std::env::var_os("ACP2API_TRACE_230").is_some() {
                            let id = message["trace_hold"].as_str().unwrap_or("unknown");
                            if add && state.trace_holds.len() < 256 {
                                state.trace_holds.insert(id.into(), json!({"id":id,"kind":kind,"reader":reader,"process":line_process}));
                            } else if !add {
                                state.trace_holds.remove(id);
                            }
                            state.trace_230_event(json!({"op":"hold","add":add,"kind":kind,"id":id,"reader":reader,"process":message["process"],"line_process":line_process}));
                        }
                        json!({"now":state.now,"process":line_process})
                    }
                }
                "io" => {
                    let n = message["lines"].as_u64().unwrap_or(0);
                    let process = message["process"].as_u64().unwrap() as usize;
                    if message["direction"] == "stdin" {
                        state.process_io[process].stdin_written += n;
                    } else {
                        state.process_io[process].read += n;
                        state
                            .last_read_process_by_task
                            .insert(message["reader"].as_str().unwrap().to_owned(), process);
                    }
                    state.trace_230_event(json!({"op":"io","direction":message["direction"],"lines":n,"process":process,"reader":message["reader"]}));
                    json!({"ok":true})
                }
                "trace_230" => {
                    state.trace_230_event(json!({"op":"write-pending","process":message["process"],"reader":message["reader"]}));
                    json!({"ok":true})
                }
                "process" => {
                    state.trace_230_event(
                        json!({"op":"process-register-begin","pid":message["pid"]}),
                    );
                    let pid = message["pid"].as_u64().expect("fake process PID");
                    let (registered, timeout) = self
                        .shared
                        .1
                        .wait_timeout_while(state, real_limit(SETTLE_CAP), |state| {
                            !state.pending_processes.contains_key(&pid)
                        })
                        .unwrap();
                    state = registered;
                    if timeout.timed_out() {
                        let message =
                            format!("fake Hello {} s cap: pid {pid}", SETTLE_CAP.as_secs());
                        state.failure = Some(message.clone());
                        json!({"error":message})
                    } else {
                        let process = state.pending_processes.remove(&pid).unwrap();
                        state.trace_230_event(
                            json!({"op":"process-register-done","process":process}),
                        );
                        json!({"process":process})
                    }
                }
                "process_stop" => {
                    let process = message["process"].as_u64().unwrap() as usize;
                    state.process_io[process].stop_requested = true;
                    state.trace_230_event(json!({"op":"process-stop","process":process}));
                    json!({"ok":true})
                }
                "trace_230_stop_done" => {
                    state.trace_230_event(
                        json!({"op":"process-stop-done","process":message["process"]}),
                    );
                    json!({"ok":true})
                }
                "now" | "drain" => {
                    if message["op"] == "now" {
                        state.trace_230_event(json!({"op":"now","reader":message["trace_task"]}));
                    }
                    json!({"now":state.now})
                }
                "sleep" => {
                    state.next_sleep += 1;
                    let id = state.next_sleep;
                    let at = message["at"].as_u64().unwrap().max(state.now);
                    state.sleepers.insert(
                        id,
                        Sleeper {
                            at,
                            task: None,
                            writer: None,
                        },
                    );
                    state.trace_230_event(json!({"op":"sleep","id":id,"at":at,"requested_at":message["at"],"reader":message["trace_task"]}));
                    json!({"id":id,"now":state.now})
                }
                "wait" => {
                    let id = message["id"].as_u64().unwrap();
                    if let Some(s) = state.sleepers.get_mut(&id) {
                        s.writer = Some(writer.clone());
                    }
                    if state.wake_pending.contains(&id) {
                        let _ = send(
                            &mut writer.lock().unwrap(),
                            &json!({"wake":id,"now":state.now}),
                        );
                    }
                    continue;
                }
                "sleep_waiting" => {
                    let id = message["id"].as_u64().unwrap();
                    if let Some(sleeper) = state.sleepers.get_mut(&id) {
                        sleeper.task = Some(message["task"].as_str().unwrap().to_owned());
                        state.counts.revision += 1;
                    }
                    state.trace_230_event(
                        json!({"op":"sleep-waiting","id":id,"reader":message["task"]}),
                    );
                    json!({"ok":true})
                }
                "sleep_delivered" => {
                    let id = message["id"].as_u64().unwrap();
                    state.sleepers.remove(&id);
                    state.trace_230_event(
                        json!({"op":"sleep-delivered","id":id,"reader":message["trace_task"]}),
                    );
                    json!({"ok":true})
                }
                "sleep_done" => {
                    let id = message["id"].as_u64().unwrap();
                    state.sleepers.remove(&id);
                    if state.wake_pending.contains(&id)
                        && let Some(task) = message["task"].as_str()
                    {
                        state.completed_timers.insert(id, task.to_owned());
                    }
                    state.wake_pending.retain(|x| *x != id);
                    state.counts.wake_pending = state.wake_pending.len() as u64;
                    state.trace_230_event(
                        json!({"op":"sleep-done","id":id,"reader":message["trace_task"]}),
                    );
                    json!({"ok":true})
                }
                "stop_wait" => {
                    if !state.stop {
                        state.servers.insert("stop".into(), writer.clone());
                        continue;
                    }
                    json!({"stop":true})
                }
                "account_hold" => {
                    state.servers.insert("account-first".into(), writer.clone());
                    state.record(json!({"kind":"account_held","pid":message["pid"]}));
                    self.shared.1.notify_all();
                    continue;
                }
                "launch" => {
                    state
                        .trace_230_event(json!({"op":"launch-begin","allowed":message["allowed"]}));
                    state.record(json!({"kind":"launch","allowed":message["allowed"],"program":message["program"]}));
                    json!({"ok":true})
                }
                "browser" => {
                    state.record(json!({"kind":"browser","url":message["url"]}));
                    json!({"ok":true})
                }
                other => {
                    state.failure = Some(format!("unknown hub operation {other}"));
                    json!({"error":"unknown operation"})
                }
            };
            self.shared.1.notify_all();
            let _ = send(&mut writer.lock().unwrap(), &result);
            if role == "rpc" {
                break;
            }
        }
        if role == "server" || role == "harness" {
            let mut state = self.shared.0.lock().unwrap();
            state.record(json!({"kind":"process_end","label":label,"role":role}));
            if let Some(process) = process {
                state.process_io[process].ended = true;
                if state.server_processes.get(&label) == Some(&process) {
                    state.servers.remove(&label);
                }
            }
            self.shared.1.notify_all();
        }
        let _ = reader.get_ref().shutdown(Shutdown::Both);
    }
    pub fn entries(&self) -> Vec<Entry> {
        self.shared.0.lock().unwrap().entries.clone()
    }
    pub fn active_request(&self) -> (usize, Value) {
        self.shared
            .0
            .lock()
            .unwrap()
            .active_request
            .clone()
            .expect("an API request precedes its prompt assertion")
    }
    pub fn trace_230_history(&self) -> Value {
        let Ok(s) = self.shared.0.try_lock() else {
            return json!({"hub":self.address,"lock_busy":true});
        };
        json!({"hub":self.address,"seq":s.trace_sequence,"recent":s.trace_events})
    }
    pub fn trace_230_settled(&self, counts: Counts) {
        if std::env::var_os("ACP2API_TRACE_230").is_some() {
            let mut state = self.shared.0.lock().unwrap();
            let actual = format!("{:?}", Self::counts_for(&state));
            state.trace_230_event(json!({"op":"settle-return","observed_counts":format!("{counts:?}"),"current_counts":actual}));
        }
    }
    #[allow(
        clippy::disallowed_methods,
        reason = "Bounded temporary diagnostics use wall time"
    )]
    pub fn trace_230_watch(&self, scope: &'static str) -> Option<Trace230Watch> {
        std::env::var_os("ACP2API_TRACE_230")?;
        let done = Arc::new(AtomicBool::new(false));
        let watching = done.clone();
        let hub = self.clone();
        let started = Instant::now();
        trace_230_line(json!({"scope":scope,"phase":"begin","snapshot":self.trace_230_snapshot()}));
        thread::spawn(move || {
            let mut previous = None;
            for second in 1..=105 {
                thread::sleep(Duration::from_secs(1));
                if watching.load(Ordering::Relaxed) {
                    return;
                }
                let snapshot = hub.trace_230_snapshot();
                let sequence = snapshot["seq"].as_u64();
                let history = if second >= 5 && sequence.is_some() && sequence != previous {
                    hub.trace_230_history()
                } else {
                    Value::Null
                };
                if second >= 5 {
                    previous = sequence;
                }
                trace_230_line(
                    json!({"scope":scope,"phase":"watch","second":second,"snapshot":snapshot,"history":history}),
                );
            }
        });
        Some(Trace230Watch {
            done,
            hub: self.clone(),
            scope,
            started,
        })
    }
    pub fn trace_230_phase(&self, scope: &str, phase: &str) {
        if std::env::var_os("ACP2API_TRACE_230").is_some() {
            self.shared
                .0
                .lock()
                .unwrap()
                .trace_230_event(json!({"op":"fixture-phase","scope":scope,"phase":phase}));
            trace_230_line(json!({"scope":scope,"phase":phase,"hub":self.address}));
        }
    }
    pub fn trace_230_snapshot(&self) -> Value {
        let Ok(s) = self.shared.0.try_lock() else {
            return json!({"hub":self.address,"lock_busy":true,"settle_stage":self.trace_settle_stage.load(Ordering::Relaxed)});
        };
        let mut sleepers: Vec<_> = s
            .sleepers
            .iter()
            .map(|(id, v)| json!({"id":id,"at":v.at,"task":v.task,"writer":v.writer.is_some()}))
            .collect();
        sleepers.sort_by_key(|v| v["id"].as_u64());
        json!({"hub":self.address,"now":s.now,"settle_stage":self.trace_settle_stage.load(Ordering::Relaxed),"ops":s.trace_ops,"connections":s.trace_connections,"seq":s.trace_sequence,"counts":format!("{:?}",Self::counts_for(&s)),"raw_counts":format!("{:?}",s.counts),"active_holds":s.trace_holds.values().collect::<Vec<_>>(),"process_io":s.process_io.iter().map(|p|json!({"stdin_written":p.stdin_written,"stdin_got":p.stdin_got,"ordered":p.ordered,"read":p.read,"processed":p.processed,"stop":p.stop_requested,"ended":p.ended})).collect::<Vec<_>>(),"sleepers":sleepers,"external":s.external.keys().collect::<Vec<_>>(),"wake_pending":s.wake_pending,"completed_timers":s.completed_timers,"clients":s.client_sockets.iter().map(|(id,v)|json!({"id":id,"read":v.read,"consumed":v.consumed})).collect::<Vec<_>>()})
    }
    pub fn now(&self) -> u64 {
        self.shared.0.lock().unwrap().now
    }
    pub fn counts(&self) -> Counts {
        let s = self.shared.0.lock().unwrap();
        Self::counts_for(&s)
    }
    fn counts_for(s: &State) -> Counts {
        let mut counts = s.counts;
        for process in &s.process_io {
            counts.stdin_written += process.stdin_written;
            counts.stdin_got += process.stdin_got;
            counts.ordered += process.ordered;
            counts.read += process.read;
            counts.processed += process.processed;
            if process.stop_requested && process.ended {
                counts.derived_stdin_closed +=
                    process.stdin_written.saturating_sub(process.stdin_got);
                counts.derived_read_closed += process.ordered.saturating_sub(process.read);
                counts.derived_processed_closed +=
                    process.ordered.saturating_sub(process.processed);
            } else if process.stdin_written != process.stdin_got
                || process.ordered != process.read
                || process.ordered != process.processed
            {
                counts.derived_open_process_io += 1;
            }
        }
        for socket in s.client_sockets.values() {
            counts.client_read += socket.read;
            counts.client_consumed += socket.consumed;
            let mut byte = [0u8; 1];
            if socket.socket.peek(&mut byte).is_ok_and(|n| n > 0) {
                counts.client_available = true;
            }
        }
        counts
    }
    pub fn failure(&self) -> Option<String> {
        self.shared.0.lock().unwrap().failure.clone()
    }
    pub fn ready(&self) -> bool {
        self.shared.0.lock().unwrap().ready
    }
    pub fn terminal(&self, bytes: &[u8]) {
        let mut s = self.shared.0.lock().unwrap();
        s.terminal_bytes += bytes.len();
        if s.terminal_bytes > 4 * 1024 * 1024 {
            s.failure = Some("terminal capture cap: 4 MiB".into());
            return;
        }
        if String::from_utf8_lossy(bytes).contains("Listening http://127.0.0.1:")
            || String::from_utf8_lossy(bytes).contains("management panel at http://127.0.0.1:")
        {
            s.ready = true;
        }
        s.record(json!({"kind":"terminal","text":String::from_utf8_lossy(bytes)}));
        self.shared.1.notify_all();
    }
    pub fn wait_change(&self, deadline: Instant) {
        let s = self.shared.0.lock().unwrap();
        let _ = self
            .shared
            .1
            .wait_timeout(
                s,
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(10)),
            )
            .unwrap();
    }
    pub fn command(&self, label: &str, command: Value) {
        self.shared.0.lock().unwrap().out(label, command);
        self.shared.1.notify_all();
    }
    pub fn stop(&self) {
        let mut s = self.shared.0.lock().unwrap();
        s.stop = true;
        if let Some(writer) = s.servers.get("stop") {
            let _ = send(&mut writer.lock().unwrap(), &json!({"stop":true}));
        }
        self.shared.1.notify_all();
    }
    pub fn close(&self) {
        let first_close = self.alive.swap(false, Ordering::SeqCst);
        let s = self.shared.0.lock().unwrap();
        if first_close && std::env::var_os("ACP2API_TRACE_230").is_some() {
            let line = format!(
                "[TRACE-230-HUB] {}\n",
                json!({"hub":self.address,"connections":s.trace_connections,"ops":s.trace_ops,"sleeps":s.next_sleep})
            );
            std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
        }
        for writer in s.sleepers.values().filter_map(|s| s.writer.as_ref()) {
            let _ = writer.lock().unwrap().shutdown(Shutdown::Both);
        }
        for server in s.servers.values() {
            let _ = server.lock().unwrap().shutdown(Shutdown::Both);
        }
        for c in &s.connections {
            let _ = c.shutdown(Shutdown::Both);
        }
        self.shared.1.notify_all();
    }
}
pub fn send(stream: &mut TcpStream, value: &Value) -> std::io::Result<()> {
    serde_json::to_writer(&mut *stream, value)?;
    stream.write_all(b"\n")?;
    stream.flush()
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct HubLink {
    pub address: String,
    pub token: String,
    #[serde(skip)]
    connection: Arc<Mutex<Option<BufReader<TcpStream>>>>,
}
impl HubLink {
    pub fn new(hub: &Hub) -> Self {
        Self {
            address: hub.address.clone(),
            token: hub.token.clone(),
            connection: Arc::default(),
        }
    }
    fn open(&self) -> std::io::Result<TcpStream> {
        let stream = TcpStream::connect(&self.address)?;
        stream.set_read_timeout(Some(real_limit(Duration::from_secs(300))))?;
        stream.set_write_timeout(Some(real_limit(Duration::from_secs(10))))?;
        Ok(stream)
    }
    pub fn connect(&self, mut message: Value) -> std::io::Result<BufReader<TcpStream>> {
        let mut stream = self.open()?;
        message["token"] = json!(self.token);
        message["role"] = json!("rpc");
        send(&mut stream, &message)?;
        Ok(BufReader::new(stream))
    }
    pub fn call(&self, mut message: Value) -> Value {
        let trace = Trace230Rpc::begin(&self.address, &message);
        if std::env::var_os("ACP2API_TRACE_230").is_some() {
            message["trace_task"] = json!(trace_230_task());
        }
        message["token"] = json!(self.token);
        message["role"] = json!("rpc_stream");
        let mut connection = self.connection.lock().unwrap();
        if let Some(t) = &trace {
            t.stage.store(2, Ordering::Relaxed);
        }
        let result = (|| -> std::io::Result<String> {
            let mut reader = match connection.take() {
                Some(reader) => reader,
                None => BufReader::new(self.open()?),
            };
            let mut result = Self::exchange(&mut reader, &message, trace.as_ref());
            if result.is_err() {
                drop(reader);
                reader = BufReader::new(self.open()?);
                result = Self::exchange(&mut reader, &message, trace.as_ref());
            }
            if result.is_ok() {
                *connection = Some(reader);
            }
            result
        })();
        drop(connection);
        serde_json::from_str(&result.expect("hub RPC response")).expect("hub RPC JSON")
    }
    fn exchange(
        reader: &mut BufReader<TcpStream>,
        message: &Value,
        trace: Option<&Trace230Rpc>,
    ) -> std::io::Result<String> {
        if let Some(t) = trace {
            t.stage.store(3, Ordering::Relaxed);
        }
        send(reader.get_mut(), message)?;
        if let Some(t) = trace {
            t.stage.store(4, Ordering::Relaxed);
        }
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        Ok(line)
    }
}

fn redact(value: &mut Value) {
    match value {
        Value::String(text) => {
            if let Some(start) = text.find("/mcp/") {
                text.truncate(start + 5);
                text.push_str("<REDACTED>");
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact),
        Value::Object(values) => values.values_mut().for_each(redact),
        _ => {}
    }
}

impl std::fmt::Debug for HubLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubLink")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl Hub {
    pub async fn prepare_restart(&self) {
        let deadline = Instant::now() + real_limit(Duration::from_secs(10));
        while crate::fake::FakeRuntime::new(self.clone())
            .record()
            .processes_alive()
            > 0
        {
            assert!(Instant::now() < deadline, "restart process-drain cap: 10 s");
            let hub = self.clone();
            tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                .await
                .unwrap();
        }
        let mut state = self.shared.0.lock().unwrap();
        state.stop = false;
        state.ready = false;
        state.servers.remove("stop");
    }
}

#[cfg(test)]
pub(crate) struct RpcPause {
    pub link: HubLink,
    pub publication: tokio::sync::oneshot::Receiver<()>,
    pub release: std::sync::mpsc::SyncSender<()>,
    pub peer: thread::JoinHandle<()>,
}

#[cfg(test)]
pub(crate) fn pause_rpc_after(
    hub: &Hub,
    request_cap: usize,
    mut matches: impl FnMut(&Value, &Value) -> bool + Send + 'static,
) -> RpcPause {
    let link = HubLink::new(hub);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    *link.connection.lock().unwrap() = Some(BufReader::new(socket));
    let (published, publication) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let upstream = HubLink::new(hub);
    let peer = thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(real_limit(Duration::from_secs(3))))
            .unwrap();
        socket
            .set_write_timeout(Some(real_limit(Duration::from_secs(3))))
            .unwrap();
        let mut reader = BufReader::new(socket);
        let mut published = Some(published);
        let deadline = Instant::now() + real_limit(Duration::from_secs(3));
        for _ in 0..request_cap {
            assert!(Instant::now() < deadline, "RPC proxy run cap: 3 s");
            let mut line = String::new();
            let n = reader.by_ref().take(4097).read_line(&mut line).unwrap();
            assert!(n <= 4096, "RPC proxy frame cap: 4 KiB");
            if n == 0 {
                return;
            }
            let request: Value = serde_json::from_str(&line).unwrap();
            let reply = upstream.call(request.clone());
            if published.is_some() && matches(&request, &reply) {
                published.take().unwrap().send(()).unwrap();
                released
                    .recv_timeout(real_limit(Duration::from_secs(3)))
                    .unwrap();
            }
            send(reader.get_mut(), &reply).unwrap();
        }
        panic!("RPC proxy request cap: {request_cap}");
    });
    RpcPause {
        link,
        publication,
        release,
        peer,
    }
}

pub struct Trace230Watch {
    done: Arc<AtomicBool>,
    hub: Hub,
    scope: &'static str,
    started: Instant,
}
impl Drop for Trace230Watch {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        trace_230_line(
            json!({"scope":self.scope,"phase":"end","real_us":self.started.elapsed().as_micros(),"snapshot":self.hub.trace_230_snapshot(),"history":if self.started.elapsed() >= Duration::from_secs(5) { self.hub.trace_230_history() } else { Value::Null }}),
        );
    }
}
pub(crate) fn trace_230_line(value: Value) {
    let line = format!("[TRACE-230-STATE] {value}\n");
    static BYTES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    const CAP: usize = 4 * 1024 * 1024;
    let used = BYTES.fetch_add(line.len(), Ordering::Relaxed);
    if used + line.len() <= CAP {
        std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
    } else if used < CAP {
        eprintln!("[TRACE-230-CAP] state trace cap: 4 MiB");
    }
}

pub(crate) fn trace_230_task() -> String {
    let task = tokio::task::try_id()
        .map(|id| id.to_string())
        .unwrap_or_else(|| format!("thread:{:?}", thread::current().id()));
    format!("{}:{task}", std::process::id())
}
struct Trace230Rpc {
    id: u64,
    stage: Arc<std::sync::atomic::AtomicU8>,
}
struct Trace230RpcEntry {
    details: Value,
    started: Instant,
    stage: Arc<std::sync::atomic::AtomicU8>,
    samples: u8,
}
static TRACE_230_RPC: std::sync::OnceLock<Mutex<HashMap<u64, Trace230RpcEntry>>> =
    std::sync::OnceLock::new();
impl Trace230Rpc {
    fn begin(address: &str, message: &Value) -> Option<Self> {
        std::env::var_os("ACP2API_TRACE_230")?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let active = TRACE_230_RPC.get_or_init(|| {
            thread::spawn(|| {
                for _ in 0..900 {
                    thread::sleep(Duration::from_secs(1));
                    let Some(active) = TRACE_230_RPC.get() else { continue };
                    let Ok(mut calls) = active.try_lock() else { continue };
                    let mut rows = Vec::new();
                    for (id, call) in calls.iter_mut() {
                        if call.started.elapsed() >= Duration::from_secs(1) && call.samples < 105 {
                            call.samples += 1;
                            rows.push(json!({"scope":"rpc","phase":"watch","call_id":id,"details":call.details,"age_ms":call.started.elapsed().as_millis(),"stage":call.stage.load(Ordering::Relaxed)}));
                        }
                    }
                    drop(calls);
                    for row in rows { trace_230_line(row); }
                }
            });
            Mutex::new(HashMap::new())
        });
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let stage = Arc::new(std::sync::atomic::AtomicU8::new(1));
        let mut calls = active.lock().unwrap();
        if calls.len() == 256 {
            return None;
        }
        // Deliberately whitelist fields: never serialize the authenticated message.
        let details = json!({"hub":address,"reader":trace_230_task(),"op":message["op"],"kind":message["kind"],"add":message["add"],"timer":message["id"],"at":message["at"]});
        calls.insert(
            id,
            Trace230RpcEntry {
                details,
                started: Instant::now(),
                stage: stage.clone(),
                samples: 0,
            },
        );
        Some(Self { id, stage })
    }
}
impl Drop for Trace230Rpc {
    fn drop(&mut self) {
        if let Some(call) = TRACE_230_RPC
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .remove(&self.id)
            && call.started.elapsed() >= Duration::from_secs(1)
        {
            trace_230_line(
                json!({"scope":"rpc","phase":"end","call_id":self.id,"details":call.details,"age_ms":call.started.elapsed().as_millis(),"stage":self.stage.load(Ordering::Relaxed)}),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ended_shared_handler_closes_its_socket() {
        let hub = Hub::new("local-handler-test".into());
        let stream = TcpStream::connect(&hub.address).unwrap();
        stream
            .set_read_timeout(Some(real_limit(Duration::from_secs(2))))
            .unwrap();
        let mut reader = BufReader::new(stream);
        send(
            reader.get_mut(),
            &json!({"token":hub.token,"role":"rpc_stream","op":"now"}),
        )
        .unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap(),
            json!({"now":0})
        );
        reader.get_mut().write_all(b"invalid-json\n").unwrap();
        line.clear();
        let ended = reader.read_line(&mut line);
        hub.close();
        assert_eq!(
            ended.unwrap(),
            0,
            "the peer sees EOF after the handler ends"
        );
    }

    #[test]
    fn shared_rpc_recovers_from_eof_and_write_failure() {
        let mut replies = Vec::new();
        for fault in [Shutdown::Read, Shutdown::Write] {
            let hub = Hub::new("local-reconnect-test".into());
            let link = HubLink::new(&hub);
            assert_eq!(link.call(json!({"op":"now"})), json!({"now":0}));
            link.connection
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .get_ref()
                .shutdown(fault)
                .unwrap();
            for caller in [link.clone(), link] {
                replies.push(std::panic::catch_unwind(|| caller.call(json!({"op":"now"}))).ok());
            }
            hub.close();
        }
        assert_eq!(replies, vec![Some(json!({"now":0})); 4]);
    }

    #[test]
    fn shared_rpc_stops_after_one_retry_and_can_be_used_after_the_panic() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let link: HubLink = serde_json::from_value(json!({
            "address":listener.local_addr().unwrap().to_string(),
            "token":"local-retry-limit-test"
        }))
        .unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let peer_running = running.clone();
        let peer = thread::spawn(move || {
            let deadline = Instant::now() + real_limit(Duration::from_secs(5));
            let mut requests = Vec::new();
            for attempt in 0..3 {
                let stream = loop {
                    if !peer_running.load(Ordering::SeqCst) {
                        return requests;
                    }
                    assert!(Instant::now() < deadline, "retry peer cap: 5 s");
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("retry peer accept: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(real_limit(Duration::from_secs(2))))
                    .unwrap();
                stream
                    .set_write_timeout(Some(real_limit(Duration::from_secs(2))))
                    .unwrap();
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                let size = reader.by_ref().take(4097).read_line(&mut line).unwrap();
                assert!((1..=4096).contains(&size), "retry peer frame cap: 4 KiB");
                let request: Value = serde_json::from_str(&line).unwrap();
                requests.push(request["value"].clone());
                if attempt == 2 {
                    send(reader.get_mut(), &json!({"answer":request["value"]})).unwrap();
                }
            }
            requests
        });
        let failed = std::panic::catch_unwind(|| link.call(json!({"op":"echo","value":0})));
        let later = std::panic::catch_unwind(|| link.call(json!({"op":"echo","value":1})));
        running.store(false, Ordering::SeqCst);
        let requests = peer.join().unwrap();
        assert!(failed.is_err(), "two failed exchanges end the call");
        assert_eq!(later.ok(), Some(json!({"answer":1})));
        assert_eq!(requests, vec![json!(0), json!(0), json!(1)]);
        assert!(
            std::panic::catch_unwind(|| link.call(json!({"op":"echo","value":2}))).is_err(),
            "a stopped peer ends the call"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wake_before_wait_does_not_take_a_shared_rpc_reply() {
        let hub = Hub::new("local-wake-test".into());
        let link = HubLink::new(&hub);
        assert_eq!(link.call(json!({"op":"sleep","at":0}))["id"], 1);
        let mut time = crate::timeline::TimeLine::new(hub.clone());
        let advancing = tokio::spawn(async move { time.advance_to(1).await });
        let deadline = Instant::now() + real_limit(Duration::from_secs(2));
        while hub.counts().wake_pending == 0 {
            assert!(Instant::now() < deadline, "early wake cap: 2 s");
            hub.wait_change(deadline);
        }
        let before_wait = link.call(json!({"op":"now"}));
        let mut waiting = link.connect(json!({"op":"wait","id":1})).unwrap();
        waiting
            .get_ref()
            .set_read_timeout(Some(real_limit(Duration::from_secs(2))))
            .unwrap();
        let mut wake = String::new();
        waiting.read_line(&mut wake).unwrap();
        link.call(json!({"op":"sleep_done","id":1}));
        advancing.await.unwrap();
        let after_wait = link.call(json!({"op":"now"}));
        hub.close();
        assert_eq!(before_wait, json!({"now":0}));
        assert_eq!(
            serde_json::from_str::<Value>(&wake).unwrap(),
            json!({"wake":1,"now":0})
        );
        assert_eq!(after_wait, json!({"now":1}));
    }

    #[test]
    fn repeated_rpc_calls_share_a_client_port_and_keep_their_responses() {
        const REQUESTS: usize = 32;
        const FRAME_CAP: u64 = 4096;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let link: HubLink = serde_json::from_value(json!({
            "address": listener.local_addr().unwrap().to_string(),
            "token": "local-rpc-test"
        }))
        .unwrap();
        let peer = thread::spawn(move || {
            let deadline = Instant::now() + real_limit(Duration::from_secs(5));
            let mut ports = HashSet::new();
            let mut received = 0;
            while received < REQUESTS {
                assert!(Instant::now() < deadline, "RPC peer run cap: 5 s");
                let (stream, address) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => panic!("RPC peer accept: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                ports.insert(address.port());
                stream
                    .set_read_timeout(Some(real_limit(Duration::from_secs(2))))
                    .unwrap();
                stream
                    .set_write_timeout(Some(real_limit(Duration::from_secs(2))))
                    .unwrap();
                let mut reader = BufReader::new(stream);
                while received < REQUESTS {
                    assert!(Instant::now() < deadline, "RPC peer run cap: 5 s");
                    let mut line = String::new();
                    let size = reader
                        .by_ref()
                        .take(FRAME_CAP + 1)
                        .read_line(&mut line)
                        .unwrap();
                    assert!(size <= FRAME_CAP as usize, "RPC peer frame cap: 4 KiB");
                    if size == 0 {
                        break;
                    }
                    let request: Value = serde_json::from_str(&line).unwrap();
                    send(reader.get_mut(), &json!({"answer": request["value"]})).unwrap();
                    received += 1;
                }
            }
            ports.len()
        });
        thread::scope(|scope| {
            for worker in 0..2 {
                let link = link.clone();
                scope.spawn(move || {
                    for request in 0..REQUESTS / 2 {
                        let value = worker * 100 + request;
                        assert_eq!(
                            link.call(json!({"op":"echo", "value":value})),
                            json!({"answer":value})
                        );
                    }
                });
            }
        });
        assert_eq!(
            peer.join().unwrap(),
            1,
            "short RPC polling uses one client port"
        );
    }
}
