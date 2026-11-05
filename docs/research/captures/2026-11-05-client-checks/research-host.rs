use acp2api_acceptance::{
    Setup, TurnEndPlan, TurnPlan, TurnStep,
    fake::{FakeRuntime, ModelSpec},
    hub::Hub,
    rpc_script::RpcReply,
    setup::{TestRoot, file_hash, random_hex},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

fn emit(value: Value) {
    println!("SC239 {}", value);
    std::io::stdout().flush().unwrap();
}

fn error() -> TurnPlan {
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::SessionStderr(
        "Antigravity SDK error processing prompt for session {session}".into(),
    ));
    plan.steps.push(TurnStep::ErrorLine {
        code: 503,
        status: "UNAVAILABLE".into(),
        message: "SC239 synthetic temporary upstream error".into(),
    });
    plan.end = TurnEndPlan::RpcError {
        code: -32603,
        message: "SC239 synthetic temporary upstream error".into(),
    };
    plan
}

fn gate(_: &[acp2api_acceptance::hub::Entry], call: &Value) -> TurnPlan {
    let mut plan = TurnPlan::new();
    plan.steps.push(TurnStep::HoldUntil(format!(
        "request-{}-{}",
        call["params"]["sessionId"].as_str().unwrap(),
        call["id"]
    )));
    plan
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().collect();
    let path = PathBuf::from(&args[1]);
    let port = args[2].parse().unwrap();
    let scenario = args[3].clone();
    let seconds: u64 = args[4].parse().unwrap();
    let pause: u64 = args[5].parse().unwrap();
    let partial_ack = PathBuf::from(&args[6]).join("partial-seen.json");
    assert!((1..=420).contains(&seconds) && pause <= 330);
    // The Python supervisor also stops the whole process group at this deadline.
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(seconds));
        std::process::exit(71);
    });
    let start = Instant::now();
    let hub = Hub::new(random_hex(24));
    let fake = FakeRuntime::new(hub.clone());
    fake.catalog(&[ModelSpec {
        value: "gemini-3.8-flash".into(),
        name: "Gemini 3.8 Flash".into(),
        levels: vec!["low".into(), "medium".into(), "high".into()],
    }]);
    let root = TestRoot {
        path: path.join("server"),
        port,
        key: "made-up-sc239-key".into(),
        password: "made-up-sc239-password".into(),
    };
    for child in [
        "account",
        "home",
        "local-app-data",
        "temp",
        "path",
        "exe",
        "data",
    ] {
        fs::create_dir_all(root.path.join(child)).unwrap();
    }
    let mut run = root.seed(&hub, &Setup::normal());
    let descriptor = root
        .path
        .join("data/official-runtime/localharness_external");
    // The acceptance fake multiplies only its watchdog, never the product clock.
    // ACP2API_TEST_TIME_FACTOR=2 permits the real 330-second pause.
    let mut descriptor_text = fs::read_to_string(&descriptor).unwrap();
    descriptor_text.push_str(&format!(
        "line_cap = \"300\"\nrun_ms = \"{}\"\n",
        seconds * 500
    ));
    fs::write(&descriptor, descriptor_text).unwrap();
    run.release.builds[0].localharness_external_sha256 = file_hash(&descriptor);
    if scenario == "lockout" {
        fs::write(
            root.path.join("data/state.json"),
            json!({
                "format":1,"checked_builds":[],"lockout":{
                    "since":"2026-11-05T00:00:00Z", "signal":"synthetic 403 PERMISSION_DENIED",
                    "original_error_text":"SC239 synthetic lockout"
                }
            })
            .to_string(),
        )
        .unwrap();
    }
    if scenario == "sign-in" {
        fake.set_signed_in(false);
    }
    for _ in 0..8 {
        fake.script_call("session/prompt", RpcReply::Prompt(gate));
    }
    let observer = hub.clone();
    let stop_path = path.join("stop");
    emit(
        json!({"kind":"seeded","port":port,"scenario":scenario,"clock":"SystemClock",
        "http_cap":32,"acp_line_cap":300,"run_seconds":seconds,"hub_log_cap_bytes":16*1024*1024}),
    );
    thread::spawn(move || {
        let mut seen = 0;
        let mut prompts = HashMap::new();
        let mut failed = false;
        let mut delegated = false;
        let mut long_delayed = false;
        let mut delayed = Vec::new();
        let mut partial_errors = Vec::new();
        let mut stopping = false;
        loop {
            if !stopping && stop_path.is_file() {
                stopping = true;
                observer.stop();
            }
            let entries = observer.entries();
            for entry in &entries[seen..] {
                let event = &entry.event;
                emit(json!({"kind":"hub","wall_ms":start.elapsed().as_millis(),"event":event}));
                if event["kind"] == "got" {
                    let call: Value =
                        serde_json::from_str(event["line"].as_str().unwrap()).unwrap();
                    if call["method"] == "session/prompt" {
                        let text = call["params"]["prompt"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter_map(|block| block["text"].as_str())
                            .collect::<String>();
                        let session = call["params"]["sessionId"].as_str().unwrap().to_owned();
                        prompts
                            .insert(format!("request-{session}-{}", call["id"]), (session, text));
                    }
                    if call["method"] == "session/cancel" {
                        let session = call["params"]["sessionId"].as_str().unwrap();
                        delayed.retain(|item: &(Instant, String, String, bool)| item.2 != session);
                        partial_errors.retain(|item: &(String, String, Instant)| item.1 != session);
                    }
                }
                if event["kind"] == "gate" {
                    let gate = event["name"].as_str().unwrap();
                    if gate.starts_with("resume-") || gate.starts_with("error-") {
                        continue;
                    }
                    fake.script_call("session/prompt", RpcReply::Prompt(crate::gate));
                    let prompt = prompts.remove(gate).unwrap();
                    let subagent = prompt
                        .1
                        .lines()
                        .any(|line| line.starts_with("SC239-SUBAGENT"));
                    let main = prompt.1.lines().any(|line| line.starts_with("SC239-MAIN"));
                    emit(
                        json!({"kind":"classified","session":prompt.0,"main":main,"subagent":subagent}),
                    );
                    let mut plan = TurnPlan::new().chunk("SC239-OK").usage(1000, 1_048_576);
                    if scenario == "subagent" && main && !subagent && !delegated {
                        delegated = true;
                        plan = TurnPlan::new();
                        plan.steps.push(TurnStep::ClientToolCall {
                            name: "Agent".into(),
                            arguments: json!({"description":"SC239 child check", "subagent_type":"general-purpose",
                                "prompt":"SC239-SUBAGENT Reply with SC239-CHILD only."}).to_string(),
                        });
                        plan.end = TurnEndPlan::Never;
                    } else if !failed
                        && ((scenario == "subagent" && subagent)
                            || (main
                                && ["temporary", "before", "after", "nonstream"]
                                    .contains(&scenario.as_str())))
                    {
                        failed = true;
                        if ["after", "subagent"].contains(&scenario.as_str()) {
                            let error_gate = format!("error-{gate}");
                            plan = TurnPlan::new()
                                .chunk("SC239-PARTIAL\n")
                                .chunk("SC239-PARTIAL-END\n");
                            plan.steps.push(TurnStep::HoldUntil(error_gate.clone()));
                            partial_errors.push((error_gate, prompt.0.clone(), Instant::now()));
                            emit(json!({"kind":"await_partial_forward","session":prompt.0}));
                        } else {
                            plan = error();
                        }
                    } else if main
                        && (scenario == "pause" || (scenario == "nonstream" && !long_delayed))
                    {
                        long_delayed = true;
                        delayed.push((
                            Instant::now() + Duration::from_secs(pause / 2),
                            gate.to_owned(),
                            prompt.0.clone(),
                            false,
                        ));
                        emit(
                            json!({"kind":"pause","gate":gate,"seconds":pause,"wall_ms":start.elapsed().as_millis()}),
                        );
                        continue;
                    }
                    fake.finish_gate(gate, plan);
                }
            }
            seen = entries.len();
            let mut index = 0;
            while index < delayed.len() {
                if Instant::now() >= delayed[index].0 {
                    let (_, gate, session, last) = delayed.remove(index);
                    if last {
                        fake.finish_gate(
                            &gate,
                            TurnPlan::new().chunk("SC239-OK").usage(1000, 1_048_576),
                        );
                    } else {
                        // A usage update keeps FR-051 from cancelling the 310/330-second turn.
                        // It sends no reply text. The product and API client still use real time.
                        let resume = format!("resume-{gate}");
                        let mut plan = TurnPlan::new().usage(900, 1_048_576);
                        plan.steps.push(TurnStep::HoldUntil(resume.clone()));
                        fake.finish_gate(&gate, plan);
                        delayed.push((
                            Instant::now() + Duration::from_secs(pause - pause / 2),
                            resume,
                            session,
                            true,
                        ));
                    }
                } else {
                    index += 1;
                }
            }
            if partial_ack.is_file() {
                for (gate, session, _) in partial_errors.drain(..) {
                    emit(json!({"kind":"partial_forward_ack","session":session,
                                "wall_ms":start.elapsed().as_millis()}));
                    fake.finish_gate(&gate, error());
                }
            } else if partial_errors
                .iter()
                .any(|item| item.2.elapsed() >= Duration::from_secs(5))
            {
                emit(
                    json!({"kind":"failure","message":"partial text was not forwarded within 5 seconds"}),
                );
                std::process::exit(73);
            }
            if let Some(failure) = observer.failure() {
                emit(json!({"kind":"failure","message":failure}));
                std::process::exit(72);
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    let mut ports = run.ports();
    ports.clock = acp2api::Ports::production().clock;
    acp2api::run(
        ports,
        [
            "sc239-host".into(),
            "--data-dir".into(),
            root.path.join("data").into_os_string(),
        ],
        run.environment,
    )
}
