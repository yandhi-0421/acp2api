use super::{Attempt, COMMAND_CAP, LOG_CAP, RECORD_CAP, records};
use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
};

const PROFILE: &str = "(version 1)(allow default)(deny network-outbound (remote ip \"*:*\"))(allow network-outbound (remote ip \"localhost:*\"))";
const PREDICATE: &str = "eventMessage CONTAINS[c] 'network-outbound' OR eventType == 'lossEvent' OR (eventMessage BEGINSWITH 'Sandbox:' AND eventMessage CONTAINS[c] 'suppressed') OR (process == 'logd' AND (eventMessage CONTAINS[c] 'dropped' OR eventMessage CONTAINS[c] 'lost'))";
#[derive(Debug)]
pub(super) struct Capture {
    child: Mutex<Child>,
    bytes: Arc<Mutex<Result<Vec<u8>, String>>>,
    readers: Vec<JoinHandle<()>>,
}
impl Capture {
    pub fn begin() -> Result<Self, String> {
        let mut command = Command::new("/usr/bin/log");
        command.args([
            "stream",
            "--style",
            "ndjson",
            "--info",
            "--debug",
            "--timeout",
            "2m",
            "--predicate",
            PREDICATE,
        ]);
        Self::start(command)
    }
    fn start(mut command: Command) -> Result<Self, String> {
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("log stream start: {e}"))?;
        let mut capture = Self {
            child: Mutex::new(child),
            bytes: Arc::new(Mutex::new(Ok(Vec::new()))),
            readers: Vec::new(),
        };
        let child = capture.child.get_mut().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (ready, receive) = mpsc::channel();
        let bytes = Arc::clone(&capture.bytes);
        let stdout_ready = ready.clone();
        capture.readers.push(thread::spawn(move || {
            if let Err(error) = read_stdout(stdout, &bytes, &stdout_ready) {
                *bytes.lock().unwrap() = Err(error.clone());
                let _ = stdout_ready.send(Err(error));
            }
        }));
        let bytes = Arc::clone(&capture.bytes);
        capture.readers.push(thread::spawn(move || {
            let mut chunk = [0; 8192];
            let error = match stderr.read(&mut chunk) {
                Ok(0) => return,
                Ok(size) => format!(
                    "log stream stderr: {}",
                    String::from_utf8_lossy(&chunk[..size])
                ),
                Err(error) => format!("log stream stderr read: {error}"),
            };
            *bytes.lock().unwrap() = Err(error.clone());
            let _ = ready.send(Err(error));
        }));
        receive
            .recv_timeout(COMMAND_CAP)
            .map_err(|e| format!("log stream header within 8 seconds: {e}"))??;
        capture.read()?;
        Ok(capture)
    }
    pub fn command(&self, program: &Path) -> Command {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.args(["-p", PROFILE]).arg(program);
        command
    }
    pub fn read(&self) -> Result<Vec<Attempt>, String> {
        let bytes = self.bytes.lock().unwrap();
        let bytes = bytes.as_ref().map_err(Clone::clone)?;
        if let Some(status) = self
            .child
            .lock()
            .unwrap()
            .try_wait()
            .map_err(|e| format!("log stream status: {e}"))?
        {
            return Err(format!("log stream exited early: {status}"));
        }
        records::macos(bytes)
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let child = self.child.get_mut().unwrap();
        let kill = child.kill();
        child
            .wait()
            .unwrap_or_else(|error| panic!("log stream cleanup: kill={kill:?}; wait={error}"));
        for reader in self.readers.drain(..) {
            reader.join().expect("join log stream reader");
        }
    }
}
fn read_stdout(
    stdout: impl Read,
    bytes: &Mutex<Result<Vec<u8>, String>>,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    let mut stdout = BufReader::new(stdout.take(LOG_CAP + 1));
    let mut line = Vec::new();
    let mut total = stdout
        .read_until(b'\n', &mut line)
        .map_err(|e| format!("log stream header read: {e}"))?;
    if total as u64 > LOG_CAP {
        return Err("log stream exceeds 1 MiB".into());
    }
    if !line.starts_with(b"Filtering the log data using \"") || !line.ends_with(b"\"\n") {
        return Err("log stream missing filtering header".into());
    }
    let _ = ready.send(Ok(()));
    let mut records = 0;
    loop {
        line.clear();
        let count = stdout
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("log stream stdout read: {e}"))?;
        total += count;
        if total as u64 > LOG_CAP {
            return Err("log stream exceeds 1 MiB".into());
        }
        if count == 0 || !line.ends_with(b"\n") {
            return Err("log stream stdout ended".into());
        }
        if records == RECORD_CAP {
            return Err("log stream exceeds 4096 records".into());
        }
        records += 1;
        bytes
            .lock()
            .unwrap()
            .as_mut()
            .map_err(|e| e.clone())?
            .extend_from_slice(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        time::{Duration, Instant},
    };

    const HEADER: &str = "Filtering the log data using \"test\"\n";

    fn command(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    fn assert_reaped(pid: u32) {
        let waited = unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) };
        let error = std::io::Error::last_os_error().raw_os_error();
        if waited == 0 {
            // Clean up the planted leak before the regression assertion fails.
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
                libc::waitpid(pid as i32, std::ptr::null_mut(), 0);
            }
        }
        assert_eq!((waited, error), (-1, Some(libc::ECHILD)));
    }

    #[test]
    fn drop_reaps_the_stream_on_normal_return_and_panic() {
        for panic in [false, true] {
            let capture = Capture::start(command(
                "printf 'Filtering the log data using \"test\"\n'; exec /bin/sleep 30",
            ))
            .unwrap();
            let pid = capture.child.lock().unwrap().id();
            assert_eq!(unsafe { libc::kill(pid as i32, 0) }, 0);
            let result = catch_unwind(AssertUnwindSafe(move || {
                let _capture = capture;
                assert!(!panic, "planted scenario panic");
            }));
            assert_eq!(result.is_err(), panic);
            assert_reaped(pid);
        }
    }

    #[test]
    fn stream_reads_retain_the_denial_across_polls() {
        let capture = Capture::start(command(
            "printf 'Filtering the log data using \"test\"\n'; printf '%s\n' '{\"eventType\":\"logEvent\",\"eventMessage\":\"Sandbox: Python(72753) deny(1) network-outbound remote:*:9\"}'; exec /bin/sleep 30",
        )).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while capture.read().unwrap().is_empty() {
            assert!(
                Instant::now() < deadline,
                "local stream fixture did not emit its record"
            );
            thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..2 {
            assert_eq!(
                capture.read().unwrap(),
                vec![Attempt {
                    pid: 72753,
                    destination: super::super::Destination::SandboxDeny,
                }]
            );
        }
    }

    #[test]
    fn missing_header_start_error_reaps_the_child() {
        for (script, expected) in [
            (
                "printf '%s' \"$$\" > \"$PID_FILE\"; printf 'wrong header\n'; exec /bin/sleep 30",
                "missing filtering header",
            ),
            (
                "printf '%s' \"$$\" > \"$PID_FILE\"; exec /bin/sleep 30",
                "header within 8 seconds",
            ),
        ] {
            let folder = crate::setup::TestRoot::new();
            let pid_file = folder.path.join("stream.pid");
            let mut command = command(script);
            command.env("PID_FILE", &pid_file);
            let error = Capture::start(command).unwrap_err();
            assert!(error.contains(expected), "{error}");
            let pid: u32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
            assert_reaped(pid);
            folder.cleanup();
        }
    }

    #[test]
    fn stream_errors_fail_the_capture() {
        for (script, expected) in [
            (
                "printf 'Filtering the log data using \"test\"\n'; printf 'stream error' >&2; exec /bin/sleep 30",
                &["log stream stderr:"][..],
            ),
            ("exit 0", &["missing filtering header"][..]),
            (
                "printf 'Filtering the log data using \"test\"\n'",
                &["stdout ended", "exited early"][..],
            ),
            (
                "printf 'Filtering the log data using \"test\"\n'; i=0; while [ \"$i\" -lt 4097 ]; do printf '%s\n' '{\"eventType\":\"logEvent\",\"eventMessage\":\"ignored\"}'; i=$((i + 1)); done; exec /bin/sleep 30",
                &["exceeds 4096 records"][..],
            ),
        ] {
            let error = match Capture::start(command(script)) {
                Err(error) => error,
                Ok(capture) => {
                    let deadline = Instant::now() + Duration::from_secs(2);
                    loop {
                        if let Err(error) = capture.read() {
                            break error;
                        }
                        assert!(Instant::now() < deadline, "an ended stream must fail");
                        thread::sleep(Duration::from_millis(5));
                    }
                }
            };
            assert!(
                expected.iter().any(|message| error.contains(message)),
                "{error}"
            );
        }
    }

    #[test]
    fn stdout_caps_fail_instead_of_truncating() {
        for (tail, expected) in [
            (vec![b'x'; LOG_CAP as usize + 1], "log stream exceeds 1 MiB"),
            (
                b"{\"eventType\":\"logEvent\",\"eventMessage\":\"ignored\"}\n"
                    .repeat(RECORD_CAP + 1),
                "log stream exceeds 4096 records",
            ),
        ] {
            let mut input = HEADER.as_bytes().to_vec();
            input.extend(tail);
            let bytes = Mutex::new(Ok(Vec::new()));
            let (ready, receive) = mpsc::channel();
            assert_eq!(
                read_stdout(input.as_slice(), &bytes, &ready).unwrap_err(),
                expected
            );
            assert_eq!(receive.recv().unwrap(), Ok(()));
            assert!(bytes.lock().unwrap().as_ref().unwrap().len() as u64 <= LOG_CAP);
        }
    }
}
