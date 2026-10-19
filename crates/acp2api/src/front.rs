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
    key: String,
    journal: crate::journal::Journal,
}
pub(crate) fn router(
    conductor: Conductor,
    clock: Arc<dyn Clock>,
    key: String,
    tools: crate::runtime_driver::mcp::ToolSets,
    journal: crate::journal::Journal,
) -> Router {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/messages", post(messages))
        .route("/v1/responses", post(responses))
        .route("/v1/chat/completions", post(chat))
        .with_state(App {
            conductor,
            clock: clock.clone(),
            key,
            journal,
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
    serve(
        app,
        headers,
        lease,
        body,
        dialect::MESSAGES,
        crate::journal::Api::Messages,
    )
    .await
}
async fn responses(
    State(app): State<App>,
    headers: HeaderMap,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    serve(
        app,
        headers,
        lease,
        body,
        dialect::RESPONSES,
        crate::journal::Api::Responses,
    )
    .await
}
async fn chat(
    State(app): State<App>,
    headers: HeaderMap,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    body: axum::body::Bytes,
) -> axum::response::Response {
    serve(
        app,
        headers,
        lease,
        body,
        dialect::CHAT,
        crate::journal::Api::Chat,
    )
    .await
}
async fn serve(
    app: App,
    headers: HeaderMap,
    lease: RequestHold,
    body: axum::body::Bytes,
    dialect: &'static dyn Dialect,
    api: crate::journal::Api,
) -> axum::response::Response {
    let hold = lease.0.lock().unwrap().take().expect("request hold");
    let request = app.journal.begin(api, app.clock.now());
    let id = request.id.clone();
    let mut response = dispatch(app, headers, hold, body, dialect, request).await;
    response.headers_mut().insert(
        "x-request-id",
        id.parse().expect("generated request identifier"),
    );
    response
}
async fn dispatch(
    app: App,
    headers: HeaderMap,
    hold: crate::Hold,
    body: axum::body::Bytes,
    dialect: &'static dyn Dialect,
    mut request: crate::journal::Request,
) -> axum::response::Response {
    let authorized = authorized(&headers, &app.key);
    if !authorized {
        return refuse(&app, request, dialect, Message::Key.into());
    }
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
    headers: HeaderMap,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let hold = lease.0.lock().unwrap().take().expect("request hold");
    let request = app
        .journal
        .begin(crate::journal::Api::Models, app.clock.now());
    let id = request.id.clone();
    let mut response = if !authorized(&headers, &app.key) {
        refuse(&app, request, dialect::CHAT, Message::Key.into())
    } else {
        match app.conductor.model_names(hold).await {
            Ok(entries) => {
                let completion = crate::journal::Completion {
                    request,
                    session: None,
                    outcome: crate::journal::Outcome::ModelList,
                };
                match app.journal.complete(completion, app.clock.as_ref()) {
                    Ok(()) => axum::Json(serde_json::json!({"object":"list","data":entries}))
                        .into_response(),
                    Err(_) => dialect::CHAT
                        .refusal(&Message::Upstream("cannot write request metadata".into()).into()),
                }
            }
            Err(error) => refuse(&app, request, dialect::CHAT, error),
        }
    };
    response.headers_mut().insert(
        "x-request-id",
        id.parse().expect("generated request identifier"),
    );
    response
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
