use crate::{
    BoxFuture, Clock, Hold,
    conversation::{Answer, ReplyForm, Ticket, TurnEvent, TurnRequest},
};
use axum::{
    Json,
    response::{IntoResponse, Response, Sse, sse::Event},
};
use futures_util::stream;
use serde_json::{Value, json};
use std::{collections::VecDeque, convert::Infallible, sync::Arc};
mod chat;
mod messages;
mod responses;
pub(crate) static MESSAGES: &dyn Dialect = &messages::Messages;
pub(crate) static RESPONSES: &dyn Dialect = &responses::Responses;
pub(crate) static CHAT: &dyn Dialect = &chat::Chat;

pub(crate) struct Parsed {
    pub turn: TurnRequest,
    pub form: ReplyForm,
}
pub(crate) trait Dialect: Sync {
    fn parse(&self, body: &[u8]) -> Result<Parsed, String>;
    fn prefix(&self) -> &'static str;
    fn body(&self, wire: &Encoding, answer: &Answer) -> Value;
    fn start(&self, wire: &mut Encoding) -> Vec<Event>;
    fn text(&self, wire: &mut Encoding, text: &str) -> Vec<Event>;
    fn end(&self, wire: &mut Encoding, answer: &Answer) -> Vec<Event>;
    fn refusal(&self, status: axum::http::StatusCode, message: &str) -> Response {
        (
            status,
            Json(json!({"error":{"type":if status==axum::http::StatusCode::UNAUTHORIZED {"authentication_error"} else {"api_error"},"message":message}})),
        )
            .into_response()
    }
    fn respond(
        &'static self,
        form: ReplyForm,
        mut ticket: Ticket,
        clock: Arc<dyn Clock>,
    ) -> BoxFuture<'static, Response> {
        Box::pin(async move {
            let mut wire = match Encoding::new(self.prefix(), form.model_name, clock.wall()) {
                Ok(wire) => wire,
                Err(error) => {
                    return self.refusal(axum::http::StatusCode::INTERNAL_SERVER_ERROR, &error);
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
                            TurnEvent::End(Ok(usage)) => {
                                let answer = Answer {
                                    text: state.wire.text.clone(),
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
                        TurnEvent::End(Ok(usage)) => {
                            return Json(self.body(
                                &wire,
                                &Answer {
                                    text,
                                    input_tokens: usage.input_tokens,
                                    output_tokens: usage.output_tokens,
                                },
                            ))
                            .into_response();
                        }
                        TurnEvent::End(Err(error)) => {
                            return self.refusal(axum::http::StatusCode::BAD_GATEWAY, &error);
                        }
                    }
                }
                self.refusal(axum::http::StatusCode::BAD_GATEWAY, "conductor stopped")
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
pub(crate) struct Encoding {
    id: String,
    item_id: String,
    model: String,
    created: u64,
    sequence: u64,
    text: String,
}
impl Encoding {
    fn new(prefix: &str, model: String, wall: std::time::SystemTime) -> Result<Self, String> {
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
fn parsed(model: String, text: String, stream: bool) -> Parsed {
    Parsed {
        turn: TurnRequest {
            model: model.clone(),
            text,
        },
        form: ReplyForm {
            stream,
            model_name: model,
        },
    }
}
