use crate::{World, runtime_support as process};
use acp2api_acceptance::{build_fixture, client::Answer};
use serde_json::Value;
use std::path::PathBuf;

pub async fn three_failures(w: &mut World) {
    process::start(w).await;
    let origin = w.scenario.hub.now();
    for (index, minute) in [0, 4, 9].into_iter().enumerate() {
        w.scenario.time.advance_to(origin + minute * 60_000).await;
        assert_eq!(
            process::servers(w).len(),
            index + 1,
            "three-failure setup requires each earlier replacement"
        );
        process::exit(w).await;
        if index < 2 {
            process::observe(w, |w| process::servers(w).len() == index + 2).await;
        }
    }
    w.scenario.time.settle().await;
}

pub fn install(w: &World, folder: &str, version: &str, label: &str) -> PathBuf {
    let folder = w.scenario.root.path.join(folder);
    build_fixture::install(
        &w.scenario.root.path.join("builds/normal"),
        &folder,
        version,
        label,
    )
    .unwrap();
    folder
}

pub async fn action(w: &mut World, path: &str, body: Value) -> Answer {
    if w.panel.address.is_empty() {
        w.panel.scenario(&mut w.scenario).await;
        w.panel.login_initial("unavailable-owner").await;
        assert_eq!(
            w.panel.login_answer.as_ref().unwrap().status,
            303,
            "authenticated recovery action"
        );
    }
    w.panel.submit("unavailable-owner", path, body).await
}
