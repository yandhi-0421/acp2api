use crate::{
    Clock, HoldKind,
    conductor::Conductor,
    dialect::{self, Dialect},
    messages::Message,
};
use axum::{Router, extract::State, http::HeaderMap, routing::post};
use std::sync::{Arc, Mutex};
#[derive(Clone)]
struct RequestHold(Arc<Mutex<Option<crate::Hold>>>);
#[derive(Clone)]
struct App {
    conductor: Conductor,
    clock: Arc<dyn Clock>,
    key: String,
}
pub(crate) fn router(
    conductor: Conductor,
    clock: Arc<dyn Clock>,
    key: String,
    tools: crate::runtime_driver::mcp::ToolSets,
) -> Router {
    Router::new()
        .route("/v1/messages", post(messages))
        .route("/v1/responses", post(responses))
        .route("/v1/chat/completions", post(chat))
        .with_state(App {
            conductor,
            clock: clock.clone(),
            key,
        })
        .merge(tools.router())
        .layer(axum::middleware::from_fn_with_state(
            clock.clone(),
            input_hold,
        ))
}
async fn messages(
    State(app): State<App>,
    headers: HeaderMap,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    serve(app, headers, lease, body, dialect::MESSAGES).await
}
async fn responses(
    State(app): State<App>,
    headers: HeaderMap,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    serve(app, headers, lease, body, dialect::RESPONSES).await
}
async fn chat(
    State(app): State<App>,
    headers: HeaderMap,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    serve(app, headers, lease, body, dialect::CHAT).await
}
async fn serve(
    app: App,
    headers: HeaderMap,
    lease: RequestHold,
    body: axum::body::Bytes,
    dialect: &'static dyn Dialect,
) -> axum::response::Response {
    let hold = lease.0.lock().unwrap().take().expect("request hold");
    let authorized = headers.get("x-api-key").and_then(|v| v.to_str().ok()) == Some(&app.key)
        || headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            == Some(&app.key);
    if !authorized {
        return dialect.refusal(&Message::Key.into());
    }
    let parsed = match dialect.parse(&body) {
        Ok(parsed) => parsed,
        Err(error) => return dialect.refusal(&error),
    };
    match app.conductor.admit(parsed.turn, hold).await {
        Ok(ticket) => dialect.respond(parsed.form, ticket, app.clock).await,
        Err(error) => dialect.refusal(&Message::Upstream(error).into()),
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
