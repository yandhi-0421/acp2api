use std::{path::Path, process::Command};

const PROFILE: &str = "(version 1)(allow default)(deny network-outbound (with send-signal SIGKILL) (remote ip \"*:*\"))(allow network-outbound (remote ip \"localhost:*\"))";

pub(super) fn command(program: &Path) -> Command {
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command.args(["-p", PROFILE]).arg(program);
    command
}
