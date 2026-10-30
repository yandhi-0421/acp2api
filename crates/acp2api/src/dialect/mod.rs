use crate::{
    BoxFuture, Clock, Hold,
    conversation::{ReplyForm, Stop, Ticket, ToolChoice, TurnEvent, TurnRequest, Usage},
    messages::{Message, Refusal, Row},
};
use axum::{
    Json,
    body::{Body, Bytes},
    response::{IntoResponse, Response, Sse, sse::Event},
};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    convert::Infallible,
    sync::{Arc, Mutex},
};
mod chat;
mod content;
mod fields;
mod messages;
mod responses;
pub(crate) use fields::request_model;
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
    fn refusal_status(&self, refusal: &Refusal) -> axum::http::StatusCode {
        refusal_status(refusal)
    }
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal>;
    fn refusal_body(&self, refusal: &Refusal) -> Value {
        let kind = if matches!(refusal.row(), Row::Quota(_)) {
            "rate_limit_error"
        } else if refusal_status(refusal).is_server_error() {
            "server_error"
        } else {
            "invalid_request_error"
        };
        json!({"error": {
            "message": refusal.message.to_string(),
            "type": kind,
            "param": refusal.param(),
            "code": refusal_code(refusal)
        }})
    }
    fn error_events(&self, _wire: &mut Encoding, refusal: &Refusal) -> Vec<Event> {
        vec![data(self.refusal_body(refusal))]
    }
    fn prefix(&self) -> &'static str;
    fn call_prefix(&self) -> &'static str {
        "call_"
    }
    fn body(&self, wire: &Encoding, usage: &Usage) -> Value;
    fn start(&self, wire: &mut Encoding) -> Vec<Event>;
    fn keep_alive(&self, wire: &mut Encoding) -> Event;
    fn text(&self, wire: &mut Encoding, text: &str) -> Vec<Event>;
    fn end(&self, wire: &mut Encoding, usage: &Usage) -> Vec<Event>;
}
impl<T: WireDialect + 'static> Dialect for T {
    fn parse(&self, body: &[u8]) -> Result<Parsed, Refusal> {
        WireDialect::parse(self, body)
    }
    fn refusal(&self, refusal: &Refusal) -> Response {
        let mut response = (
            self.refusal_status(refusal),
            Json(self.refusal_body(refusal)),
        )
            .into_response();
        match refusal.row() {
            Row::Quota(retry_after) | Row::Temporary503(retry_after) => {
                response
                    .headers_mut()
                    .insert("retry-after", retry_after.to_string().parse().unwrap());
            }
            Row::QueueFull => {
                response
                    .headers_mut()
                    .insert("retry-after", axum::http::HeaderValue::from_static("10"));
            }
            Row::Key | Row::Rejected | Row::TooLarge | Row::Other502 => {}
        }
        if matches!(refusal.row(), Row::Other502)
            || matches!(
                refusal.message,
                Message::Lockout(_)
                    | Message::RuntimeUnavailable(_)
                    | Message::SignInRequired(_)
                    | Message::Stopped
            )
        {
            response.headers_mut().insert(
                "x-should-retry",
                axum::http::HeaderValue::from_static("false"),
            );
        }
        response
    }
    fn respond(
        &'static self,
        form: ReplyForm,
        ticket: Ticket,
        clock: Arc<dyn Clock>,
    ) -> BoxFuture<'static, Response> {
        Box::pin(async move {
            let Ticket { mut events, hold } = ticket;
            let stream = form.stream;
            let mut wire =
                match Encoding::new(self.prefix(), self.call_prefix(), form, clock.wall()) {
                    Ok(wire) => wire,
                    Err(error) => {
                        return self.refusal(&Message::Upstream(error).into());
                    }
                };
            if stream {
                let initial = self.start(&mut wire).into();
                let output_hold = Arc::new(Mutex::new(None));
                let state = StreamState {
                    events,
                    wire,
                    pending: initial,
                    hold: Some(hold),
                    output_hold: output_hold.clone(),
                    ended: false,
                };
                let events = stream::unfold(state, move |mut state| async move {
                    loop {
                        if let Some(event) = state.pending.pop_front() {
                            // Sse encodes one event per frame in this poll; ordered batches need only their last frame to retain the hold.
                            if state.pending.is_empty() {
                                *state.output_hold.lock().unwrap() = state.hold.take();
                            }
                            return Some((Ok::<_, Infallible>(event), state));
                        }
                        state.hold.take();
                        if state.ended {
                            return None;
                        }
                        let (event, hold) = state.events.recv().await?;
                        state.hold = Some(hold);
                        match event {
                            TurnEvent::KeepAlive => {
                                state.pending.push_back(self.keep_alive(&mut state.wire))
                            }
                            TurnEvent::Stop(stop) => state.wire.stop = stop,
                            TurnEvent::Text(text) => {
                                state.pending.extend(self.text(&mut state.wire, &text))
                            }
                            TurnEvent::ToolCall(call) => state.wire.tool_call = Some(call),
                            TurnEvent::End(Ok(usage)) => {
                                state.pending.extend(self.end(&mut state.wire, &usage));
                                state.ended = true;
                            }
                            TurnEvent::End(Err(error)) => {
                                state
                                    .pending
                                    .extend(self.error_events(&mut state.wire, &error.into()));
                                state.ended = true;
                            }
                        }
                    }
                });
                let (parts, body) = Sse::new(events).into_response().into_parts();
                Response::from_parts(
                    parts,
                    hold_body(body, move || output_hold.lock().unwrap().take()),
                )
            } else {
                drop(hold);
                while let Some((event, _hold)) = events.recv().await {
                    match event {
                        TurnEvent::KeepAlive => {}
                        TurnEvent::Stop(stop) => wire.stop = stop,
                        TurnEvent::Text(chunk) => wire.text.push_str(&chunk),
                        TurnEvent::ToolCall(call) => wire.tool_call = Some(call),
                        TurnEvent::End(Ok(usage)) => {
                            return Json(self.body(&wire, &usage)).into_response();
                        }
                        TurnEvent::End(Err(error)) => {
                            return self.refusal(&error.into());
                        }
                    }
                }
                self.refusal(&Message::Upstream("conductor stopped".into()).into())
            }
        })
    }
}
struct StreamState {
    events: tokio::sync::mpsc::UnboundedReceiver<(TurnEvent, Hold)>,
    wire: Encoding,
    pending: VecDeque<Event>,
    hold: Option<Hold>,
    output_hold: Arc<Mutex<Option<Hold>>>,
    ended: bool,
}

