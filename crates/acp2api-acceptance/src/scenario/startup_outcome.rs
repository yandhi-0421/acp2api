use super::*;

impl Scenario {
    pub async fn restart_outcome(&mut self) -> Result<(), std::process::ExitStatus> {
        self.stop().await;
        self.hub.prepare_restart().await;
        self.client = None;
        let config: toml::Value = toml::from_str(
            &std::fs::read_to_string(self.root.path.join("data/config.toml")).unwrap(),
        )
        .unwrap();
        let port = config["port"].as_integer().expect("saved port");
        let key = config["local_api_key"]
            .as_str()
            .expect("saved key")
            .to_owned();
        self.launch_host(&[], &self.root.path.join("run.toml"));
        let deadline = Instant::now() + real_limit(Duration::from_secs(10));
        loop {
            if let Some(status) = self.host.as_mut().unwrap().try_wait().unwrap() {
                return Err(status);
            }
            if self.hub.ready() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "startup outcome observation cap: 10 s"
            );
            let hub = self.hub.clone();
            tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                .await
                .unwrap();
        }
        self.time.settle().await;
        self.client = Some(ApiClient::new(
            format!("127.0.0.1:{port}"),
            key,
            self.hub.clone(),
        ));
        Ok(())
    }
}
