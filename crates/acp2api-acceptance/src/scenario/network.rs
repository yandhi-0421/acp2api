use super::*;
use crate::network_observer::{FORBIDDEN, Observer, check_exit};

impl Scenario {
    pub async fn start_network_observed(
        &mut self,
        observer: &Observer,
        env: &[(&str, &str)],
    ) -> u32 {
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
            .env(TIME_FACTOR_ENV, time_factor().to_string())
            .envs(env.iter().copied());
        #[cfg(windows)]
        command.env("SystemRoot", std::env::var_os("SystemRoot").unwrap());
        let mut host = command
            .env("ACP2API_TEST_RUN", path)
            .args(["--data-dir", self.root.path.join("data").to_str().unwrap()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("observed test host launch");
        for pipe in [
            Box::new(host.stdout.take().unwrap()) as Box<dyn Read + Send>,
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
        let pid = host.id();
        self.host = Some(host);
        let deadline = Instant::now() + real_limit(Duration::from_secs(10));
        while !self.hub.ready() {
            self.check_network_host()
                .expect("observed host before ready");
            assert!(
                Instant::now() < deadline,
                "observed host startup cap: 10 seconds"
            );
            let hub = self.hub.clone();
            tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                .await
                .unwrap();
        }
        self.check_network_host()
            .expect("observed host after ready");
        self.time.settle().await;
        self.check_network_host()
            .expect("observed host after startup");
        self.client = Some(ApiClient::new(
            format!("127.0.0.1:{}", self.root.port),
            self.root.key.clone(),
            self.hub.clone(),
        ));
        pid
    }
    pub fn check_network_host(&mut self) -> Result<(), String> {
        if let Some(host) = self.host.as_mut()
            && let Some(status) = host
                .try_wait()
                .map_err(|e| format!("{FORBIDDEN}: host status: {e}"))?
        {
            return Err(format!(
                "{FORBIDDEN}: host exited before harness stop: {status}; terminal: {:?}",
                self.hub
                    .entries()
                    .iter()
                    .filter(|entry| entry.event["kind"] == "terminal")
                    .map(|entry| entry.event["text"].clone())
                    .collect::<Vec<_>>()
            ));
        }
        Ok(())
    }
    pub async fn stop_network_observed(&mut self) -> Result<std::process::ExitStatus, String> {
        self.stop_network_host(real_limit(Duration::from_secs(10)))
            .await
    }
    async fn stop_network_host(
        &mut self,
        cap: Duration,
    ) -> Result<std::process::ExitStatus, String> {
        self.check_network_host()?;
        self.hub.stop();
        let deadline = Instant::now() + cap;
        loop {
            if let Some(status) = self
                .host
                .as_mut()
                .expect("observed host")
                .try_wait()
                .map_err(|e| format!("{FORBIDDEN}: host status: {e}"))?
            {
                self.host.take();
                check_exit(status)?;
                return Ok(status);
            }
            if Instant::now() >= deadline {
                let host = self.host.as_mut().unwrap();
                let kill = host.kill();
                let status = host.wait();
                self.host.take();
                return Err(format!(
                    "{FORBIDDEN}: harness forced host shutdown; kill={kill:?}; exit={status:?}"
                ));
            }
            let hub = self.hub.clone();
            tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                .await
                .unwrap();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn observed_shutdown_rejects_a_host_that_already_exited() {
        for script in ["exit 0", "kill -KILL $$"] {
            let mut scenario = Scenario::default();
            let mut child = Command::new("/bin/sh")
                .args(["-c", script])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            child.wait().unwrap();
            scenario.host = Some(child);
            let error = scenario.stop_network_observed().await.unwrap_err();
            assert!(
                error.starts_with(
                    "FR-046 outbound attempt outside 127.0.0.1: host exited before harness stop:"
                ),
                "{error}"
            );
            scenario.finish(true).await;
        }
    }

    #[tokio::test]
    async fn observed_shutdown_reports_a_forced_kill() {
        let mut scenario = Scenario::default();
        let child = Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        scenario.host = Some(child);
        let error = scenario
            .stop_network_host(Duration::ZERO)
            .await
            .unwrap_err();
        assert!(
            error.starts_with(
                "FR-046 outbound attempt outside 127.0.0.1: harness forced host shutdown;"
            ),
            "{error}"
        );
        assert_eq!(
            unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
        scenario.finish(true).await;
    }
}