pub(crate) fn hold_body(
    body: Body,
    mut next_hold: impl FnMut() -> Option<Hold> + Send + 'static,
) -> Body {
    let bytes = body.into_data_stream().map(move |data| {
        data.map(|bytes| match next_hold() {
            Some(hold) => Bytes::from_owner(HeldBytes { bytes, _hold: hold }),
            None => bytes,
        })
    });
    Body::from_stream(bytes)
}

// Retain the hold through the socket write so virtual time cannot move before the client has the bytes.
struct HeldBytes {
    bytes: Bytes,
    _hold: Hold,
}
impl AsRef<[u8]> for HeldBytes {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}
struct Encoding {
    stop: Stop,
    id: String,
    item_id: String,
    model: String,
    tools: Vec<Value>,
    tool_choice: ToolChoice,
    include_usage: bool,
    input_tokens: u64,
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
        form: ReplyForm,
        wall: std::time::SystemTime,
    ) -> Result<Self, String> {
        Ok(Self {
            stop: Stop::EndTurn,
            id: identifier(prefix)?,
            item_id: identifier("msg_")?,
            model: form.model_name,
            tools: form.tools,
            tool_choice: form.tool_choice,
            include_usage: form.include_usage,
            input_tokens: form.input_tokens,
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
    let random = crate::random_hex::<16>().map_err(|e| e.to_string())?;
    Ok(format!("{prefix}{random}"))
}
fn named(name: &str, value: Value) -> Event {
    Event::default().event(name).data(value.to_string())
}
fn data(value: Value) -> Event {
    Event::default().data(value.to_string())
}

fn refusal_status(refusal: &Refusal) -> axum::http::StatusCode {
    use axum::http::StatusCode;
    match refusal.row() {
        Row::Key => StatusCode::UNAUTHORIZED,
        Row::Quota(_) => StatusCode::TOO_MANY_REQUESTS,
        Row::Rejected => StatusCode::BAD_REQUEST,
        Row::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        Row::Other502 => StatusCode::BAD_GATEWAY,
        Row::Temporary503(_) | Row::QueueFull => StatusCode::SERVICE_UNAVAILABLE,
    }
}

pub(crate) fn refusal_code(refusal: &Refusal) -> &'static str {
    match refusal.message {
        Message::Lockout(_) => "lockout",
        Message::RuntimeUnavailable(_) => "runtime_unavailable",
        Message::SignInRequired(_) => "sign_in_required",
        Message::Model { .. } => "model_not_found",
        Message::Field { .. } => "unsupported_parameter",
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
        Message::InvalidJson => "invalid_json",
        Message::Invalid(_) => "invalid_request_error",
        Message::Key => "invalid_api_key",
        Message::TooLarge => "request_too_large",
        Message::QueueFull => "queue_full",
        Message::QuotaWait { .. } => "quota_wait",
        Message::Upstream(_)
        | Message::MissingSessionId
        | Message::RuntimeStdoutClosed
        | Message::RuntimeNotAvailable
        | Message::ConfigOptionNotApplied { .. }
        | Message::UpstreamHttp(_)
        | Message::UpstreamAcp(_) => "upstream_error",
        Message::BuiltInToolRan(_) => "built_in_tool_ran",
        Message::RuntimeFailed => "runtime_failed",
        Message::NoUpdate => "no_update",
        Message::MaxTurnRequests => "max_turn_requests",
        Message::RetryWait { .. } => "retry_wait",
        Message::Stopped => "stopped",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn flagged_hold() -> (Arc<AtomicBool>, Hold) {
        let released = Arc::new(AtomicBool::new(false));
        let flag = released.clone();
        (
            released,
            Hold::on_drop(move || flag.store(true, Ordering::SeqCst)),
        )
    }

    #[tokio::test]
    async fn stream_bytes_keep_work_live_until_the_transport_releases_the_batch() {
        let (send, events) = tokio::sync::mpsc::unbounded_channel();
        let (initial_released, hold) = flagged_hold();
        let response = MESSAGES
            .respond(
                ReplyForm {
                    stream: true,
                    include_usage: false,
                    input_tokens: 1,
                    model_name: "buffered-stream".into(),
                    tools: Vec::new(),
                    tool_choice: ToolChoice::Auto,
                },
                Ticket { events, hold },
                Arc::new(crate::os::SystemClock::default()),
            )
            .await;
        let mut body = response.into_body().into_data_stream();
        let initial = body.next().await.unwrap().unwrap();
        assert!(initial.starts_with(b"event: message_start\n"));
        assert!(futures_util::poll!(body.next()).is_pending());
        assert!(
            !initial_released.load(Ordering::SeqCst),
            "a pending socket write still owns the initial event"
        );
        drop(initial);
        assert!(initial_released.load(Ordering::SeqCst));

        let (text_released, hold) = flagged_hold();
        send.send((TurnEvent::Text("buffered".into()), hold))
            .ok()
            .unwrap();
        let start = body.next().await.unwrap().unwrap();
        let delta = body.next().await.unwrap().unwrap();
        assert!(start.starts_with(b"event: content_block_start\n"));
        assert!(delta.starts_with(b"event: content_block_delta\n"));
        assert!(futures_util::poll!(body.next()).is_pending());
        drop(start);
        drop(body);
        assert!(
            !text_released.load(Ordering::SeqCst),
            "disconnect cannot release bytes still owned by the transport"
        );
        drop(delta);
        assert!(text_released.load(Ordering::SeqCst));
    }

    #[test]
    fn runtime_unavailable_refusals_tell_each_client_not_to_retry() {
        let refusal =
            Message::RuntimeUnavailable(crate::messages::Unavailable::no_executable()).into();
        for dialect in [CHAT, MESSAGES, RESPONSES] {
            let response = dialect.refusal(&refusal);
            assert_eq!(
                (
                    response.status().as_u16(),
                    response
                        .headers()
                        .get("x-should-retry")
                        .and_then(|value| value.to_str().ok())
                ),
                (400, Some("false")),
                "FR-033 marks M-14 as non-retryable"
            );
        }
    }
}
