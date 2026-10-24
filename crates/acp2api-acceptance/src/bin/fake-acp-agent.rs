#![allow(clippy::disallowed_methods)]
use acp2api_acceptance::real_limit;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    process::{Command, ExitCode},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
const LINES: usize = 5000;
const BYTES: usize = 1024 * 1024;
fn main() -> ExitCode {
    let executable = std::env::current_exe().expect("executable");
    let descriptor = executable.parent().unwrap().join(if cfg!(windows) {
        "localharness_external.exe"
    } else {
        "localharness_external"
    });
    let text = std::fs::read_to_string(descriptor).expect("fake descriptor");
    let fields: HashMap<String, String> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().into(), v.trim().trim_matches('"').into()))
        .collect();
    assert_eq!(
        fields.get("marker").map(String::as_str),
        Some("acp2api-local-fake-v1")
    );
    let cap = fields
        .get("line_cap")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(LINES)
        .min(LINES);
    let run_ms = fields
        .get("run_ms")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(300_000)
        // The run cap is this base times ACP2API_TEST_TIME_FACTOR.
        .min(300_000);
    let run_limit = real_limit(Duration::from_millis(run_ms));
    thread::spawn(move || {
        thread::sleep(run_limit);
        std::process::exit(71);
    });
    let stream = TcpStream::connect(&fields["hub"]).expect("hub");
    let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));
    let harness = std::env::args().any(|a| a == "--harness");
    let hello = json!({"op":"Hello","token":fields["token"],"label":fields["label"],"role":if harness {"harness"}else{"server"},"pid":std::process::id(),"parent_pid":parent_pid(),"argv":std::env::args().collect::<Vec<_>>(),"cwd":std::env::current_dir().unwrap(),"environment_names":std::env::vars_os().map(|(k,_)|k.to_string_lossy().into_owned()).collect::<Vec<_>>(),"GEMINI_HOME":std::env::var("GEMINI_HOME").ok(),"TEMP":std::env::var("TEMP").ok(),"TMP":std::env::var("TMP").ok()});
    send(&writer, &hello);
    let children = Arc::new(Mutex::new(Vec::<std::process::Child>::new()));
    if !harness {
        let writer = writer.clone();
        let children = children.clone();
        thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut reader = stdin.lock();
            let mut count = 0;
            loop {
                let mut line = String::new();
                let n = reader
                    .by_ref()
                    .take((BYTES + 1) as u64)
                    .read_line(&mut line)
                    .unwrap_or(0);
                if n == 0 {
                    break;
                }
                if n > BYTES {
                    std::process::exit(70);
                }
                count += 1;
                if count > cap {
                    std::process::exit(70);
                }
                send(
                    &writer,
                    &json!({"op":"Got","line":line.trim_end_matches('\n')}),
                );
            }
            if cfg!(not(windows)) {
                for child in children.lock().unwrap().iter_mut() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                std::process::exit(0);
            }
        });
    }
    let mut reader = BufReader::new(stream);
    let directions = AtomicUsize::new(0);
    loop {
        let mut line = String::new();
        match reader
            .by_ref()
            .take((BYTES + 1) as u64)
            .read_line(&mut line)
        {
            Ok(0) | Err(_) => break,
            Ok(n) if n > BYTES => return ExitCode::from(70),
            Ok(_) => {}
        }
        let command: Value = serde_json::from_str(&line).expect("hub JSON");
        if command.get("op").is_none() {
            continue;
        }
        if directions.fetch_add(1, Ordering::SeqCst) >= cap {
            return ExitCode::from(70);
        }
        match command["op"].as_str().unwrap_or("") {
            "Out" => {
                let mut out = std::io::stdout().lock();
                writeln!(out, "{}", command["line"].as_str().unwrap()).unwrap();
                out.flush().unwrap();
            }
            "Err" => {
                let mut out = std::io::stderr().lock();
                writeln!(out, "{}", command["line"].as_str().unwrap()).unwrap();
                out.flush().unwrap();
            }
            "StartHarness" => {
                let count = command["count"].as_u64().unwrap_or(0).min(32);
                for _ in 0..count {
                    children.lock().unwrap().push(
                        Command::new(&executable)
                            .arg("--harness")
                            .env("ACP2API_PARENT_PID", std::process::id().to_string())
                            .stdin(std::process::Stdio::null())
                            .spawn()
                            .expect("harness"),
                    );
                }
            }
            "Exit" => return ExitCode::from(command["code"].as_u64().unwrap_or(0) as u8),
            _ => return ExitCode::from(70),
        }
    }
    for child in children.lock().unwrap().iter_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    ExitCode::SUCCESS
}
fn send(writer: &Arc<Mutex<TcpStream>>, value: &Value) {
    let mut writer = writer.lock().unwrap();
    if serde_json::to_writer(&mut *writer, value)
        .and_then(|_| writer.write_all(b"\n").map_err(serde_json::Error::io))
        .is_err()
    {
        std::process::exit(0);
    }
    let _ = writer.flush();
}
#[cfg(unix)]
fn parent_pid() -> u32 {
    unsafe extern "C" {
        fn getppid() -> i32;
    }
    unsafe { getppid() as u32 }
}
#[cfg(windows)]
fn parent_pid() -> u32 {
    std::env::var("ACP2API_PARENT_PID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
