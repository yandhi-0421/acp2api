use crate::conversation::ClientTool;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub(crate) struct ToolSets {
    port: u16,
    sets: Arc<RwLock<HashMap<String, Vec<Value>>>>,
}
impl ToolSets {
    pub(crate) fn new(port: u16) -> Self {
        Self {
            port,
            sets: Arc::default(),
        }
    }
    pub(crate) fn insert(&self, tools: &[ClientTool]) -> Result<String, String> {
        let mut random = [0; 32];
        getrandom::fill(&mut random).map_err(|e| e.to_string())?;
        let secret: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let tools:Result<Vec<_>,_>=tools.iter().map(|tool|serde_json::from_str::<Value>(&tool.schema).map(|schema|json!({"name":tool.name,"description":tool.description,"inputSchema":schema}))).collect();
        match self.sets.write().unwrap().entry(secret.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(tools.map_err(|e| e.to_string())?);
            }
            Entry::Occupied(_) => return Err("MCP path collision".into()),
        }
        Ok(format!("http://127.0.0.1:{}/mcp/{secret}", self.port))
    }
    pub(crate) fn router(self) -> Router {
        Router::new()
            .route("/mcp/{secret}", post(serve))
            .with_state(self)
    }
}
async fn serve(
    State(sets): State<ToolSets>,
    Path(secret): Path<String>,
    Json(call): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let Some(tools) = sets.sets.read().unwrap().get(&secret).cloned() else {
        return (StatusCode::NOT_FOUND, Json(Value::Null));
    };
    let id = call["id"].clone();
    let result = match call["method"].as_str() {
        Some("initialize") => {
            json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"acp2api","version":env!("CARGO_PKG_VERSION")}})
        }
        Some("notifications/initialized") => return (StatusCode::ACCEPTED, Json(Value::Null)),
        Some("tools/list") => json!({"tools":tools}),
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[allow(
        clippy::disallowed_methods,
        reason = "The test calls only its loopback MCP listener."
    )]
    async fn post(address: std::net::SocketAddr, path: &str, method: &str) -> (u16, Value) {
        tokio::time::timeout(std::time::Duration::from_secs(2),async {
            let mut socket=tokio::net::TcpStream::connect(address).await.unwrap();
            let body=json!({"jsonrpc":"2.0","id":1,"method":method,"params":{"name":"must_not_execute","arguments":{}}}).to_string();
            socket.write_all(format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            let mut bytes=Vec::new();socket.take(8193).read_to_end(&mut bytes).await.unwrap();assert!(bytes.len()<=8192,"MCP test answer cap");
            let split=bytes.windows(4).position(|p|p==b"\r\n\r\n").unwrap();let status=std::str::from_utf8(&bytes[..split]).unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
            (status,serde_json::from_slice(&bytes[split+4..]).unwrap())
        }).await.expect("MCP test request cap: 2 s")
    }
    #[tokio::test]
    async fn mcp_paths_keep_their_tool_set_and_calls_finish_with_an_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let sets = ToolSets::new(address.port());
        let first = sets
            .insert(&[ClientTool {
                name: "must_not_execute".into(),
                description: "First immutable tool".into(),
                schema: r#"{"type":"object"}"#.into(),
            }])
            .unwrap();
        let second = sets.insert(&[]).unwrap();
        assert_ne!(first, second);
        let router = sets.router();
        let server = tokio::spawn(async move {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                axum::serve(listener, router).into_future(),
            )
            .await;
        });
        // Four bounded requests exercise the production HTTP handler.
        let path = |url: &str| {
            url.strip_prefix(&format!("http://{address}"))
                .unwrap()
                .to_owned()
        };
        let (status, listed) = post(address, &path(&first), "tools/list").await;
        assert_eq!(status, 200);
        assert_eq!(
            listed["result"]["tools"],
            json!([{"name":"must_not_execute","description":"First immutable tool","inputSchema":{"type":"object"}}])
        );
        let (_, empty) = post(address, &path(&second), "tools/list").await;
        assert_eq!(empty["result"]["tools"], json!([]));
        let (_, call) = post(address, &path(&first), "tools/call").await;
        assert_eq!(
            call["result"]["isError"], true,
            "MCP cannot execute a client tool"
        );
        assert_eq!(
            call["result"]["content"][0]["text"],
            "Client tools execute in the API client."
        );
        assert_eq!(
            post(address, "/mcp/unregistered", "tools/list").await.0,
            404
        );
        server.abort();
    }
}
