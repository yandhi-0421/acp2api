use super::*;
pub(super) struct Responses;
impl Responses {
    fn part(&self, text: &str) -> Value {
        json!({"type":"output_text","text":text,"annotations":[],"logprobs":[]})
    }
    fn item(&self, w: &Encoding, text: Option<&str>) -> Value {
        json!({"id":w.item_id,"type":"message","status":if text.is_some(){"completed"}else{"in_progress"},"role":"assistant","content":text.map(|t|vec![self.part(t)]).unwrap_or_default()})
    }
    fn call(&self, w: &Encoding, call: &crate::conversation::ToolCall, completed: bool) -> Value {
        let mut item = match call.kind {
            crate::conversation::ToolKind::Function => {
                json!({"id":w.call_item_id,"type":"function_call","call_id":w.call_id,"name":call.name,"arguments":if completed{&call.arguments}else{""},"status":if completed{"completed"}else{"in_progress"}})
            }
            crate::conversation::ToolKind::Custom => {
                json!({"id":w.call_item_id,"type":"custom_tool_call","call_id":w.call_id,"name":call.name,"input":if completed{&call.arguments}else{""}})
            }
        };
        if let Some(namespace) = &call.namespace {
            item["namespace"] = json!(namespace);
        }
        item
    }
    fn output(&self, w: &Encoding) -> Vec<Value> {
        let mut output = Vec::new();
        if !w.text.is_empty() || w.tool_call.is_none() {
            output.push(self.item(w, Some(&w.text)));
        }
        if let Some(call) = &w.tool_call {
            output.push(self.call(w, call, true));
        }
        output
    }
    fn open_text(&self, w: &mut Encoding) -> Vec<Event> {
        vec![w.event("response.output_item.added",json!({"output_index":0,"item":self.item(w,None)})),w.event("response.content_part.added",json!({"output_index":0,"content_index":0,"item_id":w.item_id,"part":self.part("")}))]
    }
    fn response(&self, w: &Encoding, usage: Option<&Usage>) -> Value {
        let status = if usage.is_some() {
            match w.stop {
                Stop::MaxTokens | Stop::Refusal => "incomplete",
                Stop::EndTurn | Stop::Sequence(_) => "completed",
            }
        } else {
            "in_progress"
        };
        let output = usage.map(|_| self.output(w)).unwrap_or_default();
        let usage = usage.map(|u| {
            json!({
                "input_tokens": u.input_tokens,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens": u.output_tokens,
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": u.input_tokens + u.output_tokens
            })
        });
        json!({
            "id": w.id,
            "object": "response",
            "created_at": w.created,
            "status": status,
            "error": null,
            "incomplete_details": match w.stop { Stop::MaxTokens => json!({"reason":"max_output_tokens"}), Stop::Refusal => json!({"reason":"content_filter"}), Stop::EndTurn | Stop::Sequence(_) => Value::Null },
            "instructions": null,
            "max_output_tokens": null,
            "model": w.model,
            "output": output,
            "parallel_tool_calls": true,
            "previous_response_id": null,
            "reasoning": {"effort": null, "summary": null},
            "store": false,
            "temperature": 1.0,
            "text": {"format": {"type": "text"}},
            "tool_choice": match w.tool_choice {
                ToolChoice::Auto => "auto",
                ToolChoice::Off => "none",
            },
            "tools": w.tools,
            "top_p": 1.0,
            "truncation": "disabled",
            "usage": usage,
            "user": null,
            "metadata": {}
        })
    }
}
impl WireDialect for Responses {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        content::parse(content::Api::Responses, body)
    }
    fn error_events(&self, wire: &mut Encoding, refusal: &Refusal) -> Vec<Event> {
        let mut response = self.body(
            wire,
            &Usage {
                input_tokens: 0,
                output_tokens: 0,
            },
        );
        response["status"] = json!("failed");
        let temporary = matches!(refusal.row(), Row::Temporary503);
        response["error"] = json!({"code": if temporary {"server_error"} else {"invalid_prompt"},"message":refusal.message.to_string()});
        if temporary {
            response["error"]["headers"] = json!({"retry-after":"30"});
        }
        response["usage"] = Value::Null;
        vec![wire.event("response.failed", json!({"response":response}))]
    }
    fn prefix(&self) -> &'static str {
        "resp_"
    }
    fn body(&self, w: &Encoding, usage: &Usage) -> Value {
        self.response(w, Some(usage))
    }
    fn start(&self, w: &mut Encoding) -> Vec<Event> {
        vec![
            w.event(
                "response.created",
                json!({"response":self.response(w,None)}),
            ),
            w.event(
                "response.in_progress",
                json!({"response":self.response(w,None)}),
            ),
        ]
    }
    fn text(&self, w: &mut Encoding, text: &str) -> Vec<Event> {
        if text.is_empty() {
            return Vec::new();
        }
        let mut events = if w.text.is_empty() {
            self.open_text(w)
        } else {
            Vec::new()
        };
        w.text.push_str(text);
        events.push(w.event("response.output_text.delta",json!({"item_id":w.item_id,"output_index":0,"content_index":0,"delta":text,"logprobs":[]})));
        events
    }
    fn end(&self, w: &mut Encoding, usage: &Usage) -> Vec<Event> {
        let mut events = Vec::new();
        if w.text.is_empty() && w.tool_call.is_none() {
            events.extend(self.open_text(w));
        }
        if !w.text.is_empty() || w.tool_call.is_none() {
            events.push(w.event("response.output_text.done",json!({"item_id":w.item_id,"output_index":0,"content_index":0,"text":w.text,"logprobs":[]})));
            events.push(w.event("response.content_part.done",json!({"item_id":w.item_id,"output_index":0,"content_index":0,"part":self.part(&w.text)})));
            events.push(w.event(
                "response.output_item.done",
                json!({"output_index":0,"item":self.item(w,Some(&w.text))}),
            ));
        }
        if let Some(call) = w.tool_call.clone() {
            let index = usize::from(!w.text.is_empty());
            events.push(w.event(
                "response.output_item.added",
                json!({"output_index":index,"item":self.call(w,&call,false)}),
            ));
            let (delta, done, field) = match call.kind {
                crate::conversation::ToolKind::Function => (
                    "response.function_call_arguments.delta",
                    "response.function_call_arguments.done",
                    "arguments",
                ),
                crate::conversation::ToolKind::Custom => (
                    "response.custom_tool_call_input.delta",
                    "response.custom_tool_call_input.done",
                    "input",
                ),
            };
            events.push(w.event(
                delta,
                json!({"item_id":w.call_item_id,"output_index":index,"delta":call.arguments}),
            ));
            let mut payload = json!({"item_id":w.call_item_id,"output_index":index});
            payload[field] = json!(call.arguments);
            events.push(w.event(done, payload));
            events.push(w.event(
                "response.output_item.done",
                json!({"output_index":index,"item":self.call(w,&call,true)}),
            ));
        }
        events.push(w.event(
            match w.stop {
                Stop::MaxTokens | Stop::Refusal => "response.incomplete",
                Stop::EndTurn | Stop::Sequence(_) => "response.completed",
            },
            json!({"response":self.response(w,Some(usage))}),
        ));
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_responses_keep_the_null_error_field() {
        let form = WireDialect::parse(&Responses, br#"{"model":"m","input":"Hi"}"#)
            .ok()
            .unwrap()
            .form;
        let mut wire = Encoding::new("resp_", "call_", form, std::time::UNIX_EPOCH).unwrap();
        for stop in [Stop::EndTurn, Stop::MaxTokens] {
            wire.stop = stop;
            let body = Responses.body(
                &wire,
                &Usage {
                    input_tokens: 2,
                    output_tokens: 0,
                },
            );
            assert_eq!(
                body.get("error"),
                Some(&Value::Null),
                "Responses preserves its success schema"
            );
        }
    }
}
