use crate::{Clock, HoldKind, conductor::Conductor, conversation::TurnRequest};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
#[derive(Clone)]
struct RequestHold(Arc<Mutex<Option<crate::Hold>>>);
#[derive(Clone)]
struct App {
    conductor: Conductor,
    clock: Arc<dyn Clock>,
    key: String,
    secret: String,
}
#[derive(Deserialize)]
struct Request {
    model: String,
    messages: Vec<Message>,
}
#[derive(Deserialize)]
struct Message {
    role: String,
    content: String,
}
pub(crate) fn router(
    conductor: Conductor,
    clock: Arc<dyn Clock>,
    key: String,
    secret: String,
) -> Router {
    Router::new()
        .route("/v1/messages", post(messages))
        .route("/mcp/{secret}", post(mcp))
        .layer(axum::middleware::from_fn_with_state(
            clock.clone(),
            input_hold,
        ))
        .with_state(App {
            conductor,
            clock,
            key,
            secret,
        })
}
async fn messages(
    State(app): State<App>,
    headers: HeaderMap,
    axum::extract::Extension(lease): axum::extract::Extension<RequestHold>,
    Json(request): Json<Request>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let hold = lease.0.lock().unwrap().take().expect("request hold");
    if headers.get("x-api-key").and_then(|v| v.to_str().ok()) != Some(&app.key) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":{"type":"authentication_error","message":"Invalid API key"}})),
        ));
    }
    if request.messages.len() != 1 || request.messages[0].role != "user" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error":{"message":"one user message required"}})),
        ));
    }
    let answer = app
        .conductor
        .answer(
            TurnRequest {
                text: request.messages[0].content.clone(),
                model: request.model.clone(),
            },
            hold,
        )
        .await
        .map_err(|message| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error":{"type":"api_error","message":message}})),
            )
        })?;
    let _hold = app.clock.hold(HoldKind::TurnEvent);
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":{"type":"api_error","message":e.to_string()}})),
        )
    })?;
    let id = format!(
        "msg_{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    Ok(Json(
        json!({"id":id,"type":"message","role":"assistant","model":request.model,"content":[{"type":"text","text":answer.text}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":answer.input_tokens,"output_tokens":answer.output_tokens}}),
    ))
}

async fn mcp(
    State(app): State<App>,
    axum::extract::Path(secret): axum::extract::Path<String>,
    Json(call): Json<Value>,
) -> (StatusCode, Json<Value>) {
    if secret != app.secret {
        return (StatusCode::NOT_FOUND, Json(Value::Null));
    }
    let id = call["id"].clone();
    let result = match call["method"].as_str() {
        Some("initialize") => {
            json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"acp2api","version":env!("CARGO_PKG_VERSION")}})
        }
        Some("notifications/initialized") => return (StatusCode::ACCEPTED, Json(Value::Null)),
        Some("tools/list") => json!({"tools":[]}),
        Some("tools/call") => {
            json!({"isError":true,"content":[{"type":"text","text":"Client tools execute in the API client."}]})
        }
        _ => {
            return (
                StatusCode::OK,
                Json(
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
                ),
            );
        }
    };
    (
        StatusCode::OK,
        Json(json!({"jsonrpc":"2.0","id":id,"result":result})),
    )
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
