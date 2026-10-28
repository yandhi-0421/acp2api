fn main() -> std::process::ExitCode {
    let run = acp2api_acceptance::adapters::RunFile::read_from_env();
    use std::io::{BufRead, Read};
    let mut locks = Vec::new();
    for name in ["startup.lock", "instance.lock"] {
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_SHARE_READ: u32 = 1;
            const FILE_SHARE_WRITE: u32 = 2;
            options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }
        let lock = options
            .open(run.environment.account_root.join(name))
            .expect("account fixture lock");
        lock.try_lock().expect("first host owns the account lock");
        locks.push(lock);
    }
    let mut command = run
        .hub
        .connect(serde_json::json!({"op":"account_hold","pid":std::process::id()}))
        .expect("account fixture hub");
    let mut line = String::new();
    command
        .by_ref()
        .take(1025)
        .read_line(&mut line)
        .expect("bounded account command");
    assert!(line.len() <= 1024, "account command cap: 1 KiB");
    let command: serde_json::Value = serde_json::from_str(&line).expect("account command JSON");
    assert_eq!(command["op"], "go", "one account command: go");
    while let Some(lock) = locks.pop() {
        drop(lock);
    }
    acp2api::run(run.ports(), std::env::args_os(), run.environment)
}
