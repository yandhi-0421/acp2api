//! Field-policy checks cross real HTTP and the guarded fake ACP process.
use crate::{
    Scenario, Setup,
    client::{Answer, ApiCall},
    content_check, system_check, tool_check,
};
use serde_json::{Value, json};
use std::time::Duration;

pub const JSON_MESSAGE: &str =
    "acp2api: the request body is not valid JSON, or it has more than 128 levels.";
pub const PROMPT: &str = "Field policy prompt. 雪";
pub const SYSTEM: &str = "Field policy system. 雪";

pub fn request(endpoint: &str, document: bool) -> ApiCall {
    let mut call = tool_check::request(endpoint, &[tool_check::tool("read_file")], false, "auto");
    call.body.as_object_mut().unwrap().remove("max_tokens");
    let mut parts = vec![content_check::text(endpoint, PROMPT)];
    if document {
        parts.push(json!({"type":"document","title":"Notes","source":{"type":"text","media_type":"text/plain","data":"Field policy document."}}));
    }
    match endpoint {
        "POST /v1/messages" => {
            call.body["system"] = json!(SYSTEM);
            call.body["messages"] = json!([{"role":"user","content":parts}]);
        }
        "POST /v1/responses" => {
            call.body["instructions"] = json!(SYSTEM);
            call.body["input"] = json!([{"role":"user","content":parts}]);
        }
        _ => {
            call.body["messages"] =
                json!([{"role":"system","content":SYSTEM},{"role":"user","content":parts}])
        }
    }
    call
}
pub fn set(call: &mut ApiCall, field: &str, value: Value) {
    let target = match field {
        "cache_control of a text block" => &mut call.body["messages"][0]["content"][0],
        "citations of a document" => &mut call.body["messages"][0]["content"][1],
        "strict of a tool" if call.path == "/v1/chat/completions" => {
            &mut call.body["tools"][0]["function"]
        }
        "strict of a tool" | "defer_loading of a tool" | "eager_input_streaming of a tool" => {
            &mut call.body["tools"][0]
        }
        _ => &mut call.body,
    };
    let field = field.split(" of ").next().unwrap();
    let mut target = target;
    let mut path = field.split('.').peekable();
    while let Some(part) = path.next() {
        if path.peek().is_none() {
            target[part] = value;
            return;
        }
        if target[part].is_null() {
            target[part] = json!({});
        }
        target = &mut target[part];
    }
}
pub fn content(s: &Scenario) -> Value {
    let calls = system_check::requests(s);
    let prompts: Vec<_> = calls
        .iter()
        .filter(|v| v["method"] == "session/prompt")
        .map(|v| v["params"]["prompt"].clone())
        .collect();
    let systems: Vec<_> = calls
        .iter()
        .filter(|v| v["method"] == "session/new")
        .filter_map(|v| {
            v["params"]["_meta"]["antigravity"]["systemPrompt"]["append"]
                .as_str()
                .map(Value::from)
        })
        .collect();
    let config: Vec<_> = calls
        .iter()
        .filter(|v| v["method"] == "session/set_config_option")
        .map(|v| {
            let mut p = v["params"].clone();
            p.as_object_mut().unwrap().remove("sessionId");
            p
        })
        .collect();
    let tools: Vec<_> = s
        .fake()
        .record()
        .offered_tools()
        .iter()
        .map(|v| v["result"]["tools"].clone())
        .collect();
    json!({"prompt":prompts,"system":systems,"tools":tools,"config":config,"session_meta":s.fake().record().session_meta()})
}
pub fn literal_control(actual: &Value, document: bool) {
    let mut parts = vec![json!({"type":"text","text":PROMPT})];
    if document {
        parts.push(json!({"type":"text","text":"[document: Notes]\nField policy document.\n[end of document]"}));
    }
    assert_eq!(
        actual["prompt"],
        json!([parts]),
        "literal nonempty ACP prompt control"
    );
    assert_eq!(
        actual["system"],
        json!([SYSTEM]),
        "literal nonempty system addition control"
    );
    assert_eq!(
        actual["tools"],
        json!([[tool_check::tool("read_file")]]),
        "literal nonempty actual MCP tool control"
    );
    assert!(
        actual["config"].is_array(),
        "actual ACP config observations available"
    );
}
pub async fn control(endpoint: &str, document: bool) -> Value {
    let mut s = Scenario::default();
    s.plan(Setup::normal());
    content_check::send(&mut s, request(endpoint, document)).await;
    assert_eq!(
        s.answer.as_ref().unwrap().status,
        200,
        "control HTTP request accepted"
    );
    let result = content(&s);
    literal_control(&result, document);
    s.finish(true).await;
    result
}
pub fn refusal(s: &Scenario, endpoint: &str, message: &str, path: Option<&str>, code: &str) {
    content_check::refusal(s, endpoint, message, code, path);
}
pub fn expected(field: &str, value: &Value) -> (String, String) {
    let (path, show) = match field {
        "web_search_options" | "previous_response_id" | "conversation" | "output_config.format" => {
            (field.to_owned(), false)
        }
        "response_format" | "text.format" | "tool_choice" if value.is_object() => {
            (format!("{field}.type"), true)
        }
        "citations of a document" => ("messages[0].content[1].citations.enabled".into(), true),
        _ => (field.to_owned(), true),
    };
    let value = if field == "citations of a document" {
        &value["enabled"]
    } else if value.is_object() && path.ends_with(".type") {
        &value["type"]
    } else {
        value
    };
    let label = if show {
        format!("{path} = {value}")
    } else {
        path.clone()
    };
    (
        path,
        format!("acp2api: the request field {label} is not supported."),
    )
}
/// Root object counts as level 1. Alternating arrays and objects add one level each.
pub fn deep_body(endpoint: &str, depth: usize) -> Vec<u8> {
    assert!((8..=256).contains(&depth), "raw depth input cap: 8..256");
    let mut raw = serde_json::to_string(&request(endpoint, false).body).unwrap();
    raw.pop();
    raw.push_str(",\"depth_probe\":");
    for n in 1..depth {
        raw.push_str(if n % 2 == 0 { "{\"v\":" } else { "[" });
    }
    raw.push('0');
    for n in (1..depth).rev() {
        raw.push(if n % 2 == 0 { '}' } else { ']' });
    }
    raw.push('}');
    raw.into_bytes()
}
pub async fn raw(s: &mut Scenario, endpoint: &str, bytes: Vec<u8>) -> Answer {
    use bytes::Bytes;
    use http_body_util::{BodyExt, Full};
    use hyper::{Request, client::conn::http1};
    use hyper_util::rt::TokioIo;
    static REQUESTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    assert!(
        REQUESTS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 300,
        "raw HTTP request cap: 300 per process"
    );
    assert!(bytes.len() <= 64 * 1024, "field raw request cap: 64 KiB");
    s.start().await;
    let address = format!("127.0.0.1:{}", s.root.port);
    let key = s.root.key.clone();
    let path = endpoint.strip_prefix("POST ").unwrap();
    s.hub.shared.0.lock().unwrap().counts.http_sent += 1;
    s.hub.shared.1.notify_all();
    let answer = tokio::time::timeout(Duration::from_secs(10), async {
        let socket = tokio::net::TcpStream::connect(&address).await.unwrap();
        let (mut sender, connection) = http1::handshake(TokioIo::new(socket)).await.unwrap();
        let connection = tokio::spawn(async move {
            connection.await.unwrap();
        });
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("host", address)
            .header("x-api-key", key)
            .header("content-type", "application/json")
            .header("connection", "close")
            .body(Full::new(Bytes::from(bytes)))
            .unwrap();
        let response = sender.send_request(request).await.unwrap();
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().into()))
            .collect();
        let mut incoming = response.into_body();
        let mut body = Vec::new();
        while let Some(frame) = incoming.frame().await {
            if let Ok(data) = frame.unwrap().into_data() {
                assert!(
                    body.len() + data.len() <= 64 * 1024,
                    "raw answer cap: 64 KiB"
                );
                body.extend_from_slice(&data);
            }
        }
        drop(sender);
        connection.await.unwrap();
        Answer {
            status,
            headers,
            body,
            events: Vec::new(),
        }
    })
    .await
    .expect("field raw HTTP cap: 10 s");
    s.hub.shared.0.lock().unwrap().counts.http_finished += 1;
    s.hub.shared.1.notify_all();
    answer
}
pub fn readme_has(readme: &str, field: &str) -> bool {
    let mut section_ignores = false;
    readme.lines().any(|line| {
        let line = line.to_ascii_lowercase().replace('`', "");
        if line.starts_with('#') {
            section_ignores = line.contains("ignor");
        }
        if !(section_ignores || line.contains("ignor")) {
            return false;
        }
        let has = |name: &str| {
            line.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                .any(|word| word == name)
        };
        match field {
            "store with each value" => {
                has("store")
                    && ["each value", "any value", "all values"]
                        .iter()
                        .any(|v| line.contains(v))
            }
            "response_format with the type json_object" => {
                has("response_format") && has("json_object")
            }
            "include: [\"reasoning.encrypted_content\"]" => {
                has("include") && has("reasoning.encrypted_content")
            }
            _ => has(field),
        }
    })
}
