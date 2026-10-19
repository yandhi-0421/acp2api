use super::*;
pub(super) struct Messages;
impl WireDialect for Messages {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        content::parse(content::Api::Messages, body)
    }
    fn refusal_body(&self, refusal: &Refusal) -> Value {
        json!({"type":"error","error":{"type":if refusal_status(refusal)==axum::http::StatusCode::BAD_REQUEST {"invalid_request_error"} else if refusal_status(refusal)==axum::http::StatusCode::UNAUTHORIZED {"authentication_error"} else {"api_error"},"message":refusal.message.to_string()}})
    }
    fn prefix(&self) -> &'static str {
        "msg_"
    }
    fn call_prefix(&self) -> &'static str {
        "toolu_"
    }
    fn body(&self, w: &Encoding, a: &Answer) -> Value {
        let mut content = Vec::new();
        if !a.text.is_empty() || a.tool_call.is_none() {
            content.push(json!({"type":"text","text":a.text}));
        }
        if let Some(call) = &a.tool_call {
            content.push(json!({"type":"tool_use","id":w.call_id,"name":call.name,"input":serde_json::from_str::<Value>(&call.arguments).expect("validated tool arguments")}));
        }
        json!({"id":w.id,"type":"message","role":"assistant","model":w.model,"content":content,"stop_reason":if a.tool_call.is_some(){"tool_use"}else{"end_turn"},"stop_sequence":null,"usage":{"input_tokens":a.input_tokens,"output_tokens":a.output_tokens}})
    }
    fn start(&self, w: &mut Encoding) -> Vec<Event> {
        vec![named(
            "message_start",
            json!({"type":"message_start","message":{"id":w.id,"type":"message","role":"assistant","model":w.model,"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}}),
        )]
    }
    fn text(&self, w: &mut Encoding, text: &str) -> Vec<Event> {
        if text.is_empty() {
            return Vec::new();
        }
        let mut events = Vec::new();
        if w.text.is_empty() {
            events.push(named("content_block_start",json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})));
        }
        w.text.push_str(text);
        events.push(named("content_block_delta",json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}})));
        events
    }
    fn end(&self, w: &mut Encoding, a: &Answer) -> Vec<Event> {
        let mut events = Vec::new();
        if a.text.is_empty() && a.tool_call.is_none() {
            events.push(named("content_block_start",json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})));
        }
        if !a.text.is_empty() || a.tool_call.is_none() {
            events.push(named(
                "content_block_stop",
                json!({"type":"content_block_stop","index":0}),
            ));
        }
        if let Some(call) = &a.tool_call {
            let index = usize::from(!a.text.is_empty());
            events.push(named("content_block_start",json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":w.call_id,"name":call.name,"input":{}}})));
            events.push(named("content_block_delta",json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":call.arguments}})));
            events.push(named(
                "content_block_stop",
                json!({"type":"content_block_stop","index":index}),
            ));
        }
        events.push(named("message_delta",json!({"type":"message_delta","delta":{"stop_reason":if a.tool_call.is_some(){"tool_use"}else{"end_turn"},"stop_sequence":null},"usage":{"input_tokens":a.input_tokens,"output_tokens":a.output_tokens}})));
        events.push(named("message_stop", json!({"type":"message_stop"})));
        events
    }
}
