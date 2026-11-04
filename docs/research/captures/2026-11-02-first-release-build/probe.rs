use acp2api_acceptance::{Setup, TurnPlan, TurnStep, fake::FakeRuntime, hub::Hub, setup::TestRoot};
use serde_json::json;
use std::{fs, io::{BufRead, BufReader, Read, Write}, net::TcpStream, path::Path,
          process::{Command, Stdio}, sync::{Arc, atomic::{AtomicBool, Ordering}}, thread,
          time::{Duration, Instant}};

fn wait(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(15);
    while !condition() {
        assert!(Instant::now() < end, "probe condition exceeded 15 s");
        thread::sleep(Duration::from_millis(20));
    }
}
fn phase(name: &str, pid: u32) {
    println!("{}", json!({"phase":name,"pid":pid}));
    std::io::stdout().flush().unwrap();
}
fn request(port: u16, key: String, endpoint: &str, marker: &str, end: &str) -> thread::JoinHandle<()> {
    let endpoint = endpoint.to_owned();
    let marker = marker.to_owned();
    let end = end.to_owned();
    thread::spawn(move || {
        let body = if endpoint == "/v1/responses" {
            json!({"model":"gemini-3.8-flash","input":marker,"stream":true})
        } else {
            json!({"model":"gemini-3.8-flash","max_tokens":128,"messages":[{"role":"user","content":marker}],"stream":true})
        }.to_string();
        let mut stream = TcpStream::connect(("127.0.0.1",port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(stream, "POST {endpoint} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {key}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut bytes = Vec::new();
        stream.take(1024 * 1024).read_to_end(&mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("HTTP/1.1 200"), "request did not succeed");
        assert!(text.contains("probe-reply") && text.contains(&end), "reply or stream end absent");
        println!("{}",json!({"request":endpoint,"status":200,"reply_verified":true,"bytes":text.len()}));
    })
}
fn main() {
    thread::spawn(|| { thread::sleep(Duration::from_secs(75)); std::process::exit(71); });
    let program = std::env::args().nth(1).unwrap();
    let hub = Hub::new(acp2api_acceptance::setup::random_hex(24));
    let fake = FakeRuntime::new(hub.clone());
    let root = TestRoot::new();
    let run = root.seed(&hub, &Setup::normal());
    let data = root.path.join("data");
    let binary = data.join("official-runtime/agy_acp_server.par");
    let descriptor = data.join("official-runtime/localharness_external");
    let mut text = fs::read_to_string(&descriptor).unwrap();
    text.push_str("line_cap = \"500\"\nrun_ms = \"60000\"\n");
    fs::write(&descriptor,text).unwrap();
    let mut checked = serde_json::to_value(&run.release.builds[0]).unwrap();
    checked["localharness_external_sha256"] = acp2api_acceptance::setup::file_hash(&descriptor).into();
    checked["checked_at"] = "2026-11-02T00:00:00Z".into();
    fs::write(data.join("state.json"),json!({"format":1,"lockout":null,"checked_builds":[checked]}).to_string()).unwrap();
    let mut config = fs::OpenOptions::new().append(true).open(data.join("config.toml")).unwrap();
    writeln!(config,"agy_acp_server_path = {:?}",binary.to_str().unwrap()).unwrap();
    let mut child = Command::new(&program).args(["--data-dir",data.to_str().unwrap()])
        .env("ACP56_ACCOUNT_ROOT",root.path.join("account"))
        .env("HOME",root.path.join("home")).env("TMPDIR",root.path.join("temp"))
        .env("PATH",root.path.join("path")).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let pid=child.id();
    println!("{}",json!({"phase":"startup","pid":pid,"probe_root":root.path}));
    std::io::stdout().flush().unwrap();
    let ready=Arc::new(AtomicBool::new(false));
    let signal=ready.clone();
    let stdout=child.stdout.take().unwrap();
    thread::spawn(move || {
        for line in BufReader::new(stdout).take(64*1024).lines() {
            let line=line.unwrap();
            if line.contains("management panel at http://127.0.0.1:") { signal.store(true,Ordering::SeqCst); }
        }
    });
    // The child stays alive independently of its captured stdout handle.
    wait(|| ready.load(Ordering::SeqCst));
    phase("ready",pid);
    thread::sleep(Duration::from_secs(1));
    phase("idle",pid);
    thread::sleep(Duration::from_secs(5));
    let before=fake.record().prompts().len();
    for gate in ["one","two"] {
        let mut plan=TurnPlan::new().chunk("probe-reply");
        plan.steps.push(TurnStep::HoldUntil(gate.into()));
        fake.next_prompt(plan);
    }
    phase("streams",pid);
    let one=request(root.port,root.key.clone(),"/v1/messages","probe-one","message_stop");
    let two=request(root.port,root.key.clone(),"/v1/responses","probe-two","response.completed");
    wait(|| fake.record().prompts().len() >= before+2);
    thread::sleep(Duration::from_secs(3));
    fake.open_gate("one"); fake.open_gate("two");
    one.join().unwrap(); two.join().unwrap();
    phase("after_streams",pid);
    thread::sleep(Duration::from_secs(2));
    let servers=|| hub.entries().iter().filter(|e| e.event["kind"] == "hello" && e.event["hello"]["role"] == "server").count();
    let old=servers();
    phase("replacement",pid);
    hub.command("normal",json!({"op":"Exit","code":0}));
    wait(|| servers()>old);
    thread::sleep(Duration::from_secs(2));
    fake.next_prompt(TurnPlan::new().chunk("probe-reply"));
    request(root.port,root.key.clone(),"/v1/chat/completions","probe-three","[DONE]").join().unwrap();
    phase("recovered",pid);
    thread::sleep(Duration::from_secs(3));
    unsafe extern "C" { fn kill(pid:i32,sig:i32)->i32; }
    unsafe { kill(pid as i32,2); }
    wait(|| child.try_wait().unwrap().is_some());
    println!("{}",json!({"runtime_processes":servers(),"requests":3,"fake_failure":hub.failure()}));
    assert!(hub.failure().is_none());
    hub.close();
    if Path::new(&root.path).exists() { root.cleanup(); }
}
