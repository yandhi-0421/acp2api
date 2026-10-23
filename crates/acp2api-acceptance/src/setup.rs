use crate::{
    adapters::{RunFile, RunFileReleaseFacts},
    hub::{Hub, HubLink},
};
use acp2api::{Environment, ListedBuild};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
type SetupEdit = std::sync::Arc<dyn Fn(&TestRoot, &Hub, &mut RunFile) + Send + Sync>;
#[derive(Clone)]
pub struct Setup {
    pub model: String,
    edits: Vec<SetupEdit>,
}
impl std::fmt::Debug for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup")
            .field("model", &self.model)
            .field("edits", &self.edits.len())
            .finish()
    }
}
impl Setup {
    pub fn configure(
        mut self,
        edit: impl Fn(&TestRoot, &Hub, &mut RunFile) + Send + Sync + 'static,
    ) -> Self {
        self.edits.push(std::sync::Arc::new(edit));
        self
    }
    pub fn highest_concurrency_limit(self, limit: u8) -> Self {
        assert!((1..=3).contains(&limit));
        self.configure(move |_, _, run| run.release.highest = limit)
    }
    pub fn normal() -> Self {
        Self {
            model: "gemini-3.8-flash".into(),
            edits: Vec::new(),
        }
    }
}
pub struct TestRoot {
    pub path: PathBuf,
    pub key: String,
    pub password: String,
    pub port: u16,
}
impl std::fmt::Debug for TestRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestRoot")
            .field("path", &self.path)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}
impl TestRoot {
    pub fn new() -> Self {
        let runs = scenario_roots();
        cleanup_stale_runs(&runs);
        let run = runs.join(std::process::id().to_string());
        fs::create_dir_all(&run).unwrap();
        fs::write(run.join(".acp2api-run"), "acp2api-scenario-run-v1\n").unwrap();
        let path = run.join(random_hex(12));
        for child in [
            "account",
            "home",
            "local-app-data",
            "temp",
            "path",
            "exe",
            "data",
            "builds",
        ] {
            fs::create_dir_all(path.join(child)).unwrap();
        }
        let lease = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = lease.local_addr().unwrap().port();
        Self {
            path,
            key: format!("acp2api-{}", base64(&random(32), true)),
            password: random_password(),
            port,
        }
    }
    pub fn seed(&self, hub: &Hub, setup: &Setup) -> RunFile {
        let fake = binary("fake-acp-agent");
        let program = self
            .path
            .join("data/official-runtime")
            .join(if cfg!(windows) {
                "agy_acp_server.exe"
            } else {
                "agy_acp_server.par"
            });
        fs::create_dir_all(program.parent().unwrap()).unwrap();
        fs::copy(&fake, &program).unwrap();
        let descriptor = program.parent().unwrap().join(if cfg!(windows) {
            "localharness_external.exe"
        } else {
            "localharness_external"
        });
        let descriptor_text = format!(
            "marker = \"acp2api-local-fake-v1\"\nversion = \"1.4.0\"\nplatform = \"{}\"\nlabel = \"normal\"\nhub = \"{}\"\ntoken = \"{}\"\n",
            platform(),
            hub.address,
            hub.token
        );
        fs::write(&descriptor, &descriptor_text).unwrap();
        let build_folder = self.path.join("builds/normal");
        fs::create_dir_all(&build_folder).unwrap();
        fs::copy(&program, build_folder.join(program.file_name().unwrap())).unwrap();
        fs::copy(
            &descriptor,
            build_folder.join(descriptor.file_name().unwrap()),
        )
        .unwrap();
        let salt = random(16);
        let mut hash = [0u8; 32];
        argon2::Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(19456, 2, 1, Some(32)).unwrap(),
        )
        .hash_password_into(self.password.as_bytes(), &salt, &mut hash)
        .unwrap();
        let password_hash = format!(
            "$argon2id$v=19$m=19456,t=2,p=1${}${}",
            base64(&salt, false),
            base64(&hash, false)
        );
        let config = format!(
            "port = {}\nconcurrency_limit = 2\nlocal_api_key = \"{}\"\nmanagement_password_hash = \"{}\"\n",
            self.port, self.key, password_hash
        );
        fs::write(self.path.join("data/config.toml"), config).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                self.path.join("data/config.toml"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        let mut run = RunFile {
            hub: HubLink::new(hub),
            fake_program: fake,
            release: RunFileReleaseFacts {
                version: env!("CARGO_PKG_VERSION").into(),
                platform: platform().into(),
                highest: 3,
                builds: vec![ListedBuild {
                    version: "1.4.0".into(),
                    platform: platform().into(),
                    agy_acp_server_sha256: hex(&Sha256::digest(fs::read(program).unwrap())),
                    localharness_external_sha256: hex(&Sha256::digest(descriptor_text.as_bytes())),
                }],
            },
            environment: Environment {
                account_root: self.path.join("account"),
                home: Some(self.path.join("home")),
                local_app_data: Some(self.path.join("local-app-data")),
                temp: self.path.join("temp"),
                path: vec![self.path.join("path")],
                exe_dir: self.path.join("exe"),
            },
        };
        for edit in &setup.edits {
            edit(self, hub, &mut run);
        }
        run
    }
    pub fn check_size(&self) {
        assert!(
            directory_size(&self.path) <= 128 * 1024 * 1024,
            "test root cap: 128 MiB"
        );
    }
    pub fn cleanup(&self) {
        self.check_size();
        fs::remove_dir_all(&self.path).unwrap();
    }
}
impl Default for TestRoot {
    fn default() -> Self {
        Self::new()
    }
}
fn directory_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            let meta = e.metadata().unwrap();
            if meta.is_dir() {
                directory_size(&e.path())
            } else {
                meta.len()
            }
        })
        .sum()
}
pub fn binary(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap();
    let dir = if dir.file_name().is_some_and(|n| n == "deps") {
        dir.parent().unwrap()
    } else {
        dir
    };
    dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    })
}
pub fn random_hex(n: usize) -> String {
    hex(&random(n))
}
fn random(n: usize) -> Vec<u8> {
    let mut bytes = vec![0; n];
    getrandom::fill(&mut bytes).unwrap();
    bytes
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn base64(bytes: &[u8], url: bool) -> String {
    let alphabet = if url {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    } else {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    };
    let mut result = String::new();
    let mut bits = 0u32;
    let mut count = 0;
    for b in bytes {
        bits = (bits << 8) | u32::from(*b);
        count += 8;
        while count >= 6 {
            count -= 6;
            result.push(alphabet[((bits >> count) & 63) as usize] as char);
        }
    }
    if count > 0 {
        result.push(alphabet[((bits << (6 - count)) & 63) as usize] as char);
    }
    result
}
fn platform() -> &'static str {
    if cfg!(windows) {
        if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else {
            "windows-x64"
        }
    } else if cfg!(target_arch = "aarch64") {
        "macos-arm64"
    } else {
        "macos-x64"
    }
}

