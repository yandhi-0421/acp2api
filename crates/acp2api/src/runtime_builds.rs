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
}
#[derive(Clone)]
struct Found {
    folder: PathBuf,
    listed: Option<ListedBuild>,
}
pub(crate) struct Summary {
    pub(crate) found: Vec<(PathBuf, String)>,
    pub(crate) newer: Option<String>,
}
pub(crate) enum Action {
    Initial,
    Search,
    Path(PathBuf),
    Build(String),
}
pub(crate) enum Selection {
    Keep,
    Start(Option<PathBuf>),
}
impl Builds {
    pub(crate) fn new(
        ports: &Ports,
        environment: Environment,
        data: PathBuf,
        temporary: crate::account::Temporary,
        tools: crate::runtime_driver::mcp::ToolSets,
        port: u16,
    ) -> Self {
        Self {
            start: RuntimeStart {
                clock: ports.clock.clone(),
                stopped: tokio::sync::watch::channel(false).1,
                launcher: ports.launcher.clone(),
                release: ports.release.clone(),
                data,
                temporary,
                tool_sets: tools,
            },
            environment,
            port,
            found: Arc::default(),
        }
    }
    fn identify(&self, folder: &Path) -> Result<Option<ListedBuild>, String> {
        let mut hashes = Vec::new();
        for name in names() {
            let path = folder.join(name);
            match fs::read(&path) {
                Ok(bytes) => hashes.push(crate::hex(&Sha256::digest(bytes))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
            }
        }
        Ok(self
            .start
            .release
            .listed_builds()
            .iter()
            .find(|build| {
                build.agy_acp_server_sha256 == hashes[0]
                    && build.localharness_external_sha256 == hashes[1]
            })
            .cloned())
    }
    pub(crate) fn accepted_copy(&self) -> Result<Option<ListedBuild>, String> {
        Ok(self
            .identify(&self.start.data.join("official-runtime"))?
            .filter(|build| build.platform == self.start.release.platform()))
    }
    fn discover(&self, configured: Option<&Path>) -> Result<(), String> {
        let mut folders = vec![
            self.environment.exe_dir.clone(),
            self.start.data.join("official-runtime"),
        ];
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
            if names().iter().all(|name| folder.join(name).is_file())
                && let Ok(listed) = self.identify(&folder)
            {
                found.push(Found { listed, folder });
            }
        }
        *self.found.lock().unwrap() = found;
        Ok(())
    }
    pub(crate) fn select(
        &self,
        action: Action,
        settings: &mut Settings,
        configuration: &Path,
    ) -> Result<(Selection, String), String> {
        let initial = matches!(action, Action::Initial);
        match action {
            Action::Path(path) => {
                if !path.is_absolute() {
                    return Err("Enter an absolute path of agy_acp_server.".into());
                }
                if !path
                    .parent()
                    .is_some_and(|folder| folder.join(names()[1]).is_file())
                {
                    return Ok((Selection::Keep, "This folder has no localharness_external, so acp2api did not use this path.".into()));
                }
                if !path.is_file() {
                    return Err("This path has no agy_acp_server.".into());
                }
                settings.agy_acp_server_path = Some(path);
                crate::store::save(configuration, settings)?;
            }
            Action::Build(version) => {
                let found = self.found.lock().unwrap();
                let selected = found
                    .iter()
                    .find(|found| {
                        found.listed.as_ref().is_some_and(|build| {
                            build.version == version
                                && build.platform == self.start.release.platform()
                        })
                    })
                    .ok_or("Choose an accepted build from the search results.")?;
                return Ok((
                    Selection::Start(Some(selected.folder.clone())),
                    String::new(),
                ));
            }
            Action::Initial if self.accepted_copy()?.is_some() => {
                return Ok((Selection::Start(None), String::new()));
            }
            Action::Initial | Action::Search => {}
        }
        self.discover(settings.agy_acp_server_path.as_deref())?;
        if !initial && self.accepted_copy()?.is_some() {
            return Ok((Selection::Keep, String::new()));
        }
        let found = self.found.lock().unwrap();
        let best = found
            .iter()
            .filter(|found| {
                found
                    .listed
                    .as_ref()
                    .is_some_and(|build| build.platform == self.start.release.platform())
            })
            .max_by_key(|found| version(&found.listed.as_ref().unwrap().version));
        Ok((
            Selection::Start(best.map(|found| found.folder.clone())),
            String::new(),
        ))
    }
    pub(crate) async fn launch(
        &self,
        source: Option<PathBuf>,
    ) -> Result<(Runtime, crate::catalog::Catalog), String> {
        if let Some(source) = source {
            let copy = self.start.data.join("official-runtime");
            let source = fs::canonicalize(source).map_err(|error| error.to_string())?;
            if fs::canonicalize(&copy).ok().as_ref() != Some(&source) {
                let suffix = crate::random_hex::<12>().map_err(|error| error.to_string())?;
                let staging = self.start.data.join(format!(".official-runtime-{suffix}"));
                let backup = self
                    .start
                    .data
                    .join(format!(".official-runtime-old-{suffix}"));
                fs::create_dir(&staging).map_err(|error| error.to_string())?;
                let result = (|| {
                    for name in names() {
                        fs::copy(source.join(name), staging.join(name))
                            .map_err(|error| error.to_string())?;
                    }
                    if self
                        .identify(&staging)?
                        .filter(|build| build.platform == self.start.release.platform())
                        .is_none()
                    {
                        return Err("no accepted copy".into());
                    }
                    let previous = copy.exists();
                    if previous {
                        fs::rename(&copy, &backup).map_err(|error| error.to_string())?;
                    }
                    if let Err(error) = fs::rename(&staging, &copy) {
                        if previous {
                            fs::rename(&backup, &copy).map_err(|error| error.to_string())?;
                        }
                        return Err(error.to_string());
                    }
                    if previous {
                        fs::remove_dir_all(backup).map_err(|error| error.to_string())?;
                    }
                    Ok(())
                })();
                if staging.exists() {
                    fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
                }
                result?;
            }
        }
        let build = self.accepted_copy()?.ok_or("no accepted copy")?;
        Runtime::start(&self.start, &build.version).await
    }
    pub(crate) fn after_start(&self, settings: &Settings) -> Result<(), String> {
        self.discover(settings.agy_acp_server_path.as_deref())
    }
    pub(crate) fn unavailable(&self, error: &str) -> String {
        let reason = if error != "no accepted copy" {
            error
        } else if self.found.lock().unwrap().is_empty() {
            "acp2api found no executable of the official runtime"
        } else {
            "no build that acp2api found is an accepted version"
        };
        let versions = self
            .start
            .release
            .listed_builds()
            .iter()
            .filter(|build| build.platform == self.start.release.platform())
            .map(|build| build.version.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "acp2api: the official runtime is unavailable: {reason}. Accepted versions: {versions}. See http://127.0.0.1:{}/.",
            self.port
        )
    }
    pub(crate) fn summary(&self) -> Result<Summary, String> {
        let current = self.accepted_copy()?.map(|build| build.version);
        let found = self.found.lock().unwrap();
        let rows = found
            .iter()
            .map(|found| {
                (
                    found.folder.join(names()[0]),
                    found
                        .listed
                        .as_ref()
                        .map(|build| build.version.clone())
                        .unwrap_or_else(|| "Not an accepted version".into()),
                )
            })
            .collect();
        let newer = current.as_ref().and_then(|current| {
            found
                .iter()
                .filter_map(|found| found.listed.as_ref())
                .filter(|build| {
                    build.platform == self.start.release.platform()
                        && version(&build.version) > version(current)
                })
                .max_by_key(|build| version(&build.version))
                .map(|build| build.version.clone())
        });
        Ok(Summary { found: rows, newer })
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
pub(crate) fn names() -> [&'static str; 2] {
    if cfg!(windows) {
        ["agy_acp_server.exe", "localharness_external.exe"]
    } else {
        ["agy_acp_server.par", "localharness_external"]
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
            assert_eq!(self.0.fetch_add(1, Ordering::SeqCst), 0, "one launch cap");
            Err(crate::LaunchError::Refused {
                program: spec.program,
            })
        }
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
        for name in names() {
            fs::write(root.join("official-runtime").join(name), b"old").unwrap();
        }
        fs::write(root.join("source").join(names()[0]), b"new").unwrap();
        fs::create_dir(root.join("source").join(names()[1])).unwrap();
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
            0,
        );
        let failed = builds.launch(Some(root.join("source"))).await.is_err();
        let bytes = names().map(|name| fs::read(root.join("official-runtime").join(name)).unwrap());
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

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn search_skips_an_unreadable_place_and_keeps_other_results() {
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
        for name in names() {
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
            0,
        );
        let config = root.join("config.toml");
        let mut settings = crate::store::read(&config, 3).unwrap().settings;
        let result = builds.select(Action::Search, &mut settings, &config);
        let found = builds.summary().unwrap().found;
        fs::remove_dir_all(root).unwrap();
        assert!(
            result.is_ok() && found.len() == 1,
            "a bad search place must not hide a readable build"
        );
    }

    #[tokio::test]
    #[allow(clippy::disallowed_methods)]
    async fn an_accepted_copy_reports_the_actual_launch_failure() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let root = std::env::temp_dir().join(format!(
                "acp2api-launch-reason-{}",
                crate::random_hex::<12>().unwrap()
            ));
            fs::create_dir_all(root.join("official-runtime")).unwrap();
            for name in names() {
                fs::write(root.join("official-runtime").join(name), b"accepted").unwrap();
            }
            let hash = crate::hex(&Sha256::digest(b"accepted"));
            let mut ports = Ports::production();
            ports.release = Arc::new(Facts(vec![ListedBuild {
                version: "1.4.0".into(),
                platform: ports.release.platform().into(),
                agy_acp_server_sha256: hash.clone(),
                localharness_external_sha256: hash,
            }]));
            ports.launcher = Arc::new(RefusingLauncher(Arc::new(AtomicUsize::new(0))));
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
                0,
            );
            let config = root.join("config.toml");
            let settings = crate::store::read(&config, 3).unwrap().settings;
            let journal =
                crate::journal::Journal::open(&root.join("logs"), ports.clock.wall()).unwrap();
            let (conductor, task) = crate::conductor::Conductor::start(
                None,
                ports.clock.clone(),
                journal,
                settings,
                config,
                3,
            );
            conductor
                .builds(builds, Action::Initial, crate::Hold::empty())
                .await
                .unwrap();
            let error = conductor
                .model_names(crate::Hold::empty())
                .await
                .err()
                .unwrap();
            let actual = error.message.to_string();
            drop(conductor);
            task.await.unwrap();
            fs::remove_dir_all(root).unwrap();
            assert!(
                actual.contains("runtime launch: Refused"),
                "M-14 must preserve the actual accepted-copy start error: {actual}"
            );
        })
        .await
        .expect(
            "launch failure cap: 5 seconds; one refused launch, no child process or HTTP requests",
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
            let copy = root.join("official-runtime");
            fs::create_dir_all(&copy).unwrap();
            let original = [
                b"official executable".as_slice(),
                b"official companion".as_slice(),
            ];
            for (name, bytes) in names().into_iter().zip(original) {
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
                0,
            );
            let config = root.join("config.toml");
            let mut settings = crate::store::read(&config, 3).unwrap().settings;
            builds.after_start(&settings).unwrap();
            let (Selection::Start(source), _) = builds
                .select(Action::Build("1.4.0".into()), &mut settings, &config)
                .unwrap()
            else {
                panic!("the listed build must be selectable")
            };
            let _ = builds.launch(source).await;
            let observed = names().map(|name| fs::read(copy.join(name)).unwrap());
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
