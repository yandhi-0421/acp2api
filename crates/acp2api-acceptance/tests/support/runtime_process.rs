#![allow(clippy::disallowed_methods)]
use crate::World;
use acp2api_acceptance::{
    Setup, TIME_FACTOR_ENV, TurnPlan, TurnStep,
    client::{Answer, ApiCall, Call},
    real_limit,
    setup::binary,
    time_factor,
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct ProcessState {
    pub calls: Vec<(String, Option<Call>)>,
    pub original: Vec<u32>,
    pub cancelled_at: u64,
    pub exits: Vec<usize>,
    pub pending_start: Option<Child>,
    pub identity_starts: usize,
}
impl std::fmt::Debug for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessState")
            .field("original", &self.original)
            .finish_non_exhaustive()
    }
}
impl Drop for ProcessState {
    fn drop(&mut self) {
        if let Some(mut host) = self.pending_start.take() {
            let _ = host.kill();
            let _ = host.wait();
        }
    }
}

pub fn servers(w: &World) -> Vec<Value> {
    w.scenario
        .hub
        .entries()
        .into_iter()
        .filter(|e| e.event["kind"] == "hello" && e.event["hello"]["role"] == "server")
        .map(|e| e.event["hello"].clone())
        .collect()
}
pub fn calls(w: &World, method: &str) -> Vec<Value> {
    acp2api_acceptance::system_check::requests(&w.scenario)
        .into_iter()
        .filter(|v| v["method"] == method)
        .collect()
}
pub fn process_ids(w: &World) -> Vec<u32> {
    w.scenario
        .hub
        .entries()
        .iter()
        .filter(|e| e.event["kind"] == "hello")
        .map(|e| e.event["hello"]["pid"].as_u64().unwrap() as u32)
        .collect()
}
#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    if unsafe { libc::kill(pid as i32, 0) } == 0 {
        return true;
    }
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH),
        "inspect the owned fake PID"
    );
    false
}
#[cfg(windows)]
pub fn alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(ERROR_INVALID_PARAMETER as i32),
                "inspect the owned fake PID"
            );
            return false;
        }
        let result = WaitForSingleObject(handle, 0);
        CloseHandle(handle);
        assert!(
            matches!(result, WAIT_OBJECT_0 | WAIT_TIMEOUT),
            "inspect fake process exit"
        );
        result == WAIT_TIMEOUT
    }
}

pub async fn observe(w: &mut World, condition: impl Fn(&World) -> bool) {
    let deadline = Instant::now() + real_limit(Duration::from_secs(2));
    loop {
        w.scenario.time.settle().await;
        if condition(w) || Instant::now() >= deadline {
            return;
        }
        let hub = w.scenario.hub.clone();
        tokio::task::spawn_blocking(move || hub.wait_change(deadline))
            .await
            .unwrap();
    }
}

pub async fn start(w: &mut World) {
    crate::places_support::start(w).await;
    if w.process.original.is_empty() {
        w.process.original = servers(w)
            .iter()
            .map(|v| v["pid"].as_u64().unwrap() as u32)
            .collect();
    }
}
pub async fn held(w: &mut World, name: &str, stream: bool) {
    start(w).await;
    let mut plan = TurnPlan::new();
    plan.steps
        .push(TurnStep::HoldUntil(format!("runtime-{name}")));
    plan.steps.push(TurnStep::Chunk(format!("reply {name}")));
    // A cancelled turn remains in the fake until this scenario explicitly ends it.
    if name == "A" {
        plan.end = acp2api_acceptance::fake::TurnEndPlan::IgnoreCancel;
    }
    w.request.plan = Some(plan);
    w.request.queue_next(&w.scenario);
    assert!(
        w.process.calls.len() < 8,
        "runtime scenario API request cap: 8"
    );
    let mut call =
        ApiCall::one_user_message("POST /v1/messages", &format!("runtime conversation {name}"));
    call.body["stream"] = json!(stream);
    let live = w.scenario.client().await.send(call);
    w.process.calls.push((name.into(), Some(live)));
    w.scenario.time.settle().await;
}
pub fn take(w: &mut World, name: &str) -> Call {
    w.process
        .calls
        .iter_mut()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
        .take()
        .unwrap()
}
pub fn status(w: &World, name: &str) -> Option<u16> {
    w.process
        .calls
        .iter()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .status()
}
pub async fn answer(w: &mut World, name: &str) -> Answer {
    take(w, name).finish(&mut w.scenario.time).await
}
pub async fn exit(w: &mut World) {
    let before = servers(w).len();
    w.scenario
        .hub
        .command("normal", json!({"op":"Exit","code":17}));
    observe(w, |w| {
        w.scenario
            .hub
            .entries()
            .iter()
            .filter(|e| e.event["kind"] == "process_end" && e.event["role"] == "server")
            .count()
            >= before
    })
    .await;
    assert!(
        w.scenario
            .hub
            .entries()
            .iter()
            .any(|e| e.event["kind"] == "process_end" && e.event["role"] == "server"),
        "fake boundary exit was delivered"
    );
}
pub fn error(answer: &Answer, status: u16, message: &str) {
    assert_eq!(answer.status, status, "runtime failure HTTP row");
    assert_eq!(
        answer.json()["error"]["message"],
        message,
        "runtime failure message"
    );
    if status == 503 {
        assert_eq!(answer.json()["error"]["type"], "api_error");
        assert_eq!(
            answer.headers.get("retry-after").map(String::as_str),
            Some("30")
        );
    }
}

pub async fn pending_start(w: &mut World) {
    use acp2api_acceptance::rpc_script::RpcReply;
    w.scenario
        .fake()
        .script_call("initialize", RpcReply::Silent);
    let run = w.scenario.root.seed(&w.scenario.hub, &Setup::normal());
    let path = w.scenario.root.path.join("pending-run.toml");
    std::fs::write(&path, toml::to_string(&run).unwrap()).unwrap();
    let mut command = Command::new(binary("acp2api-test-host"));
    command
        .env_clear()
        .env(TIME_FACTOR_ENV, time_factor().to_string());
    #[cfg(windows)]
    command.env("SystemRoot", std::env::var_os("SystemRoot").unwrap());
    let mut host = command
        .env("ACP2API_TEST_RUN", path)
        .args([
            "--data-dir",
            w.scenario.root.path.join("data").to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    for pipe in [
        Box::new(host.stdout.take().unwrap()) as Box<dyn Read + Send>,
        Box::new(host.stderr.take().unwrap()),
    ] {
        let hub = w.scenario.hub.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(pipe);
            loop {
                let mut line = Vec::new();
                match reader.by_ref().take(65537).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        assert!(line.len() <= 65536, "pending start line cap: 64 KiB");
                        hub.terminal(&line);
                    }
                }
            }
        });
    }
    w.process.pending_start = Some(host);
    observe(w, |w| calls(w, "initialize").len() == 1).await;
    assert_eq!(
        calls(w, "initialize").len(),
        1,
        "verified fake reaches initialize before virtual time moves"
    );
}

pub async fn cleanup(w: &mut World) {
    if w.process.calls.is_empty() {
        return;
    }
    w.process.calls.clear();
    w.scenario.time.settle().await;
}
