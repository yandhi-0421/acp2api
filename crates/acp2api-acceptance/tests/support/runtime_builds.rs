use crate::World;
use acp2api_acceptance::{
    client::{ApiCall, ApiClient},
    panel_check,
    setup::file_hash,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[derive(Debug, Default)]
pub struct BuildState {
    pub action: Option<acp2api_acceptance::client::Answer>,
    bound: bool,
}
pub const CHANGED_COPY_LABEL: &str = "n0rmal";

pub fn program() -> &'static str {
    acp2api_acceptance::build_fixture::SERVER
}
pub fn descriptor() -> &'static str {
    acp2api_acceptance::build_fixture::HARNESS
}
pub async fn prepare(w: &mut World) {
    w.startup.initialize().await;
    w.startup.seed_config();
    if !w.builds.bound {
        w.scenario.hub.close();
        w.scenario.hub = w.startup.panel_hub();
        w.scenario.time = acp2api_acceptance::timeline::TimeLine::new(w.scenario.hub.clone());
        w.builds.bound = true;
    }
}
pub fn place(w: &World, name: &str) -> PathBuf {
    let run = w.startup.run.as_ref().unwrap();
    match name {
        "the folder of the acp2api executable" => run.environment.exe_dir.clone(),
        "a folder of PATH" => run.environment.path[0].clone(),
        "the install folder of Zed" => {
            let base = if cfg!(windows) {
                run.environment.local_app_data.as_ref().unwrap().join("Zed")
            } else {
                run.environment
                    .home
                    .as_ref()
                    .unwrap()
                    .join("Library/Application Support/Zed")
            };
            base.join("external_agents")
                .join("registry")
                .join("antigravity-acp")
                .join("1.4.0")
        }
        "the install folder of a JetBrains IDE" => {
            let base = if cfg!(windows) {
                run.environment
                    .local_app_data
                    .as_ref()
                    .unwrap()
                    .join("JetBrains")
            } else {
                run.environment
                    .home
                    .as_ref()
                    .unwrap()
                    .join("Library/Caches/JetBrains")
            };
            base.join("IntelliJIdea2026.2")
                .join("acp-agents")
                .join("antigravity-acp")
                .join("1.4.0")
        }
        "copy" => w.startup.data.join("official-runtime"),
        other => w.startup.root().path.join(other),
    }
}
pub fn install(w: &mut World, name: &str, version: &str, listed: bool) -> PathBuf {
    let folder = place(w, name);
    let label = if version == "1.4.0" {
        "normal".into()
    } else {
        format!("build-{version}")
    };
    acp2api_acceptance::build_fixture::install(
        &w.startup.root().path.join("builds/normal"),
        &folder,
        version,
        &label,
    )
    .unwrap();
    if listed {
        let run = w.startup.run.as_mut().unwrap();
        run.release.builds.push(acp2api::ListedBuild {
            version: version.into(),
            platform: run.release.platform.clone(),
            agy_acp_server_sha256: file_hash(&folder.join(program())),
            localharness_external_sha256: file_hash(&folder.join(descriptor())),
        });
    }
    folder
}
pub async fn panel(w: &mut World) {
    let address = format!("127.0.0.1:{}", w.startup.root().port);
    if w.panel.address != address {
        // Bind directly: Panel::startup seeds a normal copy, which would alter a search fixture.
        w.panel.bind(
            w.startup.root(),
            &w.startup.data,
            w.startup.panel_hub(),
            address,
        );
        w.panel.login_initial("build-owner").await;
        assert_eq!(
            w.panel.login_answer.as_ref().unwrap().status,
            303,
            "authenticated build panel"
        );
    }
}
pub async fn page(w: &mut World) -> String {
    panel(w).await;
    let answer = w.panel.page("build-owner").await;
    assert_eq!(answer.status, 200, "real authenticated management page");
    panel_check::text(&answer)
}
pub async fn action(w: &mut World, path: &str, body: Value) {
    panel(w).await;
    let answer = w.panel.submit("build-owner", path, body).await;
    w.builds.action = Some(answer);
}
pub async fn search(w: &mut World) {
    action(w, "/manage/search", json!({})).await;
}
pub fn root(w: &World) -> &acp2api_acceptance::setup::TestRoot {
    if w.builds.bound {
        w.startup.root()
    } else {
        &w.scenario.root
    }
}
pub async fn request(w: &mut World) -> acp2api_acceptance::client::Answer {
    w.request.queue_next(&w.scenario);
    let root = root(w);
    ApiClient::new(
        format!("127.0.0.1:{}", root.port),
        root.key.clone(),
        w.scenario.hub.clone(),
    )
    .send(ApiCall::one_user_message(
        "POST /v1/messages",
        "build availability",
    ))
    .finish(&mut w.scenario.time)
    .await
}
pub fn launches(w: &World) -> Vec<PathBuf> {
    w.startup
        .panel_hub()
        .entries()
        .iter()
        .filter(|e| e.event["kind"] == "launch")
        .map(|e| PathBuf::from(e.event["program"].as_str().unwrap()))
        .collect()
}
pub async fn assert_found(w: &mut World, name: &str) {
    let path = place(w, name).join(program());
    assert!(
        page(w).await.contains(path.to_str().unwrap()),
        "FR-042 search result must list the actual discovered path in {name}"
    );
}
pub fn assert_copy_launch(w: &World) {
    let path = place(w, "copy").join(program());
    assert_eq!(
        launches(w),
        vec![path.clone()],
        "ADR-0006 actual launch comes only from the accepted copy"
    );
    let entries = w.startup.panel_hub().entries();
    let servers: Vec<_> = entries
        .iter()
        .filter(|e| e.event["hello"]["role"] == "server")
        .collect();
    assert_eq!(servers.len(), 1, "one actual runtime process starts");
    assert_eq!(
        PathBuf::from(servers[0].event["hello"]["argv"][0].as_str().unwrap()),
        path
    );
    assert!(
        !entries
            .iter()
            .any(|e| e.event["kind"] == "process_end" && e.event["role"] == "server"),
        "the selected runtime process is still running"
    );
}
pub fn copied(w: &World, version: &str) {
    let copy = place(w, "copy");
    assert!(
        copy.join(program()).is_file() && copy.join(descriptor()).is_file(),
        "FR-043 data folder must hold the selected accepted copy {version}"
    );
    let text = fs::read_to_string(copy.join(descriptor())).unwrap();
    assert!(
        text.contains(&format!("version = \"{version}\"")),
        "FR-043 copied build must be {version}"
    );
    let actual = [
        file_hash(&copy.join(program())),
        file_hash(&copy.join(descriptor())),
    ];
    assert!(
        w.startup
            .run
            .as_ref()
            .unwrap()
            .release
            .builds
            .iter()
            .any(|b| b.version == version
                && actual
                    == [
                        b.agy_acp_server_sha256.clone(),
                        b.localharness_external_sha256.clone()
                    ]),
        "FR-043 both copied hashes must identify the selected build"
    );
    let directories: Vec<_> = fs::read_dir(&copy)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        directories.len(),
        2,
        "FR-043 one pair of copied executables"
    );
}
