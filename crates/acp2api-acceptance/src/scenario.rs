use crate::{
    client::{Answer, ApiClient, Owner},
    fake::FakeRuntime,
    hub::Hub,
    setup::{Setup, TestRoot, binary, random_hex},
    timeline::TimeLine,
};
use std::{
    io::{BufRead, BufReader, Read},
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
    owner: Option<Owner>,
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
            owner: None,
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
        if self.host.is_some() {
            return;
        }
        let run = self
            .root
            .seed(&self.hub, self.plan.as_ref().expect("Given normal state"));
        let path = self.root.path.join("run.toml");
        std::fs::write(&path, toml::to_string(&run).unwrap()).unwrap();
        let mut host = Command::new(binary("acp2api-test-host"))
            .env_clear()
            .env("ACP2API_TEST_RUN", path)
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
        let deadline = Instant::now() + Duration::from_secs(10);
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
        self.client = Some(ApiClient::new(
            format!("127.0.0.1:{}", self.root.port),
            self.root.key.clone(),
            self.hub.clone(),
        ));
    }
    pub async fn client(&mut self) -> &mut ApiClient {
        self.start().await;
        self.client.as_mut().unwrap()
    }
    pub async fn owner(&mut self) -> &mut Owner {
        self.start().await;
        if self.owner.is_none() {
            self.owner = Some(Owner::new(self.client.as_ref().unwrap().clone()));
        }
        self.owner.as_mut().unwrap()
    }
    pub async fn stop(&mut self) {
        if let Some(mut host) = self.host.take() {
            self.hub.stop();
            let deadline = Instant::now() + Duration::from_secs(10);
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
