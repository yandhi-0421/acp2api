use crate::{
    BoxFuture, Clock, Hold,
    conversation::{Answer, ReplyForm, Ticket, TurnEvent, TurnRequest},
    messages::{Message, Refusal, Row},
};
use axum::{
    Json,
    response::{IntoResponse, Response, Sse, sse::Event},
};
use futures_util::stream;
use serde_json::{Value, json};
use std::{collections::VecDeque, convert::Infallible, sync::Arc};
mod chat;
mod content;
mod messages;
mod responses;
pub(crate) static MESSAGES: &dyn Dialect = &messages::Messages;
pub(crate) static RESPONSES: &dyn Dialect = &responses::Responses;
pub(crate) static CHAT: &dyn Dialect = &chat::Chat;

pub(crate) struct Parsed {
    pub turn: TurnRequest,
    pub form: ReplyForm,
    pub unknown_fields: Vec<String>,
}
pub(crate) trait Dialect: Sync {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal>;
    fn refusal(&self, refusal: &Refusal) -> Response;
    fn respond(
        &'static self,
        form: ReplyForm,
        ticket: Ticket,
        clock: Arc<dyn Clock>,
    ) -> BoxFuture<'static, Response>;
}
trait WireDialect: Sync {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal>;
    fn refusal_body(&self, refusal: &Refusal) -> Value {
        let code = refusal_code(refusal);
        json!({"error":{"message":refusal.message.to_string(),"type":if refusal_status(refusal).is_server_error(){"server_error"}else{"invalid_request_error"},"param":refusal.param(),"code":code}})
    }
    fn prefix(&self) -> &'static str;
    fn call_prefix(&self) -> &'static str {
        "call_"
    }
    fn body(&self, wire: &Encoding, answer: &Answer) -> Value;
    fn start(&self, wire: &mut Encoding) -> Vec<Event>;
    fn text(&self, wire: &mut Encoding, text: &str) -> Vec<Event>;
    fn end(&self, wire: &mut Encoding, answer: &Answer) -> Vec<Event>;
}
impl<T: WireDialect + 'static> Dialect for T {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        WireDialect::parse(self, body)
    }
    fn refusal(&self, refusal: &Refusal) -> Response {
        (refusal_status(refusal), Json(self.refusal_body(refusal))).into_response()
    }
    fn respond(
        &'static self,
        form: ReplyForm,
        mut ticket: Ticket,
        clock: Arc<dyn Clock>,
    ) -> BoxFuture<'static, Response> {
        Box::pin(async move {
            let mut wire = match Encoding::new(
                self.prefix(),
                self.call_prefix(),
                form.model_name,
                clock.wall(),
            ) {
                Ok(wire) => wire,
                Err(error) => {
                    return self.refusal(&Message::Upstream(error).into());
                }
            };
            if form.stream {
                let initial = self.start(&mut wire).into();
                let state = StreamState {
                    ticket,
                    wire,
                    pending: initial,
                    hold: Some(clock.hold(crate::HoldKind::TurnEvent)),
                    ended: false,
                };
                let events = stream::unfold(state, move |mut state| async move {
                    loop {
                        if let Some(event) = state.pending.pop_front() {
                            return Some((Ok::<_, Infallible>(event), state));
                        }
                        state.hold.take();
                        if state.ended {
                            return None;
                        }
                        let (event, hold) = state.ticket.recv().await?;
                        state.hold = Some(hold);
                        match event {
                            TurnEvent::Text(text) => {
                                state.pending.extend(self.text(&mut state.wire, &text))
                            }
                            TurnEvent::ToolCall(call) => state.wire.tool_call = Some(call),
                            TurnEvent::End(Ok(usage)) => {
                                let answer = Answer {
                                    text: state.wire.text.clone(),
                                    tool_call: state.wire.tool_call.clone(),
                                    input_tokens: usage.input_tokens,
                                    output_tokens: usage.output_tokens,
                                };
                                state.pending.extend(self.end(&mut state.wire, &answer));
                                state.ended = true;
                            }
                            TurnEvent::End(Err(error)) => {
                                state.pending.push_back(named("error",json!({"type":"error","error":{"type":"api_error","message":error}})));
                                state.ended = true;
                            }
                        }
                    }
                });
                Sse::new(events).into_response()
            } else {
                let mut text = String::new();
                while let Some((event, _hold)) = ticket.recv().await {
                    match event {
                        TurnEvent::Text(chunk) => text.push_str(&chunk),
                        TurnEvent::ToolCall(call) => wire.tool_call = Some(call),
                        TurnEvent::End(Ok(usage)) => {
                            return Json(self.body(
                                &wire,
                                &Answer {
                                    text,
                                    tool_call: wire.tool_call.clone(),
                                    input_tokens: usage.input_tokens,
                                    output_tokens: usage.output_tokens,
                                },
                            ))
                            .into_response();
                        }
                        TurnEvent::End(Err(error)) => {
                            return self.refusal(&Message::Upstream(error).into());
                        }
                    }
                }
                self.refusal(&Message::Upstream("conductor stopped".into()).into())
            }
        })
    }
}
struct StreamState {
    ticket: Ticket,
    wire: Encoding,
    pending: VecDeque<Event>,
    hold: Option<Hold>,
    ended: bool,
}
struct Encoding {
    id: String,
    item_id: String,
    model: String,
    created: u64,
    sequence: u64,
    text: String,
    tool_call: Option<crate::conversation::ToolCall>,
    call_id: String,
    call_item_id: String,
}
impl Encoding {
    fn new(
        prefix: &str,
        call_prefix: &str,
        model: String,
        wall: std::time::SystemTime,
    ) -> Result<Self, String> {
        Ok(Self {
            id: identifier(prefix)?,
            item_id: identifier("msg_")?,
            model,
            created: wall
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
            sequence: 0,
            text: String::new(),
            tool_call: None,
            call_id: identifier(call_prefix)?,
            call_item_id: identifier("fc_")?,
        })
    }
    fn event(&mut self, kind: &str, mut value: Value) -> Event {
        value["type"] = kind.into();
        value["sequence_number"] = self.sequence.into();
        self.sequence += 1;
        named(kind, value)
    }
}
fn identifier(prefix: &str) -> Result<String, String> {
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    Ok(format!(
        "{prefix}{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}
fn named(name: &str, value: Value) -> Event {
    Event::default().event(name).data(value.to_string())
}
fn data(value: Value) -> Event {
    Event::default().data(value.to_string())
}
fn parsed(model: String, conversation: crate::conversation::Conversation, stream: bool) -> Parsed {
    Parsed {
        unknown_fields: Vec::new(),
        turn: TurnRequest {
            model: model.clone(),
            conversation,
            tools: Vec::new(),
            tool_choice: crate::conversation::ToolChoice::Auto,
        },
        form: ReplyForm {
            stream,
            model_name: model,
        },
    }
}

fn refusal_status(refusal: &Refusal) -> axum::http::StatusCode {
    use axum::http::StatusCode;
    match refusal.row() {
        Row::Key => StatusCode::UNAUTHORIZED,
        Row::Rejected => StatusCode::BAD_REQUEST,
        Row::Other502 => StatusCode::BAD_GATEWAY,
    }
}

pub(crate) fn refusal_code(refusal: &Refusal) -> &'static str {
    match refusal.message {
        Message::Model { .. } => "model_not_found",
        Message::Field(_) => "unsupported_parameter",
        Message::Content(_) => "unsupported_content_type",
        Message::Media(_) => "unsupported_media_type",
        Message::Url(_) => "unsupported_url",
        Message::Pdf(_) => "unsupported_pdf",
        Message::Prefill => "unsupported_prefill",
        Message::ToolSearch => "unsupported_tool",
        Message::ToolName(_) => "unsupported_tool_name",
        Message::AdvisorTool(_) => "unsupported_advisor_tool",
        Message::ToolType { .. } => "unsupported_tool_type",
        Message::ServerTool { .. } | Message::CodexWebSearch => "unsupported_server_tool",
        Message::Invalid(_) => "invalid_request_error",
        Message::Key => "invalid_api_key",
        Message::Upstream(_) => "upstream_error",
    }
}
