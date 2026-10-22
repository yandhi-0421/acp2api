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
        json!({"id":w.id,"object":"response","created_at":w.created,"status":if usage.is_some(){"completed"}else{"in_progress"},"error":null,"incomplete_details":null,"instructions":null,"max_output_tokens":null,"model":w.model,"output":usage.map(|_|self.output(w)).unwrap_or_default(),"parallel_tool_calls":true,"previous_response_id":null,"reasoning":{"effort":null,"summary":null},"store":false,"temperature":1.0,"text":{"format":{"type":"text"}},"tool_choice":"auto","tools":[],"top_p":1.0,"truncation":"disabled","usage":usage.map(|u|json!({"input_tokens":u.input_tokens,"input_tokens_details":{"cached_tokens":0},"output_tokens":u.output_tokens,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":u.input_tokens+u.output_tokens})),"user":null,"metadata":{}})
    }
}
impl WireDialect for Responses {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        content::parse(content::Api::Responses, body)
    }
    fn built_in_tool_ran_events(&self, wire: &mut Encoding, message: String) -> Vec<Event> {
        let mut response = self.body(
            wire,
            &Usage {
                input_tokens: 0,
                output_tokens: 0,
            },
        );
        response["status"] = json!("failed");
        response["error"] = json!({"code":"invalid_prompt","message":message});
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
            "response.completed",
            json!({"response":self.response(w,Some(usage))}),
        ));
        events
    }
}
