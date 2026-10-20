use crate::{
    Clock, HoldKind,
    conductor::Conductor,
    dialect::{self, Dialect},
    messages::Message,
};
use axum::{
    Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use std::sync::{Arc, Mutex};
#[derive(Clone)]
struct RequestHold(Arc<Mutex<Option<crate::Hold>>>);
#[derive(Clone)]
struct App {
    conductor: Conductor,
    clock: Arc<dyn Clock>,
    port: u16,
    journal: crate::journal::Journal,
}
pub(crate) fn router(
    conductor: Conductor,
    clock: Arc<dyn Clock>,
    tools: crate::runtime_driver::mcp::ToolSets,
    journal: crate::journal::Journal,
    port: u16,
) -> Router {
    let app = App {
        conductor: conductor.clone(),
        clock: clock.clone(),
        journal,
        port,
    };
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/messages", post(messages))
        .route("/v1/responses", post(responses))
        .route("/v1/chat/completions", post(chat))
        .layer(axum::middleware::from_fn_with_state(
            app.clone(),
            access_guard,
        ))
        .with_state(app)
        .merge(tools.router())
        .merge(crate::panel::router(conductor, clock.clone(), port))
        .layer(axum::middleware::from_fn_with_state(
            clock.clone(),
            input_hold,
        ))
}
async fn access_guard(
    State(app): State<App>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use crate::journal::Api;
    use axum::http::{Method, StatusCode};
    let (api, dialect, method) = match request.uri().path() {
        "/v1/models" => (Api::Models, dialect::CHAT, Method::GET),
        "/v1/messages" => (Api::Messages, dialect::MESSAGES, Method::POST),
        "/v1/responses" => (Api::Responses, dialect::RESPONSES, Method::POST),
        "/v1/chat/completions" => (Api::Chat, dialect::CHAT, Method::POST),
        _ => return next.run(request).await,
    };
    let record = app.journal.begin(api, app.clock.now());
    let id = record.id.clone();
    let mut response = if let Some(error) = access_error(request.headers(), &app) {
        refuse(&app, record, dialect, error)
    } else if request.method() != method {
        let mut response = refuse(
            &app,
            record,
            dialect,
            crate::messages::Refusal::invalid("acp2api: the HTTP method is not supported."),
        );
        *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
        response
    } else {
        request.extensions_mut().insert(record);
        next.run(request).await
    };
    response.headers_mut().insert(
        "x-request-id",
        id.parse().expect("generated request identifier"),
    );
    response
}
fn access_error(headers: &HeaderMap, app: &App) -> Option<crate::messages::Refusal> {
    if headers.contains_key("origin") {
        return Some(crate::messages::Refusal::invalid(
            "acp2api: the APIs do not accept an Origin header.",
        ));
    }
    let host = headers.get("host").and_then(|v| v.to_str().ok());
    let address = format!("127.0.0.1:{}", app.port);
    let localhost = format!("localhost:{}", app.port);
    if host != Some(address.as_str()) && host != Some(localhost.as_str()) {
        return Some(crate::messages::Refusal::invalid(
            "acp2api: the Host header does not match the local listener.",
        ));
    }
    if !authorized(headers, &app.conductor.settings().local_api_key) {
        return Some(Message::Key.into());
    }
    None
}
async fn messages(
    State(app): State<App>,
    axum::extract::Extension(request): axum::extract::Extension<crate::journal::Request>,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    serve(app, request, lease, body, dialect::MESSAGES).await
}
async fn responses(
    State(app): State<App>,
    axum::extract::Extension(request): axum::extract::Extension<crate::journal::Request>,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    serve(app, request, lease, body, dialect::RESPONSES).await
}
async fn chat(
    State(app): State<App>,
    axum::extract::Extension(request): axum::extract::Extension<crate::journal::Request>,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    serve(app, request, lease, body, dialect::CHAT).await
}
async fn serve(
    app: App,
    request: crate::journal::Request,
    lease: RequestHold,
    body: axum::body::Bytes,
    dialect: &'static dyn Dialect,
) -> axum::response::Response {
    let hold = lease.0.lock().unwrap().take().expect("request hold");
    dispatch(app, hold, body, dialect, request).await
}
async fn dispatch(
    app: App,
    hold: crate::Hold,
    body: axum::body::Bytes,
    dialect: &'static dyn Dialect,
    mut request: crate::journal::Request,
) -> axum::response::Response {
    let parsed = match dialect.parse(&body) {
        Ok(parsed) => parsed,
        Err(error) => return refuse(&app, request, dialect, error),
    };
    request.model = Some(parsed.turn.model.clone());
    request.unknown_fields = parsed.unknown_fields;
    match app
        .conductor
        .admit(parsed.turn, hold, request.clone())
        .await
    {
        Ok(ticket) => dialect.respond(parsed.form, ticket, app.clock).await,
        Err(error) => refuse(&app, request, dialect, error),
    }
}
fn authorized(headers: &HeaderMap, key: &str) -> bool {
    headers.get("x-api-key").and_then(|v| v.to_str().ok()) == Some(key)
        || headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            == Some(key)
}
async fn models(
    State(app): State<App>,
    axum::extract::Extension(request): axum::extract::Extension<crate::journal::Request>,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let hold = lease.0.lock().unwrap().take().expect("request hold");
    match app.conductor.model_names(hold).await {
        Ok(entries) => {
            let completion = crate::journal::Completion {
                request,
                session: None,
                outcome: crate::journal::Outcome::ModelList,
            };
            match app.journal.complete(completion, app.clock.as_ref()) {
                Ok(()) => {
                    axum::Json(serde_json::json!({"object":"list","data":entries})).into_response()
                }
                Err(_) => dialect::CHAT
                    .refusal(&Message::Upstream("cannot write request metadata".into()).into()),
            }
        }
        Err(error) => refuse(&app, request, dialect::CHAT, error),
    }
}

async fn input_hold(
    State(clock): State<Arc<dyn Clock>>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let kind = if request.uri().path().starts_with("/mcp/") {
        HoldKind::McpRequest
    } else {
        HoldKind::HttpRequest
    };
    let lease = RequestHold(Arc::new(Mutex::new(Some(clock.hold(kind)))));
    request.extensions_mut().insert(lease.clone());
    let response = next.run(request).await;
    drop(lease);
    response
}

fn refuse(
    app: &App,
    request: crate::journal::Request,
    dialect: &dyn Dialect,
    error: crate::messages::Refusal,
) -> axum::response::Response {
    let completion = crate::journal::Completion {
        request,
        session: None,
        outcome: crate::journal::Outcome::Refused(dialect::refusal_code(&error)),
    };
    match app.journal.complete(completion, app.clock.as_ref()) {
        Ok(()) => dialect.refusal(&error),
        Err(_) => {
            dialect.refusal(&Message::Upstream("cannot write request metadata".into()).into())
        }
    }
}
