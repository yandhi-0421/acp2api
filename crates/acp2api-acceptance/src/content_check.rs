//! Content assertions use loopback HTTP and the fake process's captured ACP input.
use crate::{Scenario, client::ApiCall};
use serde_json::{Value, json};
pub const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jX1sAAAAASUVORK5CYII=";
pub fn request(endpoint: &str, parts: Value) -> ApiCall {
    let mut call = ApiCall::one_user_message(endpoint, "");
    let message = json!({"role":"user","content":parts});
    if endpoint == "POST /v1/responses" {
        call.body = json!({"model":"gemini-3.8-flash","input":[message]});
    } else {
        call.body["messages"] = json!([message]);
    }
    call
}
pub fn text(endpoint: &str, text: &str) -> Value {
    json!({"type":if endpoint=="POST /v1/responses" {"input_text"} else {"text"},"text":text})
}
pub fn image(endpoint: &str, data: &str) -> Value {
    match endpoint {
        "POST /v1/messages" => {
            json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":data}})
        }
        "POST /v1/responses" => {
            json!({"type":"input_image","image_url":format!("data:image/png;base64,{data}")})
        }
        _ => {
            json!({"type":"image_url","image_url":{"url":format!("data:image/png;base64,{data}")}})
        }
    }
}
pub fn url(endpoint: &str, url: &str, file: bool) -> Value {
    match (endpoint, file) {
        ("POST /v1/messages", false) => json!({"type":"image","source":{"type":"url","url":url}}),
        ("POST /v1/messages", true) => json!({"type":"document","source":{"type":"url","url":url}}),
        ("POST /v1/responses", false) => json!({"type":"input_image","image_url":url}),
        ("POST /v1/responses", true) => json!({"type":"input_file","file_url":url}),
        (_, false) => json!({"type":"image_url","image_url":{"url":url}}),
        (_, true) => json!({"type":"file","file":{"file_url":url}}),
    }
}
pub fn document(endpoint: &str, media: &str, title: Option<&str>) -> Value {
    let mut doc = if endpoint == "POST /v1/messages" {
        json!({"type":"document","source":{"type":"base64","media_type":media,"data":"TGluZSBvbmUu"}})
    } else {
        json!({"type":"input_file","file_data":format!("data:{media};base64,TGluZSBvbmUu")})
    };
    if endpoint == "POST /v1/chat/completions" {
        doc["type"] = "file".into();
        doc = json!({"type":"file","file":doc});
    }
    if let Some(title) = title {
        doc["title"] = title.into();
    }
    doc
}
pub async fn send(s: &mut Scenario, call: ApiCall) {
    s.answer = Some(s.client().await.send(call).finish(&mut s.time).await);
}
pub fn blocks(s: &Scenario) -> Vec<Value> {
    s.hub
        .entries()
        .iter()
        .filter_map(|e| {
            let v: Value = serde_json::from_str(e.event["line"].as_str()?).ok()?;
            (v["method"] == "session/prompt").then(|| v["params"]["prompt"].clone())
        })
        .collect()
}
pub fn refusal(s: &Scenario, endpoint: &str, message: &str, code: &str, param: Option<&str>) {
    let a = s.answer.as_ref().unwrap();
    assert_eq!(a.status, 400, "content refusal status");
    assert!(a.events.is_empty(), "refusal emits no success SSE");
    let expected = if endpoint == "POST /v1/messages" {
        json!({"type":"error","error":{"type":"invalid_request_error","message":message}})
    } else {
        json!({"error":{"message":message,"type":"invalid_request_error","param":param,"code":code}})
    };
    assert_eq!(a.json(), expected, "content refusal envelope");
    assert!(blocks(s).is_empty(), "refusal starts no ACP prompt");
}
/// A reachable, bounded local sink observes accidental URL fetches without contacting example.com.
pub async fn no_fetch(s: &mut Scenario, endpoint: &str, file: bool) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = count.clone();
    let task = tokio::spawn(async move {
        for _ in 0..4 {
            let Ok(Ok((mut socket, _))) =
                tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept()).await
            else {
                break;
            };
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut bytes = [0; 1024];
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(1), socket.read(&mut bytes))
                    .await;
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
        }
    });
    let mut control = tokio::net::TcpStream::connect(addr).await.unwrap();
    control
        .write_all(b"GET /control HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut reply = [0; 128];
    let n = tokio::time::timeout(std::time::Duration::from_secs(1), control.read(&mut reply))
        .await
        .unwrap()
        .unwrap();
    assert!(
        reply[..n].starts_with(b"HTTP/1.1 200"),
        "sink reachability control"
    );
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    send(
        s,
        request(
            endpoint,
            json!([url(endpoint, &format!("http://{addr}/forbidden"), file)]),
        ),
    )
    .await;
    task.await.unwrap();
    assert_eq!(
        count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "no URL fetch; one control request only"
    );
}
