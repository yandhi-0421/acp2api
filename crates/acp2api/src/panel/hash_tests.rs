use super::*;
use crate::{
    ListedBuild,
    runtime_builds::{Builds, COPY, HARNESS, SERVER},
};
use std::{path::PathBuf, thread::ThreadId};

const SERVER_HASH: &str = "5256ec18f11624025905d057d6befb03d77b243511ac5f77ed5e0221ce6d84b5";
const ABC_HASH: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const EMPTY_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

struct Facts {
    builds: Vec<ListedBuild>,
    visits: Arc<Mutex<Vec<ThreadId>>>,
}
impl crate::ReleaseFacts for Facts {
    fn version(&self) -> &str {
        "test"
    }
    fn platform(&self) -> &str {
        &self.builds[0].platform
    }
    fn listed_builds(&self) -> &[ListedBuild] {
        self.visits
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        &self.builds
    }
    fn highest_concurrency_limit(&self) -> u8 {
        1
    }
}

async fn hash_page<F, Fut>(test: F)
where
    F: FnOnce(Panel, PathBuf, Arc<Mutex<Vec<ThreadId>>>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    use futures_util::FutureExt;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let root = std::env::temp_dir().join(format!("acp2api-hash-page-{}", crate::random_hex::<12>().unwrap()));
        std::fs::create_dir_all(root.join(COPY)).unwrap();
        std::fs::write(root.join(COPY).join(SERVER), vec![b'a'; 2 * 1024 * 1024]).unwrap();
        std::fs::write(root.join(COPY).join(HARNESS), b"abc").unwrap();
        let clock = crate::Ports::production().clock;
        let journal = crate::journal::Journal::open(&root.join("logs"), clock.wall()).unwrap();
        let mut builds = Builds::fixture(&root, clock.clone());
        let visits = Arc::new(Mutex::new(Vec::new()));
        builds.start.release = Arc::new(Facts {
            builds: vec![ListedBuild {
                version: "1.4.0".into(), platform: builds.start.release.platform().into(),
                agy_acp_server_sha256: SERVER_HASH.into(), localharness_external_sha256: ABC_HASH.into(),
            }, ListedBuild {
                version: "1.6.0".into(), platform: builds.start.release.platform().into(),
                agy_acp_server_sha256: EMPTY_HASH.into(), localharness_external_sha256: ABC_HASH.into(),
            }], visits: visits.clone(),
        });
        let (stop, stopped) = tokio::sync::watch::channel(false);
        builds.start.stopped = stopped;
        let settings = crate::store::Settings {
            port: 8329, concurrency_limit: 1, local_api_key: "synthetic-test-key".into(),
            management_password_hash: "synthetic-unused-hash".into(), agy_acp_server_path: None,
        };
        let (conductor, task) = Conductor::start(builds, clock.clone(), journal,
            "http://127.0.0.1:8329/".into(), settings, root.join("config.toml"), 1);
        let panel = Panel::new(conductor, clock, 8329);
        panel.sessions.lock().unwrap().insert("hash-page-test".into(), Session {
            since: panel.clock.now(), password_hash: panel.conductor.settings().management_password_hash.clone(),
        });
        let result = std::panic::AssertUnwindSafe(test(panel.clone(), root.clone(), visits)).catch_unwind().await;
        stop.send(true).unwrap();
        drop(panel);
        task.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
        if let Err(panic) = result { std::panic::resume_unwind(panic); }
    }).await.expect("hash page cap: ten seconds, at most eight handler calls, two files of at most 2 MiB, no runtime process");
}

async fn view(panel: &Panel) -> String {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        "acp2api_session=hash-page-test".parse().unwrap(),
    );
    index(State(panel.clone()), Extension(Language::English), headers)
        .await
        .0
}

#[tokio::test]
async fn a_page_checks_the_build_away_from_the_async_worker() {
    hash_page(|panel, _, visits| async move {
        let worker = std::thread::current().id();
        let html = view(&panel).await;
        assert!(html.contains("On the release list") && !html.contains("No build in use."));
        let form = password(
            State(panel),
            Extension(Language::English),
            Form(Password {
                password: String::new(),
            }),
        )
        .await;
        assert_eq!(form.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let form = axum::body::to_bytes(form.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert!(
            std::str::from_utf8(&form)
                .unwrap()
                .contains("Enter a new management password.")
        );
        let visits = visits.lock().unwrap();
        assert!(
            !visits.is_empty() && visits.iter().all(|thread| *thread != worker),
            "page build verification must leave the async worker: {visits:?}"
        );
    })
    .await;
}

#[tokio::test]
async fn each_page_rechecks_both_hashes_and_keeps_the_accepted_versions() {
    hash_page(|panel, root, _| async move {
        let server = root.join(COPY).join(SERVER);
        let harness = root.join(COPY).join(HARNESS);
        let first = view(&panel).await;
        assert!(
            first.contains("<span>1.4.0</span>"),
            "the known multi-chunk hash must identify 1.4.0"
        );
        std::fs::write(&harness, b"abd").unwrap();
        assert!(
            view(&panel).await.contains("No build in use."),
            "a changed companion must not stay accepted"
        );
        std::fs::write(&harness, b"abc").unwrap();
        std::fs::write(&server, b"").unwrap();
        assert!(
            view(&panel).await.contains("<span>1.6.0</span>"),
            "the empty and abc hashes must identify 1.6.0"
        );
        std::fs::remove_file(&server).unwrap();
        assert!(
            view(&panel).await.contains("No build in use."),
            "a missing executable must not stay accepted"
        );
        std::fs::write(&server, b"").unwrap();
        assert!(
            view(&panel).await.contains("<span>1.6.0</span>"),
            "restoration must appear without a display cache"
        );
    })
    .await;
}

#[cfg(unix)]
#[tokio::test]
async fn an_unreadable_copy_keeps_its_page_error_and_recovers() {
    use std::os::unix::fs::PermissionsExt;
    hash_page(|panel, root, _| async move {
        let path = root.join(COPY).join(HARNESS);
        let original = std::fs::metadata(&path).unwrap().permissions();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();
        let error = std::fs::read(&path).unwrap_err();
        let html = view(&panel).await;
        std::fs::set_permissions(&path, original).unwrap();
        let expected = format!("cannot read {}: {error}", path.display());
        assert!(
            html.contains(&format!("<p role=\"status\">{expected}</p>")),
            "the page must retain the filesystem error: {expected}"
        );
        assert!(
            !view(&panel).await.contains("No build in use."),
            "the repaired copy must be accepted on the next response"
        );
    })
    .await;
}
