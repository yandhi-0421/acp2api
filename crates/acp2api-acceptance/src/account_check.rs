use crate::{
    Setup, TIME_FACTOR_ENV,
    adapters::RunFile,
    hub::Hub,
    real_limit,
    setup::{TestRoot, binary, random_hex},
    time_factor,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct Instance {
    pub root: TestRoot,
    run: RunFile,
    pub hub: Hub,
    data: PathBuf,
    child: Option<Child>,
    capture: Arc<Mutex<Vec<u8>>>,
    readers: Vec<std::thread::JoinHandle<()>>,
    pub exited: Option<bool>,
}
impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("exited", &self.exited)
            .finish_non_exhaustive()
    }
}
impl Instance {
    fn new(data: &str) -> Self {
        let root = TestRoot::new();
        let hub = Hub::new(random_hex(24));
        let run = root.seed(&hub, &Setup::normal());
        let data = root.path.join(data);
        fs::rename(root.path.join("data"), &data).unwrap();
        Self {
            root,
            run,
            hub,
            data,
            child: None,
            capture: Arc::default(),
            readers: vec![],
            exited: None,
        }
    }
    pub fn terminal(&self) -> String {
        String::from_utf8_lossy(&self.capture.lock().unwrap()).into_owned()
    }
    pub fn address(&self) -> String {
        format!("http://127.0.0.1:{}/", self.root.port)
    }
    pub fn published(&self) -> bool {
        self.terminal()
            .contains(&format!("management panel at {}", self.address()))
    }
    pub fn pid(&self) -> u32 {
        self.child.as_ref().unwrap().id()
    }
    pub fn client(&self) -> crate::client::ApiClient {
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(self.data.join("config.toml")).unwrap()).unwrap();
        crate::client::ApiClient::new(
            format!("127.0.0.1:{}", self.root.port),
            config["local_api_key"].as_str().unwrap().into(),
            self.hub.clone(),
        )
    }
    pub fn spawn(&mut self, host: &str) {
        assert!(self.child.is_none());
        let path = self.root.path.join("account-run.toml");
        fs::write(&path, toml::to_string(&self.run).unwrap()).unwrap();
        let mut command = Command::new(binary(host));
        command
            .env_clear()
            .env(TIME_FACTOR_ENV, time_factor().to_string());
        #[cfg(windows)]
        command.env("SystemRoot", std::env::var_os("SystemRoot").unwrap());
        for key in ["TMPDIR", "TEMP", "TMP"] {
            command.env(key, &self.run.environment.temp);
        }
        let mut child = command
            .env("ACP2API_TEST_RUN", path)
            .args([
                "--data-dir",
                self.data.to_str().unwrap(),
                "--port",
                &self.root.port.to_string(),
            ])
            .current_dir(&self.root.path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("existing test host with isolated local account resources");
        for mut pipe in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let capture = self.capture.clone();
            self.readers.push(std::thread::spawn(move || {
                let mut bytes = [0; 1024];
                while let Ok(n) = pipe.read(&mut bytes) {
                    if n == 0 {
                        break;
                    }
                    let mut saved = capture.lock().unwrap();
                    assert!(
                        saved.len() + n <= 65536,
                        "account host terminal cap: 64 KiB"
                    );
                    saved.extend_from_slice(&bytes[..n]);
                }
            }));
        }
        self.child = Some(child);
    }
    pub async fn observe(&mut self) {
        let deadline = Instant::now() + real_limit(Duration::from_secs(8));
        loop {
            if self.poll_exit() || self.published() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "test host neither exited nor published within infrastructure cap; this is not an FR-006 deadline"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    fn poll_exit(&mut self) -> bool {
        let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() else {
            return false;
        };
        self.exited = Some(status.success());
        for reader in self.readers.drain(..) {
            reader.join().unwrap();
        }
        true
    }
    pub async fn observe_pending_start(&mut self) {
        let deadline = Instant::now() + real_limit(Duration::from_secs(8));
        loop {
            if self.poll_exit()
                || self.published()
                || crate::timeline::TimeLine::new(self.hub.clone())
                    .next_deadline()
                    .is_some()
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "account startup observation infrastructure cap"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    pub fn kill(&mut self) {
        self.child.as_mut().unwrap().kill().unwrap();
        let status = self.child.as_mut().unwrap().wait().unwrap();
        self.exited = Some(status.success());
        for reader in self.readers.drain(..) {
            reader.join().unwrap();
        }
    }
    pub async fn stop(&mut self) {
        self.hub.stop();
        let deadline = Instant::now() + real_limit(Duration::from_secs(8));
        let mut time = crate::timeline::TimeLine::new(self.hub.clone());
        while !self.poll_exit() {
            assert!(
                Instant::now() < deadline,
                "account host shutdown infrastructure cap"
            );
            time.settle().await;
            if let Some(next) = time.next_deadline() {
                time.advance_to(next).await;
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.hub.close();
        for reader in self.readers.drain(..) {
            reader.join().unwrap();
        }
        self.root.cleanup();
    }
}

#[derive(Debug, Default)]
pub struct AccountState {
    pub instances: Vec<Instance>,
    temporary: Option<PathBuf>,
    session: Option<PathBuf>,
    session_file: Option<PathBuf>,
    locks: Vec<(PathBuf, Option<FileIdentity>)>,
    pub failed_path: PathBuf,
    pub early_publication: bool,
    first_files: BTreeMap<PathBuf, String>,
    unreadable: Option<(PathBuf, fs::Permissions)>,
    #[cfg(windows)]
    unreadable_handle: Option<File>,
}
impl AccountState {
    pub fn first(&mut self, data: &str) {
        assert!(self.instances.is_empty());
        self.instances.push(Instance::new(data));
    }
    pub fn second(&mut self, data: &str, other_account: bool) {
        assert_eq!(self.instances.len(), 1, "account scenario host cap: 2");
        let mut second = Instance::new(data);
        if !other_account {
            second.run.environment.account_root =
                self.instances[0].run.environment.account_root.clone();
        }
        if data == self.instances[0].data.file_name().unwrap() {
            second.data = self.instances[0].data.clone();
        }
        self.instances.push(second);
    }
    pub async fn launch(&mut self, index: usize, temp: &str) {
        self.spawn(index, temp);
        self.instances[index].observe().await;
    }
    fn spawn(&mut self, index: usize, temp: &str) {
        self.spawn_host(index, temp, "acp2api-test-host");
    }
    fn spawn_host(&mut self, index: usize, temp: &str, host: &str) {
        let temp = self.instances[0].root.path.join("temps").join(temp);
        fs::create_dir_all(&temp).unwrap();
        self.instances[index].run.environment.temp = temp;
        self.instances[index].spawn(host);
    }
    pub async fn launch_waiting(&mut self, index: usize, temp: &str) {
        self.spawn(index, temp);
        self.instances[index].observe_pending_start().await;
    }
    pub async fn hold_first(&mut self) {
        self.spawn_host(0, "T1", "acp2api-held-test-host");
        let deadline = Instant::now() + real_limit(Duration::from_secs(8));
        while !self.instances[0]
            .hub
            .entries()
            .iter()
            .any(|e| e.event["kind"] == "account_held" && e.event["pid"] == self.instances[0].pid())
        {
            assert!(
                Instant::now() < deadline,
                "account fixture lock-acquisition infrastructure cap"
            );
            let hub = self.instances[0].hub.clone();
            tokio::task::spawn_blocking(move || hub.wait_change(deadline))
                .await
                .unwrap();
        }
        let account = &self.instances[0].run.environment.account_root;
        for name in ["startup.lock", "instance.lock"] {
            let path = account.join(name);
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            assert!(
                matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
                "test host really owns both OS locks before publication"
            );
            self.locks.push((path.clone(), identity(&path)));
        }
        assert!(
            !self.instances[0].published(),
            "first host is held before acp2api::run"
        );
        self.first_files = self.files_of_first();
    }
    pub async fn release_first(&mut self) {
        self.instances[0]
            .hub
            .command("account-first", serde_json::json!({"op":"go"}));
        self.instances[0].observe().await;
        assert!(
            self.instances[0].published(),
            "first test host publishes before the second clock resumes"
        );
    }
    pub async fn resume_second(&mut self) {
        let mut time = crate::timeline::TimeLine::new(self.instances[1].hub.clone());
        if let Some(next) = time.next_deadline() {
            time.advance_to(next).await;
        }
        self.instances[1].observe().await;
    }
    fn files_of_first(&self) -> BTreeMap<PathBuf, String> {
        let first = &self.instances[0];
        let mut files = BTreeMap::new();
        for path in [
            &first.data,
            &first.run.environment.account_root,
            &first.run.environment.temp,
        ] {
            record_files(path, &mut files);
        }
        files
    }
    pub fn first_files_unchanged(&self) {
        assert_eq!(
            self.files_of_first(),
            self.first_files,
            "FR-006 cancellation changes no file of the first instance"
        );
        self.preserved_locks();
    }
    pub fn keep_folders(&mut self) {
        let entries = self.instances[0].hub.entries();
        let temporary = PathBuf::from(
            entries
                .iter()
                .find(|e| e.event["hello"]["role"] == "server")
                .expect("real runtime Hello")
                .event["hello"]["cwd"]
                .as_str()
                .unwrap(),
        );
        let session = entries
            .iter()
            .filter(|e| e.event["kind"] == "got")
            .filter_map(|e| {
                serde_json::from_str::<serde_json::Value>(e.event["line"].as_str()?).ok()
            })
            .find(|v| v["method"] == "session/new")
            .map(|v| PathBuf::from(v["params"]["cwd"].as_str().unwrap()))
            .expect("real startup ACP session cwd");
        assert!(
            temporary.is_dir() && session.is_dir(),
            "existing first-instance folders"
        );
        fs::write(
            temporary.join("account-temporary-sentinel"),
            "first temporary folder",
        )
        .unwrap();
        fs::write(
            session.join("account-session-sentinel"),
            "first session cwd",
        )
        .unwrap();
        let session_file = self.instances[0]
            .data
            .join("runtime-home/antigravity-acp/conversations/account-sentinel.db");
        fs::create_dir_all(session_file.parent().unwrap()).unwrap();
        fs::write(&session_file, "first session file").unwrap();
        self.temporary = Some(temporary);
        self.session = Some(session);
        self.session_file = Some(session_file);
    }
    pub fn preserved(&self) {
        for (path, contents) in [
            (
                self.temporary
                    .as_ref()
                    .unwrap()
                    .join("account-temporary-sentinel"),
                "first temporary folder",
            ),
            (
                self.session
                    .as_ref()
                    .unwrap()
                    .join("account-session-sentinel"),
                "first session cwd",
            ),
            (self.session_file.clone().unwrap(), "first session file"),
        ] {
            assert_eq!(
                fs::read_to_string(&path).ok().as_deref(),
                Some(contents),
                "FR-006 first instance keeps its actual temporary/session folders and their contents: {}",
                path.display()
            );
        }
    }
    pub fn damage(&mut self, condition: &str) {
        let root = &self.instances[0].run.environment.account_root;
        for name in ["startup.lock", "instance.lock"] {
            let path = root.join(name);
            self.locks.push((path.clone(), identity(&path)));
        }
        self.failed_path = root.join("panel");
        fs::write(&self.failed_path, "damaged account publication").unwrap();
        if condition == "unreadable" {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let permissions = fs::metadata(&self.failed_path).unwrap().permissions();
                self.unreadable = Some((self.failed_path.clone(), permissions));
                fs::set_permissions(&self.failed_path, fs::Permissions::from_mode(0o000)).unwrap();
                assert_eq!(
                    fs::read(&self.failed_path).unwrap_err().kind(),
                    std::io::ErrorKind::PermissionDenied,
                    "actual unreadable coordination fixture"
                );
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                self.unreadable_handle = Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(0)
                        .open(&self.failed_path)
                        .unwrap(),
                );
                assert!(
                    fs::read(&self.failed_path).is_err(),
                    "actual sharing denial on panel record"
                );
            }
        }
    }
    pub fn preserved_locks(&self) {
        for (path, before) in &self.locks {
            assert!(
                before.is_some(),
                "FR-006 first owner must create both permanent lock files"
            );
            assert_eq!(
                &identity(path),
                before,
                "FR-006 account lock identity must not change"
            );
            assert_eq!(
                fs::metadata(path).unwrap().len(),
                0,
                "FR-006 account locks remain empty"
            );
        }
    }
}
impl Drop for AccountState {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            self.unreadable_handle.take();
        }
        if let Some((path, permissions)) = self.unreadable.take() {
            fs::set_permissions(path, permissions).unwrap();
        }
        // A duplicate can refer to the first data folder. Stop it before its owner is removed.
        while self.instances.pop().is_some() {}
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FileIdentity(u64, u64);
fn record_files(path: &std::path::Path, files: &mut BTreeMap<PathBuf, String>) {
    if path.is_dir() {
        files.insert(path.into(), "directory".into());
        for entry in fs::read_dir(path).unwrap() {
            record_files(&entry.unwrap().path(), files);
        }
    } else {
        let hash = Sha256::digest(fs::read(path).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        files.insert(path.into(), hash);
    }
}
fn identity(path: &std::path::Path) -> Option<FileIdentity> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => panic!("lock identity observation: {e}"),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata().unwrap();
        Some(FileIdentity(meta.dev(), meta.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        #[derive(Default)]
        struct Info {
            attributes: u32,
            created: [u32; 2],
            accessed: [u32; 2],
            written: [u32; 2],
            volume: u32,
            size_high: u32,
            size_low: u32,
            links: u32,
            index_high: u32,
            index_low: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Info) -> i32;
        }
        let mut info = Info::default();
        assert_ne!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) },
            0,
            "native lock identity"
        );
        Some(FileIdentity(
            u64::from(info.volume),
            (u64::from(info.index_high) << 32) | u64::from(info.index_low),
        ))
    }
}
