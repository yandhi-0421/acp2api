use super::*;
use serde::Deserialize;
pub(super) struct Responses;
#[derive(Deserialize)]
struct Request {
    model: String,
    input: Input,
    #[serde(default)]
    stream: bool,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Input {
    Text(String),
    Messages(Vec<Message>),
}
#[derive(Deserialize)]
struct Message {
    role: String,
    content: Content,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Parts(Vec<TextPart>),
}
#[derive(Deserialize)]
struct TextPart {
    r#type: String,
    text: String,
}
impl Responses {
    fn part(&self, text: &str) -> Value {
        json!({"type":"output_text","text":text,"annotations":[],"logprobs":[]})
    }
    fn item(&self, w: &Encoding, text: Option<&str>) -> Value {
        json!({"id":w.item_id,"type":"message","status":if text.is_some(){"completed"}else{"in_progress"},"role":"assistant","content":text.map(|t|vec![self.part(t)]).unwrap_or_default()})
    }
    fn response(&self, w: &Encoding, answer: Option<&Answer>) -> Value {
        json!({"id":w.id,"object":"response","created_at":w.created,"status":if answer.is_some(){"completed"}else{"in_progress"},"error":null,"incomplete_details":null,"instructions":null,"max_output_tokens":null,"model":w.model,"output":answer.map(|a|vec![self.item(w,Some(&a.text))]).unwrap_or_default(),"parallel_tool_calls":true,"previous_response_id":null,"reasoning":{"effort":null,"summary":null},"store":false,"temperature":1.0,"text":{"format":{"type":"text"}},"tool_choice":"auto","tools":[],"top_p":1.0,"truncation":"disabled","usage":answer.map(|a|json!({"input_tokens":a.input_tokens,"input_tokens_details":{"cached_tokens":0},"output_tokens":a.output_tokens,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":a.input_tokens+a.output_tokens})),"user":null,"metadata":{}})
    }
}
impl Dialect for Responses {
    fn parse(&self, body: &[u8]) -> Result<Parsed, String> {
        let request: Request = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let text = match request.input {
            Input::Text(text) => text,
            Input::Messages(messages) => {
                let [message]: [Message; 1] = messages
                    .try_into()
                    .map_err(|_| "one user message required")?;
                if message.role != "user" {
                    return Err("one user message required".into());
                }
                match message.content {
                    Content::Text(text) => text,
                    Content::Parts(parts) => {
                        let mut text = String::new();
                        for part in parts {
                            if part.r#type != "input_text" {
                                return Err("input_text content required".into());
                            }
                            text.push_str(&part.text);
                        }
                        text
                    }
                }
            }
        };
        Ok(parsed(request.model, text, request.stream))
    }
    fn prefix(&self) -> &'static str {
        "resp_"
    }
    fn body(&self, w: &Encoding, a: &Answer) -> Value {
        self.response(w, Some(a))
    }
    fn start(&self, w: &mut Encoding) -> Vec<Event> {
        vec![w.event("response.created",json!({"response":self.response(w,None)})),w.event("response.in_progress",json!({"response":self.response(w,None)})),w.event("response.output_item.added",json!({"output_index":0,"item":self.item(w,None)})),w.event("response.content_part.added",json!({"output_index":0,"content_index":0,"item_id":w.item_id,"part":self.part("")}))]
    }
    fn text(&self, w: &mut Encoding, text: &str) -> Vec<Event> {
        w.text.push_str(text);
        vec![w.event("response.output_text.delta",json!({"item_id":w.item_id,"output_index":0,"content_index":0,"delta":text,"logprobs":[]}))]
    }
    fn end(&self, w: &mut Encoding, a: &Answer) -> Vec<Event> {
        vec![w.event("response.output_text.done",json!({"item_id":w.item_id,"output_index":0,"content_index":0,"text":a.text,"logprobs":[]})),w.event("response.content_part.done",json!({"item_id":w.item_id,"output_index":0,"content_index":0,"part":self.part(&a.text)})),w.event("response.output_item.done",json!({"output_index":0,"item":self.item(w,Some(&a.text))})),w.event("response.completed",json!({"response":self.response(w,Some(a))}))]
    }
}
