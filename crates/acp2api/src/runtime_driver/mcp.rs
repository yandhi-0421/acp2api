use crate::conversation::ClientTool;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    sync::{Arc, RwLock},
};

pub(crate) const SERVER_NAME: &str = "client-tools";
const FUNCTION_NAME_LIMIT: usize = 64;
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
        let tools:Result<Vec<_>,_>=tools.iter().zip(tool_names(tools)).map(|(tool,name)|serde_json::from_str::<Value>(&tool.schema).map(|schema|json!({"name":name,"description":tool.description,"inputSchema":schema}))).collect();
        match self.sets.write().unwrap().entry(secret.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(tools.map_err(|e| e.to_string())?);
            }
            Entry::Occupied(_) => return Err("MCP path collision".into()),
        }
        Ok(format!("http://127.0.0.1:{}/mcp/{secret}", self.port))
    }
    pub(crate) fn remove(&self, url: &str) {
        if let Some((_, secret)) = url.rsplit_once('/') {
            self.sets.write().unwrap().remove(secret);
        }
    }
    pub(crate) fn router(self) -> Router {
        Router::new()
            .route("/mcp/{secret}", post(serve))
            .with_state(self)
    }
}
pub(crate) fn tool_names(tools: &[ClientTool]) -> Vec<String> {
    const LIMIT: usize = FUNCTION_NAME_LIMIT - "mcp_".len() - SERVER_NAME.len() - "_".len();
    let mut used: HashSet<String> = tools
        .iter()
        .filter(|tool| tool.namespace.is_none())
        .map(|tool| tool.name.clone())
        .collect();
    tools
        .iter()
        .map(|tool| {
            let Some(namespace) = &tool.namespace else {
                return tool.name.clone();
            };
            let readable = format!("{namespace}__{}", tool.name);
            if readable.len() <= LIMIT && used.insert(readable.clone()) {
                return readable;
            }
            let mut end = readable.len().min(LIMIT - "_12345678".len());
            while !readable.is_char_boundary(end) {
                end -= 1;
            }
            (0u32..)
                .map(|attempt| {
                    let hash = Sha256::new()
                        .chain_update(namespace)
                        .chain_update([0])
                        .chain_update(&tool.name)
                        .chain_update(attempt.to_le_bytes())
                        .finalize();
                    let suffix: String = hash[..4].iter().map(|b| format!("{b:02x}")).collect();
                    format!("{}_{suffix}", &readable[..end])
                })
                .find(|name| used.insert(name.clone()))
                .unwrap()
        })
        .collect()
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
    #[test]
    fn a_removed_tool_set_is_gone() {
        let sets = ToolSets::new(1);
        let url = sets.insert(&[]).unwrap();
        sets.remove(&url);
        assert!(sets.sets.read().unwrap().is_empty());
    }
    #[tokio::test]
    async fn mcp_paths_keep_their_tool_set_and_calls_finish_with_an_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let sets = ToolSets::new(address.port());
        let first = sets
            .insert(&[ClientTool {
                name: "must_not_execute".into(),
                kind: crate::conversation::ToolKind::Function,
                namespace: None,
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
