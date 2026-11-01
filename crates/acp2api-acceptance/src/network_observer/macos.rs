use super::{Attempt, output, records};
use std::{path::Path, process::Command};

const PROFILE: &str = "(version 1)(allow default)(deny network-outbound (remote ip \"*:*\"))(allow network-outbound (remote ip \"localhost:*\"))";
#[derive(Debug)]
pub(super) struct Capture {
    start: String,
}
impl Capture {
    pub fn begin() -> Result<Self, String> {
        Ok(Self {
            start: date(false)?,
        })
    }
    pub fn command(&self, program: &Path) -> Command {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.args(["-p", PROFILE]).arg(program);
        command
    }
    pub fn read(&self) -> Result<Vec<Attempt>, String> {
        let mut command = Command::new("/usr/bin/log");
        command.args([
            "show",
            "--style",
            "json",
            "--info",
            "--debug",
            "--loss",
            "--start",
            &self.start,
            "--end",
            &date(true)?,
            "--predicate",
            "eventMessage CONTAINS[c] 'network-outbound'",
        ]);
        records::macos(&output(command)?.stdout)
    }
}
fn date(end: bool) -> Result<String, String> {
    let mut command = Command::new("/bin/date");
    if end {
        command.arg("-v+1S");
    }
    command.arg("+%Y-%m-%d %H:%M:%S%z");
    String::from_utf8(output(command)?.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|e| format!("observation date: {e}"))
}
