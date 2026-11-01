use super::*;
use crate::network_observer::Observer;

impl Scenario {
    pub async fn start_network_observed(&mut self, observer: &Observer) -> u32 {
        assert!(
            self.host.is_none(),
            "network observation starts before the host"
        );
        let run = self
            .root
            .seed(&self.hub, self.plan.as_ref().expect("network fixture"));
        let path = self.root.path.join("run.toml");
        std::fs::write(&path, toml::to_string(&run).unwrap()).unwrap();
        let mut command = observer.command(&binary("acp2api-test-host"));
        command
            .env_clear()
            .env(TIME_FACTOR_ENV, time_factor().to_string());
        #[cfg(windows)]
        command.env("SystemRoot", std::env::var_os("SystemRoot").unwrap());
        let mut host = command
            .env("ACP2API_TEST_RUN", path)
            .args(["--data-dir", self.root.path.join("data").to_str().unwrap()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("observed test host launch");
        let stdout = host.stdout.take().unwrap();
        let hub = self.hub.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
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
        let pid = host.id();
        self.host = Some(host);
        let deadline = Instant::now() + real_limit(Duration::from_secs(10));
        while !self.hub.ready() {
            assert!(
                self.host.as_mut().unwrap().try_wait().unwrap().is_none(),
                "observed host exited before ready"
            );
            assert!(
                Instant::now() < deadline,
                "observed host startup cap: 10 seconds"
            );
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
        pid
    }
}
