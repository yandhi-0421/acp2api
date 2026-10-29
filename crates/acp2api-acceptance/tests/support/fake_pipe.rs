use acp2api::{LaunchSpec, Launched, ProcessLauncher};
use acp2api_acceptance::real_limit;
use acp2api_acceptance::{Scenario, Setup, adapters::GuardedLauncher};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// Each test uses at most 12 ACP requests. Each read and shutdown has a 3 s cap;
// the fake and hub retain their request, time and byte caps. No log is written.
pub struct FakePipe {
    pub scenario: Scenario,
    pub child: Launched,
    out: BufReader<acp2api::ProcessRead>,
    err: BufReader<acp2api::ProcessRead>,
}
impl FakePipe {
    pub fn new() -> Self {
        let scenario = Scenario::default();
        let run = scenario.root.seed(&scenario.hub, &Setup::normal());
        let launcher = GuardedLauncher::new(&run.fake_program, run.hub);
        let mut child =
            launcher
                .launch(LaunchSpec {
                    program: scenario.root.path.join("data/official-runtime").join(
                        if cfg!(windows) {
                            "agy_acp_server.exe"
                        } else {
                            "agy_acp_server.par"
                        },
                    ),
                    args: vec![],
                    env_set: vec![(
                        "GEMINI_HOME".into(),
                        scenario
                            .root
                            .path
                            .join("data/runtime-home")
                            .into_os_string(),
                    )],
                    cwd: scenario.root.path.clone(),
                    own_temp: None,
                })
                .unwrap();
        let out = BufReader::new(std::mem::replace(
            &mut child.stdout,
            Box::pin(tokio::io::empty()),
        ));
        let err = BufReader::new(std::mem::replace(
            &mut child.stderr,
            Box::pin(tokio::io::empty()),
        ));
        Self {
            scenario,
            child,
            out,
            err,
        }
    }
    pub async fn send(&mut self, call: Value) {
        self.child
            .stdin
            .write_all(format!("{call}\n").as_bytes())
            .await
            .unwrap();
    }
    pub async fn prompt(&mut self, session: &str, id: u64) {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":"session/prompt","params":{"sessionId":session,"prompt":[]}})).await;
    }
    pub async fn cancel(&mut self, session: &str) {
        self.send(
            json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session}}),
        )
        .await;
    }
    pub async fn stdout(&mut self) -> Value {
        let mut line = String::new();
        tokio::time::timeout(
            real_limit(Duration::from_secs(3)),
            self.out.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        serde_json::from_str(&line).unwrap()
    }
    pub async fn stderr(&mut self) -> String {
        let mut line = String::new();
        tokio::time::timeout(
            real_limit(Duration::from_secs(3)),
            self.err.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        line.trim_end().into()
    }
    pub async fn finish(mut self) {
        drop(self.child.stdin);
        self.scenario
            .hub
            .command("normal", json!({"op":"Exit","code":0}));
        tokio::time::timeout(real_limit(Duration::from_secs(3)), self.child.tree.exited())
            .await
            .unwrap();
        self.scenario.hub.close();
        self.scenario.root.cleanup();
    }
}
