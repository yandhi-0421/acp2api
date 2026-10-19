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
    fn output(&self, w: &Encoding, a: &Answer) -> Vec<Value> {
        let mut output = Vec::new();
        if !a.text.is_empty() || a.tool_call.is_none() {
            output.push(self.item(w, Some(&a.text)));
        }
        if let Some(call) = &a.tool_call {
            output.push(self.call(w, call, true));
        }
        output
    }
    fn open_text(&self, w: &mut Encoding) -> Vec<Event> {
        vec![w.event("response.output_item.added",json!({"output_index":0,"item":self.item(w,None)})),w.event("response.content_part.added",json!({"output_index":0,"content_index":0,"item_id":w.item_id,"part":self.part("")}))]
    }
    fn response(&self, w: &Encoding, answer: Option<&Answer>) -> Value {
        json!({"id":w.id,"object":"response","created_at":w.created,"status":if answer.is_some(){"completed"}else{"in_progress"},"error":null,"incomplete_details":null,"instructions":null,"max_output_tokens":null,"model":w.model,"output":answer.map(|a|self.output(w,a)).unwrap_or_default(),"parallel_tool_calls":true,"previous_response_id":null,"reasoning":{"effort":null,"summary":null},"store":false,"temperature":1.0,"text":{"format":{"type":"text"}},"tool_choice":"auto","tools":[],"top_p":1.0,"truncation":"disabled","usage":answer.map(|a|json!({"input_tokens":a.input_tokens,"input_tokens_details":{"cached_tokens":0},"output_tokens":a.output_tokens,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":a.input_tokens+a.output_tokens})),"user":null,"metadata":{}})
    }
}
impl WireDialect for Responses {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        content::parse(content::Api::Responses, body)
    }
    fn prefix(&self) -> &'static str {
        "resp_"
    }
    fn body(&self, w: &Encoding, a: &Answer) -> Value {
        self.response(w, Some(a))
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
    fn end(&self, w: &mut Encoding, a: &Answer) -> Vec<Event> {
        let mut events = Vec::new();
        if a.text.is_empty() && a.tool_call.is_none() {
            events.extend(self.open_text(w));
        }
        if !a.text.is_empty() || a.tool_call.is_none() {
            events.push(w.event("response.output_text.done",json!({"item_id":w.item_id,"output_index":0,"content_index":0,"text":a.text,"logprobs":[]})));
            events.push(w.event("response.content_part.done",json!({"item_id":w.item_id,"output_index":0,"content_index":0,"part":self.part(&a.text)})));
            events.push(w.event(
                "response.output_item.done",
                json!({"output_index":0,"item":self.item(w,Some(&a.text))}),
            ));
        }
        if let Some(call) = &a.tool_call {
            let index = usize::from(!a.text.is_empty());
            events.push(w.event(
                "response.output_item.added",
                json!({"output_index":index,"item":self.call(w,call,false)}),
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
                json!({"output_index":index,"item":self.call(w,call,true)}),
            ));
        }
        events.push(w.event(
            "response.completed",
            json!({"response":self.response(w,Some(a))}),
        ));
        events
    }
}
