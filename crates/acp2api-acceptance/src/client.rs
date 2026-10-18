use crate::{hub::Hub, timeline::TimeLine};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, client::conn::http1};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
const BODY_CAP: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
    pub id: Option<String>,
    pub at: u64,
}
#[derive(Debug, Default)]
pub struct SseReader {
    pending: Vec<u8>,
    event: String,
    data: Vec<String>,
    id: Option<String>,
    skip_lf: bool,
}
impl SseReader {
    pub fn feed(&mut self, bytes: &[u8], at: u64) -> Result<Vec<SseEvent>, String> {
        let mut events = Vec::new();
        for b in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if *b == b'\n' {
                    continue;
                }
            }
            if *b == b'\r' || *b == b'\n' {
                self.skip_lf = *b == b'\r';
                let raw = std::mem::take(&mut self.pending);
                let line = String::from_utf8(raw).map_err(|_| "invalid SSE UTF-8")?;
                if line.is_empty() {
                    if !self.data.is_empty() {
                        events.push(SseEvent {
                            event: if self.event.is_empty() {
                                "message".into()
                            } else {
                                std::mem::take(&mut self.event)
                            },
                            data: self.data.join("\n"),
                            id: self.id.clone(),
                            at,
                        });
                        self.data.clear();
                    }
                    self.event.clear();
                } else if !line.starts_with(':') {
                    let (field, value) = line.split_once(':').unwrap_or((&line, ""));
                    let value = value.strip_prefix(' ').unwrap_or(value);
                    match field {
                        "event" => self.event = value.into(),
                        "data" => self.data.push(value.into()),
                        "id" if !value.contains('\0') => self.id = Some(value.into()),
                        _ => {}
                    }
                }
            } else {
                self.pending.push(*b);
                if self.pending.len() > BODY_CAP {
                    return Err("SSE line cap: 8 MiB".into());
                }
            }
        }
        Ok(events)
    }
}
#[derive(Clone)]
pub struct ApiClient {
    address: String,
    key: String,
    hub: Hub,
    requests: Arc<AtomicUsize>,
}
#[derive(Clone, Debug)]
pub struct Answer {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub events: Vec<SseEvent>,
}
impl Answer {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("JSON response")
    }
}
pub struct ApiCall {
    pub path: String,
    pub body: Value,
    pub headers: Vec<(String, String)>,
    pub method: String,
}
impl ApiCall {
    pub fn one_user_message(endpoint: &str, text: &str) -> Self {
        let (method, path) = endpoint.split_once(' ').expect("METHOD /path");
        Self {
            path: path.into(),
            method: method.into(),
            body: json!({"model":"gemini-3.8-flash","max_tokens":1024,"messages":[{"role":"user","content":text}],"stream":false}),
            headers: Vec::new(),
        }
    }
}
pub struct Call {
    task: tokio::task::JoinHandle<Result<Answer, String>>,
    hub: Hub,
    progress: Arc<Mutex<Vec<SseEvent>>>,
}
impl Drop for Call {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Call {
    pub fn events(&self) -> Vec<SseEvent> {
        self.progress.lock().unwrap().clone()
    }
    pub async fn finish(mut self, time: &mut TimeLine) -> Answer {
        let start = time.hub.now();
        let real = Instant::now() + Duration::from_secs(30);
        loop {
            time.settle().await;
            if self.task.is_finished() {
                return (&mut self.task)
                    .await
                    .expect("API request task")
                    .expect("API request");
            }
            assert!(Instant::now() < real, "API request cap: 30 s");
            if let Some(next) = time.next_deadline() {
                assert!(
                    next - start <= 3 * 60 * 60 * 1000,
                    "API finish virtual cap: 3 hours"
                );
                time.advance_to(next).await;
            } else {
                let hub = self.hub.clone();
                tokio::task::spawn_blocking(move || hub.wait_change(real))
                    .await
                    .unwrap();
            }
        }
    }
}
impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiClient")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}
impl ApiClient {
    pub fn new(address: String, key: String, hub: Hub) -> Self {
        Self {
            address,
            key,
            hub,
            requests: Arc::new(AtomicUsize::new(0)),
        }
    }
    pub fn send(&self, call: ApiCall) -> Call {
        assert!(
            self.requests.fetch_add(1, Ordering::SeqCst) < 300,
            "API request cap: 300"
        );
        self.hub.shared.0.lock().unwrap().counts.http_sent += 1;
        let client = self.clone();
        let progress = Arc::new(Mutex::new(Vec::new()));
        let capture = progress.clone();
        let task = tokio::spawn(async move {
            let result =
                tokio::time::timeout(Duration::from_secs(30), client.exchange(call, capture))
                    .await
                    .map_err(|_| "API request cap: 30 s".to_owned())
                    .and_then(|v| v);
            let mut s = client.hub.shared.0.lock().unwrap();
            s.counts.http_finished += 1;
            client.hub.shared.1.notify_all();
            result
        });
        Call {
            task,
            hub: self.hub.clone(),
            progress,
        }
    }
    async fn exchange(
        &self,
        call: ApiCall,
        progress: Arc<Mutex<Vec<SseEvent>>>,
    ) -> Result<Answer, String> {
        let stream = tokio::net::TcpStream::connect(&self.address)
            .await
            .map_err(|e| e.to_string())?;
        let stream = stream.into_std().map_err(|e| e.to_string())?;
        let id = {
            let mut s = self.hub.shared.0.lock().unwrap();
            s.next_socket += 1;
            let id = s.next_socket;
            s.client_sockets.insert(
                id,
                crate::hub::ClientSocket {
                    socket: stream.try_clone().map_err(|e| e.to_string())?,
                    read: 0,
                    consumed: 0,
                },
            );
            id
        };
        let lease = SocketLease {
            id,
            hub: self.hub.clone(),
        };
        let stream = CountSocket {
            inner: tokio::net::TcpStream::from_std(stream).map_err(|e| e.to_string())?,
            id,
            hub: self.hub.clone(),
        };
        let (mut sender, connection) = http1::handshake(TokioIo::new(stream))
            .await
            .map_err(|e| e.to_string())?;
        let connection = AbortOnDrop(tokio::spawn(async move {
            let _ = connection.await;
        }));
        let mut builder = Request::builder()
            .method(call.method.as_str())
            .uri(&call.path)
            .header("host", &self.address)
            .header("content-type", "application/json")
            .header("x-api-key", &self.key);
        for (key, value) in call.headers {
            builder.headers_mut().expect("request headers").insert(
                hyper::header::HeaderName::from_bytes(key.as_bytes()).map_err(|e| e.to_string())?,
                hyper::header::HeaderValue::from_str(&value).map_err(|e| e.to_string())?,
            );
        }
        let encoded = if builder
            .headers_ref()
            .and_then(|h| h.get("content-type"))
            .is_some_and(|v| v == "application/x-www-form-urlencoded")
        {
            call.body
                .as_object()
                .ok_or("form fields")?
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        form_encode(k),
                        form_encode(v.as_str().unwrap_or(""))
                    )
                })
                .collect::<Vec<_>>()
                .join("&")
        } else if call.method == "GET" {
            String::new()
        } else {
            call.body.to_string()
        };
        let request = builder
            .body(Full::new(Bytes::from(encoded)))
            .map_err(|e| e.to_string())?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_owned()))
            .collect();
        let is_sse = response
            .headers()
            .get("content-type")
            .is_some_and(|v| v.as_bytes().starts_with(b"text/event-stream"));
        let mut body = response.into_body();
        let mut bytes = Vec::new();
        let mut events = Vec::new();
        let mut sse = SseReader::default();
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|e| e.to_string())?;
            if let Ok(data) = frame.into_data() {
                if bytes.len() + data.len() > BODY_CAP {
                    return Err("API answer cap: 8 MiB".into());
                }
                bytes.extend_from_slice(&data);
                if is_sse {
                    let batch = sse.feed(&data, self.hub.now())?;
                    progress.lock().unwrap().extend(batch.clone());
                    events.extend(batch);
                }
                {
                    let mut s = self.hub.shared.0.lock().unwrap();
                    if let Some(socket) = s.client_sockets.get_mut(&id) {
                        socket.consumed = socket.read;
                    }
                }
                self.hub.shared.1.notify_all();
            }
        }
        drop(sender);
        drop(connection);
        drop(lease);
        Ok(Answer {
            status,
            headers,
            body: bytes,
            events,
        })
    }
}
pub struct Owner {
    pub client: ApiClient,
    pub cookies: HashMap<String, String>,
    pub language: String,
}
impl std::fmt::Debug for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Owner")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}
impl Owner {
    pub fn new(client: ApiClient) -> Self {
        Self {
            client,
            cookies: HashMap::new(),
            language: "en".into(),
        }
    }
    pub fn request(&self, method: &str, path: &str, body: Value) -> Call {
        let cookie = self
            .cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        self.client.send(ApiCall {
            method: method.into(),
            path: path.into(),
            body,
            headers: vec![
                ("accept-language".into(), self.language.clone()),
                ("cookie".into(), cookie),
            ],
        })
    }
    pub fn page(&self) -> Call {
        self.request("GET", "/", Value::Null)
    }
    pub fn submit(&self, path: &str, fields: Value) -> Call {
        self.client.send(ApiCall {
            method: "POST".into(),
            path: path.into(),
            body: fields,
            headers: vec![
                (
                    "content-type".into(),
                    "application/x-www-form-urlencoded".into(),
                ),
                ("accept-language".into(), self.language.clone()),
                (
                    "cookie".into(),
                    self.cookies
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join("; "),
                ),
            ],
        })
    }
    pub fn remember(&mut self, answer: &Answer) {
        if let Some(cookie) = answer.headers.get("set-cookie")
            && let Some((name, value)) = cookie.split(';').next().and_then(|v| v.split_once('='))
        {
            self.cookies.insert(name.into(), value.into());
        }
    }
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn form_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

struct SocketLease {
    id: u64,
    hub: Hub,
}
impl Drop for SocketLease {
    fn drop(&mut self) {
        self.hub
            .shared
            .0
            .lock()
            .unwrap()
            .client_sockets
            .remove(&self.id);
        self.hub.shared.1.notify_all();
    }
}
struct CountSocket {
    inner: tokio::net::TcpStream,
    id: u64,
    hub: Hub,
}
impl tokio::io::AsyncRead for CountSocket {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        if let std::task::Poll::Ready(Ok(())) = result {
            let n = buf.filled().len() - before;
            if let Some(socket) = self
                .hub
                .shared
                .0
                .lock()
                .unwrap()
                .client_sockets
                .get_mut(&self.id)
            {
                socket.read += n as u64;
            }
            self.hub.shared.1.notify_all();
        }
        result
    }
}
impl tokio::io::AsyncWrite for CountSocket {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_utf8_crlf_multiline_and_comments() {
        let mut reader = SseReader::default();
        let data = "event: chunk\r\nid: 7\r\ndata: é\r\ndata: next\r\n: ignored\r\n\r\n".as_bytes();
        let mut events = Vec::new();
        for b in data {
            events.extend(reader.feed(&[*b], 42).unwrap());
        }
        assert_eq!(
            events,
            vec![SseEvent {
                event: "chunk".into(),
                data: "é\nnext".into(),
                id: Some("7".into()),
                at: 42
            }]
        );
    }
    #[test]
    fn multiple_events_empty_data_and_invalid_utf8() {
        let mut reader = SseReader::default();
        let events = reader.feed(b":ping\n\ndata:\n\ndata: x\n\n", 1).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "");
        assert_eq!(events[1].event, "message");
        assert!(reader.feed(&[0xff, b'\n'], 2).is_err());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn request_cap_stops_before_request_301() {
        let hub = Hub::new(crate::setup::random_hex(24));
        let client = ApiClient::new("127.0.0.1:0".into(), String::new(), hub.clone());
        let mut calls = Vec::new();
        for _ in 0..300 {
            calls.push(client.send(ApiCall::one_user_message("POST /v1/messages", "cap")));
        }
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || client.send(ApiCall::one_user_message("POST /v1/messages", "overflow"))
            ))
            .is_err()
        );
        drop(calls);
        hub.close();
    }
    #[tokio::test]
    async fn http_body_cap_and_sse_reader_cross_real_http() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for oversized in [false, true] {
            let hub = Hub::new(crate::setup::random_hex(24));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(30), async move {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut request = [0; 4096];
                    let _ = stream.read(&mut request).await.unwrap();
                    let data = if oversized {
                        vec![b'x'; BODY_CAP + 1]
                    } else {
                        b"event: chunk
data: first
data: second

data: done

"
                        .to_vec()
                    };
                    let head = format!(
                        "HTTP/1.1 200 OK
Content-Type: text/event-stream
Content-Length: {}
Connection: close

",
                        data.len()
                    );
                    stream.write_all(head.as_bytes()).await.unwrap();
                    let _ = stream.write_all(&data).await;
                })
                .await
                .expect("fake HTTP server run cap: 30 s, request cap: 1, log cap: 0");
            });
            let client = ApiClient::new(address, String::new(), hub.clone());
            let answer = client
                .exchange(
                    ApiCall::one_user_message("POST /v1/messages", "sse"),
                    Arc::new(Mutex::new(Vec::new())),
                )
                .await;
            if oversized {
                assert!(answer.unwrap_err().contains("API answer cap"));
            } else {
                let answer = answer.unwrap();
                assert_eq!(answer.events.len(), 2);
                assert_eq!(
                    answer.events[0].data,
                    "first
second"
                );
                assert_eq!(answer.events[1].at, 0);
            }
            server.await.unwrap();
            hub.close();
        }
    }
}
