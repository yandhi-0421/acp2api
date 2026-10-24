use crate::real_limit;
use crate::{
    client::{Answer, ApiClient},
    fake::FakeRuntime,
    hub::Hub,
    setup::{Setup, TestRoot, binary, random_hex},
    timeline::TimeLine,
};
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Debug)]
pub struct Scenario {
    pub root: TestRoot,
    pub hub: Hub,
    pub time: TimeLine,
    host: Option<Child>,
    client: Option<ApiClient>,
    plan: Option<Setup>,
    pub answer: Option<Answer>,
}
impl Default for Scenario {
    fn default() -> Self {
        let hub = Hub::new(random_hex(24));
        Self {
            root: TestRoot::new(),
            time: TimeLine::new(hub.clone()),
            hub,
            host: None,
            client: None,
            plan: None,
            answer: None,
        }
    }
}
impl Scenario {
    pub fn plan(&mut self, setup: Setup) {
        assert!(self.host.is_none(), "setup precedes first use");
        self.fake().model(&setup.model);
        self.plan = Some(setup);
    }
    pub fn fake(&self) -> FakeRuntime {
        FakeRuntime::new(self.hub.clone())
    }
    pub async fn start(&mut self) {
        self.start_fresh(&[]).await;
    }
    pub async fn start_isolated_owner(&mut self) {
        let env = [
            ("HOME", self.root.path.join("home")),
            ("GEMINI_HOME", self.root.path.join("home/.gemini")),
        ];
        self.start_fresh(&env).await;
    }
    async fn start_fresh(&mut self, env: &[(&str, PathBuf)]) {
        if self.host.is_some() {
            return;
        }
        let run = self
            .root
            .seed(&self.hub, self.plan.as_ref().expect("Given normal state"));
        std::fs::write(
            self.root.path.join("run.toml"),
            toml::to_string(&run).unwrap(),
        )
        .unwrap();
        let address = format!("127.0.0.1:{}", self.root.port);
        self.spawn_host(env, address, self.root.key.clone()).await;
    }
    async fn start_saved(&mut self) {
        assert!(
            self.root.path.join("run.toml").is_file(),
            "restart requires an existing run file"
        );
        let config: toml::Value = toml::from_str(
            &std::fs::read_to_string(self.root.path.join("data/config.toml")).unwrap(),
        )
        .unwrap();
        let port = config["port"].as_integer().expect("saved port");
        let key = config["local_api_key"]
            .as_str()
            .expect("saved key")
            .to_owned();
        self.spawn_host(&[], format!("127.0.0.1:{port}"), key).await;
    }
    async fn spawn_host(&mut self, env: &[(&str, PathBuf)], address: String, key: String) {
        let mut command = Command::new(binary("acp2api-test-host"));
        command.env_clear();
        command.env(
            "ACP2API_TEST_TIME_FACTOR",
            real_limit(Duration::from_secs(1)).as_secs().to_string(),
        );
        #[cfg(windows)]
        command.env("SystemRoot", std::env::var_os("SystemRoot").unwrap());
        let mut host = command
            .env("ACP2API_TEST_RUN", self.root.path.join("run.toml"))
            .envs(env.iter().map(|(name, value)| (name, value)))
            .args(["--data-dir", self.root.path.join("data").to_str().unwrap()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("test host launch");
        for pipe in [
            Box::new(host.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>,
            Box::new(host.stderr.take().unwrap()),
        ] {
            let hub = self.hub.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(pipe);
                loop {
                    let mut line = Vec::new();
                    match reader
                        .by_ref()
                        .take(4 * 1024 * 1024 + 1)
                        .read_until(b'\n', &mut line)
                    {
                        Ok(0) | Err(_) => break,
                        Ok(_) => hub.terminal(&line),
                    }
                }
            });
        }
        self.host = Some(host);
        let deadline = Instant::now() + real_limit(Duration::from_secs(10));
        while !self.hub.ready() {
            if let Some(status) = self.host.as_mut().unwrap().try_wait().unwrap() {
                panic!(
                    "test host exited {status}; terminal: {:?}",
                    self.hub
                        .entries()
                        .iter()
                        .filter(|e| e.event["kind"] == "terminal")
                        .map(|e| e.event["text"].clone())
                        .collect::<Vec<_>>()
                );
            }
            assert!(Instant::now() < deadline, "test host startup cap: 10 s");
            let hub = self.hub.clone();
            tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                .await
                .unwrap();
        }
        self.time.settle().await;
        self.client = Some(ApiClient::new(address, key, self.hub.clone()));
    }
    pub async fn client(&mut self) -> &mut ApiClient {
        self.start().await;
        self.client.as_mut().unwrap()
    }
    pub async fn stop(&mut self) {
        if let Some(mut host) = self.host.take() {
            self.hub.stop();
            let deadline = Instant::now() + real_limit(Duration::from_secs(10));
            loop {
                if host.try_wait().unwrap().is_some() {
                    break;
                }
                assert!(Instant::now() < deadline, "test host shutdown cap: 10 s");
                self.time.settle().await;
                if let Some(next) = self.time.next_deadline() {
                    self.time.advance_to(next).await;
                } else {
                    let hub = self.hub.clone();
                    tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                        .await
                        .unwrap();
                }
            }
        }
    }
    pub async fn kill(&mut self) {
        if let Some(mut host) = self.host.take() {
            let _ = host.kill();
            let _ = host.wait();
        }
    }
    pub async fn restart(&mut self) {
        self.stop().await;
        self.hub.prepare_restart().await;
        self.client = None;
        self.start_saved().await;
        self.start().await;
    }
    pub async fn finish(&mut self, passed: bool) {
        self.stop().await;
        self.root.check_size();
        self.hub.close();
        if passed {
            self.root.cleanup();
        }
    }
}
impl Drop for Scenario {
    fn drop(&mut self) {
        if let Some(mut host) = self.host.take() {
            let _ = host.kill();
            let _ = host.wait();
        }
        self.hub.close();
    }
}
