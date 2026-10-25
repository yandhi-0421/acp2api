use super::*;
pub(super) struct Chat;
impl Chat {
    fn chunk(&self, w: &Encoding, delta: Value, finish: Value) -> Event {
        data(
            json!({"id":w.id,"object":"chat.completion.chunk","created":w.created,"model":w.model,"choices":[{"index":0,"delta":delta,"logprobs":null,"finish_reason":finish}]}),
        )
    }
}
impl WireDialect for Chat {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        content::parse(content::Api::Chat, body)
    }
    fn prefix(&self) -> &'static str {
        "chatcmpl-"
    }
    fn body(&self, w: &Encoding, u: &Usage) -> Value {
        let content = if w.text.is_empty() && w.tool_call.is_some() {
            Value::Null
        } else {
            json!(w.text)
        };
        let mut message = json!({"role": "assistant", "content": content, "refusal": null});
        if let Some(call) = &w.tool_call {
            message["tool_calls"] = json!([{
                "id": w.call_id,
                "type": "function",
                "function": {"name": call.name, "arguments": call.arguments}
            }]);
        }
        let finish_reason = if w.tool_call.is_some() {
            "tool_calls"
        } else {
            "stop"
        };
        json!({
            "id": w.id,
            "object": "chat.completion",
            "created": w.created,
            "model": w.model,
            "choices": [{
                "index": 0,
                "message": message,
                "logprobs": null,
                "finish_reason": finish_reason
            }],
            "usage": {
                "prompt_tokens": u.input_tokens,
                "completion_tokens": u.output_tokens,
                "total_tokens": u.input_tokens + u.output_tokens
            }
        })
    }
    fn start(&self, _: &mut Encoding) -> Vec<Event> {
        Vec::new()
    }
    fn text(&self, w: &mut Encoding, text: &str) -> Vec<Event> {
        if text.is_empty() {
            return Vec::new();
        }
        let delta = if w.text.is_empty() {
            json!({"role":"assistant","content":text})
        } else {
            json!({"content":text})
        };
        w.text.push_str(text);
        vec![self.chunk(w, delta, Value::Null)]
    }
    fn end(&self, w: &mut Encoding, u: &Usage) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some(call) = &w.tool_call {
            let mut header = json!({"tool_calls":[{"index":0,"id":w.call_id,"type":"function","function":{"name":call.name,"arguments":""}}]});
            if w.text.is_empty() {
                header["role"] = "assistant".into();
            }
            events.push(self.chunk(w, header, Value::Null));
            events.push(self.chunk(
                w,
                json!({"tool_calls":[{"index":0,"function":{"arguments":call.arguments}}]}),
                Value::Null,
            ));
        } else if w.text.is_empty() {
            events.push(self.chunk(w, json!({"role":"assistant","content":""}), Value::Null));
        }
        events.push(
            self.chunk(
                w,
                json!({}),
                if w.tool_call.is_some() {
                    "tool_calls"
                } else {
                    "stop"
                }
                .into(),
            ),
        );
        if w.include_usage {
            events.push(data(json!({
                "id": w.id,
                "object": "chat.completion.chunk",
                "created": w.created,
                "model": w.model,
                "choices": [],
                "usage": {
                    "prompt_tokens": u.input_tokens,
                    "completion_tokens": u.output_tokens,
                    "total_tokens": u.input_tokens + u.output_tokens
                }
            })));
        }
        events.push(Event::default().data("[DONE]"));
        events
    }
}
