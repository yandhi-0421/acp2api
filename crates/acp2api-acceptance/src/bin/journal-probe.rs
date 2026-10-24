use acp2api_acceptance::real_limit;
use acp2api_acceptance::{Scenario, Setup, journal_check as check};
use serde_json::{Value, json};
fn report(value: Value) {
    let output = value.to_string();
    assert!(output.len() <= 65_536, "probe output cap: 64 KiB");
    println!("{output}");
}
#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() <= 6 && args.iter().all(|s| s.len() <= 256),
        "probe argument caps"
    );
    tokio::time::timeout(real_limit(std::time::Duration::from_secs(45)), async {
        match args.first().map(String::as_str) {
            Some("http") if args.len() == 6 => {
                let (model, field, marker) = (&args[1], &args[2], &args[3]);
                let usage: u64 = args[4].parse().unwrap();
                let delay: u64 = args[5].parse().unwrap();
                assert!(delay <= 700, "single-record probe UTC-second cap");
                let mut s = Scenario::default();
                let mut setup = Setup::normal();
                setup.model = model.clone();
                s.plan(setup);
                let expected = check::send(&mut s, "POST /v1/chat/completions", model, field, marker, usage, delay).await;
                check::wait_terminal(&s, 1).await;
                check::verify(&s, &[expected]);
                for p in check::files(&s.root.path.join("data/logs")) {
                    assert!(!std::fs::read_to_string(p).unwrap().contains(marker), "request content must not reach journal");
                }
                assert!(s.fake().record().prompts().iter().any(|p| p.text.contains(marker)), "fresh marker crossed real HTTP and ACP");
                report(json!({"mode":"http","records":check::records(&s),"requests":1,"request_cap":1,"run_cap_seconds":real_limit(std::time::Duration::from_secs(45)).as_secs(),"output_cap_bytes":65536}));
                s.finish(true).await;
            }
            Some("retention") if args.len() == 2 => {
                let s = Scenario::default();
                let r = check::stress(&s.root.path.join("writer-logs"), &args[1]);
                check::verify_stress(&r, &args[1]);
                s.root.check_size();
                let retained: Vec<_> = r.retained.iter().map(|(p, n)| {
                    let text = std::fs::read_to_string(p).unwrap();
                    let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
                    json!({"file":p.file_name().unwrap().to_string_lossy(),"bytes":n,"first_request_id":lines.first().unwrap()["request_id"],"last_request_id":lines.last().unwrap()["request_id"]})
                }).collect();
                report(json!({"mode":"retention","written":r.written,"oldest":r.first.file_name().unwrap().to_string_lossy(),"oldest_deleted":!r.first.exists(),"retained_bytes":r.retained.iter().map(|(_,n)|n).sum::<u64>(),"retained":retained,"verified_writer_origin":true,"record_cap":60,"input_cap_bytes":61000000,"run_cap_seconds":real_limit(std::time::Duration::from_secs(12)).as_secs(),"output_cap_bytes":65536}));
                s.root.cleanup();
            }
            _ => panic!("usage: journal-probe http MODEL FIELD MARKER USAGE DELAY_MS | retention MARKER"),
        }
    }).await.expect("journal probe run cap: 45 s");
}
