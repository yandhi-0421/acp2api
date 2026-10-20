use crate::{
    Setup,
    adapters::RunFile,
    hub::Hub,
    setup::{TestRoot, binary, random_hex},
};
use std::{
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
#[derive(Default)]
pub struct Startup {
    pub root: Option<TestRoot>,
    pub run: Option<RunFile>,
    pub data: PathBuf,
    hub: Option<Hub>,
    original: String,
    capture: Arc<Mutex<Vec<u8>>>,
    child: Option<Child>,
    readers: Vec<std::thread::JoinHandle<()>>,
    pub exited: Option<bool>,
    pub occupied: Option<std::net::TcpListener>,
    pub keys: Vec<String>,
    pub passwords: Vec<String>,
    guard: Option<tokio::sync::OwnedMutexGuard<()>>,
    requests: usize,
}
impl std::fmt::Debug for Startup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Startup")
            .field("initialized", &self.root.is_some())
            .finish()
    }
}
impl Startup {
    pub async fn initialize(&mut self) {
        if self.root.is_some() {
            return;
        }
        static LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
        self.guard = Some(
            LOCK.get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
                .lock_owned()
                .await,
        );
        let root = TestRoot::new();
        let hub = Hub::new(random_hex(24));
        let run = root.seed(&hub, &Setup::normal());
        self.data = root.path.join("data");
        self.original = std::fs::read_to_string(self.data.join("config.toml")).unwrap();
        // This explicit startup mode removes every seed before a first start.
        std::fs::remove_dir_all(&self.data).unwrap();
        std::fs::create_dir_all(&self.data).unwrap();
        self.root = Some(root);
        self.run = Some(run);
        self.hub = Some(hub);
    }
    pub fn root(&self) -> &TestRoot {
        self.root.as_ref().unwrap()
    }
    pub fn config_path(&self) -> PathBuf {
        self.data.join("config.toml")
    }
    pub fn saved(&self) -> Option<toml::Value> {
        std::fs::read_to_string(self.config_path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
    }
    pub fn seed_config(&self) {
        if !self.config_path().exists() {
            std::fs::write(self.config_path(), &self.original).unwrap();
        }
    }
    pub fn set(&self, key: &str, value: Option<toml::Value>) {
        self.seed_config();
        let mut config = self.saved().unwrap();
        let table = config.as_table_mut().unwrap();
        if let Some(value) = value {
            table.insert(key.into(), value);
        } else {
            table.remove(key);
        }
        std::fs::write(self.config_path(), toml::to_string(&config).unwrap()).unwrap();
    }
    pub fn terminal(&self) -> String {
        String::from_utf8_lossy(&self.capture.lock().unwrap()).into_owned()
    }
    pub async fn launch(&mut self, args: Vec<String>) {
        assert!(self.child.is_none(), "stop before next start");
        let path = self.root().path.join("startup-run.toml");
        std::fs::write(&path, toml::to_string(self.run.as_ref().unwrap()).unwrap()).unwrap();
        self.capture = Arc::new(Mutex::new(Vec::new()));
        self.exited = None;
        let mut child = Command::new(binary("acp2api-test-host"))
            .env_clear()
            .env("ACP2API_TEST_RUN", path)
            .current_dir(&self.root().path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("guarded test host");
        for mut pipe in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let capture = self.capture.clone();
            self.readers.push(std::thread::spawn(move || {
                let mut bytes = [0u8; 1024];
                loop {
                    match pipe.read(&mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let mut saved = capture.lock().unwrap();
                            if saved.len() + n > 65536 {
                                break;
                            }
                            saved.extend_from_slice(&bytes[..n]);
                        }
                    }
                }
            }));
        }
        self.child = Some(child);
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                self.exited = Some(status.success());
                break;
            }
            let text = self.terminal();
            if text.contains("Listening http://") || text.contains("management panel at http://") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "startup observation deadline (8 s), terminal withheld"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Drain completion is observed through EOF when the child exits; a short yield is insufficient.
        if self.exited.is_some() {
            self.stop().await;
        }
    }
    pub async fn launch_data(&mut self, extra: Vec<String>) {
        let mut args = vec!["--data-dir".into(), self.data.to_str().unwrap().into()];
        args.extend(extra);
        self.launch(args).await;
    }
    pub async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().unwrap().is_none() {
                self.hub.as_ref().unwrap().stop();
            }
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("startup shutdown deadline (8 s), terminal withheld");
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            // Readers own the pipe handles; wait for all bytes without exposing them.
            for reader in self.readers.drain(..) {
                reader.join().expect("bounded terminal reader");
            }
            self.hub.as_ref().unwrap().prepare_restart().await;
        }
    }
    pub async fn responds(&mut self, port: u16) -> bool {
        self.requests += 1;
        assert!(self.requests <= 32, "startup HTTP request cap: 32");
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            let mut stream =
                tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
            stream
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .await?;
            let mut bytes = [0u8; 64];
            let n = stream.read(&mut bytes).await?;
            Ok::<_, std::io::Error>(bytes[..n].starts_with(b"HTTP/1.1 "))
        })
        .await;
        matches!(result, Ok(Ok(true)))
    }
    pub fn remember_secrets(&mut self) {
        let text = self.terminal();
        let key = text
            .lines()
            .find_map(|l| l.strip_prefix("acp2api: local API key: "))
            .map(str::to_owned);
        let password = text
            .lines()
            .find_map(|l| l.strip_prefix("acp2api: management password: "))
            .map(str::to_owned);
        assert!(
            key.is_some(),
            "first start must display its newly generated local API key (withheld)"
        );
        assert!(
            password.is_some(),
            "first start must display its newly generated management password (withheld)"
        );
        let key = key.unwrap();
        let password = password.unwrap();
        let saved = self.saved().expect("generated configuration must parse");
        assert!(
            saved["local_api_key"].as_str() == Some(&key),
            "saved API key differs from generated key (withheld)"
        );
        use argon2::PasswordVerifier;
        let hash = saved["management_password_hash"].as_str().unwrap();
        let verified = argon2::password_hash::phc::PasswordHash::new(hash)
            .ok()
            .is_some_and(|hash| {
                argon2::Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            });
        assert!(
            verified,
            "generated management password does not verify against saved hash (withheld)"
        );
        assert!(
            !std::fs::read_to_string(self.config_path())
                .unwrap()
                .contains(&password),
            "plaintext password saved (withheld)"
        );
        self.keys.push(key);
        self.passwords.push(password);
    }
    pub fn browser_urls(&self) -> Vec<String> {
        self.hub
            .as_ref()
            .unwrap()
            .entries()
            .iter()
            .filter(|e| e.event["kind"] == "browser")
            .filter_map(|e| e.event["url"].as_str().map(str::to_owned))
            .collect()
    }
}
impl Drop for Startup {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(root) = self.root.take() {
            root.cleanup();
        }
    }
}

impl Startup {
    pub fn panel_hub(&self) -> Hub {
        self.hub.as_ref().expect("initialized startup").clone()
    }
    pub fn panel_running(&self) -> bool {
        self.child.is_some()
    }
    pub fn panel_runtime(&self) {
        let source = self.root().path.join("builds/normal");
        let target = self.data.join("official-runtime");
        std::fs::create_dir_all(&target).unwrap();
        for file in std::fs::read_dir(source).unwrap() {
            let file = file.unwrap();
            std::fs::copy(file.path(), target.join(file.file_name())).unwrap();
        }
    }
}
