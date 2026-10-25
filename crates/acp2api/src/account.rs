use crate::{Clock, Environment, StopSignal};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub(super) struct Account {
    root: PathBuf,
    gate: Option<File>,
    instance: File,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity {
    pid: u32,
    created: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    format: u8,
    address: String,
    process: Identity,
}
fn failed(path: &Path) -> String {
    format!(
        "cannot use the account coordination data at {}.",
        path.display()
    )
}
fn open_lock(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1 | 2);
    }
    let file = options.open(path).map_err(|_| failed(path))?;
    let meta = file.metadata().map_err(|_| failed(path))?;
    if !meta.is_file() || meta.len() != 0 {
        return Err(failed(path));
    }
    Ok(file)
}
async fn lock(
    file: &File,
    path: &Path,
    clock: &dyn Clock,
    stop: &mut dyn StopSignal,
) -> Result<bool, String> {
    let stopped = stop.stopped();
    tokio::pin!(stopped);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(true),
            Err(fs::TryLockError::WouldBlock) => {
                tokio::select! {
                    biased;
                    _ = &mut stopped => return Ok(false),
                    hold = clock.sleep_until(clock.now() + 1_000) => drop(hold),
                }
            }
            Err(_) => return Err(failed(path)),
        }
    }
}
impl Account {
    pub(super) async fn claim(
        env: &Environment,
        clock: &dyn Clock,
        stop: &mut dyn StopSignal,
    ) -> Result<Option<Self>, String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&env.account_root)
                .map_err(|_| failed(&env.account_root))?;
        }
        #[cfg(windows)]
        fs::create_dir_all(&env.account_root).map_err(|_| failed(&env.account_root))?;
        let gate_path = env.account_root.join("startup.lock");
        let gate = open_lock(&gate_path)?;
        if !lock(&gate, &gate_path, clock, stop).await? {
            return Ok(None);
        }
        let instance_path = env.account_root.join("instance.lock");
        let instance = open_lock(&instance_path)?;
        match instance.try_lock() {
            Ok(()) => Ok(Some(Self {
                root: env.account_root.clone(),
                gate: Some(gate),
                instance,
            })),
            Err(fs::TryLockError::WouldBlock) => {
                let path = env.account_root.join("panel");
                let bytes = fs::read(&path).map_err(|_| failed(&path))?;
                let panel: Publication =
                    serde_json::from_slice(&bytes).map_err(|_| failed(&path))?;
                let address: axum::http::Uri = panel.address.parse().map_err(|_| failed(&path))?;
                if panel.format != 1
                    || address.scheme_str() != Some("http")
                    || address.host() != Some("127.0.0.1")
                    || address.port_u16().is_none_or(|port| port == 0)
                    || address.path() != "/"
                    || address.query().is_some()
                    || identity(panel.process.pid).as_ref() != Some(&panel.process)
                {
                    return Err(failed(&path));
                }
                Err(format!(
                    "another acp2api runs for this computer account. Its management panel is at {}.",
                    panel.address
                ))
            }
            Err(_) => Err(failed(&instance_path)),
        }
    }
    pub(super) fn publish(&mut self, address: &str) -> Result<(), String> {
        let path = self.root.join("panel");
        let process = identity(std::process::id()).ok_or_else(|| failed(&path))?;
        let publication = Publication {
            format: 1,
            address: address.into(),
            process,
        };
        write(
            &path,
            &serde_json::to_vec(&publication).map_err(|_| failed(&path))?,
        )?;
        self.gate.take();
        Ok(())
    }
    pub(super) async fn finish(
        self,
        clock: &dyn Clock,
        listener: std::net::TcpListener,
    ) -> Result<(), String> {
        let gate_path = self.root.join("startup.lock");
        let gate = open_lock(&gate_path)?;
        loop {
            match gate.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) => {
                    drop(clock.sleep_until(clock.now() + 1_000).await)
                }
                Err(_) => return Err(failed(&gate_path)),
            }
        }
        drop(listener);
        let path = self.root.join("panel");
        let cleared = fs::remove_file(&path).map_err(|_| failed(&path));
        drop(self.instance);
        drop(gate);
        cleared
    }
}
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!(
        "{}.new",
        crate::random_hex::<12>().map_err(|_| failed(path))?
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|_| failed(path))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| failed(path))?;
    fs::rename(temp, path).map_err(|_| failed(path))
}
#[cfg(target_os = "macos")]
fn identity(pid: u32) -> Option<Identity> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    let read = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    (read == size && info.pbi_status != 5).then_some(Identity {
        pid,
        created: info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec,
    })
}
#[cfg(windows)]
fn identity(pid: u32) -> Option<Identity> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{
            GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let mut code = 0;
        let valid = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user)
            != 0
            && GetExitCodeProcess(process, &mut code) != 0
            && code == 259;
        CloseHandle(process);
        valid.then_some(Identity {
            pid,
            created: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
        })
    }
}
