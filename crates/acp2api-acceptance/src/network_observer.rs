use std::{
    io::Read,
    net::{IpAddr, Ipv4Addr},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
#[path = "network_observer/macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "network_observer/windows.rs"]
mod platform;
mod records;

const LOG_CAP: u64 = 1024 * 1024;
const RECORD_CAP: usize = 4096;
const COMMAND_CAP: Duration = Duration::from_secs(8);
const WINDOW_CAP: Duration = Duration::from_secs(90);

#[derive(Debug, PartialEq)]
enum Destination {
    Address(IpAddr),
    #[cfg(any(target_os = "macos", test))]
    SandboxDeny,
}
#[derive(Debug, PartialEq)]
struct Attempt {
    pid: u32,
    destination: Destination,
}
impl Attempt {
    fn forbidden(&self) -> bool {
        self.destination != Destination::Address(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }
}

#[derive(Debug)]
pub struct Observation(Vec<Attempt>);
impl Observation {
    fn select(pid: u32, attempts: Vec<Attempt>) -> Self {
        Self(attempts.into_iter().filter(|a| a.pid == pid).collect())
    }
    pub fn only_loopback(&self) -> Result<(), String> {
        if let Some(attempt) = self.0.iter().find(|attempt| attempt.forbidden()) {
            return Err(format!(
                "FR-046 outbound attempt outside 127.0.0.1: {attempt:?}"
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Observer {
    capture: platform::Capture,
    started: Instant,
    before: u32,
}
impl Observer {
    pub fn begin() -> Result<Self, String> {
        let started = Instant::now();
        let capture = platform::Capture::begin()?;
        let before = control(&capture, "short")?;
        Ok(Self {
            capture,
            started,
            before,
        })
    }
    pub fn command(&self, program: &Path) -> Command {
        self.capture.command(program)
    }
    pub fn finish(self, pid: u32) -> Result<Observation, String> {
        let result = self.collect(pid);
        #[cfg(windows)]
        {
            let mut capture = self.capture;
            let restored = capture.restore();
            match (result, restored) {
                (result, Ok(())) => result,
                (Ok(_), Err(error)) => Err(error),
                (Err(error), Err(restore)) => Err(format!("{error}; {restore}")),
            }
        }
        #[cfg(not(windows))]
        result
    }
    fn collect(&self, pid: u32) -> Result<Observation, String> {
        if self.started.elapsed() > WINDOW_CAP {
            return Err("network observation exceeded 90 seconds".into());
        }
        let after = control(&self.capture, "failed")?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let attempts = self.capture.read()?;
            if [self.before, after]
                .iter()
                .all(|pid| attempts.iter().any(|a| a.pid == *pid && a.forbidden()))
            {
                let selected = Observation::select(pid, attempts);
                #[cfg(windows)]
                if selected.0.is_empty() {
                    return Err("no outbound audit evidence for the observed Windows PID".into());
                }
                return Ok(selected);
            }
            if Instant::now() >= deadline {
                return Err("network observer record gap: missing bracketing control PID".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
fn control(capture: &platform::Capture, mode: &str) -> Result<u32, String> {
    let mut command = capture.command(&crate::setup::binary("network-observer-control"));
    command.arg(mode);
    let completed = output(command)?;
    let report: serde_json::Value =
        serde_json::from_slice(&completed.stdout).map_err(|e| format!("control output: {e}"))?;
    if report["attempts"] != 1 || report["mode"] != mode {
        return Err("control did not complete its single attempt".into());
    }
    Ok(completed.pid)
}

struct Completed {
    pid: u32,
    stdout: Vec<u8>,
}
fn output(mut command: Command) -> Result<Completed, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("network observer command start: {e}"))?;
    let pid = child.id();
    let (send, receive) = mpsc::channel();
    for (stderr, pipe) in [
        (
            false,
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
        ),
        (
            true,
            Box::new(child.stderr.take().unwrap()) as Box<dyn Read + Send>,
        ),
    ] {
        let send = send.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = pipe.take(LOG_CAP + 1).read_to_end(&mut bytes);
            let _ = send.send((stderr, result, bytes));
        });
    }
    drop(send);
    let deadline = Instant::now() + COMMAND_CAP;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut readers = 0;
    let result = (|| {
        loop {
            if Instant::now() >= deadline {
                return Err("network observer command exceeded 8 seconds".into());
            }
            match receive.recv_timeout(Duration::from_millis(5)) {
                Ok((is_stderr, result, bytes)) => {
                    readers += 1;
                    result.map_err(|e| format!("observer command pipe: {e}"))?;
                    if bytes.len() as u64 > LOG_CAP {
                        return Err("network observer output exceeded 1 MiB".into());
                    }
                    if is_stderr {
                        stderr = bytes;
                    } else {
                        stdout = bytes;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {}
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())?
                && readers == 2
            {
                if !status.success() || !stderr.is_empty() {
                    return Err(format!(
                        "network observer command failed ({status}): {}",
                        String::from_utf8_lossy(&stderr)
                    ));
                }
                return Ok(Completed { pid, stdout });
            }
        }
    })();
    if child.try_wait().map_err(|e| e.to_string())?.is_none() {
        child
            .kill()
            .map_err(|e| format!("observer child cleanup: {e}"))?;
        child.wait().map_err(|e| e.to_string())?;
    }
    result
}
