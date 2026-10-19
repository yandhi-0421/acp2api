use super::*;
use serde::Deserialize;
pub(super) struct Chat;
#[derive(Deserialize)]
struct Request {
    model: String,
    messages: Vec<Message>,
    #[serde(default)]
    stream: bool,
}
#[derive(Deserialize)]
struct Message {
    role: String,
    content: String,
}
impl Chat {
    fn chunk(&self, w: &Encoding, delta: Value, finish: Value) -> Event {
        data(
            json!({"id":w.id,"object":"chat.completion.chunk","created":w.created,"model":w.model,"choices":[{"index":0,"delta":delta,"logprobs":null,"finish_reason":finish}]}),
        )
    }
}
impl WireDialect for Chat {
    fn parse(&self, body: &[u8]) -> Result<Parsed, String> {
        let request: Request = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let [message]: [Message; 1] = request
            .messages
            .try_into()
            .map_err(|_| "one user message required")?;
        if message.role != "user" {
            return Err("one user message required".into());
        }
        Ok(parsed(request.model, message.content, request.stream))
    }
    fn prefix(&self) -> &'static str {
        "chatcmpl-"
    }
    fn body(&self, w: &Encoding, a: &Answer) -> Value {
        json!({"id":w.id,"object":"chat.completion","created":w.created,"model":w.model,"choices":[{"index":0,"message":{"role":"assistant","content":a.text,"refusal":null},"logprobs":null,"finish_reason":"stop"}],"usage":{"prompt_tokens":a.input_tokens,"completion_tokens":a.output_tokens,"total_tokens":a.input_tokens+a.output_tokens}})
    }
    fn start(&self, _: &mut Encoding) -> Vec<Event> {
        Vec::new()
    }
    fn text(&self, w: &mut Encoding, text: &str) -> Vec<Event> {
        let delta = if w.text.is_empty() {
            json!({"role":"assistant","content":text})
        } else {
            json!({"content":text})
        };
        w.text.push_str(text);
        vec![self.chunk(w, delta, Value::Null)]
    }
    fn end(&self, w: &mut Encoding, _: &Answer) -> Vec<Event> {
        vec![
            self.chunk(w, json!({}), "stop".into()),
            Event::default().data("[DONE]"),
        ]
    }
}
