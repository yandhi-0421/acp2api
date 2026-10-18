use crate::hub::HubLink;
use acp2api::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::{self, BufRead},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
#[derive(Clone)]
pub struct HarnessClock {
    link: HubLink,
}
impl HarnessClock {
    pub fn new(link: HubLink) -> Self {
        Self { link }
    }
}
impl Clock for HarnessClock {
    fn now(&self) -> Moment {
        self.link.call(json!({"op":"now"}))["now"].as_u64().unwrap()
    }
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(1_792_281_600_000 + self.now())
    }
    fn sleep_until(&self, at: Moment) -> BoxFuture<'static, ()> {
        let id = self.link.call(json!({"op":"sleep","at":at}))["id"]
            .as_u64()
            .unwrap();
        let mut reader = self
            .link
            .connect(json!({"op":"wait","id":id}))
            .expect("timer registration");
        let socket = reader.get_ref().try_clone().unwrap();
        let link = self.link.clone();
        let (send, recv) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = reader.read_line(&mut line);
            let _ = send.send(result.is_ok() && !line.is_empty());
        });
        Box::pin(Sleep {
            id,
            link,
            socket,
            recv,
            ready: false,
        })
    }
    fn hold(&self, kind: HoldKind) -> Hold {
        let kind = format!("{kind:?}");
        let link = self.link.clone();
        link.call(json!({"op":"hold","kind":kind,"add":true}));
        Hold::on_drop(move || {
            link.call(json!({"op":"hold","kind":kind,"add":false}));
        })
    }
}
struct Sleep {
    id: u64,
    link: HubLink,
    socket: std::net::TcpStream,
    recv: tokio::sync::oneshot::Receiver<bool>,
    ready: bool,
}
impl std::future::Future for Sleep {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match Pin::new(&mut self.recv).poll(cx) {
            Poll::Ready(Ok(true)) => {
                self.ready = true;
                Poll::Ready(())
            }
            Poll::Ready(_) => panic!("timer hub closed"),
            Poll::Pending => Poll::Pending,
        }
    }
}
impl Drop for Sleep {
    fn drop(&mut self) {
        self.link
            .call(json!({"op":"sleep_done","id":self.id,"ready":self.ready}));
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
    }
}
pub struct GuardedLauncher {
    inner: acp2api::os::OsLauncher,
    hash: Vec<u8>,
    link: HubLink,
}
impl GuardedLauncher {
    pub fn new(fake_program: &std::path::Path, link: HubLink) -> Self {
        Self {
            inner: acp2api::os::OsLauncher,
            hash: Sha256::digest(std::fs::read(fake_program).expect("fake executable")).to_vec(),
            link,
        }
    }
}
impl ProcessLauncher for GuardedLauncher {
    fn launch(&self, mut spec: LaunchSpec) -> Result<Launched, LaunchError> {
        let allowed = std::fs::read(&spec.program)
            .map(|bytes| Sha256::digest(bytes).as_slice() == self.hash)
            .unwrap_or(false);
        self.link
            .call(json!({"op":"launch","allowed":allowed,"program":spec.program}));
        if !allowed {
            return Err(LaunchError::Refused {
                program: spec.program,
            });
        }
        spec.env_set.push((
            "ACP2API_PARENT_PID".into(),
            std::process::id().to_string().into(),
        ));
        let mut launched = self.inner.launch(spec)?;
        launched.stdin = Box::pin(CountWrite {
            inner: launched.stdin,
            link: self.link.clone(),
        });
        launched.stdout = Box::pin(CountRead {
            inner: launched.stdout,
            link: self.link.clone(),
        });
        launched.stderr = Box::pin(CountRead {
            inner: launched.stderr,
            link: self.link.clone(),
        });
        Ok(launched)
    }
}
struct CountRead {
    inner: ProcessRead,
    link: HubLink,
}
impl AsyncRead for CountRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let result = self.inner.as_mut().poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = result {
            let lines = buf.filled()[before..]
                .iter()
                .filter(|b| **b == b'\n')
                .count();
            if lines > 0 {
                self.link
                    .call(json!({"op":"io","direction":"stdout","lines":lines}));
            }
        }
        result
    }
}
struct CountWrite {
    inner: ProcessWrite,
    link: HubLink,
}
impl AsyncWrite for CountWrite {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = self.inner.as_mut().poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = result {
            let lines = buf[..n].iter().filter(|b| **b == b'\n').count();
            if lines > 0 {
                self.link
                    .call(json!({"op":"io","direction":"stdin","lines":lines}));
            }
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.inner.as_mut().poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.inner.as_mut().poll_shutdown(cx)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunFileReleaseFacts {
    pub version: String,
    pub platform: String,
    pub builds: Vec<ListedBuild>,
    pub highest: u8,
}
impl ReleaseFacts for RunFileReleaseFacts {
    fn version(&self) -> &str {
        &self.version
    }
    fn platform(&self) -> &str {
        &self.platform
    }
    fn listed_builds(&self) -> &[ListedBuild] {
        &self.builds
    }
    fn highest_concurrency_limit(&self) -> u8 {
        self.highest
    }
}
pub struct RecordingBrowser(pub HubLink);
impl Browser for RecordingBrowser {
    fn open(&self, panel: &str) {
        self.0.call(json!({"op":"browser","url":panel}));
    }
}
pub struct HubStop(pub HubLink);
impl StopSignal for HubStop {
    fn stopped(&mut self) -> BoxFuture<'_, ()> {
        let link = self.0.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let mut reader = link
                    .connect(json!({"op":"stop_wait"}))
                    .expect("stop registration");
                let mut line = String::new();
                reader.read_line(&mut line).expect("stop signal");
            })
            .await
            .unwrap();
        })
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct RunFile {
    pub hub: HubLink,
    pub fake_program: std::path::PathBuf,
    pub release: RunFileReleaseFacts,
    pub environment: Environment,
}
impl RunFile {
    pub fn read_from_env() -> Self {
        let path = std::env::var_os("ACP2API_TEST_RUN").expect("test run path");
        toml::from_str(&std::fs::read_to_string(path).expect("run file")).expect("run TOML")
    }
    pub fn ports(&self) -> Ports {
        Ports {
            clock: Arc::new(HarnessClock::new(self.hub.clone())),
            launcher: Arc::new(GuardedLauncher::new(&self.fake_program, self.hub.clone())),
            release: Arc::new(self.release.clone()),
            browser: Arc::new(RecordingBrowser(self.hub.clone())),
            stop: Box::new(HubStop(self.hub.clone())),
        }
    }
}
