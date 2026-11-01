use crate::{
    Environment, ListedBuild, Ports,
    runtime_driver::{Runtime, RuntimeStart},
    store::Settings,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub(crate) struct Builds {
    pub(crate) start: RuntimeStart,
    environment: Environment,
    port: u16,
    found: Arc<Mutex<Vec<Found>>>,
    checked: Arc<Mutex<Vec<ListedBuild>>>,
    source: Arc<Mutex<Option<PathBuf>>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildIdentity {
    server: String,
    harness: String,
}
#[cfg(test)]
impl BuildIdentity {
    pub(crate) fn fixture() -> Self {
        Self {
            server: String::new(),
            harness: String::new(),
        }
    }
}
impl std::fmt::Display for BuildIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.server, self.harness)
    }
}
#[derive(Clone)]
struct Found {
    folder: PathBuf,
    listed: Option<ListedBuild>,
    identity: BuildIdentity,
}
impl Found {
    fn selected_build(&self) -> SelectedBuild {
        SelectedBuild {
            folder: self.folder.clone(),
            version: self
                .listed
                .as_ref()
                .expect("selected builds are listed")
                .version
                .clone(),
        }
    }
}
pub(crate) struct SelectedBuild {
    pub(crate) folder: PathBuf,
    pub(crate) version: String,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BuildAcceptance {
    OnTheReleaseList,
    CheckedOnThisComputer,
}
pub(crate) struct BuildRow {
    pub(crate) path: PathBuf,
    pub(crate) version: Option<String>,
    pub(crate) identity: String,
    pub(crate) acceptance: Option<BuildAcceptance>,
}
pub(crate) struct Summary {
    pub(crate) current: Option<CurrentBuild>,
    pub(crate) found: Vec<BuildRow>,
    pub(crate) release: Vec<String>,
    pub(crate) choices: Vec<(String, String)>,
    pub(crate) newer: Option<String>,
}
pub(crate) struct CurrentBuild {
    pub(crate) version: String,
    pub(crate) acceptance: BuildAcceptance,
    pub(crate) source: Option<PathBuf>,
}
pub(crate) enum Action {
    Initial,
    Search,
    Path(PathBuf),
    Build(String),
    Check(String),
}
pub(crate) enum Selection {
    Keep,
    Notice(crate::messages::OwnerMessage),
    Start(Option<SelectedBuild>),
    Check {
        folder: PathBuf,
        identity: BuildIdentity,
    },
}
#[derive(Debug)]
pub(crate) enum LaunchFailure {
    NoExecutable,
    NotAccepted,
    Io(std::io::Error),
    Runtime(String),
    Lockout {
        signal: &'static str,
        error: crate::messages::Message,
    },
    Stopped,
    Owner(crate::messages::OwnerMessage),
}
impl LaunchFailure {
    pub(crate) fn io_kind(&self) -> Option<std::io::ErrorKind> {
        match self {
            Self::Io(error) => Some(error.kind()),
            _ => None,
        }
    }
}
impl From<std::io::Error> for LaunchFailure {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl std::fmt::Display for LaunchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoExecutable => crate::messages::RuntimeFault::NoExecutable.fmt(f),
            Self::NotAccepted => crate::messages::RuntimeFault::NotAccepted.fmt(f),
            Self::Io(error) => error.fmt(f),
            Self::Runtime(error) => f.write_str(error),
            Self::Lockout { error, .. } => error.fmt(f),
            Self::Stopped => f.write_str("acp2api is exiting"),
            Self::Owner(message) => message.fmt(f),
        }
    }
}
impl From<String> for LaunchFailure {
    fn from(error: String) -> Self {
        Self::Runtime(error)
    }
}
impl From<crate::messages::Message> for LaunchFailure {
    fn from(error: crate::messages::Message) -> Self {
        match error.lockout_signal() {
            Some(signal) => Self::Lockout { signal, error },
            None => Self::Owner(crate::messages::OwnerMessage::Api(error)),
        }
    }
}
impl Builds {
    pub(crate) fn new(
        ports: &Ports,
        environment: Environment,
        data: PathBuf,
        temporary: crate::account::Temporary,
        tools: crate::runtime_driver::mcp::ToolSets,
        stopped: tokio::sync::watch::Receiver<bool>,
        port: u16,
    ) -> Self {
        Self {
            start: RuntimeStart {
                clock: ports.clock.clone(),
                stopped,
                launcher: ports.launcher.clone(),
                release: ports.release.clone(),
                data,
                temporary,
                tool_sets: tools,
            },
            environment,
            port,
            found: Arc::default(),
            checked: Arc::default(),
            source: Arc::default(),
        }
    }
    #[cfg(test)]
    pub(crate) fn fixture(root: &Path, clock: Arc<dyn crate::Clock>) -> Self {
        let mut ports = Ports::production();
        ports.clock = clock;
        Self::new(
            &ports,
            Environment {
                account_root: root.join("account"),
                home: None,
                local_app_data: None,
                temp: root.join("temp"),
                path: vec![],
                exe_dir: root.join("exe"),
            },
            root.into(),
            crate::account::Temporary::fixture(root),
            crate::runtime_driver::mcp::ToolSets::new(0),
            tokio::sync::watch::channel(false).1,
            0,
        )
    }
    fn accepted_builds(&self) -> impl Iterator<Item = ListedBuild> {
        let mut accepted = self.start.release.listed_builds().to_vec();
        accepted.extend(self.checked.lock().unwrap().iter().cloned());
        let platform = self.start.release.platform().to_owned();
        accepted
            .into_iter()
            .filter(move |build| build.platform == platform)
    }
    fn hashes(folder: &Path) -> Result<BuildIdentity, LaunchFailure> {
        let hash = |name: &str| -> Result<Option<String>, LaunchFailure> {
            let path = folder.join(name);
            match fs::read(&path) {
                Ok(bytes) => Ok(Some(crate::hex(&Sha256::digest(bytes)))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(LaunchFailure::Io(std::io::Error::new(
                    error.kind(),
                    format!("cannot read {}: {error}", path.display()),
                ))),
            }
        };
        let (Some(server), Some(harness)) = (hash(SERVER)?, hash(HARNESS)?) else {
            return Err(LaunchFailure::NoExecutable);
        };
        Ok(BuildIdentity { server, harness })
    }
    fn identify(&self, folder: &Path) -> Result<ListedBuild, LaunchFailure> {
        let hashes = Self::hashes(folder)?;
        self.identify_hashes(&hashes)
            .ok_or(LaunchFailure::NotAccepted)
    }
    fn identify_hashes(&self, hashes: &BuildIdentity) -> Option<ListedBuild> {
        self.accepted_builds()
            .find(|build| identity(build) == *hashes)
    }
    fn accepted_copy(&self) -> Result<Option<ListedBuild>, String> {
        match self.identify(&self.start.data.join(COPY)) {
            Ok(build) => Ok(Some(build)),
            Err(LaunchFailure::NoExecutable | LaunchFailure::NotAccepted) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
    fn scan_places(&self, configured: Option<&Path>) {
        let mut folders = vec![self.environment.exe_dir.clone(), self.start.data.join(COPY)];
        folders.extend(self.environment.path.iter().cloned());
        if let Some(path) = configured
            && let Some(folder) = path.parent()
        {
            folders.push(folder.into());
        }
        if let Some(home) = &self.environment.home {
            scan(&home.join("Library/Application Support/Zed"), &mut folders);
            scan(&home.join("Library/Caches/JetBrains"), &mut folders);
        }
        if let Some(local) = &self.environment.local_app_data {
            scan(&local.join("Zed"), &mut folders);
            scan(&local.join("JetBrains"), &mut folders);
        }
        folders.sort();
        folders.dedup();
        let mut found = Vec::new();
        for folder in folders {
            if [SERVER, HARNESS]
                .iter()
                .all(|name| folder.join(name).is_file())
            {
                let Ok(identity) = Self::hashes(&folder) else {
                    continue;
                };
                let listed = self.identify_hashes(&identity);
                found.push(Found {
                    listed,
                    folder,
                    identity,
                });
            }
        }
        *self.found.lock().unwrap() = found;
    }
    fn select_blocking(
        &self,
        action: Action,
        settings: &mut Settings,
        configuration: &Path,
    ) -> Result<Selection, LaunchFailure> {
        use crate::messages::OwnerMessage;
        let initial = matches!(action, Action::Initial);
        if initial {
            let path = self.start.data.join(crate::store::STATE_FILE);
            let state = crate::store::StateFile::read(&path).map_err(|error| error.to_string())?;
            *self.checked.lock().unwrap() = state
                .checked_builds
                .into_iter()
                .map(|checked| checked.build)
                .collect();
            for entry in fs::read_dir(&self.start.data).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".official-runtime-old-")
                    && entry
                        .file_type()
                        .map_err(|error| error.to_string())?
                        .is_dir()
                {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
        match action {
            Action::Path(path) => {
                if !path.is_absolute() {
                    return Err(LaunchFailure::Owner(OwnerMessage::PathNotAbsolute));
                }
                if !path
                    .parent()
                    .is_some_and(|folder| folder.join(HARNESS).is_file())
                {
                    return Ok(Selection::Notice(OwnerMessage::PathWithoutHarness));
                }
                if !path.is_file() {
                    return Err(LaunchFailure::Owner(OwnerMessage::PathWithoutServer));
                }
                settings.agy_acp_server_path = Some(path);
                crate::store::save(configuration, settings)?;
            }
            Action::Check(choice) => {
                let found = self.found.lock().unwrap();
                let selected = found
                    .iter()
                    .find(|found| found.listed.is_none() && found.identity.to_string() == choice)
                    .ok_or(LaunchFailure::Owner(OwnerMessage::ChooseUnacceptedBuild))?;
                return Ok(Selection::Check {
                    folder: selected.folder.clone(),
                    identity: selected.identity.clone(),
                });
            }
            Action::Build(choice) => {
                let found = self.found.lock().unwrap();
                let selected = found
                    .iter()
                    .find(|found| {
                        found
                            .listed
                            .as_ref()
                            .is_some_and(|build| identity(build).to_string() == choice)
                    })
                    .ok_or(LaunchFailure::Owner(OwnerMessage::ChooseAcceptedBuild))?;
                return Ok(Selection::Start(Some(selected.selected_build())));
            }
            Action::Initial => {
                if self.accepted_copy()?.is_some() {
                    return Ok(Selection::Start(None));
                }
            }
            Action::Search => {}
        }
        self.scan_places(settings.agy_acp_server_path.as_deref());
        if !initial && self.accepted_copy()?.is_some() {
            return Ok(Selection::Keep);
        }
        let found = self.found.lock().unwrap();
        let best = found
            .iter()
            .filter(|found| found.listed.is_some())
            .max_by_key(|found| version(&found.listed.as_ref().unwrap().version));
        Ok(Selection::Start(best.map(Found::selected_build)))
    }
    pub(crate) fn copy(&self, source: PathBuf) -> Result<(), LaunchFailure> {
        let copy = self.start.data.join(COPY);
        let source = fs::canonicalize(source)?;
        if fs::canonicalize(&copy).ok().as_ref() != Some(&source) {
            let suffix = crate::random_hex::<12>().map_err(|error| error.to_string())?;
            let staging = self.start.data.join(format!(".official-runtime-{suffix}"));
            let backup = self
                .start
                .data
                .join(format!(".official-runtime-old-{suffix}"));
            fs::create_dir(&staging)?;
            let result = (|| {
                for name in [SERVER, HARNESS] {
                    fs::copy(source.join(name), staging.join(name))?;
                }
                let build = self.identify(&staging)?;
                let previous = copy.exists();
                if previous {
                    fs::rename(&copy, &backup)?;
                }
                if let Err(error) = fs::rename(&staging, &copy) {
                    if previous {
                        fs::rename(&backup, &copy)?;
                    }
                    return Err(LaunchFailure::Io(error));
                }
                if previous {
                    let _ = fs::remove_dir_all(backup);
                }
                Ok(identity(&build))
            })();
            if staging.exists() {
                fs::remove_dir_all(&staging)?;
            }
            *self.source.lock().unwrap() = self.source_for(&result?);
        }
        Ok(())
    }
    fn source_for(&self, hashes: &BuildIdentity) -> Option<PathBuf> {
        self.found
            .lock()
            .unwrap()
            .iter()
            .find(|found| {
                found.folder != self.start.data.join(COPY)
                    && found.folder != self.start.data.join(CHECK)
                    && found.identity == *hashes
            })
            .map(|found| found.folder.join(SERVER))
    }
    pub(crate) fn check_copy(
        &self,
        source: &Path,
        identity: &BuildIdentity,
    ) -> Result<PathBuf, LaunchFailure> {
        let check = self.start.data.join(CHECK);
        if check.exists() {
            fs::remove_dir_all(&check).map_err(|error| error.to_string())?;
        }
        fs::create_dir(&check).map_err(|error| error.to_string())?;
        let result = (|| {
            for name in [SERVER, HARNESS] {
                fs::copy(source.join(name), check.join(name)).map_err(|error| error.to_string())?;
            }
            if Self::hashes(&check)? != *identity {
                return Err(LaunchFailure::Owner(
                    crate::messages::OwnerMessage::BuildChangedBeforeCheck,
                ));
            }
            Ok(check.join(SERVER))
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&check);
        }
        result
    }
    pub(crate) async fn check(
        &self,
        program: PathBuf,
        retained: Vec<(String, PathBuf)>,
    ) -> Result<String, crate::compatibility_check::Failure> {
        self.start
            .clean_sessions(retained)
            .await
            .map_err(|error| crate::compatibility_check::Failure::Internal(error.into()))?;
        crate::compatibility_check::run(
            &self.start,
            program,
            self.environment
                .home
                .as_ref()
                .map(|home| home.join(".gemini")),
        )
        .await
    }
    pub(crate) fn finish_check(
        &self,
        identity: &BuildIdentity,
        outcome: Result<String, crate::compatibility_check::Failure>,
    ) -> Result<crate::messages::OwnerMessage, LaunchFailure> {
        use crate::{compatibility_check::Failure, messages::OwnerMessage};
        let result = match outcome {
            Ok(version) => self
                .accept_check(version.clone(), identity)
                .map(|()| OwnerMessage::CheckPassed { version }),
            Err(Failure::Runtime(
                error @ (LaunchFailure::Stopped | LaunchFailure::Lockout { .. }),
            )) => Err(error),
            Err(Failure::Runtime(error)) => {
                Ok(OwnerMessage::CheckRunFailed(std::sync::Arc::new(error)))
            }
            Err(Failure::Incompatible(reason)) => Ok(OwnerMessage::CheckFailed(reason)),
            Err(Failure::Internal(error)) => Err(error),
        };
        let _ = fs::remove_dir_all(self.start.data.join(CHECK));
        result
    }
    fn accept_check(&self, version: String, hashes: &BuildIdentity) -> Result<(), LaunchFailure> {
        let check = self.start.data.join(CHECK);
        if Self::hashes(&check)? != *hashes {
            return Err(LaunchFailure::Owner(
                crate::messages::OwnerMessage::CheckCopyChanged,
            ));
        }
        let build = ListedBuild {
            version,
            platform: self.start.release.platform().into(),
            agy_acp_server_sha256: hashes.server.clone(),
            localharness_external_sha256: hashes.harness.clone(),
        };
        let path = self.start.data.join(crate::store::STATE_FILE);
        let mut state = crate::store::StateFile::read(&path).map_err(|error| error.to_string())?;
        state.checked_builds.retain(|checked| {
            !(checked.build.platform == build.platform && identity(&checked.build) == *hashes)
        });
        state.checked_builds.push(crate::store::CheckedBuild {
            build,
            checked_at: crate::journal::timestamp(self.start.clock.wall())
                .map_err(|error| error.to_string())?,
        });
        state.save(&path).map_err(|error| error.to_string())?;
        *self.checked.lock().unwrap() = state
            .checked_builds
            .into_iter()
            .map(|checked| checked.build)
            .collect();
        self.copy(check)
    }
    pub(crate) async fn launch(
        &self,
        retained: Option<Vec<(String, PathBuf)>>,
    ) -> Result<(Runtime, Option<crate::catalog::Catalog>), LaunchFailure> {
        let build = self
            .identify(&self.start.data.join(COPY))
            .map_err(|error| match error {
                LaunchFailure::NoExecutable if !self.found.lock().unwrap().is_empty() => {
                    LaunchFailure::NotAccepted
                }
                error => error,
            })?;
        if let Some(retained) = retained {
            self.start
                .clean_sessions(retained)
                .await
                .map_err(LaunchFailure::Runtime)?;
        }
        Runtime::start(&self.start, &build.version).await
    }
    pub(crate) async fn discover(&self, configured: Option<PathBuf>) -> Result<(), String> {
        let builds = self.clone();
        tokio::task::spawn_blocking(move || {
            builds.scan_places(configured.as_deref());
            let source = builds
                .accepted_copy()?
                .and_then(|copy| builds.source_for(&identity(&copy)));
            *builds.source.lock().unwrap() = source;
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())?
    }
    pub(crate) async fn select(
        &self,
        action: Action,
        settings: &mut Settings,
        configuration: &Path,
    ) -> Result<Selection, LaunchFailure> {
        let builds = self.clone();
        let mut changed = settings.clone();
        let configuration = configuration.to_path_buf();
        let (result, changed) = tokio::task::spawn_blocking(move || {
            let result = builds.select_blocking(action, &mut changed, &configuration);
            (result, changed)
        })
        .await
        .map_err(|error| error.to_string())?;
        *settings = changed;
        result
    }
    pub(crate) fn unavailable(
        &self,
        fault: crate::messages::RuntimeFault,
    ) -> crate::messages::Unavailable {
        let versions = self
            .accepted_builds()
            .map(|build| build.version.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        crate::messages::Unavailable {
            fault,
            versions,
            port: self.port,
        }
    }
    fn acceptance(&self, build: &ListedBuild) -> BuildAcceptance {
        if self
            .start
            .release
            .listed_builds()
            .iter()
            .any(|listed| listed.platform == build.platform && identity(listed) == identity(build))
        {
            BuildAcceptance::OnTheReleaseList
        } else {
            BuildAcceptance::CheckedOnThisComputer
        }
    }
    pub(crate) fn data_folder(&self) -> &Path {
        &self.start.data
    }
    pub(crate) fn summary(&self) -> Result<Summary, String> {
        let current = self.accepted_copy()?.map(|build| CurrentBuild {
            acceptance: self.acceptance(&build),
            version: build.version,
            // A removed source folder cannot be reconstructed from the local copy.
            source: self.source.lock().unwrap().clone(),
        });
        let found = self.found.lock().unwrap();
        let rows = found
            .iter()
            .map(|found| BuildRow {
                path: found.folder.join(SERVER),
                version: found.listed.as_ref().map(|build| build.version.clone()),
                identity: found.identity.to_string(),
                acceptance: found.listed.as_ref().map(|build| self.acceptance(build)),
            })
            .collect();
        let newer = current.as_ref().and_then(|current| {
            found
                .iter()
                .filter_map(|found| found.listed.as_ref())
                .filter(|build| version(&build.version) > version(&current.version))
                .max_by_key(|build| version(&build.version))
                .map(|build| build.version.clone())
        });
        let choices = found
            .iter()
            .filter_map(|found| {
                found.listed.as_ref().map(|build| {
                    (
                        identity(build).to_string(),
                        format!(
                            "{} — {}",
                            build.version,
                            found.folder.join(SERVER).display()
                        ),
                    )
                })
            })
            .collect();
        Ok(Summary {
            current,
            found: rows,
            release: self
                .start
                .release
                .listed_builds()
                .iter()
                .filter(|build| build.platform == self.start.release.platform())
                .map(|build| build.version.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            newer,
            choices,
        })
    }
}
fn scan(path: &Path, folders: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            folders.push(entry.path());
            scan(&entry.path(), folders);
        }
    }
}
pub(crate) const COPY: &str = "official-runtime";
pub(crate) const CHECK: &str = "official-runtime-check";
#[cfg(windows)]
pub(crate) const SERVER: &str = "agy_acp_server.exe";
#[cfg(not(windows))]
pub(crate) const SERVER: &str = "agy_acp_server.par";
#[cfg(windows)]
pub(crate) const HARNESS: &str = "localharness_external.exe";
#[cfg(not(windows))]
pub(crate) const HARNESS: &str = "localharness_external";
pub(crate) fn identity(build: &ListedBuild) -> BuildIdentity {
    BuildIdentity {
        server: build.agy_acp_server_sha256.clone(),
        harness: build.localharness_external_sha256.clone(),
    }
}

fn version(text: &str) -> Vec<u64> {
    text.split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Facts(Vec<ListedBuild>);
    impl crate::ReleaseFacts for Facts {
        fn version(&self) -> &str {
            "test"
        }
        fn platform(&self) -> &str {
            &self.0[0].platform
        }
        fn listed_builds(&self) -> &[ListedBuild] {
            &self.0
        }
        fn highest_concurrency_limit(&self) -> u8 {
            3
        }
    }
    struct RefusingLauncher(Arc<AtomicUsize>);
    impl crate::ProcessLauncher for RefusingLauncher {
        fn launch(&self, spec: crate::LaunchSpec) -> Result<crate::Launched, crate::LaunchError> {
            assert!(
                self.0.fetch_add(1, Ordering::SeqCst) < 3,
                "three launch cap"
            );
            Err(crate::LaunchError::Refused {
                program: spec.program,
            })
        }
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn an_accepted_copy_reaches_launch_before_search_and_recovers_its_source_afterward() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-search-after-start-{}",
                crate::random_hex::<12>().unwrap()
            ));
            for folder in [COPY, "Zed/1.4.0"] {
                fs::create_dir_all(root.join(folder)).unwrap();
                for name in [SERVER, HARNESS] {
                    fs::write(root.join(folder).join(name), b"accepted").unwrap();
                }
            }
            let mut builds = Builds::fixture(&root, Ports::production().clock);
            builds.environment.local_app_data = Some(root.clone());
            let hash = crate::hex(&Sha256::digest(b"accepted"));
            builds.start.release = Arc::new(Facts(vec![ListedBuild {
                version: "1.4.0".into(),
                platform: builds.start.release.platform().into(),
                agy_acp_server_sha256: hash.clone(),
                localharness_external_sha256: hash,
            }]));
            let launches = Arc::new(AtomicUsize::new(0));
            builds.start.launcher = Arc::new(RefusingLauncher(launches.clone()));
            let config = root.join("config.toml");
            let mut settings = crate::store::read(&config, 3).unwrap().settings;
            let selected = builds.select(Action::Initial, &mut settings, &config).await.unwrap();
            let _ = builds.launch(None).await;
            let at_start = builds.summary().unwrap();
            builds.discover(None).await.unwrap();
            let after_search = builds.summary().unwrap().current.unwrap().source;
            fs::remove_dir_all(&root).unwrap();
            assert_eq!(
                (
                    matches!(selected, Selection::Start(None)),
                    launches.load(Ordering::SeqCst),
                    at_start.found.len(),
                    at_start.current.unwrap().source,
                    after_search,
                ),
                (true, 1, 0, None, Some(root.join("Zed/1.4.0").join(SERVER))),
                "FR-042 reaches launch before search; FR-054 obtains the source from the later search"
            );
        }).await.expect("search order cap: 5 seconds, one refused launch, no child process, requests or logs");
    }

    #[cfg(unix)]
    #[test]
    #[allow(clippy::disallowed_methods)]
    fn a_recorded_pass_survives_check_folder_cleanup_failure() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "acp2api-check-cleanup-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let check = root.join(CHECK);
        fs::create_dir_all(&check).unwrap();
        for name in [SERVER, HARNESS] {
            fs::write(check.join(name), b"checked executables").unwrap();
        }
        let builds = Builds::fixture(&root, Ports::production().clock);
        let identity = Builds::hashes(&check).unwrap();
        fs::set_permissions(&check, fs::Permissions::from_mode(0o555)).unwrap();
        let result = builds.finish_check(&identity, Ok("1.6.0".into()));
        let leftover = check.exists();
        fs::set_permissions(&check, fs::Permissions::from_mode(0o755)).unwrap();
        let accepted = builds.identify(&root.join(COPY)).unwrap().version;
        fs::remove_dir_all(root).unwrap();
        assert!(
            matches!(result, Ok(ref notice) if notice.to_string() == "The compatibility check passed for 1.6.0.")
                && accepted == "1.6.0"
                && leftover,
            "a recorded, usable pass must reach the owner even when cleanup fails: {result:?}"
        );
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn checked_builds_are_reloaded_with_both_hashes_and_the_current_platform() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-checked-reload-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let copy = root.join(COPY);
        fs::create_dir_all(&copy).unwrap();
        for name in [SERVER, HARNESS] {
            fs::write(copy.join(name), b"checked executables").unwrap();
        }
        let ports = Ports::production();
        let hash = crate::hex(&Sha256::digest(b"checked executables"));
        let state = serde_json::json!({"format":1,"lockout":null,"checked_builds":[{
            "version":"1.6.0","platform":ports.release.platform(),
            "agy_acp_server_sha256":hash,"localharness_external_sha256":hash,"checked_at":"2026-10-18T12:00:00Z"
        }]});
        let path = root.join(crate::store::STATE_FILE);
        fs::write(&path, state.to_string()).unwrap();
        let builds = Builds::fixture(&root, ports.clock.clone());
        let config = root.join("config.toml");
        let mut settings = crate::store::read(&config, 3).unwrap().settings;
        let selected = builds
            .select(Action::Initial, &mut settings, &config)
            .await
            .unwrap();
        assert!(
            matches!(selected, Selection::Start(None)),
            "a previously checked copy must remain selected after a new start"
        );
        assert_eq!(builds.identify(&copy).unwrap().version, "1.6.0");
        for name in [SERVER, HARNESS] {
            fs::write(copy.join(name), b"changed").unwrap();
            assert!(
                matches!(builds.identify(&copy), Err(LaunchFailure::NotAccepted)),
                "each changed hash invalidates the checked build"
            );
            fs::write(copy.join(name), b"checked executables").unwrap();
        }
        let mut foreign = state;
        foreign["checked_builds"][0]["platform"] = "another-platform".into();
        fs::write(&path, foreign.to_string()).unwrap();
        let builds = Builds::fixture(&root, ports.clock);
        builds
            .select(Action::Initial, &mut settings, &config)
            .await
            .unwrap();
        assert!(
            matches!(builds.identify(&copy), Err(LaunchFailure::NotAccepted)),
            "a local check accepts only its recorded platform"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn a_later_start_removes_each_leftover_runtime_backup() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-old-copies-{}",
            crate::random_hex::<12>().unwrap()
        ));
        for folder in [
            ".official-runtime-old-first",
            ".official-runtime-old-second",
            "owner-files",
        ] {
            fs::create_dir_all(root.join(folder)).unwrap();
            fs::write(root.join(folder).join("sentinel"), b"saved").unwrap();
        }
        let builds = Builds::fixture(&root, Ports::production().clock);
        let config = root.join("config.toml");
        let mut settings = crate::store::read(&config, 3).unwrap().settings;
        builds
            .select(Action::Initial, &mut settings, &config)
            .await
            .unwrap();
        let actual = [
            ".official-runtime-old-first",
            ".official-runtime-old-second",
            "owner-files",
        ]
        .map(|folder| root.join(folder).join("sentinel").is_file());
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            actual,
            [false, false, true],
            "startup removes leftover backups and preserves other folders"
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn a_backup_deletion_failure_keeps_the_new_copy_launchable() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "acp2api-backup-{}",
            crate::random_hex::<12>().unwrap()
        ));
        for folder in [COPY, "source"] {
            fs::create_dir_all(root.join(folder)).unwrap();
            for name in [SERVER, HARNESS] {
                fs::write(root.join(folder).join(name), folder.as_bytes()).unwrap();
            }
        }
        fs::set_permissions(root.join(COPY), fs::Permissions::from_mode(0o500)).unwrap();
        let mut builds = Builds::fixture(&root, Ports::production().clock);
        let hash = crate::hex(&Sha256::digest(b"source"));
        builds.start.release = Arc::new(Facts(vec![ListedBuild {
            version: "1.4.0".into(),
            platform: builds.start.release.platform().into(),
            agy_acp_server_sha256: hash.clone(),
            localharness_external_sha256: hash,
        }]));
        let launches = Arc::new(AtomicUsize::new(0));
        builds.start.launcher = Arc::new(RefusingLauncher(launches.clone()));
        builds.copy(root.join("source")).unwrap();
        let _ = builds.launch(None).await;
        let backups: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".official-runtime-old-")
            })
            .map(|entry| entry.path())
            .collect();
        for backup in &backups {
            fs::set_permissions(backup, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let actual = (launches.load(Ordering::SeqCst), backups.len());
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            actual,
            (1, 1),
            "backup cleanup must not prevent launch after a successful copy swap"
        );
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn an_unreadable_copy_fails_startup_without_selecting_a_replacement() {
        for (unreadable, readable) in [(SERVER, Some(HARNESS)), (HARNESS, None)] {
            let root = std::env::temp_dir().join(format!(
                "acp2api-copy-read-{}",
                crate::random_hex::<12>().unwrap()
            ));
            fs::create_dir_all(root.join(COPY).join(unreadable)).unwrap();
            if let Some(readable) = readable {
                fs::write(root.join(COPY).join(readable), b"old companion").unwrap();
            }
            let ports = Ports::production();
            let builds = Builds::fixture(&root, ports.clock);
            let config = root.join("config.toml");
            let mut settings = crate::store::read(&config, 3).unwrap().settings;
            let actual = builds
                .select(Action::Initial, &mut settings, &config)
                .await
                .err();
            let preserved = root.join(COPY).join(unreadable).is_dir();
            fs::remove_dir_all(&root).unwrap();
            assert!(
                actual
                    .as_ref()
                    .is_some_and(|error| error.to_string().contains("cannot read")),
                "a copy read error must fail startup, including when the other executable is missing: {actual:?}"
            );
            assert!(preserved, "the unreadable copy must not be replaced");
        }
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn owner_selects_a_build_by_hashes_when_versions_match() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-build-identity-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let mut ports = Ports::production();
        let mut listed = Vec::new();
        for label in ["first", "second"] {
            fs::create_dir_all(root.join(label)).unwrap();
            for name in [SERVER, HARNESS] {
                fs::write(root.join(label).join(name), label.as_bytes()).unwrap();
            }
            let hash = crate::hex(&Sha256::digest(label.as_bytes()));
            listed.push(ListedBuild {
                version: "1.4.0".into(),
                platform: ports.release.platform().into(),
                agy_acp_server_sha256: hash.clone(),
                localharness_external_sha256: hash,
            });
        }
        let choice = format!(
            "{}:{}",
            listed[1].agy_acp_server_sha256, listed[1].localharness_external_sha256
        );
        ports.release = Arc::new(Facts(listed));
        let builds = Builds::new(
            &ports,
            Environment {
                account_root: root.join("account"),
                home: None,
                local_app_data: None,
                temp: root.join("temp"),
                path: vec![root.join("first"), root.join("second")],
                exe_dir: root.join("exe"),
            },
            root.clone(),
            crate::account::Temporary::fixture(&root),
            crate::runtime_driver::mcp::ToolSets::new(0),
            tokio::sync::watch::channel(false).1,
            0,
        );
        let config = root.join("config.toml");
        let mut settings = crate::store::read(&config, 3).unwrap().settings;
        builds
            .discover(settings.agy_acp_server_path.clone())
            .await
            .unwrap();
        let selected = builds
            .select(Action::Build(choice.to_string()), &mut settings, &config)
            .await
            .ok()
            .and_then(|selection| match selection {
                Selection::Start(build) => build.map(|build| build.folder),
                Selection::Keep | Selection::Notice(_) | Selection::Check { .. } => None,
            });
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            selected,
            Some(root.join("second")),
            "equal versions do not identify a build; the executable pair does"
        );
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn a_failed_copy_preserves_the_previous_executables_and_removes_staging() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-staged-copy-{}",
            crate::random_hex::<12>().unwrap()
        ));
        for folder in ["official-runtime", "source"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        for name in [SERVER, HARNESS] {
            fs::write(root.join(COPY).join(name), b"old").unwrap();
        }
        fs::write(root.join("source").join(SERVER), b"new").unwrap();
        fs::create_dir(root.join("source").join(HARNESS)).unwrap();
        let ports = Ports::production();
        let builds = Builds::new(
            &ports,
            Environment {
                account_root: root.join("account"),
                home: None,
                local_app_data: None,
                temp: root.join("temp"),
                path: vec![],
                exe_dir: root.join("exe"),
            },
            root.clone(),
            crate::account::Temporary::fixture(&root),
            crate::runtime_driver::mcp::ToolSets::new(0),
            tokio::sync::watch::channel(false).1,
            0,
        );
        let failed = builds.copy(root.join("source")).is_err();
        let bytes = [SERVER, HARNESS].map(|name| fs::read(root.join(COPY).join(name)).unwrap());
        let staging = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".official-runtime-")
            })
            .count();
        fs::remove_dir_all(root).unwrap();
        assert_eq!(
            (failed, bytes, staging),
            (true, [b"old".to_vec(), b"old".to_vec()], 0),
            "failed replacement must leave the accepted copy unchanged and no staging folder"
        );
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn rejected_staging_hashes_preserve_the_accepted_copy() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-staged-hashes-{}",
            crate::random_hex::<12>().unwrap()
        ));
        for folder in [COPY, "source"] {
            fs::create_dir_all(root.join(folder)).unwrap();
            for name in [SERVER, HARNESS] {
                fs::write(
                    root.join(folder).join(name),
                    if folder == COPY { b"old" } else { b"new" },
                )
                .unwrap();
            }
        }
        let mut builds = Builds::fixture(&root, Ports::production().clock);
        let hash = crate::hex(&Sha256::digest(b"old"));
        builds.start.release = Arc::new(Facts(vec![ListedBuild {
            version: "1.4.0".into(),
            platform: builds.start.release.platform().into(),
            agy_acp_server_sha256: hash.clone(),
            localharness_external_sha256: hash,
        }]));
        let failed = builds.copy(root.join("source")).is_err();
        let bytes = [SERVER, HARNESS].map(|name| fs::read(root.join(COPY).join(name)).unwrap());
        let staging = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".official-runtime-")
            })
            .count();
        fs::remove_dir_all(root).unwrap();
        assert_eq!(
            (failed, bytes, staging),
            (true, [b"old".to_vec(), b"old".to_vec()], 0),
            "unaccepted replacement hashes must preserve the accepted executable pair and remove staging"
        );
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn search_skips_an_unreadable_place_and_keeps_other_results() {
        let root = std::env::temp_dir().join(format!(
            "acp2api-search-error-{}",
            crate::random_hex::<12>().unwrap()
        ));
        fs::create_dir_all(root.join("home/Library/Application Support")).unwrap();
        fs::write(
            root.join("home/Library/Application Support/Zed"),
            b"not a directory",
        )
        .unwrap();
        fs::create_dir(root.join("exe")).unwrap();
        for name in [SERVER, HARNESS] {
            fs::write(root.join("exe").join(name), b"found").unwrap();
        }
        let ports = Ports::production();
        let builds = Builds::new(
            &ports,
            Environment {
                account_root: root.join("account"),
                home: Some(root.join("home")),
                local_app_data: None,
                temp: root.join("temp"),
                path: vec![],
                exe_dir: root.join("exe"),
            },
            root.clone(),
            crate::account::Temporary::fixture(&root),
            crate::runtime_driver::mcp::ToolSets::new(0),
            tokio::sync::watch::channel(false).1,
            0,
        );
        let config = root.join("config.toml");
        let mut settings = crate::store::read(&config, 3).unwrap().settings;
        let result = builds.select(Action::Search, &mut settings, &config).await;
        let found = builds.summary().unwrap().found;
        fs::remove_dir_all(root).unwrap();
        assert!(
            result.is_ok() && found.len() == 1,
            "a bad search place must not hide a readable build"
        );
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn stop_during_restart_ends_the_queued_request_with_m15() {
        use crate::{BoxFuture, Clock, Launched, ProcessLauncher, ProcessTree};
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
        struct Tree;
        impl ProcessTree for Tree {
            fn pid(&self) -> u32 {
                std::process::id()
            }
            fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
                Box::pin(std::future::pending())
            }
            fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
                Box::pin(async {})
            }
        }
        struct Launcher(
            Mutex<std::collections::VecDeque<Launched>>,
            tokio::sync::watch::Sender<bool>,
            AtomicUsize,
        );
        impl ProcessLauncher for Launcher {
            fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
                let attempt = self.2.fetch_add(1, Ordering::SeqCst);
                assert!(attempt < 2, "two runtime starts only");
                if attempt == 1 {
                    self.1.send_replace(true);
                }
                Ok(self.0.lock().unwrap().pop_front().unwrap())
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!("acp2api-stop-restart-{}", crate::random_hex::<12>().unwrap()));
            fs::create_dir_all(root.join(COPY)).unwrap();
            for name in [SERVER, HARNESS] { fs::write(root.join(COPY).join(name), b"accepted").unwrap(); }
            let hash = crate::hex(&Sha256::digest(b"accepted"));
            let (stopping, stopped) = tokio::sync::watch::channel(false);
            let (first, agent) = tokio::io::duplex(65536);
            let (second, _silent_agent) = tokio::io::duplex(65536);
            let mut children = std::collections::VecDeque::new();
            for child in [first, second] {
                let (stdout, stdin) = tokio::io::split(child);
                children.push_back(Launched { stdin: Box::pin(stdin), stdout: Box::pin(stdout), stderr: Box::pin(tokio::io::empty()), tree: Box::new(Tree) });
            }
            let prompting = Arc::new(tokio::sync::Notify::new());
            let exit = Arc::new(tokio::sync::Notify::new());
            let agent_task = {
                let prompting = prompting.clone(); let exit = exit.clone();
                tokio::spawn(async move {
                    let (input, mut output) = tokio::io::split(agent);
                    let mut input = tokio::io::BufReader::new(input);
                    for _ in 0..4 {
                        let mut bytes = Vec::new();
                        (&mut input).take(65537).read_until(b'\n', &mut bytes).await.unwrap();
                        assert!(bytes.len() <= 65536, "ACP line cap: 64 KiB");
                        let call: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                        let result = match call["method"].as_str().unwrap() {
                            "initialize" => serde_json::json!({"protocolVersion":1,"agentInfo":{"version":"1.4.0"}}),
                            "session/new" => serde_json::json!({"sessionId":"first","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]}),
                            "session/prompt" => { prompting.notify_one(); exit.notified().await; return; },
                            method => panic!("unexpected ACP method: {method}"),
                        };
                        output.write_all(format!("{}\n", serde_json::json!({"jsonrpc":"2.0","id":call["id"],"result":result})).as_bytes()).await.unwrap();
                    }
                    panic!("first runtime request cap: four");
                })
            };
            let mut ports = Ports::production();
            ports.release = Arc::new(Facts(vec![ListedBuild { version:"1.4.0".into(), platform:ports.release.platform().into(), agy_acp_server_sha256:hash.clone(), localharness_external_sha256:hash }]));
            ports.launcher = Arc::new(Launcher(Mutex::new(children), stopping, AtomicUsize::new(0)));
            let builds = Builds::new(&ports, Environment { account_root:root.join("account"), home:None, local_app_data:None, temp:root.join("temp"), path:vec![], exe_dir:root.join("exe") }, root.clone(), crate::account::Temporary::fixture(&root), crate::runtime_driver::mcp::ToolSets::new(0), stopped, 0);
            let config = root.join("config.toml");
            let mut settings = crate::store::read(&config, 3).unwrap().settings;
            settings.concurrency_limit = 1;
            let journal = crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = crate::conductor::Conductor::start(builds, ports.clock.clone(), journal.clone(), settings, config, 3);
            conductor.builds(Action::Initial, crate::Hold::empty()).await.unwrap();
            let parse = |text: &str| crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"user","content":text}]})).unwrap()).ok().unwrap().turn;
            let _a = conductor.admit(parse("A"), crate::Hold::empty(), journal.begin(crate::journal::Api::Chat, 0), crate::conductor::ResponseMode::Body).await.ok().unwrap();
            prompting.notified().await;
            let mut b = conductor.admit(parse("B"), crate::Hold::empty(), journal.begin(crate::journal::Api::Chat, 0), crate::conductor::ResponseMode::Body).await.ok().unwrap();
            exit.notify_one();
            let Some((crate::conversation::TurnEvent::End(Err(error)), _hold)) = b.events.recv().await else {
                panic!("the queued request must end with an error");
            };
            let message = error.to_string();
            drop(conductor);
            task.await.unwrap(); agent_task.await.unwrap(); fs::remove_dir_all(root).unwrap();
            assert_eq!(message, "acp2api: acp2api stopped before the end of this request.", "FR-045 a stop during restart must use the exit path");
        }).await.expect("stop-restart cap: five seconds, two starts, four first-runtime requests and 64 KiB per line");
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn the_end_of_a_stored_lockout_runs_the_deferred_start() {
        use crate::{BoxFuture, Clock, Launched, ProcessLauncher, ProcessTree};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        struct Tree;
        impl ProcessTree for Tree {
            fn pid(&self) -> u32 {
                std::process::id()
            }
            fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
                Box::pin(std::future::pending())
            }
            fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
                Box::pin(async {})
            }
        }
        struct Launcher(Arc<AtomicUsize>);
        impl ProcessLauncher for Launcher {
            fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                let (child, agent) = tokio::io::duplex(65536);
                let (stdout, stdin) = tokio::io::split(child);
                tokio::spawn(async move {
                    let (input, mut output) = tokio::io::split(agent);
                    let mut input = tokio::io::BufReader::new(input);
                    for _ in 0..4 {
                        let mut bytes = Vec::new();
                        if input.read_until(b'\n', &mut bytes).await.unwrap_or(0) == 0 {
                            break;
                        }
                        assert!(bytes.len() <= 65536, "ACP line cap: 64 KiB");
                        let call: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                        let result = match call["method"].as_str().unwrap() {
                            "initialize" => {
                                serde_json::json!({"protocolVersion":1,"agentInfo":{"version":"1.6.0"}})
                            }
                            "session/new" => {
                                serde_json::json!({"sessionId":"first","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]})
                            }
                            method => panic!("unexpected ACP method: {method}"),
                        };
                        let line = format!(
                            "{}\n",
                            serde_json::json!({"jsonrpc":"2.0","id":call["id"],"result":result})
                        );
                        if output.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                });
                Ok(Launched {
                    stdin: Box::pin(stdin),
                    stdout: Box::pin(stdout),
                    stderr: Box::pin(tokio::io::empty()),
                    tree: Box::new(Tree),
                })
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-deferred-start-{}",
                crate::random_hex::<12>().unwrap()
            ));
            let copy = root.join(COPY);
            fs::create_dir_all(&copy).unwrap();
            for name in [SERVER, HARNESS] {
                fs::write(copy.join(name), b"checked-only-binary").unwrap();
            }
            let hash = crate::hex(&Sha256::digest(b"checked-only-binary"));
            let mut ports = Ports::production();
            let platform = ports.release.platform().to_string();
            ports.release = Arc::new(Facts(vec![ListedBuild {
                version: "1.4.0".into(),
                platform: platform.clone(),
                agy_acp_server_sha256: "other-hash".repeat(2),
                localharness_external_sha256: "other-hash".repeat(2),
            }]));
            let launches = Arc::new(AtomicUsize::new(0));
            ports.launcher = Arc::new(Launcher(launches.clone()));
            let state = serde_json::json!({
                "format": 1,
                "lockout": {
                    "since": "2026-10-18T12:00:00Z",
                    "signal": "403",
                    "original_error_text": "Account suspended",
                },
                "checked_builds": [{
                    "version": "1.6.0",
                    "platform": platform,
                    "agy_acp_server_sha256": hash,
                    "localharness_external_sha256": hash,
                    "checked_at": "2026-10-18T12:00:00Z"
                }]
            });
            let state_path = root.join(crate::store::STATE_FILE);
            fs::write(&state_path, state.to_string()).unwrap();
            let builds = Builds::new(
                &ports,
                Environment {
                    account_root: root.join("account"),
                    home: None,
                    local_app_data: None,
                    temp: root.join("temp"),
                    path: vec![],
                    exe_dir: root.join("exe"),
                },
                root.clone(),
                crate::account::Temporary::fixture(&root),
                crate::runtime_driver::mcp::ToolSets::new(0),
                tokio::sync::watch::channel(false).1,
                0,
            );
            let config = root.join("config.toml");
            let settings = crate::store::read(&config, 3).unwrap().settings;
            let journal =
                crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = crate::conductor::Conductor::start(
                builds,
                ports.clock.clone(),
                journal,
                settings,
                config,
                3,
            );
            let initial = conductor
                .builds(Action::Initial, crate::Hold::empty())
                .await;
            assert!(matches!(initial, Ok(None)));
            assert_eq!(launches.load(Ordering::SeqCst), 0);
            let owner = conductor
                .owner(
                    crate::conductor::OwnerAction::EndLockout,
                    crate::Hold::empty(),
                )
                .await;
            assert!(matches!(owner, Ok(())));
            while launches.load(Ordering::SeqCst) < 1 {
                if conductor.view().state == crate::conductor::State::RuntimeUnavailable {
                    break;
                }
                tokio::task::yield_now().await;
            }
            assert_eq!(launches.load(Ordering::SeqCst), 1);
            while conductor.view().state != crate::conductor::State::Running {
                tokio::task::yield_now().await;
            }
            assert_eq!(conductor.view().state, crate::conductor::State::Running);
            drop(conductor);
            task.await.unwrap();
            fs::remove_dir_all(root).unwrap();
        })
        .await
        .expect("deferred start cap: 5 seconds");
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn a_failed_deferred_start_gives_runtime_unavailable() {
        use crate::{BoxFuture, Clock, Launched, ProcessLauncher, ProcessTree};
        use std::os::unix::fs::PermissionsExt;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        struct Tree;
        impl ProcessTree for Tree {
            fn pid(&self) -> u32 {
                std::process::id()
            }
            fn exited(&mut self) -> BoxFuture<'_, std::process::ExitStatus> {
                Box::pin(std::future::pending())
            }
            fn stop(self: Box<Self>, _: Arc<dyn Clock>) -> BoxFuture<'static, ()> {
                Box::pin(async {})
            }
        }
        struct Launcher(Arc<AtomicUsize>);
        impl ProcessLauncher for Launcher {
            fn launch(&self, _: crate::LaunchSpec) -> Result<Launched, crate::LaunchError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                let (child, agent) = tokio::io::duplex(65536);
                let (stdout, stdin) = tokio::io::split(child);
                tokio::spawn(async move {
                    let (input, mut output) = tokio::io::split(agent);
                    let mut input = tokio::io::BufReader::new(input);
                    for _ in 0..4 {
                        let mut bytes = Vec::new();
                        if input.read_until(b'\n', &mut bytes).await.unwrap_or(0) == 0 {
                            break;
                        }
                        assert!(bytes.len() <= 65536, "ACP line cap: 64 KiB");
                        let call: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                        let result = match call["method"].as_str().unwrap() {
                            "initialize" => {
                                serde_json::json!({"protocolVersion":1,"agentInfo":{"version":"1.6.0"}})
                            }
                            "session/new" => {
                                serde_json::json!({"sessionId":"first","configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]})
                            }
                            method => panic!("unexpected ACP method: {method}"),
                        };
                        let line = format!(
                            "{}\n",
                            serde_json::json!({"jsonrpc":"2.0","id":call["id"],"result":result})
                        );
                        if output.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                });
                Ok(Launched {
                    stdin: Box::pin(stdin),
                    stdout: Box::pin(stdout),
                    stderr: Box::pin(tokio::io::empty()),
                    tree: Box::new(Tree),
                })
            }
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-deferred-start-failed-{}",
                crate::random_hex::<12>().unwrap()
            ));
            let copy = root.join(COPY);
            fs::create_dir_all(&copy).unwrap();
            for name in [SERVER, HARNESS] {
                fs::write(copy.join(name), b"checked-only-binary").unwrap();
            }
            let hash = crate::hex(&Sha256::digest(b"checked-only-binary"));
            let mut ports = Ports::production();
            let platform = ports.release.platform().to_string();
            ports.release = Arc::new(Facts(vec![ListedBuild {
                version: "1.4.0".into(),
                platform: platform.clone(),
                agy_acp_server_sha256: "other-hash".repeat(2),
                localharness_external_sha256: "other-hash".repeat(2),
            }]));
            let launches = Arc::new(AtomicUsize::new(0));
            ports.launcher = Arc::new(Launcher(launches.clone()));
            let state = serde_json::json!({
                "format": 1,
                "lockout": {
                    "since": "2026-10-18T12:00:00Z",
                    "signal": "403",
                    "original_error_text": "Account suspended",
                },
                "checked_builds": [{
                    "version": "1.6.0",
                    "platform": platform,
                    "agy_acp_server_sha256": hash,
                    "localharness_external_sha256": hash,
                    "checked_at": "2026-10-18T12:00:00Z"
                }]
            });
            let state_path = root.join(crate::store::STATE_FILE);
            fs::write(&state_path, state.to_string()).unwrap();
            let builds = Builds::new(
                &ports,
                Environment {
                    account_root: root.join("account"),
                    home: None,
                    local_app_data: None,
                    temp: root.join("temp"),
                    path: vec![],
                    exe_dir: root.join("exe"),
                },
                root.clone(),
                crate::account::Temporary::fixture(&root),
                crate::runtime_driver::mcp::ToolSets::new(0),
                tokio::sync::watch::channel(false).1,
                0,
            );
            let config = root.join("config.toml");
            let settings = crate::store::read(&config, 3).unwrap().settings;
            let journal =
                crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = crate::conductor::Conductor::start(
                builds,
                ports.clock.clone(),
                journal,
                settings,
                config,
                3,
            );
            let initial = conductor
                .builds(Action::Initial, crate::Hold::empty())
                .await;
            assert!(matches!(initial, Ok(None)));
            assert_eq!(launches.load(Ordering::SeqCst), 0);
            fs::set_permissions(&copy, fs::Permissions::from_mode(0o000)).unwrap();
            let owner = conductor
                .owner(
                    crate::conductor::OwnerAction::EndLockout,
                    crate::Hold::empty(),
                )
                .await;
            assert!(matches!(owner, Ok(())));
            while conductor.view().state != crate::conductor::State::RuntimeUnavailable {
                tokio::task::yield_now().await;
            }
            assert_eq!(launches.load(Ordering::SeqCst), 0);
            fs::set_permissions(&copy, fs::Permissions::from_mode(0o755)).unwrap();
            drop(conductor);
            task.await.unwrap();
            fs::remove_dir_all(root).unwrap();
        })
        .await
        .expect("deferred start bound: 5 seconds");
    }

    #[tokio::test]
    async fn an_accepted_copy_start_failure_retries_until_runtime_unavailable() {
        refused_starts(|_| Action::Initial).await;
    }

    #[tokio::test]
    async fn a_planned_replacement_start_failure_retries_until_runtime_unavailable() {
        refused_starts(|build| Action::Build(identity(build).to_string())).await;
    }

    #[allow(clippy::disallowed_methods)]
    async fn refused_starts(action: fn(&ListedBuild) -> Action) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-launch-reason-{}",
                crate::random_hex::<12>().unwrap()
            ));
            fs::create_dir_all(root.join(COPY)).unwrap();
            for name in [SERVER, HARNESS] {
                fs::write(root.join(COPY).join(name), b"accepted").unwrap();
            }
            let hash = crate::hex(&Sha256::digest(b"accepted"));
            let mut ports = Ports::production();
            ports.release = Arc::new(Facts(vec![ListedBuild {
                version: "1.4.0".into(),
                platform: ports.release.platform().into(),
                agy_acp_server_sha256: hash.clone(),
                localharness_external_sha256: hash,
            }]));
            let launches = Arc::new(AtomicUsize::new(0));
            ports.launcher = Arc::new(RefusingLauncher(launches.clone()));
            let builds = Builds::new(
                &ports,
                Environment {
                    account_root: root.join("account"),
                    home: None,
                    local_app_data: None,
                    temp: root.join("temp"),
                    path: vec![],
                    exe_dir: root.join("exe"),
                },
                root.clone(),
                crate::account::Temporary::fixture(&root),
                crate::runtime_driver::mcp::ToolSets::new(0),
                tokio::sync::watch::channel(false).1,
                0,
            );
            builds.discover(Some(root.join(COPY).join(SERVER))).await.unwrap();
            let action = action(&ports.release.listed_builds()[0]);
            let config = root.join("config.toml");
            let settings = crate::store::read(&config, 3).unwrap().settings;
            let journal =
                crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = crate::conductor::Conductor::start(
                builds.clone(),
                ports.clock.clone(),
                journal,
                settings,
                config,
                3,
            );
            let result = conductor
                .builds(action, crate::Hold::empty())
                .await;
            let unavailable = conductor.model_names(crate::Hold::empty()).await;
            drop(conductor);
            task.await.unwrap();
            fs::remove_dir_all(root).unwrap();
            assert!(result.is_ok() && launches.load(Ordering::SeqCst) == 3
                && unavailable.is_err_and(|error| error.message.to_string().contains("the runtime process failed 3 times in 10 minutes")),
                "FR-041 counts three verified start failures before runtime unavailable: {result:?}");
        })
        .await
        .expect(
            "launch failure cap: 5 seconds; three refused launches, no child process or HTTP requests",
        );
    }

    #[tokio::test]
    #[allow(
        clippy::disallowed_methods,
        reason = "Bounded native filesystem regression; the launcher starts no process."
    )]
    async fn selecting_the_existing_copy_preserves_both_executables() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-own-copy-{}",
                crate::random_hex::<12>().unwrap()
            ));
            let copy = root.join(COPY);
            fs::create_dir_all(&copy).unwrap();
            let original = [
                b"official executable".as_slice(),
                b"official companion".as_slice(),
            ];
            for (name, bytes) in [SERVER, HARNESS].into_iter().zip(original) {
                fs::write(copy.join(name), bytes).unwrap();
            }
            let mut ports = Ports::production();
            ports.release = Arc::new(Facts(vec![ListedBuild {
                version: "1.4.0".into(),
                platform: ports.release.platform().into(),
                agy_acp_server_sha256: crate::hex(&Sha256::digest(original[0])),
                localharness_external_sha256: crate::hex(&Sha256::digest(original[1])),
            }]));
            let launches = Arc::new(AtomicUsize::new(0));
            ports.launcher = Arc::new(RefusingLauncher(launches.clone()));
            let environment = Environment {
                account_root: root.join("account"),
                home: None,
                local_app_data: None,
                temp: root.join("temp"),
                path: vec![],
                exe_dir: root.join("exe"),
            };
            let builds = Builds::new(
                &ports,
                environment,
                root.clone(),
                crate::account::Temporary::fixture(&root),
                crate::runtime_driver::mcp::ToolSets::new(0),
                tokio::sync::watch::channel(false).1,
                0,
            );
            let config = root.join("config.toml");
            let mut settings = crate::store::read(&config, 3).unwrap().settings;
            builds
                .discover(settings.agy_acp_server_path.clone())
                .await
                .unwrap();
            let Selection::Start(source) = builds
                .select(
                    Action::Build(identity(&builds.start.release.listed_builds()[0]).to_string()),
                    &mut settings,
                    &config,
                )
                .await
                .unwrap()
            else {
                panic!("the listed build must be selectable")
            };
            builds
                .copy(source.expect("selected build source").folder)
                .unwrap();
            let _ = builds.launch(None).await;
            let observed = [SERVER, HARNESS].map(|name| fs::read(copy.join(name)).unwrap());
            fs::remove_dir_all(&root).unwrap();
            assert_eq!(
                launches.load(Ordering::SeqCst),
                1,
                "selection must reach the process boundary"
            );
            assert_eq!(
                observed[0], original[0],
                "selection must retain the executable"
            );
            assert_eq!(
                observed[1], original[1],
                "selection must retain the companion"
            );
        })
        .await
        .expect("own-copy regression cap: 5 seconds");
    }
}
