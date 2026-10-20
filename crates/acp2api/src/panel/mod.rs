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
}
pub(crate) fn router(conductor: Conductor, clock: Arc<dyn Clock>, port: u16) -> Router {
    let panel = Panel {
        conductor,
        clock,
        port,
        sessions: Arc::default(),
    };
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
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html(page::login(Some(
                        "The settings could not be saved. Try again.",
                    ))),
                )
                    .into_response()
            })
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
    } else if request.uri().path() != "/"
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
    if !crate::store::verify_password(&form.password, &hash)
        || panel.conductor.settings().management_password_hash != hash
    {
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
    match panel.act(OwnerAction::ChangePassword(form.password)).await {
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
