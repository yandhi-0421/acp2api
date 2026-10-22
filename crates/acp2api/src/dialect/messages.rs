use super::*;
pub(super) struct Messages;
impl WireDialect for Messages {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        content::parse(content::Api::Messages, body)
    }
    fn refusal_body(&self, refusal: &Refusal) -> Value {
        let kind = match refusal.row() {
            Row::Key => "authentication_error",
            Row::Rejected => "invalid_request_error",
            Row::TooLarge => "request_too_large",
            Row::Other502 => "api_error",
        };
        json!({"type":"error","error":{"type":kind,"message":refusal.message.to_string()}})
    }
    fn prefix(&self) -> &'static str {
        "msg_"
    }
    fn body(&self, w: &Encoding, a: &Answer) -> Value {
        json!({"id":w.id,"type":"message","role":"assistant","model":w.model,"content":[{"type":"text","text":a.text}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":a.input_tokens,"output_tokens":a.output_tokens}})
    }
    fn start(&self, w: &mut Encoding) -> Vec<Event> {
        vec![
            named(
                "message_start",
                json!({"type":"message_start","message":{"id":w.id,"type":"message","role":"assistant","model":w.model,"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}}),
            ),
            named(
                "content_block_start",
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            ),
        ]
    }
    fn text(&self, w: &mut Encoding, text: &str) -> Vec<Event> {
        w.text.push_str(text);
        vec![named(
            "content_block_delta",
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
        )]
    }
    fn end(&self, _: &mut Encoding, a: &Answer) -> Vec<Event> {
        vec![
            named(
                "content_block_stop",
                json!({"type":"content_block_stop","index":0}),
            ),
            named(
                "message_delta",
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"input_tokens":a.input_tokens,"output_tokens":a.output_tokens}}),
            ),
            named("message_stop", json!({"type":"message_stop"})),
        ]
    }
}
