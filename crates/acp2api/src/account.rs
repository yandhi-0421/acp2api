use crate::{Clock, Environment, StopSignal};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub(super) struct Account {
    root: PathBuf,
    instance: File,
    gate: Option<File>,
    pub(crate) temporary: Temporary,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        options.share_mode(
            windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ
                | windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE,
        );
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
    let mut stopped = stop.stopped();
    poll_lock(file, path, clock, &mut stopped).await
}
async fn poll_lock(
    file: &File,
    path: &Path,
    clock: &dyn Clock,
    stopped: &mut (impl std::future::Future<Output = ()> + Unpin),
) -> Result<bool, String> {
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(true),
            Err(fs::TryLockError::WouldBlock) => {
                tokio::select! {
                    biased;
                    _ = &mut *stopped => return Ok(false),
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
            Ok(()) => {
                let temporary = Temporary::create(&env.account_root, &env.temp)?;
                Ok(Some(Self {
                    root: env.account_root.clone(),
                    gate: Some(gate),
                    instance,
                    temporary,
                }))
            }
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
        self.temporary.finish()?;
        let gate_path = self.root.join("startup.lock");
        let gate = open_lock(&gate_path)?;
        let mut stop = std::future::pending::<()>();
        poll_lock(&gate, &gate_path, clock, &mut stop).await?;
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
    let mut file = crate::os::private_file(&temp).map_err(|_| failed(path))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| failed(path))?;
    fs::rename(temp, path).map_err(|_| failed(path))
}
#[derive(Clone)]
pub(crate) struct Temporary {
    account: PathBuf,
    root: PathBuf,
    index: usize,
}
const OWNERSHIP: &str = "owned-temporary-files.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ownership {
    format: u8,
    roots: Vec<OwnedRoot>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedRoot {
    path: PathBuf,
    process: Identity,
    processes: Vec<OwnedProcess>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedProcess {
    path: PathBuf,
    // A launch interrupted before identity persistence remains unproven.
    process: Option<Identity>,
}
impl Ownership {
    fn load(account: &Path) -> Result<Self, String> {
        let path = account.join(OWNERSHIP);
        match fs::symlink_metadata(&path) {
            Ok(_) => validate_private_record(&path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    format: 1,
                    roots: Vec::new(),
                });
            }
            Err(_) => return Err(failed(&path)),
        }
        let bytes = fs::read(&path).map_err(|_| failed(&path))?;
        let record: Self = serde_json::from_slice(&bytes).map_err(|_| failed(&path))?;
        if record.format != 1 {
            return Err(failed(&path));
        }
        let id = crate::hex(&Sha256::digest(account.as_os_str().as_encoded_bytes()));
        for root in &record.roots {
            let parent = root.path.parent().ok_or_else(|| failed(&root.path))?;
            let name = root
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| failed(&root.path))?;
            if !root.path.is_absolute()
                || root
                    .path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
                || !run_name(name)
                || parent.file_name().and_then(|name| name.to_str()) != Some(&id)
                || parent
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    != Some("acp2api")
            {
                return Err(failed(&root.path));
            }
            for process in &root.processes {
                let name = process
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| failed(&process.path))?;
                if process.path.parent() != Some(root.path.join("processes").as_path())
                    || !run_name(name)
                    || process
                        .path
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
                {
                    return Err(failed(&process.path));
                }
            }
        }
        Ok(record)
    }
    fn save(&self, account: &Path) -> Result<(), String> {
        let path = account.join(OWNERSHIP);
        write(&path, &serde_json::to_vec(self).map_err(|_| failed(&path))?)
    }
}
impl Temporary {
    #[cfg(test)]
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn fixture(root: &Path) -> Self {
        for folder in ["account", "temp"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        Self::create(&root.join("account"), &root.join("temp")).unwrap()
    }
    pub(crate) fn create(account: &Path, temp: &Path) -> Result<Self, String> {
        let account = fs::canonicalize(account).map_err(|_| failed(account))?;
        let temp = fs::canonicalize(temp).map_err(|_| failed(temp))?;
        let id = crate::hex(&Sha256::digest(account.as_os_str().as_encoded_bytes()));
        let container = temp.join("acp2api").join(id);
        let shared = temp.join("acp2api");
        match fs::create_dir(&shared) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&shared, fs::Permissions::from_mode(0o1777))
                        .map_err(|_| failed(&shared))?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&shared)
                    .map_err(|_| failed(&shared))?
                    .is_dir()
                {
                    return Err(failed(&shared));
                }
            }
            Err(_) => return Err(failed(&shared)),
        }
        match fs::symlink_metadata(&container) {
            Ok(_) => validate_private_directory(&container)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                private_folder(&container)?
            }
            Err(_) => return Err(failed(&container)),
        }
        let root = container.join(crate::random_hex::<16>().map_err(|_| failed(&container))?);
        let mut record = Ownership::load(&account)?;
        let files = Self {
            account,
            root,
            index: record.roots.len(),
        };
        record.roots.push(OwnedRoot {
            path: files.root.clone(),
            process: identity(std::process::id()).ok_or_else(|| failed(&files.account))?,
            processes: Vec::new(),
        });
        record.save(&files.account)?;
        private_folder(&files.root)?;
        private_folder(&files.sessions())?;
        private_folder(&files.root.join("processes"))?;
        Ok(files)
    }
    pub(crate) fn recover(&mut self) -> Result<bool, String> {
        let mut record = Ownership::load(&self.account)?;
        let mut retained = Vec::new();
        for (index, root) in record.roots.into_iter().enumerate() {
            if index == self.index {
                self.index = retained.len();
            }
            match fs::symlink_metadata(&root.path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    retained.push(root);
                    continue;
                }
                Ok(_) => {}
            }
            if !process_ended(&root.process)
                || !root.processes.iter().all(process_tree_ended)
                || remove_root(&root.path).is_err()
            {
                retained.push(root);
            }
        }
        record.roots = retained;
        let earlier_trees_ended = record
            .roots
            .iter()
            .filter(|root| root.path != self.root)
            .all(|root| root.processes.iter().all(process_tree_ended));
        record.save(&self.account)?;
        Ok(earlier_trees_ended)
    }
    pub(crate) fn sessions(&self) -> PathBuf {
        self.root.join("sessions")
    }
    fn ownership(&self) -> Result<Ownership, String> {
        let record = Ownership::load(&self.account)?;
        if record
            .roots
            .get(self.index)
            .is_none_or(|root| root.path != self.root)
        {
            return Err(failed(&self.account.join(OWNERSHIP)));
        }
        Ok(record)
    }
    pub(crate) fn prepare_launch(&self) -> Result<(PathBuf, usize), String> {
        let path = self
            .root
            .join("processes")
            .join(crate::random_hex::<16>().map_err(|_| failed(&self.root))?);
        let mut record = self.ownership()?;
        let processes = &mut record.roots[self.index].processes;
        let index = processes.len();
        processes.push(OwnedProcess {
            path: path.clone(),
            process: None,
        });
        record.save(&self.account)?;
        private_folder(&path)?;
        Ok((path, index))
    }
    pub(crate) fn record_launch(&self, index: usize, pid: u32) -> Result<(), String> {
        let mut record = self.ownership()?;
        record.roots[self.index].processes[index].process = identity(pid);
        record.save(&self.account)
    }
    fn finish(&self) -> Result<(), String> {
        let mut record = self.ownership()?;
        let current = &record.roots[self.index];
        if !current.processes.iter().all(process_tree_ended) {
            return Ok(());
        }
        if remove_root(&self.root).is_err() {
            return Ok(());
        }
        record.roots.remove(self.index);
        record.save(&self.account)
    }
}
fn private_folder(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| failed(path))
    }
    #[cfg(windows)]
    crate::os::private_folder(path).map_err(|_| failed(path))
}
enum Access {
    Private,
    Owned,
}
fn validate_owner(path: &Path, access: Access) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(path).map_err(|_| failed(path))?;
    if meta.file_type().is_symlink() {
        return Err(failed(path));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mask = match access {
            Access::Private => 0o077,
            Access::Owned => 0o022,
        };
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & mask != 0 {
            return Err(failed(path));
        }
    }
    #[cfg(windows)]
    crate::os::validate_private(path, matches!(access, Access::Private))
        .map_err(|_| failed(path))?;
    Ok(meta)
}
fn validate_private_record(path: &Path) -> Result<(), String> {
    if validate_owner(path, Access::Private)?.is_file() {
        Ok(())
    } else {
        Err(failed(path))
    }
}
fn validate_private_directory(path: &Path) -> Result<(), String> {
    if validate_owner(path, Access::Private)?.is_dir() {
        Ok(())
    } else {
        Err(failed(path))
    }
}
fn validate_owned_entry(path: &Path) -> Result<(), String> {
    let meta = validate_owner(path, Access::Owned)?;
    if meta.is_dir() || meta.is_file() {
        Ok(())
    } else {
        Err(failed(path))
    }
}
fn run_name(name: &str) -> bool {
    name.len() == 32 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn remove_root(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(failed(path)),
        Ok(_) => {}
    }
    if fs::canonicalize(path).map_err(|_| failed(path))? != path {
        return Err(failed(path));
    }
    validate_private_directory(path)?;
    fn children(path: &Path, root: &Path) -> Result<(), String> {
        for entry in fs::read_dir(path).map_err(|_| failed(path))? {
            let entry = entry.map_err(|_| failed(path))?;
            let kind = entry.file_type().map_err(|_| failed(&entry.path()))?;
            if kind.is_symlink() {
                let target = fs::canonicalize(entry.path()).map_err(|_| failed(&entry.path()))?;
                if !target.starts_with(root) {
                    return Err(failed(&entry.path()));
                }
            } else {
                validate_owned_entry(&entry.path())?;
                if kind.is_dir() {
                    children(&entry.path(), root)?;
                }
            }
        }
        Ok(())
    }
    children(path, path)?;
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(failed(path)),
    }
}
fn process_tree_ended(process: &OwnedProcess) -> bool {
    let Some(identity) = &process.process else {
        return false;
    };
    #[cfg(target_os = "macos")]
    {
        unsafe {
            libc::killpg(identity.pid as i32, 0) == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        }
    }
    #[cfg(windows)]
    {
        let _ = identity;
        crate::os::process_tree_ended(&process.path)
    }
}
fn process_ended(process: &Identity) -> bool {
    if let Some(current) = identity(process.pid) {
        return current != *process;
    }
    #[cfg(target_os = "macos")]
    {
        unsafe {
            libc::kill(process.pid as i32, 0) == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, ERROR_INVALID_PARAMETER},
            System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        };
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process.pid);
            if handle.is_null() {
                return std::io::Error::last_os_error().raw_os_error()
                    == Some(ERROR_INVALID_PARAMETER as i32);
            }
            let mut code = 0;
            let ended = GetExitCodeProcess(handle, &mut code) != 0
                && code != windows_sys::Win32::Foundation::STILL_ACTIVE as u32;
            CloseHandle(handle);
            ended
        }
    }
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
    (read == size && info.pbi_status != libc::SZOMB).then_some(Identity {
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
            && code == windows_sys::Win32::Foundation::STILL_ACTIVE as u32;
        CloseHandle(process);
        valid.then_some(Identity {
            pid,
            created: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
        })
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn start_temporary(account: &Path, temp: &Path) -> Result<Temporary, String> {
        let mut files = Temporary::create(account, temp)?;
        files.recover()?;
        Ok(files)
    }

    #[cfg(unix)]
    #[test]
    fn shared_temporary_parent_is_sticky_and_account_subtrees_are_private() {
        use std::os::unix::fs::MetadataExt;
        let root = std::env::temp_dir().join(format!(
            "acp2api-shared-temp-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let files = Temporary::fixture(&root);
        let modes = [
            root.join("temp/acp2api"),
            files.root.parent().unwrap().to_path_buf(),
            files.root.clone(),
        ]
        .map(|path| fs::metadata(path).unwrap().mode() & 0o7777);
        fs::remove_dir_all(root).unwrap();
        assert_eq!(
            modes,
            [0o1777, 0o700, 0o700],
            "different computer accounts share only the sticky parent"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_unlinks_an_internal_symlink_without_following_it() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-internal-link-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let files = Temporary::fixture(&root);
        fs::write(files.sessions().join("target"), b"session").unwrap();
        std::os::unix::fs::symlink(
            files.sessions().join("target"),
            files.sessions().join("link"),
        )
        .unwrap();
        files.finish().unwrap();
        let result = (
            files.root.exists(),
            Ownership::load(&files.account).unwrap().roots.len(),
        );
        fs::remove_dir_all(root).unwrap();
        assert_eq!(
            result,
            (false, 0),
            "a link within an owned run must be removed as a link"
        );
    }

    #[test]
    fn normal_cleanup_forgets_a_registered_root_that_is_already_absent() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-finish-absent-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let files = Temporary::fixture(&root);
        fs::remove_dir_all(&files.root).unwrap();
        let result = files.finish();
        let count = Ownership::load(&files.account).unwrap().roots.len();
        fs::remove_dir_all(root).unwrap();
        assert!(
            result.is_ok() && count == 0,
            "normal cleanup must remove the entry after proof that its root is absent"
        );
    }

    #[test]
    fn ownership_rejects_a_process_folder_outside_its_registered_run() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-process-path-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let old = Temporary::fixture(&root);
        let mut record = Ownership::load(&old.account).unwrap();
        record.roots[0].processes.push(OwnedProcess {
            path: root.join("outside"),
            process: None,
        });
        record.save(&old.account).unwrap();
        let rejected = start_temporary(&old.account, &root.join("temp")).is_err();
        let preserved = old.root.exists();
        fs::remove_dir_all(root).unwrap();
        assert!(
            rejected && preserved,
            "a process folder must belong to its recorded run before any native check"
        );
    }

    #[tokio::test]
    async fn recovery_drops_an_entry_after_proving_its_root_is_absent() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-absent-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let old = Temporary::fixture(&root);
        fs::remove_dir_all(&old.root).unwrap();
        let new = start_temporary(&old.account, &root.join("temp")).unwrap();
        let count = Ownership::load(&old.account).unwrap().roots.len();
        let created = new.root.exists();
        fs::remove_dir_all(root).unwrap();
        assert_eq!(
            (count, created),
            (1, true),
            "proof of an absent root removes its entry before a new run"
        );
    }

    #[tokio::test]
    async fn recovery_keeps_an_unproven_launch_and_continues_startup() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!("acp2api-unproven-{}", crate::random_hex::<12>().unwrap()));
            let old = Temporary::fixture(&root);
            let mut owner = tokio::process::Command::new(if cfg!(windows) { "cmd.exe" } else { "/bin/cat" })
                .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).kill_on_drop(true).spawn().unwrap();
            let process = identity(owner.id().unwrap()).unwrap();
            owner.kill().await.unwrap();
            let mut record = Ownership::load(&old.account).unwrap();
            record.roots[0].process = process;
            record.roots[0].processes.push(OwnedProcess { path: old.root.join("processes/00000000000000000000000000000000"), process: None });
            record.save(&old.account).unwrap();
            let next = start_temporary(&old.account, &root.join("temp")).unwrap();
            let actual = (old.root.exists(), next.root.exists(), Ownership::load(&old.account).unwrap().roots.len());
            fs::remove_dir_all(root).unwrap();
            assert_eq!(actual, (true, true, 2), "a crash before process identity persistence cannot authorize deletion or refuse startup");
        }).await.expect("unproven-launch cap: 5 seconds; one child, no captured output");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn an_exited_runtime_leader_does_not_prove_its_descendants_ended() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            use tokio::io::AsyncWriteExt;
            let root = std::env::temp_dir().join(format!(
                "acp2api-descendants-{}",
                crate::random_hex::<12>().unwrap()
            ));
            for folder in ["account", "temp"] {
                fs::create_dir_all(root.join(folder)).unwrap();
            }
            let old = start_temporary(&root.join("account"), &root.join("temp")).unwrap();
            let clock: std::sync::Arc<dyn Clock> =
                std::sync::Arc::new(crate::os::SystemClock::default());
            let mut launched = crate::startup::launch(
                &old,
                &crate::os::OsLauncher,
                clock.clone(),
                crate::LaunchSpec {
                    program: "/bin/sh".into(),
                    args: vec!["-c".into(), "read line; sleep 30 &".into()],
                    env_set: vec![],
                    cwd: old.sessions(),
                    own_temp: None,
                },
            )
            .await
            .unwrap();
            let group = launched.tree.pid();
            launched.stdin.write_all(b"go\n").await.unwrap();
            launched.tree.exited().await;
            let mut record = Ownership::load(&old.account).unwrap();
            record.roots[0].process = record.roots[0].processes[0].process.clone().unwrap();
            record.save(&old.account).unwrap();
            let next = start_temporary(&root.join("account"), &root.join("temp")).unwrap();
            let kept = old.root.exists();
            unsafe {
                libc::killpg(group as i32, libc::SIGKILL);
            }
            launched.tree.stop(clock).await;
            drop(next);
            fs::remove_dir_all(root).unwrap();
            assert!(
                kept,
                "the runtime group still contains a live descendant after its leader exited"
            );
        })
        .await
        .expect("descendant recovery cap: 5 seconds; one shell and one sleep, no captured output");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn recovery_keeps_a_live_runtime_tree_when_its_owner_has_ended() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-tree-{}",
                crate::random_hex::<12>().unwrap()
            ));
            for folder in ["account", "temp"] {
                fs::create_dir_all(root.join(folder)).unwrap();
            }
            let old = start_temporary(&root.join("account"), &root.join("temp")).unwrap();
            let clock: std::sync::Arc<dyn Clock> =
                std::sync::Arc::new(crate::os::SystemClock::default());
            let launched = crate::startup::launch(
                &old,
                &crate::os::OsLauncher,
                clock.clone(),
                crate::LaunchSpec {
                    program: "/bin/cat".into(),
                    args: vec![],
                    env_set: vec![],
                    cwd: old.sessions(),
                    own_temp: None,
                },
            )
            .await
            .unwrap();
            let mut owner = tokio::process::Command::new("/bin/cat")
                .stdin(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let process = identity(owner.id().unwrap()).unwrap();
            owner.kill().await.unwrap();
            let mut record = Ownership::load(&old.account).unwrap();
            record.roots[0].process = process;
            record.save(&old.account).unwrap();
            let next = start_temporary(&root.join("account"), &root.join("temp")).unwrap();
            let kept = (
                old.root.exists(),
                Ownership::load(&old.account).unwrap().roots.len(),
            );
            drop(launched.stdin);
            launched.tree.stop(clock).await;
            drop(next);
            let recovered = start_temporary(&root.join("account"), &root.join("temp")).unwrap();
            let cleaned = !old.root.exists();
            drop(recovered);
            fs::remove_dir_all(root).unwrap();
            assert_eq!(
                (kept, cleaned),
                ((true, 2), true),
                "keep the earlier live tree, then recover its files once the native tree ends"
            );
        })
        .await
        .expect("live-tree recovery cap: 5 seconds; two native children, no captured output");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn cleanup_keeps_unsafe_roots_but_rejects_nonprivate_coordination_records() {
        use std::os::unix::{
            fs::{PermissionsExt, symlink},
            process::CommandExt,
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-permissions-{}",
                crate::random_hex::<12>().unwrap()
            ));
            for folder in ["account", "temp", "outside"] {
                fs::create_dir_all(root.join(folder)).unwrap();
            }
            fs::write(root.join("outside/marker"), b"keep").unwrap();
            let old = start_temporary(&root.join("account"), &root.join("temp")).unwrap();
            let mut command = std::process::Command::new("/bin/cat");
            command
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .process_group(0);
            let mut child = command.spawn().unwrap();
            let process = identity(child.id()).unwrap();
            child.kill().unwrap();
            child.wait().unwrap();
            let mut record = Ownership::load(&old.account).unwrap();
            record.roots[0].process = process;
            record.save(&old.account).unwrap();
            symlink(root.join("outside"), old.sessions().join("escape")).unwrap();
            let next = start_temporary(&root.join("account"), &root.join("temp"));
            let symlink_retained = next.is_ok()
                && old.root.exists()
                && Ownership::load(&old.account)
                    .unwrap()
                    .roots
                    .iter()
                    .any(|entry| entry.path == old.root);
            let _ = fs::remove_file(old.sessions().join("escape"));
            fs::set_permissions(
                old.account.join(OWNERSHIP),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap();
            let record_rejected =
                start_temporary(&root.join("account"), &root.join("temp")).is_err();
            let preserved = fs::read(root.join("outside/marker")).unwrap();
            fs::remove_dir_all(root).unwrap();
            assert_eq!(
                (symlink_retained, record_rejected, preserved),
                (true, true, b"keep".to_vec()),
                "unsafe coordination files and escaping links cannot authorize cleanup"
            );
        })
        .await
        .expect("boundary unit cap: 5 seconds; one native child, no captured output");
    }

    #[tokio::test]
    async fn an_ownership_record_cannot_authorize_deleting_an_unrelated_folder() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-untrusted-{}",
                crate::random_hex::<12>().unwrap()
            ));
            for folder in ["account", "temp", "unrelated"] {
                fs::create_dir_all(root.join(folder)).unwrap();
            }
            fs::write(root.join("unrelated/marker"), b"keep").unwrap();
            let old = start_temporary(&root.join("account"), &root.join("temp")).unwrap();
            let mut child =
                tokio::process::Command::new(if cfg!(windows) { "cmd.exe" } else { "/bin/cat" })
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .unwrap();
            let process = identity(child.id().unwrap()).unwrap();
            child.kill().await.unwrap();
            let mut record = Ownership::load(&old.account).unwrap();
            record.roots[0].path = root.join("unrelated");
            record.roots[0].process = process;
            record.save(&old.account).unwrap();
            let result = start_temporary(&root.join("account"), &root.join("temp"));
            let preserved = fs::read(root.join("unrelated/marker")).ok();
            fs::remove_dir_all(root).unwrap();
            assert!(
                result.is_err() && preserved == Some(b"keep".to_vec()),
                "invalid ownership paths must report an error and preserve unrelated bytes"
            );
        })
        .await
        .expect("untrusted-record unit cap: 5 seconds; one child, no captured output");
    }

    #[tokio::test]
    async fn recovery_removes_an_ended_run_even_after_the_temporary_folder_changes() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-recovery-{}",
                crate::random_hex::<12>().unwrap()
            ));
            for folder in ["account", "old", "new"] {
                fs::create_dir_all(root.join(folder)).unwrap();
            }
            let old = start_temporary(&root.join("account"), &root.join("old")).unwrap();
            let mut child =
                tokio::process::Command::new(if cfg!(windows) { "cmd.exe" } else { "/bin/cat" })
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .unwrap();
            let process = identity(child.id().unwrap()).unwrap();
            child.kill().await.unwrap();
            let mut record = Ownership::load(&old.account).unwrap();
            record.roots[0].process = process;
            record.save(&old.account).unwrap();
            let new = start_temporary(&root.join("account"), &root.join("new")).unwrap();
            let actual = (
                !old.root.exists(),
                new.root.exists(),
                Ownership::load(&new.account).unwrap().roots.len(),
            );
            fs::remove_dir_all(root).unwrap();
            assert_eq!(
                actual,
                (true, true, 1),
                "a native proof of exit permits cleanup of the recorded earlier root"
            );
        })
        .await
        .expect("recovery unit cap: 5 seconds; one native child, no captured output");
    }

    #[tokio::test]
    async fn account_claim_records_a_private_run_under_the_canonical_account() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-owned-{}",
                crate::random_hex::<12>().unwrap()
            ));
            fs::create_dir_all(root.join("account/alias")).unwrap();
            fs::create_dir_all(root.join("temp")).unwrap();
            let env = Environment {
                account_root: root.join("account/alias/.."),
                home: None,
                local_app_data: None,
                temp: root.join("temp"),
                path: vec![],
                exe_dir: root.join("exe"),
            };
            let clock = crate::os::SystemClock::default();
            let mut stop = crate::os::TerminalStop;
            let account = Account::claim(&env, &clock, &mut stop)
                .await
                .unwrap()
                .unwrap();
            let record = fs::read(env.account_root.join(OWNERSHIP))
                .ok()
                .map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).unwrap());
            let canonical = fs::canonicalize(&env.account_root).unwrap();
            let expected = env.temp.join("acp2api").join(crate::hex(&Sha256::digest(
                canonical.as_os_str().as_encoded_bytes(),
            )));
            let observed = record.as_ref().map(|record| {
                let path = PathBuf::from(record["roots"][0]["path"].as_str().unwrap());
                (
                    path.parent().unwrap().to_path_buf(),
                    path.join("sessions").is_dir(),
                    path.join("processes").is_dir(),
                )
            });
            drop(account);
            fs::remove_dir_all(&root).unwrap();
            assert_eq!(
                record
                    .as_ref()
                    .map(|record| record["format"].as_u64().unwrap()),
                Some(1),
                "ownership format must exist before runtime launch"
            );
            assert_eq!(
                observed,
                Some((expected, true, true)),
                "each run needs separate session and process children"
            );
        })
        .await
        .expect("account ownership unit cap: 5 seconds");
    }
}
