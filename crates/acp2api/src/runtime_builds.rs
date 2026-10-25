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
            scan(&home.join("Library/Application Support/Zed"), &mut folders)?;
            scan(&home.join("Library/Caches/JetBrains"), &mut folders)?;
        }
        if let Some(local) = &self.environment.local_app_data {
            scan(&local.join("Zed"), &mut folders)?;
            scan(&local.join("JetBrains"), &mut folders)?;
        }
        folders.sort();
        folders.dedup();
        let mut found = Vec::new();
        for folder in folders {
            if names().iter().all(|name| folder.join(name).is_file()) {
                found.push(Found {
                    listed: self.identify(&folder)?,
                    folder,
                });
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
            fs::create_dir_all(&copy).map_err(|error| error.to_string())?;
            if fs::canonicalize(&source).map_err(|error| error.to_string())?
                != fs::canonicalize(&copy).map_err(|error| error.to_string())?
            {
                for name in names() {
                    fs::copy(source.join(name), copy.join(name))
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        let build = self.accepted_copy()?.ok_or("no accepted copy")?;
        Runtime::start(&self.start, &build.version).await
    }
    pub(crate) fn after_start(&self, settings: &Settings) -> Result<(), String> {
        self.discover(settings.agy_acp_server_path.as_deref())
    }
    pub(crate) fn unavailable(&self) -> String {
        let reason = if self.found.lock().unwrap().is_empty() {
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
fn scan(path: &Path, folders: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("cannot search {}: {error}", path.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            folders.push(entry.path());
            scan(&entry.path(), folders)?;
        }
    }
    Ok(())
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
