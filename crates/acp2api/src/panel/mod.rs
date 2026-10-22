use crate::{
    Clock,
    conductor::{Conductor, OwnerAction},
};
use axum::{
    Router,
    extract::{Form, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
mod page;
const LIFETIME: u64 = 12 * 60 * 60 * 1000;
const COOKIE: &str = "acp2api_session";
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
    /// One login at a time runs its Argon2 check, so that logins without a session cannot use more
    /// than one thread and its 19 MiB.
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
            logins: Arc::new(tokio::sync::Semaphore::new(1)),
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(page::login(Some(
                "The settings could not be saved. Try again.",
            ))),
        )
            .into_response()
    }
}
async fn guard(
    State(panel): State<Panel>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let headers = request.headers();
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    let local = format!("127.0.0.1:{}", panel.port);
    let localhost = format!("localhost:{}", panel.port);
    let valid_host = headers.get_all(header::HOST).iter().count() == 1
        && (host == Some(&local) || host == Some(&localhost));
    let origin = headers.get_all(header::ORIGIN);
    let mut origins = origin.iter();
    let valid_origin = match origins.next() {
        None => true,
        Some(value) => {
            origins.next().is_none()
                && host.is_some_and(|host| value.as_bytes() == format!("http://{host}").as_bytes())
        }
    };
    let mut response = if !valid_host || !valid_origin {
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
        (
            "content-security-policy",
            "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(name),
            axum::http::HeaderValue::from_static(value),
        );
    }
    response
}
async fn index(State(panel): State<Panel>, headers: HeaderMap) -> Html<String> {
    Html(if panel.session(&headers) {
        page::settings(&panel.conductor.settings(), panel.conductor.highest, None)
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
    let mut random = [0u8; 32];
    if getrandom::fill(&mut random).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "A panel session could not be created.",
        )
            .into_response();
    }
    let token = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    panel.sessions.lock().unwrap().insert(
        token.clone(),
        Session {
            since: panel.clock.now(),
            password_hash: hash,
        },
    );
    let cookie = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=43200");
    let mut response = Redirect::to("/").into_response();
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
async fn password(State(panel): State<Panel>, Form(form): Form<Password>) -> Response {
    if form.password.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Enter a new management password.",
        )
            .into_response();
    }
    let Ok(Ok(hash)) =
        tokio::task::spawn_blocking(move || crate::store::hash_password(&form.password)).await
    else {
        return panel.failed();
    };
    match panel.act(OwnerAction::ChangePassword(hash)).await {
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
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Choose a concurrency limit supported on this platform.",
            )
                .into_response();
        }
    };
    match panel.act(OwnerAction::SetConcurrencyLimit(limit)).await {
        Ok(()) => Html(page::settings(
            &panel.conductor.settings(),
            panel.conductor.highest,
            Some("Concurrency limit saved."),
        ))
        .into_response(),
        Err(response) => response,
    }
}
#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
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
        let (conductor, _task) =
            Conductor::start(None, clock.clone(), journal, settings, configuration, 1);
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
        let response = login(State(panel), Form(password)).await;
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(
            ticks.load(Ordering::Relaxed) > 0,
            "another task runs on the one worker while the hash is checked"
        );
    }
}
