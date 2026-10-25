use crate::fake::FakeState;
use crate::real_limit;
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
            && self.stdin_written == self.stdin_got
            && self.ordered == self.read
            && self.ordered == self.processed
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
    pub writer: Arc<Mutex<TcpStream>>,
}
pub(crate) struct ClientSocket {
    pub socket: TcpStream,
    pub read: u64,
    pub consumed: u64,
}
pub(crate) struct State {
    pub client_sockets: HashMap<u64, ClientSocket>,
    pub next_socket: u64,
    pub now: u64,
    pub counts: Counts,
    pub entries: Vec<Entry>,
    pub bytes: usize,
    pub failure: Option<String>,
    pub sleepers: HashMap<u64, Sleeper>,
    pub wake_pending: Vec<u64>,
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
}
impl State {
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
    pub(crate) fn out(&mut self, label: &str, command: Value) {
        if command["op"] == "Out" || command["op"] == "Err" {
            self.counts.ordered += command["line"]
                .as_str()
                .unwrap_or("")
                .bytes()
                .filter(|b| *b == b'\n')
                .count() as u64
                + 1;
        }
        self.record(json!({"kind":"command", "label":label, "command":command}));
        if self.failure.is_some() {
            return;
        }
        if let Some(writer) = self.servers.get(label) {
            if let Err(e) = send(&mut writer.lock().unwrap(), &command) {
                self.failure = Some(format!("fake command channel: {e}"));
            }
        } else {
            self.failure = Some(format!("missing fake channel {label}"));
        }
    }
}
#[derive(Clone)]
pub struct Hub {
    pub address: String,
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
                entries: Vec::new(),
                bytes: 0,
                failure: None,
                sleepers: HashMap::new(),
                wake_pending: Vec::new(),
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
            }),
            Condvar::new(),
        ));
        let hub = Self {
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
        let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));

        let mut reader = BufReader::new(stream);
        let mut role = String::new();
        let mut label = String::new();
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
                role = message["role"].as_str().unwrap_or("rpc").to_owned();
                if role != "rpc" {
                    self.shared
                        .0
                        .lock()
                        .unwrap()
                        .connections
                        .push(reader.get_ref().try_clone().unwrap());
                }
                label = message["label"].as_str().unwrap_or("normal").to_owned();
            }
            let mut state = self.shared.0.lock().unwrap();
            let result = match message["op"].as_str().unwrap_or("") {
                "Hello" => {
                    #[cfg(windows)]
                    if !process_recorded && (role == "server" || role == "harness") {
                        let pid = message["pid"]
                            .as_u64()
                            .and_then(|pid| u32::try_from(pid).ok())
                            .expect("fake Hello PID");
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
                    }
                    let mut redacted = message.clone();
                    redacted.as_object_mut().unwrap().remove("token");
                    state.record(json!({"kind":"hello","hello":redacted}));
                    json!({"ok":true})
                }
                "Got" => {
                    state.counts.stdin_got += 1;
                    if let Some(line) = message["line"].as_str() {
                        match serde_json::from_str::<Value>(line) {
                            Ok(value) => {
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
                                let mut state = self.shared.0.lock().unwrap();
                                crate::fake::respond(&mut state, &label, value);
                            }
                            Err(_) => state.failure = Some("invalid ACP from product".into()),
                        }
                    }
                    self.shared.1.notify_all();
                    continue;
                }
                "hold" => {
                    let kind = message["kind"].as_str().unwrap_or("");
                    let add = message["add"].as_bool().unwrap_or(false);
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
                            state.counts.processed += 1;
                        }
                        if kind == "McpRequest" {
                            state.counts.mcp_finished += 1;
                        }
                    }
                    json!({"now":state.now})
                }
                "io" => {
                    let n = message["lines"].as_u64().unwrap_or(0);
                    if message["direction"] == "stdin" {
                        state.counts.stdin_written += n;
                    } else {
                        state.counts.read += n;
                    }
                    json!({"ok":true})
                }
                "now" | "drain" => json!({"now":state.now}),
                "sleep" => {
                    state.next_sleep += 1;
                    let id = state.next_sleep;
                    let at = message["at"].as_u64().unwrap().max(state.now);
                    state.sleepers.insert(
                        id,
                        Sleeper {
                            at,
                            writer: writer.clone(),
                        },
                    );
                    json!({"id":id,"now":state.now})
                }
                "wait" => {
                    let id = message["id"].as_u64().unwrap();
                    if let Some(s) = state.sleepers.get_mut(&id) {
                        s.writer = writer.clone();
                    }
                    if state.wake_pending.contains(&id) {
                        let _ = send(
                            &mut writer.lock().unwrap(),
                            &json!({"wake":id,"now":state.now}),
                        );
                    }
                    continue;
                }
                "sleep_delivered" => {
                    let id = message["id"].as_u64().unwrap();
                    state.sleepers.remove(&id);
                    json!({"ok":true})
                }
                "sleep_done" => {
                    let id = message["id"].as_u64().unwrap();
                    state.sleepers.remove(&id);
                    state.wake_pending.retain(|x| *x != id);
                    state.counts.wake_pending = state.wake_pending.len() as u64;
                    json!({"ok":true})
                }
                "stop_wait" => {
                    if !state.stop {
                        state.servers.insert("stop".into(), writer.clone());
                        continue;
                    }
                    json!({"stop":true})
                }
                "launch" => {
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
            if role == "server" {
                state.servers.remove(&label);
            }
            self.shared.1.notify_all();
        }
    }
    pub fn entries(&self) -> Vec<Entry> {
        self.shared.0.lock().unwrap().entries.clone()
    }
    pub fn now(&self) -> u64 {
        self.shared.0.lock().unwrap().now
    }
    pub fn counts(&self) -> Counts {
        let s = self.shared.0.lock().unwrap();
        let mut counts = s.counts;
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
        self.alive.store(false, Ordering::SeqCst);
        let s = self.shared.0.lock().unwrap();
        for sleeper in s.sleepers.values() {
            let _ = sleeper.writer.lock().unwrap().shutdown(Shutdown::Both);
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
        let mut connection = self.connection.lock().unwrap();
        let reader =
            connection.get_or_insert_with(|| BufReader::new(self.open().expect("hub RPC connect")));
        message["token"] = json!(self.token);
        message["role"] = json!("rpc_stream");
        send(reader.get_mut(), &message).expect("hub RPC request");
        let mut line = String::new();
        reader.read_line(&mut line).expect("hub RPC response");
        serde_json::from_str(&line).expect("hub RPC JSON")
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
mod tests {
    use super::*;
    use std::collections::HashSet;

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