fn random_password() -> String {
    let alphabet = b"abcdefghijklmnopqrstuvwxyz23456789";
    let text: String = random(20)
        .iter()
        .map(|b| alphabet[usize::from(*b) % alphabet.len()] as char)
        .collect();
    text.as_bytes()
        .chunks(5)
        .map(|p| std::str::from_utf8(p).unwrap())
        .collect::<Vec<_>>()
        .join("-")
}

fn scenario_roots() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .join("scenario-roots")
}
fn cleanup_stale_runs(root: &Path) {
    if !fs::symlink_metadata(root).is_ok_and(|meta| meta.is_dir() && !meta.is_symlink()) {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
            .filter(|pid| *pid > 0 && *pid <= i32::MAX as u32)
        else {
            continue;
        };
        if !entry
            .file_type()
            .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
        {
            continue;
        }
        let path = entry.path();
        let marker = path.join(".acp2api-run");
        let owned = fs::symlink_metadata(&marker)
            .is_ok_and(|meta| meta.is_file() && !meta.is_symlink() && meta.len() == 24)
            && fs::read_to_string(marker).is_ok_and(|text| text == "acp2api-scenario-run-v1\n");
        if owned && definitely_exited(pid) {
            // remove_dir_all does not follow links inside an owned run.
            match fs::remove_dir_all(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("stale test run cleanup failed: {error}"),
            }
        }
    }
}
#[cfg(unix)]
fn definitely_exited(pid: u32) -> bool {
    (unsafe { libc::kill(pid as i32, 0) != 0 })
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}
#[cfg(windows)]
fn definitely_exited(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return GetLastError() == ERROR_INVALID_PARAMETER;
        }
        let exited = WaitForSingleObject(process, 0) == WAIT_OBJECT_0;
        CloseHandle(process);
        exited
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn new_root_cleans_only_terminated_owned_runs() {
        let active = TestRoot::new();
        let runs = active.path.parent().unwrap().parent().unwrap();
        let mut command = if cfg!(windows) {
            let mut command = std::process::Command::new("cmd");
            command.args(["/C", "exit", "0"]);
            command
        } else {
            std::process::Command::new("/usr/bin/true")
        };
        let mut child = command.spawn().unwrap();
        let pid = child.id();
        let deadline = Instant::now() + Duration::from_secs(3);
        while child.try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "cleanup fixture process cap: 3 s"
            );
            std::thread::yield_now();
        }
        let stale = runs.join(pid.to_string());
        assert!(!stale.exists(), "fixture must not replace previous output");
        fs::create_dir(&stale).unwrap();
        fs::write(stale.join(".acp2api-run"), "acp2api-scenario-run-v1\n").unwrap();
        fs::write(stale.join("sentinel"), "owned interrupted run").unwrap();
        let unknown = runs.join(format!("unknown-{}", random_hex(12)));
        fs::create_dir(&unknown).unwrap();
        fs::write(unknown.join("sentinel"), "unknown output").unwrap();
        let outside = active.path.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), "outside output").unwrap();
        #[cfg(unix)]
        let link = {
            let link = runs.join(format!("link-{}", random_hex(12)));
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            link
        };
        let next = TestRoot::new();
        assert!(
            !stale.exists(),
            "default root startup must remove stale owned output"
        );
        assert!(active.path.is_dir(), "active run must remain");
        assert!(
            unknown.join("sentinel").is_file(),
            "unknown output must remain"
        );
        assert!(
            outside.join("sentinel").is_file(),
            "outside output must remain"
        );
        #[cfg(unix)]
        {
            assert!(link.is_symlink());
            fs::remove_file(link).unwrap();
        }
        fs::remove_dir_all(unknown).unwrap();
        next.cleanup();
        active.cleanup();
    }
}
