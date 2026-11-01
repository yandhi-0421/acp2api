#[cfg(any(windows, test))]
use std::net::{IpAddr, Ipv4Addr};
use std::{
    io::Read,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
#[path = "network_observer/macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "network_observer/windows.rs"]
mod platform;
#[cfg(any(windows, test))]
mod records;

const LOG_CAP: u64 = 1024 * 1024;
#[cfg(any(windows, test))]
const RECORD_CAP: usize = 4096;
const COMMAND_CAP: Duration = Duration::from_secs(8);
const WINDOW_CAP: Duration = Duration::from_secs(90);
#[cfg(windows)]
const RECORD_WAIT: Duration = Duration::from_secs(10);
pub(crate) const FORBIDDEN: &str = "FR-046 outbound attempt outside 127.0.0.1";

#[cfg(any(windows, test))]
fn restore_options(options: u32) -> u32 {
    if options == 0 { 4 } else { options }
}

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq)]
struct Attempt {
    pid: u32,
    destination: IpAddr,
}
#[cfg(any(windows, test))]
fn only_loopback(pid: u32, attempts: Vec<Attempt>) -> Result<(), String> {
    if let Some(attempt) = attempts.into_iter().find(|attempt| {
        attempt.pid == pid && attempt.destination != IpAddr::V4(Ipv4Addr::LOCALHOST)
    }) {
        return Err(format!("{FORBIDDEN}: {attempt:?}"));
    }
    Ok(())
}
pub(crate) fn check_exit(status: ExitStatus) -> Result<(), String> {
    if !status.success() {
        return Err(format!("{FORBIDDEN}: host exited {status}"));
    }
    Ok(())
}

#[derive(Debug)]
pub struct Observer {
    #[cfg(windows)]
    capture: platform::Capture,
    started: Instant,
    #[cfg(windows)]
    before: u32,
}
impl Observer {
    pub fn begin() -> Result<Self, String> {
        let started = Instant::now();
        let program = crate::setup::binary("network-observer-control");
        #[cfg(target_os = "macos")]
        {
            control(platform::command(&program), "loopback")?;
            control(platform::command(&program), "short")?;
            Ok(Self { started })
        }
        #[cfg(windows)]
        {
            let capture = platform::Capture::begin()?;
            let before = control(capture.command(&program), "short")?;
            Ok(Self {
                capture,
                started,
                before,
            })
        }
    }
    pub fn command(&self, program: &Path) -> Command {
        #[cfg(target_os = "macos")]
        {
            platform::command(program)
        }
        #[cfg(windows)]
        {
            self.capture.command(program)
        }
    }
    pub fn finish(self, pid: u32, status: ExitStatus) -> Result<(), String> {
        let result = self.collect(pid, status);
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
        #[cfg(target_os = "macos")]
        result
    }
    fn collect(&self, pid: u32, status: ExitStatus) -> Result<(), String> {
        if self.started.elapsed() > WINDOW_CAP {
            return Err("network observation exceeded 90 seconds".into());
        }
        let after = control(
            self.command(&crate::setup::binary("network-observer-control")),
            "failed",
        )?;
        check_exit(status)?;
        #[cfg(target_os = "macos")]
        {
            let _ = (pid, after);
            Ok(())
        }
        #[cfg(windows)]
        {
            let pids = [self.before, pid, after];
            let deadline = Instant::now() + RECORD_WAIT;
            loop {
                let attempts = self.capture.read(&pids)?;
                if [self.before, after].iter().all(|pid| {
                    attempts.iter().any(|attempt| {
                        attempt.pid == *pid
                            && attempt.destination != IpAddr::V4(Ipv4Addr::LOCALHOST)
                    })
                }) {
                    if !attempts.iter().any(|attempt| attempt.pid == pid) {
                        return Err(
                            "no outbound audit evidence for the observed Windows PID".into()
                        );
                    }
                    return only_loopback(pid, attempts);
                }
                if Instant::now() >= deadline {
                    return Err(
                        "network observer record gap: missing bracketing control PID".into(),
                    );
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}
fn control(mut command: Command, mode: &str) -> Result<u32, String> {
    command.arg(mode);
    #[cfg(target_os = "macos")]
    let completed = {
        use std::os::unix::process::ExitStatusExt;
        let completed = run(command)?;
        if mode != "loopback" {
            if completed.status.signal() != Some(libc::SIGKILL) {
                return Err(format!(
                    "macOS {mode} control did not die by SIGKILL: {}",
                    completed.status
                ));
            }
            return Ok(completed.pid);
        }
        if !completed.status.success() {
            return Err(format!(
                "macOS loopback control failed: {}",
                completed.status
            ));
        }
        completed
    };
    #[cfg(windows)]
    let completed = output(command)?;
    let report: serde_json::Value =
        serde_json::from_slice(&completed.stdout).map_err(|e| format!("control output: {e}"))?;
    if report["mode"] != mode {
        return Err("control did not complete the selected mode".into());
    }
    Ok(completed.pid)
}

struct Completed {
    pid: u32,
    status: ExitStatus,
    stdout: Vec<u8>,
}
#[cfg(windows)]
fn output(command: Command) -> Result<Completed, String> {
    let completed = run(command)?;
    if !completed.status.success() {
        return Err(format!(
            "network observer command failed ({})",
            completed.status
        ));
    }
    Ok(completed)
}
fn run(mut command: Command) -> Result<Completed, String> {
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
                if !stderr.is_empty() {
                    return Err(format!(
                        "network observer command failed ({status}): {}",
                        String::from_utf8_lossy(&stderr)
                    ));
                }
                return Ok(Completed {
                    pid,
                    status,
                    stdout,
                });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_auditing_uses_an_explicit_restore_value() {
        for (saved, restored) in [(0, 4), (4, 4), (1, 1), (2, 2), (3, 3)] {
            assert_eq!(restore_options(saved), restored);
        }
    }
    #[cfg(unix)]
    #[test]
    fn a_killed_host_fails_the_destination_check() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(check_exit(ExitStatus::from_raw(0)), Ok(()));
        assert_eq!(
            check_exit(ExitStatus::from_raw(libc::SIGKILL)).unwrap_err(),
            "FR-046 outbound attempt outside 127.0.0.1: host exited signal: 9 (SIGKILL)"
        );
    }
}
