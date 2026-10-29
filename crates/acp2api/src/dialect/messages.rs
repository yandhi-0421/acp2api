use super::*;
pub(super) struct Messages;
impl Messages {
    fn error_kinds(&self, refusal: &Refusal) -> (&'static str, &'static str) {
        match refusal.row() {
            Row::Key => ("authentication_error", "authentication_error"),
            Row::Rejected => ("invalid_request_error", "invalid_request_error"),
            Row::TooLarge => ("request_too_large", "request_too_large"),
            Row::Other502 => ("api_error", "invalid_request_error"),
            Row::Temporary503(_) => ("api_error", "api_error"),
            Row::QueueFull => ("overloaded_error", "api_error"),
            Row::Quota(_) => ("rate_limit_error", "rate_limit_error"),
        }
    }
}
impl WireDialect for Messages {
    fn refusal_status(&self, refusal: &Refusal) -> axum::http::StatusCode {
        match refusal.row() {
            Row::QueueFull => axum::http::StatusCode::from_u16(529).unwrap(),
            Row::Key
            | Row::Rejected
            | Row::TooLarge
            | Row::Other502
            | Row::Temporary503(_)
            | Row::Quota(_) => refusal_status(refusal),
        }
    }
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        content::parse(content::Api::Messages, body)
    }
    fn refusal_body(&self, refusal: &Refusal) -> Value {
        let (kind, _) = self.error_kinds(refusal);
        json!({"type":"error","error":{"type":kind,"message":refusal.message.to_string()}})
    }
    fn error_events(&self, wire: &mut Encoding, refusal: &Refusal) -> Vec<Event> {
        let mut events = Vec::new();
        if !wire.text.is_empty() {
            events.push(named(
                "content_block_stop",
                json!({"type":"content_block_stop","index":0}),
            ));
        }
        let (_, kind) = self.error_kinds(refusal);
        events.push(named(
            "error",
            json!({"type":"error","error":{"type":kind,"message":refusal.message.to_string()}}),
        ));
        events
    }
    fn prefix(&self) -> &'static str {
        "msg_"
    }
    fn call_prefix(&self) -> &'static str {
        "toolu_"
    }
    fn body(&self, w: &Encoding, u: &Usage) -> Value {
        let mut content = Vec::new();
        if !w.text.is_empty() || w.tool_call.is_none() {
            content.push(json!({"type":"text","text":w.text}));
        }
        if let Some(call) = &w.tool_call {
            content.push(json!({"type":"tool_use","id":w.call_id,"name":call.name,"input":serde_json::from_str::<Value>(&call.arguments).expect("validated tool arguments")}));
        }
        json!({
            "id": w.id,
            "type": "message",
            "role": "assistant",
            "model": w.model,
            "content": content,
            "stop_reason": stop_reason(w),
            "stop_sequence": w.stop.sequence(),
            "usage": {"input_tokens": u.input_tokens, "output_tokens": u.output_tokens}
        })
    }
    fn start(&self, w: &mut Encoding) -> Vec<Event> {
        vec![named(
            "message_start",
            json!({"type":"message_start","message":{"id":w.id,"type":"message","role":"assistant","model":w.model,"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":w.input_tokens,"output_tokens":0}}}),
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
    fn end(&self, w: &mut Encoding, u: &Usage) -> Vec<Event> {
        let mut events = Vec::new();
        if w.text.is_empty() && w.tool_call.is_none() {
            events.push(named("content_block_start",json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})));
        }
        if !w.text.is_empty() || w.tool_call.is_none() {
            events.push(named(
                "content_block_stop",
                json!({"type":"content_block_stop","index":0}),
            ));
        }
        if let Some(call) = &w.tool_call {
            let index = usize::from(!w.text.is_empty());
            events.push(named("content_block_start",json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":w.call_id,"name":call.name,"input":{}}})));
            events.push(named("content_block_delta",json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":call.arguments}})));
            events.push(named(
                "content_block_stop",
                json!({"type":"content_block_stop","index":index}),
            ));
        }
        events.push(named(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop_reason(w), "stop_sequence": w.stop.sequence()},
                "usage": {"input_tokens": u.input_tokens, "output_tokens": u.output_tokens}
            }),
        ));
        events.push(named("message_stop", json!({"type":"message_stop"})));
        events
    }
}
fn stop_reason(w: &Encoding) -> &'static str {
    if w.tool_call.is_some() {
        "tool_use"
    } else {
        match w.stop {
            Stop::EndTurn => "end_turn",
            Stop::MaxTokens => "max_tokens",
            Stop::Refusal => "refusal",
            Stop::Sequence(_) => "stop_sequence",
        }
    }
}
