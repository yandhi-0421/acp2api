use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Stop(tokio::sync::oneshot::Receiver<()>);
impl StopSignal for Stop {
    fn stopped(&mut self) -> BoxFuture<'_, ()> {
        Box::pin(async {
            let _ = (&mut self.0).await;
        })
    }
}

struct Host {
    root: PathBuf,
    settings: crate::store::Settings,
    port: u16,
}
impl Host {
    fn new(override_port: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "acp2api-panel-links-{}",
            crate::random_hex::<12>().unwrap()
        ));
        std::fs::create_dir_all(root.join("temp")).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let settings = crate::store::Settings {
            port: if override_port { 8329 } else { port },
            concurrency_limit: 1,
            local_api_key: "fixture-local-api-key".into(),
            management_password_hash: crate::store::hash_password("fixture-owner").unwrap(),
            agy_acp_server_path: None,
        };
        crate::store::save(&root.join("data/config.toml"), &settings).unwrap();
        Self {
            root,
            settings,
            port,
        }
    }
    fn lockout(&self) {
        let mut state = crate::store::StateFile::normal();
        state.lockout = Some(crate::store::Lockout {
            since: "2026-11-01T00:00:00Z".into(),
            signal: "403 PERMISSION_DENIED".into(),
            original_error_text: "Account suspended".into(),
        });
        state.save(&self.root.join("data/state.json")).unwrap();
    }
    fn start(
        &self,
        mut ports: Ports,
    ) -> (
        tokio::sync::oneshot::Sender<()>,
        tokio::task::JoinHandle<Result<(), String>>,
    ) {
        let (stop, signal) = tokio::sync::oneshot::channel();
        ports.stop = Box::new(Stop(signal));
        let mut args = vec![
            "acp2api".into(),
            "--data-dir".into(),
            self.root.join("data").into_os_string(),
        ];
        if self.port != self.settings.port {
            args.extend(["--port".into(), self.port.to_string().into()]);
        }
        let env = Environment {
            account_root: self.root.join("account"),
            home: None,
            local_app_data: None,
            temp: self.root.join("temp"),
            path: vec![],
            exe_dir: self.root.join("exe"),
        };
        (stop, tokio::spawn(start(ports, args, env)))
    }
    async fn request(&self, method: &str, path: &str, headers: &str, body: &str) -> String {
        let mut client = loop {
            match tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, self.port)).await {
                Ok(client) => break client,
                Err(_) => tokio::task::yield_now().await,
            }
        };
        client.write_all(format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
            self.port, body.len()
        ).as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        client.take(65537).read_to_end(&mut bytes).await.unwrap();
        assert!(bytes.len() <= 65536, "HTTP response cap: 64 KiB");
        String::from_utf8(bytes).unwrap()
    }
    async fn models(&self) -> String {
        self.request(
            "GET",
            "/v1/models",
            "x-api-key: fixture-local-api-key\r\n",
            "",
        )
        .await
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn message(response: &str) -> serde_json::Value {
    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "expected API refusal: {response}"
    );
    let (_, body) = response.split_once("\r\n\r\n").unwrap();
    serde_json::from_str::<serde_json::Value>(body).unwrap()["error"]["message"].clone()
}

#[tokio::test]
async fn stored_lockout_links_to_the_effective_port() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        for override_port in [false, true] {
            let host = Host::new(override_port);
            host.lockout();
            let (stop, task) = host.start(Ports::production());
            let response = host.models().await;
            stop.send(()).unwrap();
            task.await.unwrap().unwrap();
            assert_eq!(message(&response), format!(
                "acp2api: lockout since 2026-11-01T00:00:00Z after 403 PERMISSION_DENIED; acp2api sends no request to the official runtime until the owner ends the lockout at http://127.0.0.1:{}/.",
                host.port
            ));
        }
    }).await.expect("panel-link cap: 10 seconds, two starts, one HTTP request per start, 64 KiB response; stored lockout starts no process");
}

struct NoTree;
impl ProcessTree for NoTree {
    fn pid(&self) -> u32 {
        0
    }
    fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
        Box::pin(std::future::pending())
    }
    fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}
