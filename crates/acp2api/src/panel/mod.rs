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
mod texts;
use texts::Language;
static CONTENT_SECURITY_POLICY: LazyLock<axum::http::HeaderValue> = LazyLock::new(|| {
    use base64::Engine;
    use sha2::Digest;
    let scripts = [page::COUNTDOWN, page::COPY_KEY]
        .map(|script| {
            let hash =
                base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(script));
            format!("'sha256-{hash}'")
        })
        .join(" ");
    let policy = format!(
        "default-src 'none'; script-src {scripts}; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"
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
        .route("/manage/exit", post(exit))
        .route("/manage/language", post(set_language))
        .route("/manage/key", post(new_key))
        .route("/manage/password", post(password))
        .route("/manage/concurrency", post(concurrency))
        .route("/manage/refresh-quota", post(refresh_quota))
        .route("/manage/models", post(read_models))
        .route("/manage/sign-in", post(sign_in))
        .route("/manage/cancel-sign-in", post(cancel_sign_in))
        .route("/manage/end-lockout", post(end_lockout))
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
        cookie(headers, COOKIE).is_some_and(|value| sessions.contains_key(value))
    }
    async fn verify(&self, password: String, hash: String) -> bool {
        let permit = self
            .logins
            .acquire()
            .await
            .expect("the login semaphore stays open");
        let verified =
            tokio::task::spawn_blocking(move || crate::store::verify_password(&password, &hash))
                .await
                .unwrap_or(false);
        drop(permit);
        verified
    }
    async fn act(&self, language: Language, action: OwnerAction) -> Result<(), Response> {
        self.conductor
            .owner(action, self.clock.hold(crate::HoldKind::TurnEvent))
            .await
            .map_err(|_| self.failed(language))
    }
    fn failed(&self, language: Language) -> Response {
        self.page(
            language,
            StatusCode::INTERNAL_SERVER_ERROR,
            language.pick(
                "The settings could not be saved. Try again.",
                "设置没有保存。请重试。",
            ),
        )
    }
    fn notice(&self, language: Language, notice: &crate::messages::OwnerMessage) -> Response {
        let line = page::Line {
            text: &language.owner_message(notice),
            error: notice.is_failure(),
        };
        Html(self.html(language, Some(line))).into_response()
    }
    fn page(&self, language: Language, status: StatusCode, text: &str) -> Response {
        let line = page::Line {
            text,
            error: !status.is_success(),
        };
        (status, Html(self.html(language, Some(line)))).into_response()
    }
    fn html(&self, language: Language, notice: Option<page::Line<'_>>) -> String {
        page::settings(page::Page {
            language,
            settings: &self.conductor.settings(),
            highest: self.conductor.highest,
            port: self.port,
            data_folder: self.conductor.data_folder(),
            notice,
            notices: &self.conductor.notices(),
            builds: self.conductor.builds_summary(),
            quota_result: &self.conductor.quota_result(),
            history: &self.conductor.history(),
            view: &self.conductor.view(),
            now: self.clock.now(),
        })
    }
}
fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}
async fn guard(
    State(panel): State<Panel>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let language = Language::of(request.headers());
    request.extensions_mut().insert(language);
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
            language.pick(
                "The request must use the local management panel address.",
                "这个请求必须使用本机管理面板的地址。",
            ),
        )
            .into_response()
    } else if request.uri().path().starts_with("/manage/")
        && request.uri().path() != "/manage/login"
        && !panel.session(headers)
    {
        (
            StatusCode::UNAUTHORIZED,
            Html(page::login(
                language,
                Some(language.pick(
                    "Enter the management password to continue.",
                    "输入管理密码后继续。",
                )),
            )),
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
async fn index(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    headers: HeaderMap,
) -> Html<String> {
    Html(if panel.session(&headers) {
        panel.html(language, None)
    } else {
        page::login(language, None)
    })
}
#[derive(Deserialize)]
struct LanguageChoice {
    language: Language,
}
async fn set_language(Form(choice): Form<LanguageChoice>) -> Response {
    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        choice.language.cookie().parse().expect("generated cookie"),
    );
    response
}
#[derive(Deserialize)]
struct Password {
    password: String,
}
#[derive(Deserialize)]
struct EndLockoutForm {
    #[serde(default)]
    password: String,
}
fn wrong_password(language: Language) -> &'static str {
    language.pick("The management password is wrong.", "管理密码不对。")
}
async fn login(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Form(form): Form<Password>,
) -> Response {
    let hash = panel.conductor.settings().management_password_hash.clone();
    if !panel.verify(form.password, hash.clone()).await {
        return (
            StatusCode::UNAUTHORIZED,
            Html(page::login(language, Some(wrong_password(language)))),
        )
            .into_response();
    }
    let Ok(token) = crate::random_hex::<32>() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            language.pick(
                "A panel session could not be created.",
                "无法创建面板会话。",
            ),
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
async fn exit(State(panel): State<Panel>, headers: HeaderMap) -> Response {
    let token = cookie(&headers, COOKIE).expect("the guard requires a panel session cookie");
    panel.sessions.lock().unwrap().remove(token);
    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0")
            .parse()
            .expect("generated cookie"),
    );
    response
}
async fn new_key(State(panel): State<Panel>, Extension(language): Extension<Language>) -> Response {
    match panel.act(language, OwnerAction::NewLocalApiKey).await {
        Ok(()) => Redirect::to("/").into_response(),
        Err(response) => response,
    }
}
async fn refresh_quota(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Extension(lease): Extension<crate::front::RequestHold>,
) -> Response {
    let hold = panel.clock.hold(crate::HoldKind::TurnEvent);
    drop(lease.release());
    match panel.conductor.owner(OwnerAction::RefreshQuota, hold).await {
        Ok(()) => panel.page(
            language,
            StatusCode::OK,
            language.pick("Quota refreshed.", "配额已刷新。"),
        ),
        Err(error) => panel.page(
            language,
            StatusCode::UNPROCESSABLE_ENTITY,
            &language.owner_message(&error),
        ),
    }
}
async fn read_models(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Extension(lease): Extension<crate::front::RequestHold>,
) -> Response {
    let hold = panel.clock.hold(crate::HoldKind::TurnEvent);
    drop(lease.release());
    match panel
        .conductor
        .owner(OwnerAction::ReadModelNames, hold)
        .await
    {
        Ok(()) => panel.page(
            language,
            StatusCode::OK,
            language.pick("Model names read.", "模型名已读取。"),
        ),
        Err(error) => panel.page(
            language,
            StatusCode::UNPROCESSABLE_ENTITY,
            &crate::journal::redact_error(&language.owner_message(&error)),
        ),
    }
}
async fn sign_in(State(panel): State<Panel>, Extension(language): Extension<Language>) -> Response {
    panel.sign_in_action(language, OwnerAction::SignIn).await
}
async fn cancel_sign_in(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
) -> Response {
    panel
        .sign_in_action(language, OwnerAction::CancelSignIn)
        .await
}
impl Panel {
    async fn sign_in_action(&self, language: Language, action: OwnerAction) -> Response {
        // Keep the HTTP hold through page rendering. The action acknowledges the wait immediately.
        match self
            .conductor
            .owner(action, self.clock.hold(crate::HoldKind::TurnEvent))
            .await
        {
            Ok(()) => Html(self.html(language, None)).into_response(),
            Err(error) => self.page(
                language,
                StatusCode::UNPROCESSABLE_ENTITY,
                &language.owner_message(&error),
            ),
        }
    }
}
async fn end_lockout(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Form(form): Form<EndLockoutForm>,
) -> Response {
    if !panel
        .verify(
            form.password,
            panel.conductor.settings().management_password_hash.clone(),
        )
        .await
    {
        return panel.page(language, StatusCode::UNAUTHORIZED, wrong_password(language));
    }
    match panel
        .conductor
        .owner(
            OwnerAction::EndLockout,
            panel.clock.hold(crate::HoldKind::TurnEvent),
        )
        .await
    {
        Ok(()) => Redirect::to("/").into_response(),
        Err(message) => panel.notice(language, &message),
    }
}
async fn password(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Form(form): Form<Password>,
) -> Response {
    if form.password.is_empty() {
        return panel.page(
            language,
            StatusCode::UNPROCESSABLE_ENTITY,
            language.pick("Enter a new management password.", "输入新的管理密码。"),
        );
    }
    let Ok(Ok(hash)) =
        tokio::task::spawn_blocking(move || crate::store::hash_password(&form.password)).await
    else {
        return panel.failed(language);
    };
    match panel
        .act(language, OwnerAction::SetPasswordHash(hash))
        .await
    {
        Ok(()) => Redirect::to("/").into_response(),
        Err(response) => response,
    }
}
#[derive(Deserialize)]
struct ConcurrencyLimitForm {
    concurrency_limit: String,
}
async fn concurrency(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Form(form): Form<ConcurrencyLimitForm>,
) -> Response {
    let limit = match form
        .concurrency_limit
        .parse::<u8>()
        .ok()
        .filter(|n| (1..=panel.conductor.highest).contains(n))
    {
        Some(n) => n,
        None => {
            return panel.page(
                language,
                StatusCode::UNPROCESSABLE_ENTITY,
                language.pick(
                    "Choose a concurrency limit supported on this platform.",
                    "选择这个平台支持的并发上限。",
                ),
            );
        }
    };
    match panel
        .act(language, OwnerAction::SetConcurrencyLimit(limit))
        .await
    {
        Ok(()) => panel.page(
            language,
            StatusCode::OK,
            language.pick("Concurrency limit saved.", "并发上限已保存。"),
        ),
        Err(response) => response,
    }
}
#[derive(Deserialize)]
struct DismissNotice {
    notice_id: String,
}
async fn dismiss_notice(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Form(form): Form<DismissNotice>,
) -> Response {
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
            language.pick(
                "The notice could not be dismissed. Try again.",
                "通知没有关闭。请重试。",
            ),
        )
            .into_response(),
    }
}
async fn runtime_action(
    panel: Panel,
    language: Language,
    action: crate::runtime_builds::Action,
    lease: crate::front::RequestHold,
) -> Response {
    let hold = panel.clock.hold(crate::HoldKind::TurnEvent);
    drop(lease.release());
    match panel.conductor.builds(action, hold).await {
        Ok(Some(notice)) => panel.notice(language, &notice),
        Ok(None) => Html(panel.html(language, None)).into_response(),
        Err(error) => panel.page(
            language,
            StatusCode::UNPROCESSABLE_ENTITY,
            &language.launch_failure(&error),
        ),
    }
}
async fn search(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Extension(lease): Extension<crate::front::RequestHold>,
) -> Response {
    runtime_action(
        panel,
        language,
        crate::runtime_builds::Action::Search,
        lease,
    )
    .await
}
async fn retry(
    panel: State<Panel>,
    language: Extension<Language>,
    lease: Extension<crate::front::RequestHold>,
) -> Response {
    search(panel, language, lease).await
}
#[derive(Deserialize)]
struct RuntimePath {
    agy_acp_server_path: std::path::PathBuf,
}
async fn runtime_path(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Extension(lease): Extension<crate::front::RequestHold>,
    Form(form): Form<RuntimePath>,
) -> Response {
    runtime_action(
        panel,
        language,
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
    Extension(language): Extension<Language>,
    Extension(lease): Extension<crate::front::RequestHold>,
    Form(form): Form<Build>,
) -> Response {
    runtime_action(
        panel,
        language,
        crate::runtime_builds::Action::Check(form.build),
        lease,
    )
    .await
}
async fn build(
    State(panel): State<Panel>,
    Extension(language): Extension<Language>,
    Extension(lease): Extension<crate::front::RequestHold>,
    Form(form): Form<Build>,
) -> Response {
    runtime_action(
        panel,
        language,
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

    async fn session_panel<F, Fut>(test: F)
    where
        F: FnOnce(u16) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        use futures_util::FutureExt;
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let folder = std::env::temp_dir().join(format!(
                "acp2api-exit-{}",
                crate::random_hex::<12>().unwrap()
            ));
            let clock = crate::Ports::production().clock;
            let journal =
                crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
            let mut builds = crate::runtime_builds::Builds::fixture(&folder, clock.clone());
            let (stopped, stop) = tokio::sync::watch::channel(false);
            builds.start.stopped = stop;
            let settings = crate::store::Settings {
                port: 8329,
                concurrency_limit: 1,
                local_api_key: "acp2api-test".into(),
                management_password_hash: crate::store::hash_password("panel-exit-test").unwrap(),
                agy_acp_server_path: None,
            };
            let (conductor, task) = Conductor::start(
                builds,
                clock.clone(),
                journal,
                settings,
                folder.join("config.toml"),
                1,
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let requests = Arc::new(AtomicUsize::new(0));
            let app = router(conductor, clock, port).layer(axum::middleware::from_fn(
                move |request: axum::extract::Request, next: axum::middleware::Next| {
                    let requests = requests.clone();
                    async move {
                        assert!(
                            requests.fetch_add(1, Ordering::SeqCst) < 16,
                            "session request cap"
                        );
                        next.run(request).await
                    }
                },
            ));
            let (shutdown, shutdown_requested) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                axum::serve(listener, app)
                    .with_graceful_shutdown(async { shutdown_requested.await.unwrap() })
                    .await
                    .unwrap();
            });
            let result = std::panic::AssertUnwindSafe(test(port))
                .catch_unwind()
                .await;
            stopped.send(true).unwrap();
            shutdown.send(()).unwrap();
            server.await.unwrap();
            task.await.unwrap();
            std::fs::remove_dir_all(folder).unwrap();
            if let Err(panic) = result {
                std::panic::resume_unwind(panic);
            }
        })
        .await
        .expect("session test cap: ten seconds, sixteen requests, 64 KiB per response");
    }

    async fn session_http(
        port: u16,
        method: &str,
        path: &str,
        headers: &str,
        body: &str,
    ) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n{headers}\r\n{body}",
            body.len(),
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = Vec::new();
        stream
            .take(64 * 1024 + 1)
            .read_to_end(&mut answer)
            .await
            .unwrap();
        assert!(answer.len() <= 64 * 1024, "session response size cap");
        String::from_utf8(answer).unwrap()
    }

    fn session_cookie(answer: &str) -> String {
        assert!(
            answer.starts_with("HTTP/1.1 303"),
            "the password creates a session"
        );
        answer
            .split_once("\r\n\r\n")
            .unwrap()
            .0
            .lines()
            .filter_map(|line| line.split_once(':'))
            .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
            .map(|(_, value)| value.trim().split(';').next().unwrap())
            .find(|value| value.starts_with("acp2api_session="))
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn the_exit_ends_the_browser_session_and_clears_its_cookie() {
        session_panel(|port| async move {
            let login = session_http(
                port,
                "POST",
                "/manage/login",
                "",
                "password=panel-exit-test",
            )
            .await;
            let cookie = session_cookie(&login);
            let headers = format!("Cookie: {cookie}\r\nOrigin: http://127.0.0.1:{port}\r\n");
            let before = session_http(port, "GET", "/", &headers, "").await;
            assert!(
                before.contains("id=\"local-api-key\""),
                "the cookie grants access before exit"
            );
            let exit = session_http(port, "POST", "/manage/exit", &headers, "").await;
            assert!(
                exit.starts_with("HTTP/1.1 303"),
                "the exit redirects to the password page"
            );
            let response_headers = exit.split_once("\r\n\r\n").unwrap().0.to_ascii_lowercase();
            assert!(response_headers.contains("\r\nlocation: /\r\n"));
            assert!(response_headers.contains(
                "\r\nset-cookie: acp2api_session=; path=/; httponly; samesite=strict; max-age=0\r\n"
            ));
            for browser_headers in ["", &headers] {
                let after = session_http(port, "GET", "/", browser_headers, "").await;
                assert!(after.starts_with("HTTP/1.1 200"));
                assert!(after.contains("autocomplete=\"current-password\""));
                assert!(
                    !after.contains("id=\"local-api-key\""),
                    "even replaying the old cookie cannot open the panel"
                );
            }
        })
        .await;
    }

    #[tokio::test]
    async fn the_exit_keeps_another_browser_session() {
        session_panel(|port| async move {
            let first = session_http(
                port,
                "POST",
                "/manage/login",
                "",
                "password=panel-exit-test",
            )
            .await;
            let second = session_http(
                port,
                "POST",
                "/manage/login",
                "",
                "password=panel-exit-test",
            )
            .await;
            let first = session_cookie(&first);
            let second = session_cookie(&second);
            assert!(first != second, "the browsers have distinct sessions");
            let first = format!("Cookie: {first}\r\n");
            let second = format!("Cookie: {second}\r\n");
            for headers in [&first, &second] {
                let before = session_http(port, "GET", "/", headers, "").await;
                assert!(before.contains("id=\"local-api-key\""));
            }
            let exit = session_http(port, "POST", "/manage/exit", &first, "").await;
            assert!(exit.starts_with("HTTP/1.1 303"));
            let ended = session_http(port, "GET", "/", &first, "").await;
            assert!(ended.contains("autocomplete=\"current-password\""));
            assert!(!ended.contains("id=\"local-api-key\""));
            let remaining = session_http(port, "GET", "/", &second, "").await;
            assert!(
                remaining.contains("id=\"local-api-key\""),
                "the other browser keeps panel access"
            );
            let action =
                session_http(port, "POST", "/manage/language", &second, "language=en").await;
            assert!(
                action.starts_with("HTTP/1.1 303"),
                "the other browser can still act as owner"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn the_exit_requires_a_valid_session() {
        session_panel(|port| async move {
            let login = session_http(
                port,
                "POST",
                "/manage/login",
                "",
                "password=panel-exit-test",
            )
            .await;
            let cookie = session_cookie(&login);
            let headers = format!("Cookie: {cookie}\r\n");
            let exit = session_http(port, "POST", "/manage/exit", &headers, "").await;
            assert!(
                exit.starts_with("HTTP/1.1 303"),
                "a valid session reaches the exit"
            );
            for invalid in ["Cookie: acp2api_session=unknown\r\n", "", &headers] {
                for endpoint in ["/manage/exit", "/manage/language"] {
                    let denied = session_http(port, "POST", endpoint, invalid, "language=en").await;
                    assert!(
                        denied.starts_with("HTTP/1.1 401"),
                        "each owner endpoint refuses an invalid session"
                    );
                    assert!(denied.contains("autocomplete=\"current-password\""));
                    assert!(!denied.to_ascii_lowercase().contains("set-cookie:"));
                }
            }
        })
        .await;
    }

    #[tokio::test]
    async fn the_exit_keeps_the_owner_form_origin_rule() {
        session_panel(|port| async move {
            let login = session_http(
                port,
                "POST",
                "/manage/login",
                "",
                "password=panel-exit-test",
            )
            .await;
            let cookie = session_cookie(&login);
            for origin in [
                "https://example.com".to_owned(),
                "null".to_owned(),
                format!("https://127.0.0.1:{port}"),
            ] {
                let headers = format!("Cookie: {cookie}\r\nOrigin: {origin}\r\n");
                let denied = session_http(port, "POST", "/manage/exit", &headers, "").await;
                assert!(
                    denied.starts_with("HTTP/1.1 400"),
                    "a foreign origin cannot end the session"
                );
                assert!(!denied.to_ascii_lowercase().contains("set-cookie:"));
            }
            let headers = format!("Cookie: {cookie}\r\n");
            let retained = session_http(port, "GET", "/", &headers, "").await;
            assert!(
                retained.contains("id=\"local-api-key\""),
                "refused requests keep the session"
            );
            let headers = format!("Cookie: {cookie}\r\nOrigin: http://127.0.0.1:{port}\r\n");
            let exit = session_http(port, "POST", "/manage/exit", &headers, "").await;
            assert!(
                exit.starts_with("HTTP/1.1 303"),
                "the panel origin can end its session"
            );
        })
        .await;
    }

    async fn assert_state_file_panel_diagnostic(folder: &std::path::Path, expected: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let clock = crate::Ports::production().clock;
            let journal = crate::journal::Journal::open(&folder.join("logs"), clock.wall()).unwrap();
            let mut builds = crate::runtime_builds::Builds::fixture(folder, clock.clone());
            let (stopped, stop) = tokio::sync::watch::channel(false);
            builds.start.stopped = stop;
            let settings = crate::store::Settings { port: 8329, concurrency_limit: 1, local_api_key: String::new(), management_password_hash: String::new(), agy_acp_server_path: None };
            let (conductor, task) = Conductor::start(builds, clock.clone(), journal, settings, folder.join("config.toml"), 1);
            conductor.builds(crate::runtime_builds::Action::Initial, crate::Hold::empty()).await.unwrap();
            let response = conductor.model_names(crate::Hold::empty()).await.err().unwrap().message.to_string();
            let panel = Panel::new(conductor, clock.clone(), 8329);
            panel.sessions.lock().unwrap().insert("panel-test-session".into(), Session { since: clock.now(), password_hash: String::new() });
            let mut headers = HeaderMap::new();
            headers.insert(header::COOKIE, "acp2api_session=panel-test-session".parse().unwrap());
            let Html(html) = index(State(panel), Extension(Language::English), headers).await;
            stopped.send(true).unwrap();
            task.await.unwrap();
            let first_tile = html.split(&format!("<section id=\"{}\"", crate::conductor::sign_in::ENTRY_ID)).nth(1).unwrap().split("</section>").next().unwrap();
            let owner_path = &*folder.to_string_lossy();
            assert_eq!((response.contains(expected), first_tile.contains(expected), response.contains(owner_path) || first_tile.contains(owner_path)), (true, true, false), "#161: the first API refusal and the lockout tile show the same path-free state-file diagnostic");
        }).await.expect("state-file panel test cap: five seconds, no runtime requests");
    }

    #[tokio::test]
    async fn a_damaged_state_file_shows_the_same_diagnostic_in_the_panel_and_first_response() {
        let (folder, expected) =
            crate::store::fixtures::state_file(crate::store::fixtures::StateFileFault::Damaged);
        assert_state_file_panel_diagnostic(&folder, &expected).await;
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[tokio::test]
    async fn an_unreadable_state_file_shows_the_same_diagnostic_in_the_panel_and_first_response() {
        let (folder, expected) =
            crate::store::fixtures::state_file(crate::store::fixtures::StateFileFault::Unreadable);
        assert_state_file_panel_diagnostic(&folder, &expected).await;
        std::fs::remove_dir_all(folder).unwrap();
    }

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
        let zero = ConcurrencyLimitForm {
            concurrency_limit: "0".into(),
        };
        let answers = [
            new_key(State(panel.clone()), Extension(Language::English)).await,
            password(
                State(panel.clone()),
                Extension(Language::English),
                Form(empty),
            )
            .await,
            concurrency(State(panel), Extension(Language::English), Form(zero)).await,
        ];
        let mut pages = Vec::new();
        for answer in answers {
            let status = answer.status().as_u16();
            let body = axum::body::to_bytes(answer.into_body(), 1 << 16).await;
            let body = String::from_utf8(body.unwrap().to_vec()).unwrap();
            pages.push((status, body.contains("id=\"local-api-key\"")));
        }
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(pages, [(500, true), (422, true), (422, true)]);
    }
    #[tokio::test]
    async fn the_language_cookie_survives_a_new_panel_session() {
        let response = set_language(Form(LanguageChoice {
            language: Language::Chinese,
        }))
        .await;
        assert_eq!(
            (
                response.status(),
                response.headers()[header::SET_COOKIE].to_str().unwrap(),
                response.headers()[header::LOCATION].to_str().unwrap(),
            ),
            (
                StatusCode::SEE_OTHER,
                "acp2api_language=zh; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000",
                "/",
            )
        );
    }
    #[tokio::test]
    async fn a_language_choice_without_a_panel_session_gets_no_cookie() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let folder = std::env::temp_dir().join(format!("acp2api-language-{}", std::process::id()));
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
        let (conductor, _task) = Conductor::start(
            crate::runtime_builds::Builds::fixture(&folder, clock.clone()),
            clock.clone(),
            journal,
            settings,
            folder.join("config.toml"),
            1,
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            axum::serve(listener, router(conductor, clock, port))
                .await
                .unwrap();
        });
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request = format!(
            "POST /manage/language HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 11\r\nConnection: close\r\n\r\nlanguage=zh"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut answer = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stream.read_to_string(&mut answer),
        )
        .await
        .expect("language request cap: one request, five seconds")
        .unwrap();
        server.abort();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(
            (
                answer.starts_with("HTTP/1.1 401"),
                answer.to_ascii_lowercase().contains("set-cookie"),
            ),
            (true, false),
            "FR-009 each management endpoint needs a panel session"
        );
    }
    #[tokio::test]
    async fn the_meaning_of_a_build_notice_selects_its_dot() {
        use crate::{compatibility_check::Incompatibility, messages::OwnerMessage};
        let folder = std::env::temp_dir().join(format!(
            "acp2api-notice-dot-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
        let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
        let settings = crate::store::Settings {
            port: 8329,
            concurrency_limit: 1,
            local_api_key: "acp2api-test".into(),
            management_password_hash: String::new(),
            agy_acp_server_path: None,
        };
        let (conductor, _task) = Conductor::start(
            crate::runtime_builds::Builds::fixture(&folder, clock.clone()),
            clock.clone(),
            journal,
            settings,
            folder.join("config.toml"),
            1,
        );
        let panel = Panel::new(conductor, clock, 8329);
        let mut lines = Vec::new();
        for notice in [
            OwnerMessage::CheckFailed(Incompatibility::NoVersion),
            OwnerMessage::CheckPassed {
                version: "1.6.0".into(),
            },
        ] {
            let answer = panel.notice(Language::English, &notice);
            let status = answer.status();
            let body = axum::body::to_bytes(answer.into_body(), 1 << 18).await;
            let body = String::from_utf8(body.unwrap().to_vec()).unwrap();
            lines.push((
                status,
                body.contains(&format!("<span class=\"dot critical\"></span>{notice}")),
                body.contains(&format!("<span class=\"dot good\"></span>{notice}")),
            ));
        }
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(
            lines,
            [(StatusCode::OK, true, false), (StatusCode::OK, false, true)],
            "a failed check keeps its HTTP 200 and gets the critical dot"
        );
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
        let response = login(
            State(panel.clone()),
            Extension(Language::English),
            Form(password),
        )
        .await;
        let signed_in = login(
            State(panel),
            Extension(Language::English),
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
