use crate::{
    client::{Answer, ApiClient, Owner},
    hub::Hub,
    scenario::Scenario,
    setup::TestRoot,
    startup_check::Startup,
    timeline::TimeLine,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Panel {
    owners: HashMap<String, Owner>,
    proven: std::collections::HashSet<String>,
    config: PathBuf,
    client: Option<ApiClient>,
    hub: Option<Hub>,
    pub answer: Option<Answer>,
    pub login_answer: Option<Answer>,
    pub new_password: Option<String>,
    pub old_key: String,
    pub initial_password: String,
    pub new_key: Option<String>,
    pub address: String,
}
impl std::fmt::Debug for Panel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Panel").finish_non_exhaustive()
    }
}
impl Panel {
    pub fn bind(&mut self, root: &TestRoot, data: &Path, hub: Hub, address: String) {
        self.config = data.join("config.toml");
        self.address = address;
        self.client = Some(
            ApiClient::new(self.address.clone(), String::new(), hub.clone())
                .without_local_api_key(),
        );
        self.hub = Some(hub);
        self.old_key = root.key.clone();
        self.initial_password = root.password.clone();
    }
    pub async fn startup(&mut self, startup: &mut Startup) {
        startup.initialize().await;
        if !startup.panel_running() {
            startup.seed_config();
            startup.panel_runtime();
            startup.launch_data(vec![]).await;
        }
        let address = startup
            .terminal()
            .lines()
            .filter_map(|line| {
                line.split_once("management panel at http://")
                    .map(|(_, url)| url.trim_end_matches('/').to_owned())
            })
            .next_back()
            .expect("actual published panel address");
        self.bind(startup.root(), &startup.data, startup.panel_hub(), address);
    }
    pub async fn scenario(&mut self, scenario: &mut Scenario) {
        scenario.start().await;
        self.bind(
            &scenario.root,
            &scenario.root.path.join("data"),
            scenario.hub.clone(),
            format!("127.0.0.1:{}", scenario.root.port),
        );
    }
    pub fn saved(&self) -> toml::Value {
        toml::from_str(&std::fs::read_to_string(&self.config).unwrap()).unwrap()
    }
    pub fn time(&self) -> TimeLine {
        TimeLine::new(self.hub.as_ref().unwrap().clone())
    }
    pub fn owner(&mut self, name: &str) -> &mut Owner {
        let client = self.client.as_ref().expect("panel fixture bound").clone();
        self.owners
            .entry(name.into())
            .or_insert_with(|| Owner::new(client))
    }
    pub async fn login(&mut self, name: &str, password: &str) {
        let call = self
            .owner(name)
            .submit("/manage/login", json!({"password":password}));
        let answer = call.finish(&mut self.time()).await;
        self.owner(name).remember(&answer);
        let opened = answer.headers.contains_key("set-cookie");
        self.login_answer = Some(answer);
        if opened {
            let limit = self.saved()["concurrency_limit"]
                .as_integer()
                .unwrap()
                .to_string();
            let proof = self
                .submit(
                    name,
                    "/manage/concurrency",
                    json!({"concurrency_limit":limit}),
                )
                .await;
            if proof.status == 200 {
                self.proven.insert(name.into());
            }
        }
    }
    pub async fn login_initial(&mut self, name: &str) {
        let password = self.initial_password.clone();
        self.login(name, &password).await;
    }
    pub async fn page(&mut self, name: &str) -> Answer {
        let call = self.owner(name).page();
        call.finish(&mut self.time()).await
    }
    pub async fn submit(&mut self, name: &str, path: &str, body: Value) -> Answer {
        let call = self.owner(name).submit(path, body);
        call.finish(&mut self.time()).await
    }
    pub async fn api(&self, key: &str) -> Answer {
        ApiClient::new(
            self.address.clone(),
            key.into(),
            self.hub.as_ref().unwrap().clone(),
        )
        .model_names()
        .finish(&mut self.time())
        .await
    }
    pub async fn valid(&mut self, name: &str, expected: bool) {
        assert!(
            self.owners.get(name).is_some_and(|o| !o.cookies.is_empty()),
            "FR-009 correct management password must create a real panel session before its lifetime is tested"
        );
        if !expected {
            assert!(
                self.proven.contains(name),
                "FR-009 the previously-issued session must have worked on a real management action"
            );
            let before = toml::to_string(&self.saved()).unwrap();
            for (path, body) in management_actions() {
                let a = self.submit(name, path, body).await;
                assert_eq!(
                    a.status, 401,
                    "FR-009 invalid previously-issued cookie cannot change management settings: {path}"
                );
                assert!(
                    toml::to_string(&self.saved()).unwrap() == before,
                    "FR-009 invalid management action must leave every setting unchanged"
                );
            }
        }
        let page = self.page(name).await;
        let html = text(&page);
        assert_eq!(page.status, 200, "FR-009 page must remain available");
        assert_eq!(
            html.contains("id=\"local-api-key\""),
            expected,
            "FR-009 panel session validity"
        );
        assert_eq!(
            html.contains("autocomplete=\"current-password\""),
            !expected,
            "FR-009 invalid session shows password entry only"
        );
    }
    pub async fn change_password(&mut self, name: &str) {
        let password = crate::setup::random_hex(20);
        self.answer = Some(
            self.submit(name, "/manage/password", json!({"password":password}))
                .await,
        );
        self.new_password = Some(password);
    }
    pub fn saved_hash(&self, password: &str) {
        saved_hash_path(&self.config, password);
    }
}
pub fn text(answer: &Answer) -> String {
    String::from_utf8(answer.body.clone()).expect("panel UTF-8")
}
pub fn saved_hash_path(path: &Path, password: &str) {
    use argon2::PasswordVerifier;
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(
        !saved.contains(password),
        "FR-009 no plaintext management password in configuration"
    );
    let value: toml::Value = toml::from_str(&saved).unwrap();
    let hash = argon2::password_hash::phc::PasswordHash::new(
        value["management_password_hash"].as_str().unwrap(),
    )
    .expect("Argon2id PHC hash");
    assert_eq!(hash.algorithm.as_str(), "argon2id");
    assert!(
        argon2::Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok(),
        "FR-009 saved hash must verify the actual management password"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600,
            "FR-004 saved management settings must remain owner-only"
        );
    }
    #[cfg(windows)]
    crate::startup_permissions::assert_owner_only(path);
}
pub fn saved_hash_seed(startup: &Startup) {
    use argon2::PasswordVerifier;
    let config = startup.saved().unwrap();
    let hash = argon2::password_hash::phc::PasswordHash::new(
        config["management_password_hash"].as_str().unwrap(),
    )
    .unwrap();
    assert!(
        argon2::Argon2::default()
            .verify_password(startup.root().password.as_bytes(), &hash)
            .is_ok(),
        "P1 alias must retain its actual password hash"
    );
}
pub fn generated(startup: &Startup, secret: &str) -> String {
    startup
        .terminal()
        .lines()
        .find_map(|line| {
            line.strip_prefix(&format!("acp2api: {secret}: "))
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

pub fn management_actions() -> Vec<(&'static str, Value)> {
    vec![
        ("/manage/key", json!({})),
        (
            "/manage/password",
            json!({"password":crate::setup::random_hex(20)}),
        ),
        ("/manage/concurrency", json!({"concurrency_limit":"3"})),
    ]
}