struct SignInRuntime {
    child: std::sync::Mutex<Option<Launched>>,
    build: ListedBuild,
}
impl ProcessLauncher for SignInRuntime {
    fn launch(&self, _: LaunchSpec) -> Result<Launched, LaunchError> {
        Ok(self
            .child
            .lock()
            .unwrap()
            .take()
            .expect("one fake launch cap"))
    }
}
impl ReleaseFacts for SignInRuntime {
    fn version(&self) -> &str {
        "fixture"
    }
    fn platform(&self) -> &str {
        &self.build.platform
    }
    fn listed_builds(&self) -> &[ListedBuild] {
        std::slice::from_ref(&self.build)
    }
    fn highest_concurrency_limit(&self) -> u8 {
        3
    }
}

#[tokio::test]
async fn sign_in_links_to_the_effective_port_and_entry() {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncBufReadExt;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let host = Host::new(true);
        let copy = host.root.join("data/official-runtime");
        std::fs::create_dir(&copy).unwrap();
        for name in [crate::runtime_builds::SERVER, crate::runtime_builds::HARNESS] {
            std::fs::write(copy.join(name), b"fixture runtime").unwrap();
        }
        let mut ports = Ports::production();
        let hash = crate::hex(&Sha256::digest(b"fixture runtime"));
        let (product, agent) = tokio::io::duplex(65536);
        let (stdout, stdin) = tokio::io::split(product);
        let fake = Arc::new(SignInRuntime {
            child: std::sync::Mutex::new(Some(Launched {
                stdin: Box::pin(stdin),
                stdout: Box::pin(stdout),
                stderr: Box::pin(tokio::io::empty()),
                tree: Box::new(NoTree),
            })),
            build: ListedBuild {
                version: "1.4.0".into(),
                platform: ports.release.platform().into(),
                agy_acp_server_sha256: hash.clone(),
                localharness_external_sha256: hash,
            },
        });
        ports.launcher = fake.clone();
        ports.release = fake;
        let (stop, task) = host.start(ports);
        let mut agent = tokio::io::BufReader::new(agent);
        for (method, reply) in [
            ("initialize", serde_json::json!({"result":{"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}})),
            ("session/new", serde_json::json!({"error":{"code":-32000,"message":"Authentication required"}})),
        ] {
            let mut bytes = Vec::new();
            (&mut agent).take(65537).read_until(b'\n', &mut bytes).await.unwrap();
            assert!(bytes.len() <= 65536, "ACP request cap: 64 KiB");
            let call: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(call["method"], method);
            let mut reply = reply;
            reply["jsonrpc"] = "2.0".into();
            reply["id"] = call["id"].clone();
            agent.write_all(format!("{reply}\n").as_bytes()).await.unwrap();
        }
        let response = host.models().await;
        stop.send(()).unwrap();
        task.await.unwrap().unwrap();
        assert_eq!(message(&response), format!(
            "acp2api: the official runtime is not signed in. Sign in at http://127.0.0.1:{}/#sign-in.",
            host.port
        ));
    }).await.expect("sign-in link cap: 10 seconds; one in-memory fake launch, two ACP replies, one HTTP request, 64 KiB per frame");
}

#[tokio::test]
async fn panel_save_keeps_the_configured_port() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let host = Host::new(true);
        host.lockout();
        let (stop, task) = host.start(Ports::production());
        let login = host.request(
            "POST", "/manage/login", "Content-Type: application/x-www-form-urlencoded\r\n", "password=fixture-owner"
        ).await;
        assert!(login.starts_with("HTTP/1.1 303 See Other\r\n"), "owner must have a real panel session");
        let cookie = login.lines().find_map(|line| line.strip_prefix("set-cookie: ")).unwrap().split(';').next().unwrap();
        let saved = host.request(
            "POST", "/manage/concurrency",
            &format!("Content-Type: application/x-www-form-urlencoded\r\nCookie: {cookie}\r\n"),
            "concurrency_limit=2"
        ).await;
        stop.send(()).unwrap();
        task.await.unwrap().unwrap();
        assert!(saved.starts_with("HTTP/1.1 200 OK\r\n"), "panel save must succeed");
        let config: toml::Table = toml::from_str(&std::fs::read_to_string(host.root.join("data/config.toml")).unwrap()).unwrap();
        assert_eq!((config["port"].as_integer(), config["concurrency_limit"].as_integer()), (Some(8329), Some(2)), "a real panel save changes the setting and keeps the saved port");
    }).await.expect("panel-save cap: 10 seconds; two HTTP requests, 64 KiB per response; stored lockout starts no process");
}
