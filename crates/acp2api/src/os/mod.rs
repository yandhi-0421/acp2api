#![allow(clippy::disallowed_methods)]
use crate::*;
use std::{
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;

pub struct SystemClock {
    epoch: Instant,
}
impl Default for SystemClock {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }
}
impl Clock for SystemClock {
    fn now(&self) -> Moment {
        self.epoch
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }
    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
    fn sleep_until(&self, at: Moment) -> BoxFuture<'static, ()> {
        if at == u64::MAX {
            return Box::pin(std::future::pending());
        }
        let remaining = at.saturating_sub(self.now());
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(remaining)).await;
        })
    }
    fn hold(&self, _: HoldKind) -> Hold {
        Hold::empty()
    }
}
pub struct OsLauncher;
#[cfg(windows)]
mod windows;
struct OsTree {
    child: tokio::process::Child,
    #[cfg(windows)]
    job: windows::Job,
}
impl ProcessLauncher for OsLauncher {
    fn launch(&self, spec: LaunchSpec) -> Result<Launched, LaunchError> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(spec.args)
            .envs(spec.env_set)
            .current_dir(spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        cmd.process_group(0);
        if let Some(temp) = spec.own_temp {
            cmd.env("TEMP", &temp).env("TMP", temp);
        }
        let mut child = cmd.spawn()?;
        Ok(Launched {
            stdin: Box::pin(child.stdin.take().expect("piped stdin")),
            stdout: Box::pin(child.stdout.take().expect("piped stdout")),
            stderr: Box::pin(child.stderr.take().expect("piped stderr")),
            tree: Box::new(OsTree {
                #[cfg(windows)]
                job: windows::Job::assign(&child)?,
                child,
            }),
        })
    }
}
impl ProcessTree for OsTree {
    fn pid(&self) -> u32 {
        self.child.id().unwrap_or(0)
    }
    fn exited(&mut self) -> BoxFuture<'_, ExitStatus> {
        Box::pin(async { self.child.wait().await.expect("wait child") })
    }
    fn stop(mut self: Box<Self>, clock: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            #[cfg(windows)]
            {
                drop(self.job);
                let _ = self.child.wait().await;
                let _ = clock;
            }
            #[cfg(unix)]
            {
                let pid = self.pid();
                tokio::select! { _ = self.child.wait() => {}, _ = clock.sleep_until(clock.now().saturating_add(5000)) => {
                    #[cfg(unix)] if pid > 0 { unsafe { libc::killpg(pid as i32, libc::SIGKILL); } }
                    let _ = self.child.kill().await;
                    let _ = self.child.wait().await;
                } }
                if pid > 0 {
                    unsafe {
                        libc::killpg(pid as i32, libc::SIGKILL);
                    }
                }
            }
        })
    }
}
pub struct CompiledReleaseFacts {
    builds: Vec<ListedBuild>,
    highest: u8,
}
impl Default for CompiledReleaseFacts {
    fn default() -> Self {
        let value: toml::Value =
            toml::from_str(include_str!("../../release/builds.toml")).expect("release facts");
        let highest = value["highest_concurrency_limit"][platform()]
            .as_integer()
            .expect("platform limit") as u8;
        Self {
            builds: Vec::new(),
            highest,
        }
    }
}
fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
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
impl ReleaseFacts for CompiledReleaseFacts {
    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
    fn platform(&self) -> &str {
        platform()
    }
    fn listed_builds(&self) -> &[ListedBuild] {
        &self.builds
    }
    fn highest_concurrency_limit(&self) -> u8 {
        self.highest
    }
}
pub struct SystemBrowser;
impl Browser for SystemBrowser {
    fn open(&self, panel: &str) {
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("/usr/bin/open")
                .arg(panel)
                .spawn();
        }
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("rundll32.exe")
                .args(["url.dll,FileProtocolHandler", panel])
                .spawn();
        }
    }
}
pub struct TerminalStop;
impl StopSignal for TerminalStop {
    fn stopped(&mut self) -> BoxFuture<'_, ()> {
        Box::pin(async {
            let _ = tokio::signal::ctrl_c().await;
        })
    }
}
pub fn ports() -> Ports {
    Ports {
        clock: Arc::new(SystemClock::default()),
        launcher: Arc::new(OsLauncher),
        release: Arc::new(CompiledReleaseFacts::default()),
        browser: Arc::new(SystemBrowser),
        stop: Box::new(TerminalStop),
    }
}
pub fn environment() -> Environment {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    #[cfg(unix)]
    let account_root = unsafe {
        let entry = libc::getpwuid(libc::getuid());
        assert!(!entry.is_null(), "native account lookup failed");
        PathBuf::from(
            std::ffi::CStr::from_ptr((*entry).pw_dir)
                .to_string_lossy()
                .into_owned(),
        )
        .join("Library/Application Support/acp2api-account")
    };
    #[cfg(windows)]
    let account_root = windows::account_root();
    Environment {
        account_root,
        home,
        local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        temp: std::env::temp_dir(),
        path: std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect(),
        exe_dir: std::env::current_exe()
            .expect("executable")
            .parent()
            .expect("parent")
            .to_owned(),
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn release_facts_parse() {
        let facts = super::CompiledReleaseFacts::default();
        assert_eq!(facts.highest, 3);
    }
}
