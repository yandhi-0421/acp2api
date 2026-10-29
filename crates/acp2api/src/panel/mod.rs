use crate::{
    Clock,
    conductor::{Conductor, OwnerAction},
};
use axum::{
    Router,
    extract::{Extension, Form, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};
mod page;
static CONTENT_SECURITY_POLICY: LazyLock<axum::http::HeaderValue> = LazyLock::new(|| {
    use base64::Engine;
    use sha2::Digest;
    let hash =
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(page::COUNTDOWN));
    let policy = format!(
        "default-src 'none'; script-src 'sha256-{hash}'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
    );
    policy.parse().unwrap()
});
const LIFETIME: u64 = 12 * 60 * 60 * 1000;
const COOKIE: &str = "acp2api_session";
const ARGON2_CHECKS_AT_ONCE: usize = 1;
struct Session {
    since: u64,
    password_hash: String,
}
#[derive(Clone)]
struct Panel {
    conductor: Conductor,
    clock: Arc<dyn Clock>,
    port: u16,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    logins: Arc<tokio::sync::Semaphore>,
}
pub(crate) fn router(conductor: Conductor, clock: Arc<dyn Clock>, port: u16) -> Router {
    let panel = Panel::new(conductor, clock, port);
    Router::new()
        .route("/", get(index))
        .route("/manage/login", post(login))
        .route("/manage/key", post(new_key))
        .route("/manage/password", post(password))
        .route("/manage/concurrency", post(concurrency))
        .route("/manage/refresh-quota", post(refresh_quota))
        .route("/manage/sign-in", post(sign_in))
        .route("/manage/cancel-sign-in", post(cancel_sign_in))
        .route("/manage/notice", post(dismiss_notice))
        .route("/manage/search", post(search))
        .route("/manage/retry", post(retry))
        .route("/manage/runtime-path", post(runtime_path))
        .route("/manage/build", post(build))
        .route("/manage/compatibility-check", post(compatibility_check))
        .layer(axum::extract::DefaultBodyLimit::max(16384))
        .layer(axum::middleware::from_fn_with_state(panel.clone(), guard))
        .with_state(panel)
}
impl Panel {
    fn new(conductor: Conductor, clock: Arc<dyn Clock>, port: u16) -> Self {
        Self {
            conductor,
            clock,
            port,
            sessions: Arc::default(),
            logins: Arc::new(tokio::sync::Semaphore::new(ARGON2_CHECKS_AT_ONCE)),
        }
    }
    fn session(&self, headers: &HeaderMap) -> bool {
        let now = self.clock.now();
        let hash = self.conductor.settings().management_password_hash.clone();
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, s| s.password_hash == hash && now.saturating_sub(s.since) < LIFETIME);
        headers
            .get(header::COOKIE)
            .and_then(|h| h.to_str().ok())
            .and_then(|cookies| {
                cookies
                    .split(';')
                    .filter_map(|cookie| cookie.trim().split_once('='))
                    .find(|(name, _)| *name == COOKIE)
            })
            .is_some_and(|(_, value)| sessions.contains_key(value))
    }
    async fn act(&self, action: OwnerAction) -> Result<(), Response> {
        self.conductor
            .owner(action, self.clock.hold(crate::HoldKind::TurnEvent))
            .await
            .map_err(|_| self.failed())
    }
    fn failed(&self) -> Response {
        self.page(
            StatusCode::INTERNAL_SERVER_ERROR,
            "The settings could not be saved. Try again.",
        )
    }
    fn page(&self, status: StatusCode, notice: &str) -> Response {
        (status, Html(self.html(Some(notice)))).into_response()
    }
    fn html(&self, notice: Option<&str>) -> String {
        page::settings(
            &self.conductor.settings(),
            self.conductor.highest,
            notice,
            &self.conductor.notices(),
            self.conductor.builds_summary(),
            &self.conductor.quota_result(),
            &page::sign_in_section(&self.conductor.view(), self.clock.now()),
        )
    }
}
async fn guard(
    State(panel): State<Panel>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let headers = request.headers();
    let host = crate::front::local_host(headers, panel.port);
    let origin = headers.get_all(header::ORIGIN);
    let mut origins = origin.iter();
    let valid_origin = match origins.next() {
        None => true,
        Some(value) => {
            origins.next().is_none()
                && host.is_some_and(|host| value.as_bytes() == format!("http://{host}").as_bytes())
        }
    };
    let mut response = if host.is_none() || !valid_origin {
        (
            StatusCode::BAD_REQUEST,
            "The request must use the local management panel address.",
        )
            .into_response()
    } else if request.uri().path().starts_with("/manage/")
        && request.uri().path() != "/manage/login"
        && !panel.session(headers)
    {
        (
            StatusCode::UNAUTHORIZED,
            Html(page::login(Some(
                "Enter the management password to continue.",
            ))),
        )
            .into_response()
    } else {
        next.run(request).await
    };
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-frame-options", "DENY"),
        ("x-content-type-options", "nosniff"),
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(name),
            axum::http::HeaderValue::from_static(value),
        );
    }
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        CONTENT_SECURITY_POLICY.clone(),
    );
    response
}
async fn index(State(panel): State<Panel>, headers: HeaderMap) -> Html<String> {
    Html(if panel.session(&headers) {
        panel.html(None)
    } else {
        page::login(None)
    })
}
#[derive(Deserialize)]
struct Password {
    password: String,
}
async fn login(State(panel): State<Panel>, Form(form): Form<Password>) -> Response {
    let hash = panel.conductor.settings().management_password_hash.clone();
    let permit = panel
        .logins
        .acquire()
        .await
        .expect("the login semaphore stays open");
    let check = (form.password, hash.clone());
    let verified =
        tokio::task::spawn_blocking(move || crate::store::verify_password(&check.0, &check.1))
            .await
            .unwrap_or(false);
    drop(permit);
    if !verified {
        return (
            StatusCode::UNAUTHORIZED,
            Html(page::login(Some("The management password is wrong."))),
        )
            .into_response();
    }
    let Ok(token) = crate::random_hex::<32>() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "A panel session could not be created.",
        )
            .into_response();
    };
    panel.sessions.lock().unwrap().insert(
        token.clone(),
        Session {
            since: panel.clock.now(),
            password_hash: hash,
        },
    );
    let cookie = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=43200");
    let mut response =
        Redirect::to(&format!("/#{}", crate::conductor::sign_in::ENTRY_ID)).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie.parse().expect("generated cookie"),
    );
    response
}
async fn new_key(State(panel): State<Panel>) -> Response {
    match panel.act(OwnerAction::NewLocalApiKey).await {
        Ok(()) => Redirect::to("/").into_response(),
        Err(response) => response,
    }
}
async fn refresh_quota(
    State(panel): State<Panel>,
    Extension(lease): Extension<crate::front::RequestHold>,
) -> Response {
    let hold = panel.clock.hold(crate::HoldKind::TurnEvent);
    drop(lease.release());
    match panel.conductor.owner(OwnerAction::RefreshQuota, hold).await {
        Ok(()) => panel.page(StatusCode::OK, "Quota refreshed."),
        Err(error) => panel.page(StatusCode::UNPROCESSABLE_ENTITY, &error),
    }
}
async fn sign_in(State(panel): State<Panel>) -> Response {
    panel.sign_in_action(OwnerAction::SignIn).await
}
async fn cancel_sign_in(State(panel): State<Panel>) -> Response {
    panel.sign_in_action(OwnerAction::CancelSignIn).await
}
impl Panel {
    async fn sign_in_action(&self, action: OwnerAction) -> Response {
        // Keep the HTTP hold through page rendering. The action acknowledges the wait immediately.
        match self
            .conductor
            .owner(action, self.clock.hold(crate::HoldKind::TurnEvent))
            .await
        {
            Ok(()) => self.page(StatusCode::OK, ""),
            Err(error) => self.page(StatusCode::UNPROCESSABLE_ENTITY, &error),
        }
    }
}
async fn password(State(panel): State<Panel>, Form(form): Form<Password>) -> Response {
    if form.password.is_empty() {
        return panel.page(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Enter a new management password.",
        );
    }
    let Ok(Ok(hash)) =
        tokio::task::spawn_blocking(move || crate::store::hash_password(&form.password)).await
    else {
        return panel.failed();
    };
    match panel.act(OwnerAction::SetPasswordHash(hash)).await {
        Ok(()) => Redirect::to("/").into_response(),
        Err(response) => response,
    }
}
#[derive(Deserialize)]
struct Limit {
    concurrency_limit: String,
}
async fn concurrency(State(panel): State<Panel>, Form(form): Form<Limit>) -> Response {
    let limit = match form
        .concurrency_limit
        .parse::<u8>()
        .ok()
        .filter(|n| (1..=panel.conductor.highest).contains(n))
    {
        Some(n) => n,
        None => {
            return panel.page(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Choose a concurrency limit supported on this platform.",
            );
        }
    };
    match panel.act(OwnerAction::SetConcurrencyLimit(limit)).await {
        Ok(()) => panel.page(StatusCode::OK, "Concurrency limit saved."),
        Err(response) => response,
    }
}
#[derive(Deserialize)]
struct DismissNotice {
    notice_id: String,
}
async fn dismiss_notice(State(panel): State<Panel>, Form(form): Form<DismissNotice>) -> Response {
    match panel
        .conductor
        .owner(
            OwnerAction::DismissNotice(form.notice_id),
            panel.clock.hold(crate::HoldKind::TurnEvent),
        )
        .await
    {
        Ok(()) => Redirect::to("/").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The notice could not be dismissed. Try again.",
        )
            .into_response(),
    }
}
async fn runtime_action(
    panel: Panel,
    action: crate::runtime_builds::Action,
    lease: crate::front::RequestHold,
) -> Response {
    let hold = panel.clock.hold(crate::HoldKind::TurnEvent);
    drop(lease.release());
    match panel.conductor.builds(action, hold).await {
        Ok(notice) => panel.page(StatusCode::OK, &notice),
        Err(error) => panel.page(StatusCode::UNPROCESSABLE_ENTITY, &error.to_string()),
    }
}
async fn search(
    State(panel): State<Panel>,
    Extension(lease): Extension<crate::front::RequestHold>,
) -> Response {
    runtime_action(panel, crate::runtime_builds::Action::Search, lease).await
}
async fn retry(panel: State<Panel>, lease: Extension<crate::front::RequestHold>) -> Response {
    search(panel, lease).await
}
#[derive(Deserialize)]
struct RuntimePath {
    agy_acp_server_path: std::path::PathBuf,
}
async fn runtime_path(
    State(panel): State<Panel>,
    Extension(lease): Extension<crate::front::RequestHold>,
    Form(form): Form<RuntimePath>,
) -> Response {
    runtime_action(
        panel,
        crate::runtime_builds::Action::Path(form.agy_acp_server_path),
        lease,
    )
    .await
}
#[derive(Deserialize)]
struct Build {
    build: String,
}
async fn compatibility_check(
    State(panel): State<Panel>,
    Extension(lease): Extension<crate::front::RequestHold>,
    Form(form): Form<Build>,
) -> Response {
    runtime_action(
        panel,
        crate::runtime_builds::Action::Check(form.build),
        lease,
    )
    .await
}
async fn build(
    State(panel): State<Panel>,
    Extension(lease): Extension<crate::front::RequestHold>,
    Form(form): Form<Build>,
) -> Response {
    runtime_action(
        panel,
        crate::runtime_builds::Action::Build(form.build),
        lease,
    )
    .await
}
#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test]
    async fn a_failed_save_and_an_unusable_value_keep_the_settings_page() {
        let folder = std::env::temp_dir().join(format!("acp2api-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let settings = crate::store::Settings {
            port: 8329,
            concurrency_limit: 1,
            local_api_key: "acp2api-test".into(),
            management_password_hash: String::new(),
            agy_acp_server_path: None,
        };
        let unsaveable_configuration = folder.join("config.toml");
        std::fs::create_dir(&unsaveable_configuration).unwrap();
        let (conductor, _task) = Conductor::start(
            crate::runtime_builds::Builds::fixture(&folder, clock.clone()),
            clock.clone(),
            journal,
            settings,
            unsaveable_configuration,
            1,
        );
        let panel = Panel::new(conductor, clock, 8329);
        let empty = Password {
            password: String::new(),
        };
        let zero = Limit {
            concurrency_limit: "0".into(),
        };
        let answers = [
            new_key(State(panel.clone())).await,
            password(State(panel.clone()), Form(empty)).await,
            concurrency(State(panel), Form(zero)).await,
        ];
        let mut pages = Vec::new();
        for answer in answers {
            let status = answer.status().as_u16();
            let body = axum::body::to_bytes(answer.into_body(), 1 << 16).await;
            let body = String::from_utf8(body.unwrap().to_vec()).unwrap();
            pages.push((status, body.contains("<h1>Settings</h1>")));
        }
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(pages, [(500, true), (422, true), (422, true)]);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn a_login_does_not_block_the_async_worker_with_its_hash() {
        let folder = std::env::temp_dir().join(format!("acp2api-login-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let settings = crate::store::Settings {
            port: 8329,
            concurrency_limit: 1,
            local_api_key: "acp2api-test".into(),
            management_password_hash: crate::store::hash_password("right").unwrap(),
            agy_acp_server_path: None,
        };
        let configuration = folder.join("config.toml");
        let (conductor, _task) = Conductor::start(
            crate::runtime_builds::Builds::fixture(&folder, clock.clone()),
            clock.clone(),
            journal,
            settings,
            configuration,
            1,
        );
        let panel = Panel::new(conductor, clock, 8329);
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = ticks.clone();
        tokio::spawn(async move {
            loop {
                counter.fetch_add(1, Ordering::Relaxed);
                tokio::task::yield_now().await;
            }
        });
        let password = Password {
            password: "wrong".into(),
        };
        let response = login(State(panel.clone()), Form(password)).await;
        let signed_in = login(
            State(panel),
            Form(Password {
                password: "right".into(),
            }),
        )
        .await;
        assert_eq!(
            signed_in.headers()[header::LOCATION],
            "/#sign-in",
            "the password entry returns to the sign-in fragment"
        );
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(
            ticks.load(Ordering::Relaxed) > 0,
            "another task runs on the one worker while the hash is checked"
        );
    }
}
