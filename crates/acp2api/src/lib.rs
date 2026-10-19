use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    future::Future,
    io,
    path::PathBuf,
    pin::Pin,
    process::{ExitCode, ExitStatus},
    sync::Arc,
    time::SystemTime,
};
use tokio::io::{AsyncRead, AsyncWrite};

mod conductor;
mod conversation;
mod dialect;
mod front;
mod messages;
pub mod os;
mod runtime_driver;
mod startup;
mod store;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type Moment = u64;
pub type ProcessRead = Pin<Box<dyn AsyncRead + Send>>;
pub type ProcessWrite = Pin<Box<dyn AsyncWrite + Send>>;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum HoldKind {
    HttpRequest,
    RuntimeLine,
    McpRequest,
    TurnEvent,
}
#[must_use = "keep the hold until its work finishes or waits again"]
pub struct Hold(Option<Box<dyn FnOnce() + Send>>);
impl Hold {
    pub fn empty() -> Self {
        Self(None)
    }
    pub fn on_drop(release: impl FnOnce() + Send + 'static) -> Self {
        Self(Some(Box::new(release)))
    }
}
impl Drop for Hold {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            release();
        }
    }
}
pub trait Clock: Send + Sync {
    fn now(&self) -> Moment;
    fn wall(&self) -> SystemTime;
    /// Keep the returned hold until work caused by the wake finishes or waits again.
    fn sleep_until(&self, at: Moment) -> BoxFuture<'static, Hold>;
    fn hold(&self, kind: HoldKind) -> Hold;
}
pub trait ProcessLauncher: Send + Sync {
    fn launch(&self, spec: LaunchSpec) -> Result<Launched, LaunchError>;
}
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub env_set: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
    pub own_temp: Option<PathBuf>,
}
pub struct Launched {
    pub stdin: ProcessWrite,
    pub stdout: ProcessRead,
    pub stderr: ProcessRead,
    pub tree: Box<dyn ProcessTree>,
}
pub trait ProcessTree: Send {
    fn pid(&self) -> u32;
    fn exited(&mut self) -> BoxFuture<'_, ExitStatus>;
    fn stop(self: Box<Self>, clock: Arc<dyn Clock>) -> BoxFuture<'static, ()>;
}
#[derive(Debug)]
pub enum LaunchError {
    Refused { program: PathBuf },
    Io(io::Error),
}
impl From<io::Error> for LaunchError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListedBuild {
    pub version: String,
    pub platform: String,
    pub agy_acp_server_sha256: String,
    pub localharness_external_sha256: String,
}
pub trait ReleaseFacts: Send + Sync {
    fn version(&self) -> &str;
    fn platform(&self) -> &str;
    fn listed_builds(&self) -> &[ListedBuild];
    fn highest_concurrency_limit(&self) -> u8;
}
pub trait Browser: Send + Sync {
    fn open(&self, panel: &str);
}
pub trait StopSignal: Send {
    fn stopped(&mut self) -> BoxFuture<'_, ()>;
}
pub struct Ports {
    pub clock: Arc<dyn Clock>,
    pub launcher: Arc<dyn ProcessLauncher>,
    pub release: Arc<dyn ReleaseFacts>,
    pub browser: Arc<dyn Browser>,
    pub stop: Box<dyn StopSignal>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub account_root: PathBuf,
    pub home: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub temp: PathBuf,
    pub path: Vec<PathBuf>,
    pub exe_dir: PathBuf,
}
impl Environment {
    pub fn from_process() -> Self {
        os::environment()
    }
}
impl Ports {
    pub fn production() -> Self {
        os::ports()
    }
}
pub fn run(ports: Ports, args: impl IntoIterator<Item = OsString>, env: Environment) -> ExitCode {
    let args = args.into_iter().collect();
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(startup::start(ports, args, env))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("acp2api: {error}");
            ExitCode::FAILURE
        }
    }
}
