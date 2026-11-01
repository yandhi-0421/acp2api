use super::*;

struct Turn {
    core: Core,
    output: Vec<(Moment, String)>,
    root: PathBuf,
    _tickets: [mpsc::UnboundedReceiver<(TurnEvent, Hold)>; 2],
}

impl Turn {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "acp2api-upstream-order-{}",
            crate::random_hex::<8>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&root, std::time::UNIX_EPOCH).unwrap();
        let (mut core, queued, tickets) = prompting_with_waiter(&journal);
        core.handle(Event::Disconnected(queued), 0, std::time::UNIX_EPOCH);
        Self {
            core,
            output: Vec::new(),
            root,
            _tickets: tickets,
        }
    }

    fn handle(&mut self, event: Event, at: Moment) {
        for command in self.core.handle(
            event,
            at,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(at),
        ) {
            match command {
                Command::Emit(_, TurnEvent::Text(text)) => self.output.push((at, text)),
                Command::Emit(_, TurnEvent::End(Err(error))) => {
                    self.output.push((at, error.to_string()));
                }
                _ => {}
            }
        }
    }

    fn error(&mut self, code: u16, at: Moment) {
        self.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(code),
                    status: "UNAVAILABLE".into(),
                    message: "Synthetic failure".into(),
                },
            }),
            at,
        );
    }

    fn update(&mut self, update: TurnUpdate, at: Moment) {
        self.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update,
            }),
            at,
        );
    }

    fn text(&mut self, text: &str, at: Moment) {
        self.update(TurnUpdate::Text(text.into()), at);
    }

    fn end(&mut self, at: Moment) {
        self.update(
            TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::EndTurn),
            },
            at,
        );
    }

    fn disconnect(&mut self, at: Moment) {
        let id = self.core.work.by_session("running").unwrap();
        let request = self.core.work[&id].key().to_owned();
        self.handle(Event::Disconnected(request), at);
    }

    fn tool(&mut self, at: Moment) {
        self.update(
            TurnUpdate::Permission {
                request: crate::runtime_driver::PermissionRequest::new(serde_json::json!(1)),
                call: crate::conversation::ToolCall {
                    name: "read".into(),
                    arguments: "{}".into(),
                    kind: crate::conversation::ToolKind::Function,
                    namespace: None,
                },
            },
            at,
        );
    }

    fn quota_query() -> (
        Self,
        oneshot::Receiver<Result<(), crate::messages::OwnerMessage>>,
    ) {
        let mut turn = Self::new();
        turn.core = Core {
            sessions: records::AcpSessions::fixture(Vec::new(), Some("running".into()), 0),
            ..Core::fixture(1)
        };
        let (reply, answer) = oneshot::channel();
        turn.handle(Event::RefreshQuota(reply), 0);
        (turn, answer)
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn stderr_first_preserves_text_before_the_held_error_chunk() {
    let mut turn = Turn::new();
    turn.error(503, 0);
    turn.text("Sure, ", 50);
    turn.text("503 The service is unavailable.", 100);
    turn.end(110);
    assert_eq!(
        turn.output,
        [
            (100, "Sure, ".into()),
            (
                110,
                "acp2api: upstream error 503 UNAVAILABLE: Synthetic failure".into()
            ),
        ],
        "FR-028 stderr must not overtake unread reply chunks"
    );
}

#[test]
fn stderr_wait_ends_at_the_prompt_end_without_another_200_ms() {
    let mut turn = Turn::new();
    turn.error(503, 10);
    turn.handle(Event::Deadline, 109);
    assert!(turn.output.is_empty(), "FR-028 must wait for unread stdout");
    turn.end(110);
    assert_eq!(
        turn.output,
        [(
            110,
            "acp2api: upstream error 503 UNAVAILABLE: Synthetic failure".into()
        ),],
        "FR-028 the stdout result ends the error wait immediately"
    );
}

#[test]
fn stderr_wait_ends_at_200_ms_even_when_stdout_never_ends() {
    let mut turn = Turn::new();
    turn.error(503, 10);
    turn.text("Sure, ", 20);
    turn.text("Synthetic failure", 30);
    turn.update(TurnUpdate::Activity, 150);
    turn.handle(Event::Deadline, 209);
    assert_eq!(
        turn.output,
        [(30, "Sure, ".into())],
        "FR-028 the error wait lasts 200 ms"
    );
    turn.handle(Event::Deadline, 210);
    assert_eq!(
        turn.output,
        [
            (30, "Sure, ".into()),
            (
                210,
                "acp2api: upstream error 503 UNAVAILABLE: Synthetic failure".into()
            ),
        ],
        "FR-028 later activity must not extend the error wait"
    );
    assert_eq!(
        turn.core.work.places(),
        1,
        "FR-049 an unfinished turn still holds its place"
    );
}

#[test]
fn late_reply_text_starts_lockout_during_the_stderr_wait() {
    let mut turn = Turn::new();
    turn.error(400, 0);
    turn.text("Your account violates the Terms of ", 50);
    turn.text("Service.", 100);
    assert!(
        turn.core.lockout.is_some(),
        "FR-034 failed-turn text after stderr must start lockout during the wait"
    );
}

#[test]
fn late_reply_text_starts_lockout_after_the_response_ended() {
    let mut turn = Turn::new();
    turn.error(400, 0);
    turn.handle(Event::Deadline, 200);
    assert_eq!(
        turn.output.len(),
        1,
        "the response ended before the late text"
    );
    turn.text("Your account violates the Terms of ", 250);
    turn.text("Service.", 260);
    assert!(
        turn.core.lockout.is_some(),
        "FR-034 retiring failed-turn text must still start lockout"
    );
}

#[test]
fn quota_and_sign_in_errors_preserve_the_prefix_when_stderr_arrives_first() {
    for code in [429, 401] {
        let mut turn = Turn::new();
        turn.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(code),
                    status: "Synthetic status".into(),
                    message: "resets 1970-01-01T00:02:00Z".into(),
                },
            }),
            0,
        );
        turn.text("Sure, ", 50);
        turn.text("Synthetic error text", 100);
        turn.end(110);
        assert_eq!(
            turn.output.len(),
            2,
            "FR-028 code {code} must preserve exactly one prefix and one error"
        );
        assert_eq!(turn.output[0], (100, "Sure, ".into()));
        assert_eq!(
            turn.output[1].0, 110,
            "FR-028 code {code} ends at the stdout result"
        );
    }
}

#[test]
fn a_quota_error_without_a_reset_time_has_the_same_stdout_wait_bound() {
    let mut turn = Turn::new();
    turn.error(429, 0);
    turn.text("Sure, ", 50);
    turn.text("Synthetic error text", 100);
    turn.handle(Event::Deadline, 199);
    assert_eq!(turn.output, [(100, "Sure, ".into())]);
    turn.handle(Event::Deadline, 200);
    assert_eq!(
        turn.output.len(),
        2,
        "FR-028 a missing quota reset must not leave the response open"
    );
    assert_eq!(turn.output[1].0, 200);
    assert_eq!(
        turn.core.work.places(),
        1,
        "the unfinished prompt retains its place"
    );
    turn.text("Terms of Service", 250);
    assert!(
        turn.core.lockout.is_some(),
        "FR-034 quota retirement retains failed-turn evidence"
    );
}

#[test]
fn a_quota_query_error_is_bounded_and_checks_text_after_its_response() {
    let (mut turn, mut answer) = Turn::quota_query();
    turn.error(400, 0);
    turn.handle(Event::Deadline, 199);
    assert!(matches!(
        answer.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    turn.handle(Event::Deadline, 200);
    assert!(
        matches!(answer.try_recv(), Ok(Err(_))),
        "FR-028 a failed usage prompt ends its response at 200 ms"
    );
    assert_eq!(
        turn.core.work.places(),
        1,
        "FR-049 the unfinished usage prompt retains its place"
    );
    turn.text("Terms of ", 250);
    turn.text("Service", 260);
    assert!(
        turn.core.lockout.is_some(),
        "FR-034 quota-query retirement must still check failed-turn text"
    );
}

fn assert_failed_shared_quota_keeps_session(reset: bool) {
    let mut turn = Turn::new();
    if reset {
        turn.error(429, 0);
        turn.end(10);
    } else {
        turn.update(
            TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::Refusal),
            },
            10,
        );
        turn.handle(Event::Deadline, 210);
    }
    assert!(
        turn.core.work.values().any(|work| matches!(work,
            Work::QuotaWorking { phase: quota::QuotaPhase::Prompt(session), .. }
            if session == "running"
        )),
        "the response's usage query reuses its session"
    );
    turn.error(400, 220);
    let commands = turn
        .core
        .handle(Event::Deadline, 420, std::time::UNIX_EPOCH);
    assert!(
        commands.iter().any(|command| matches!(command,
            Command::Cancel(session) if session == "running"
        )),
        "the failed usage turn has a cancellation bound"
    );
    assert!(
        !commands.iter().any(|command| matches!(command,
            Command::Forget(session) if session == "running"
        )),
        "F1 unfinished shared usage must retain the runtime text route"
    );
    turn.text("Terms of Service", 430);
    assert!(
        turn.core.lockout.is_some(),
        "F1 late shared usage text locks out"
    );
}

#[test]
fn a_failed_refusal_usage_keeps_its_shared_session_until_the_turn_ends() {
    assert_failed_shared_quota_keeps_session(false);
}

#[test]
fn a_failed_reset_usage_keeps_its_shared_session_until_the_turn_ends() {
    assert_failed_shared_quota_keeps_session(true);
}

#[test]
fn a_repeat_quota_error_at_the_deadline_cannot_complete_a_running_turn() {
    for first in [503, 429] {
        let mut turn = Turn::new();
        turn.error(first, 0);
        turn.text("Sure, ", 20);
        turn.text("Error text", 30);
        let mut commands = turn.core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: "No reset time".into(),
                },
            }),
            200,
            std::time::UNIX_EPOCH,
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Ok(_))))),
            "F2 an equal deadline is not a prompt end"
        );
        commands.extend(
            turn.core
                .handle(Event::Deadline, 200, std::time::UNIX_EPOCH),
        );
        assert!(
            commands.iter().any(|command| matches!(command,
                Command::Cancel(session) if session == "running"
            )),
            "F2 the unfinished failed turn is cancelled"
        );
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, Command::Emit(_, TurnEvent::End(Err(_)))))
                .count(),
            1,
            "F2 the failed response ends exactly once with an error"
        );
    }
}

#[test]
fn a_usage_text_update_at_the_deadline_does_not_end_the_prompt() {
    let (mut turn, _answer) = Turn::quota_query();
    turn.error(400, 0);
    turn.text("Quota text", 200);
    let commands = turn
        .core
        .handle(Event::Deadline, 200, std::time::UNIX_EPOCH);
    assert!(
        commands.iter().any(|command| matches!(command,
            Command::Cancel(session) if session == "running"
        )),
        "F2 a text update at the usage deadline still needs cancellation"
    );
    assert_eq!(turn.core.work.places(), 1);
}

#[test]
fn a_process_failure_during_the_stdout_wait_preserves_the_upstream_error() {
    for closed in [false, true] {
        for code in [503, 429] {
            let mut turn = Turn::new();
            turn.handle(
                Event::Runtime(RuntimeEvent::UpstreamError {
                    session: "running".into(),
                    error: crate::messages::HttpError {
                        code: Some(code),
                        status: "UNAVAILABLE".into(),
                        message: "resets 1970-01-01T00:02:00Z".into(),
                    },
                }),
                10,
            );
            let event = if closed {
                RuntimeEvent::Closed
            } else {
                RuntimeEvent::Failed("runtime process failed".into())
            };
            let commands = turn
                .core
                .handle(Event::Runtime(event), 100, std::time::UNIX_EPOCH);
            assert!(
                commands.iter().any(|command| match command {
                    Command::Emit(
                        _,
                        TurnEvent::End(Err(crate::messages::Message::UpstreamHttp(error))),
                    ) => code == 503 && error.code == Some(503),
                    Command::Emit(
                        _,
                        TurnEvent::End(Err(crate::messages::Message::QuotaWait { .. })),
                    ) => code == 429,
                    _ => false,
                }),
                "F3 the process failure must not replace stderr error {code}, closed={closed}"
            );
            if code == 503 {
                assert_eq!(
                    turn.core.retry_waits.last().unwrap().deadline,
                    30_010,
                    "FR-029 starts at the upstream failure, not the process exit"
                );
            }
        }
    }
}

#[test]
fn stop_and_lockout_still_override_a_pending_upstream_error() {
    for lockout in [false, true] {
        let mut turn = Turn::new();
        turn.error(503, 0);
        let event = if lockout {
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(403),
                    status: "PERMISSION_DENIED".into(),
                    message: "Blocked".into(),
                },
            })
        } else {
            Event::Stop
        };
        let commands = turn.core.handle(event, 50, std::time::UNIX_EPOCH);
        assert!(
            commands.iter().any(|command| match command {
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::Lockout(_)))) =>
                    lockout,
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::Stopped))) =>
                    !lockout,
                _ => false,
            }),
            "F3 lockout and stop keep their priority"
        );
    }
}

#[test]
fn a_cut_suffix_does_not_turn_order_1403_into_a_lockout_signal() {
    let mut turn = Turn::new();
    turn.error(400, 0);
    turn.text("Order 1403 is shipped to", 20);
    assert!(turn.core.lockout.is_none());
    turn.text(" Boston.", 30);
    assert!(
        turn.core.lockout.is_none(),
        "F4 a cut token head is not a new 403 token"
    );
    turn.end(40);
    assert!(turn.core.lockout.is_none());
}

#[test]
fn an_unfinished_403_token_waits_for_the_next_chunk_or_the_turn_end() {
    for continuation in [Some("50 items"), Some(" items"), None] {
        let mut turn = Turn::new();
        turn.error(400, 0);
        turn.text("Total 403", 20);
        assert!(
            turn.core.lockout.is_none(),
            "F4 the token at a chunk end is incomplete"
        );
        if let Some(text) = continuation {
            turn.text(text, 30);
        } else {
            turn.end(30);
        }
        assert_eq!(
            turn.core.lockout.is_some(),
            continuation != Some("50 items"),
            "FR-034 distinguishes 403 from 40350, and confirms a token at turn end"
        );
    }
}

#[test]
fn failed_reply_signals_survive_a_disconnect_before_the_error() {
    let mut locked = Vec::new();
    for disconnect_first in [false, true] {
        let mut turn = Turn::new();
        if disconnect_first {
            turn.disconnect(10);
        }
        turn.text("Terms of Service", 20);
        if !disconnect_first {
            turn.disconnect(30);
        }
        turn.error(400, 40);
        locked.push(turn.core.lockout.is_some());
    }
    assert_eq!(
        locked,
        [true, true],
        "F5 text before stderr counts on either side of disconnect"
    );
}

#[test]
fn failed_reply_signals_include_text_discarded_after_a_client_tool_call() {
    let mut turn = Turn::new();
    turn.tool(10);
    turn.text("Terms of Service", 20);
    turn.error(400, 30);
    assert!(
        turn.core.lockout.is_some(),
        "F5 the signal scan precedes reply filtering"
    );
}

#[test]
fn a_repeated_error_does_not_reset_a_split_failed_reply_signal() {
    let mut turn = Turn::new();
    turn.tool(0);
    turn.error(400, 10);
    turn.text("Terms of ", 20);
    turn.error(400, 30);
    turn.text("Service", 40);
    assert!(
        turn.core.lockout.is_some(),
        "F5 repeated stderr preserves a split signal"
    );
}

#[test]
fn a_quota_error_while_a_known_session_loads_does_not_start_a_prompt_cancel_timer() {
    let mut turn = Turn::new();
    let id = turn.core.work.by_session("running").unwrap();
    let active = turn.core.work.remove(&id).unwrap().into_active().unwrap();
    let request_id = active.request.id.clone();
    let parsed = crate::dialect::CHAT
        .parse(br#"{"model":"m","messages":[{"role":"user","content":"First"}]}"#)
        .ok()
        .unwrap()
        .turn;
    let preparing = Active::new(
        Accepted {
            turn: parsed,
            model: crate::catalog::SelectedModel {
                model: "m".into(),
                level: None,
                level_from_name: false,
            },
            request: active.request,
            reply: active.reply,
        },
        Some("running".into()),
        request::Start::Prepare(crate::conversation::PromptRequest {
            blocks: Vec::new(),
            model: "m".into(),
            level: None,
        }),
        active.record,
        active.kind,
    );
    turn.core.work.restore(id, Work::AcceptedWorking(preparing));
    turn.error(429, 10);
    let commands = turn
        .core
        .handle(Event::Deadline, 210, std::time::UNIX_EPOCH);
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Cancel(_))),
        "F6 session/load is not a running prompt with a 200 ms cancel bound"
    );
    let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
    let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
    turn.handle(
        Event::Runtime(RuntimeEvent::Prepared {
            work: request_id,
            result: Ok(("running".into(), catalog)),
        }),
        500,
    );
    turn.handle(Event::Deadline, 500);
    assert!(
        !turn.core.sessions.cancellation_due(30_210),
        "F6 a completed load leaves no stale cancellation"
    );
    turn.handle(Event::Deadline, 30_210);
    assert!(matches!(turn.core.lifecycle.state(), RuntimeState::Serving));
}

#[test]
fn a_failed_usage_keeps_its_first_error_across_stderr_and_rpc_failures() {
    let mut codes = Vec::new();
    for rpc in [false, true] {
        let (mut turn, mut answer) = Turn::quota_query();
        turn.error(503, 10);
        if rpc {
            turn.update(
                TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                    crate::messages::HttpError {
                        code: Some(502),
                        status: "BAD_GATEWAY".into(),
                        message: "Later error".into(),
                    },
                )),
                20,
            );
        } else {
            turn.error(502, 20);
            turn.handle(Event::Deadline, 210);
        }
        codes.push(match answer.try_recv() {
            Ok(Err(crate::messages::OwnerMessage::Api(
                crate::messages::Message::UpstreamHttp(error),
            ))) => error.code,
            _ => None,
        });
    }
    assert_eq!(
        codes,
        [Some(503), Some(503)],
        "F7 usage must retain the first upstream error"
    );
}

#[test]
fn the_longest_lockout_signal_can_cross_a_chunk_after_sixteen_bytes() {
    let mut turn = Turn::new();
    turn.error(400, 0);
    turn.text("PERMISSION_DENIE", 10);
    assert!(turn.core.lockout.is_none());
    turn.text("D", 20);
    assert!(
        turn.core.lockout.is_some(),
        "F7 retain enough bytes for PERMISSION_DENIED"
    );
}

#[test]
fn a_reply_suffix_cuts_multibyte_text_at_a_character_boundary() {
    let mut turn = Turn::new();
    turn.error(400, 0);
    turn.text(&format!("{}safe", "界".repeat(8)), 10);
    assert!(turn.core.lockout.is_none());
    turn.text(" Terms of ", 20);
    turn.text("Service", 30);
    assert!(
        turn.core.lockout.is_some(),
        "F7 UTF-8 tail trimming preserves the next signal"
    );
}

#[test]
fn a_lockout_signal_can_start_before_the_stderr_error_and_finish_after_it() {
    let mut turn = Turn::new();
    turn.text("Terms of ", 10);
    turn.error(400, 20);
    assert!(turn.core.lockout.is_none());
    turn.text("Service", 30);
    assert!(
        turn.core.lockout.is_some(),
        "F7 the pre-error suffix remains part of this turn"
    );
}

#[test]
fn a_failed_usage_cancel_has_a_thirty_second_runtime_failure_bound() {
    let (mut turn, _answer) = Turn::quota_query();
    turn.error(400, 0);
    let expired = turn
        .core
        .handle(Event::Deadline, 200, std::time::UNIX_EPOCH);
    assert!(
        expired.iter().any(|command| matches!(command,
            Command::Cancel(session) if session == "running"
        )),
        "F7 a failed usage prompt must actually be cancelled"
    );
    turn.handle(Event::Deadline, 30_199);
    assert!(matches!(turn.core.lifecycle.state(), RuntimeState::Serving));
    assert_eq!(turn.core.work.places(), 1);
    turn.handle(Event::Deadline, 30_200);
    assert!(
        matches!(turn.core.lifecycle.state(), RuntimeState::Restarting),
        "FR-041 an unresponsive usage cancellation fails the runtime at 30 seconds"
    );
    assert_eq!(turn.core.work.places(), 0);
}

#[test]
fn a_repeated_unknown_quota_error_keeps_the_first_stdout_deadline() {
    let mut turn = Turn::new();
    turn.error(429, 0);
    turn.error(429, 199);
    assert_eq!(
        turn.core.next_deadline(),
        Some(200),
        "F7 repeated 429 must not extend the wait"
    );
    turn.handle(Event::Deadline, 200);
    assert_eq!(turn.output.len(), 1);
    assert_eq!(turn.output[0].0, 200);
}

#[test]
fn a_quota_reset_query_survives_a_non_quota_error_before_stdout_ends() {
    for disconnect in [false, true] {
        let mut turn = Turn::new();
        turn.error(429, 0);
        turn.error(503, 20);
        if disconnect {
            turn.disconnect(30);
        } else {
            turn.handle(Event::Deadline, 200);
        }
        assert_eq!(
            turn.core
                .work
                .values()
                .filter(|work| matches!(
                    work,
                    Work::QuotaQueued {
                        query: quota::QuotaQuery {
                            purpose: quota::QuotaPurpose::Reset { response: None, .. },
                            ..
                        },
                        ..
                    }
                ))
                .count(),
            1,
            "F7 replacing a pending quota error must preserve its reset query"
        );
    }
}

#[test]
fn a_quota_rpc_failure_after_stderr_starts_usage_without_a_second_wait() {
    let mut turn = Turn::new();
    turn.error(429, 0);
    let commands = turn.core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                quota_error_without_reset_time(),
            )),
        }),
        100,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command,
                Command::Usage { session, .. } if session == "running"
            ))
            .count(),
        1,
        "F7 quota_failure must not add 200 ms after stderr and the RPC end"
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Cancel(_)))
    );
}

#[test]
fn bounded_error_text_end_disconnect_tool_and_repeat_interleavings() {
    fn permutations(items: &mut [u8; 6], at: usize, output: &mut Vec<[u8; 6]>) {
        if at == items.len() {
            output.push(*items);
            return;
        }
        for next in at..items.len() {
            items.swap(at, next);
            permutations(items, at + 1, output);
            items.swap(at, next);
        }
    }
    let mut orders = Vec::new();
    permutations(&mut [0, 1, 2, 3, 4, 5], 0, &mut orders);
    let mut turn = Turn::new();
    let journal = crate::journal::Journal::open(&turn.root, std::time::UNIX_EPOCH).unwrap();
    let mut checked = 0;
    for text in ["Terms of Service", "ordinary reply"] {
        for path in ["prompt", "refresh", "reset", "refusal"] {
            for order in &orders {
                let (mut core, queued, tickets) = prompting_with_waiter(&journal);
                core.handle(Event::Disconnected(queued), 0, std::time::UNIX_EPOCH);
                turn.core = core;
                turn._tickets = tickets;
                turn.output.clear();
                let mut owner_answer = None;
                match path {
                    "refresh" => {
                        turn.core = Core {
                            sessions: records::AcpSessions::fixture(
                                Vec::new(),
                                Some("running".into()),
                                0,
                            ),
                            ..Core::fixture(1)
                        };
                        let (reply, answer) = oneshot::channel();
                        owner_answer = Some(answer);
                        turn.handle(Event::RefreshQuota(reply), 0);
                    }
                    "reset" => {
                        turn.error(429, 0);
                        turn.end(10);
                    }
                    "refusal" => {
                        turn.update(
                            TurnUpdate::PromptEnded {
                                stop_reason: Some(StopReason::Refusal),
                            },
                            10,
                        );
                        turn.handle(Event::Deadline, 210);
                    }
                    _ => {}
                }
                let mut saw_text = false;
                let mut first_error = None;
                let mut disconnected = false;
                let mut forgotten = false;
                let mut ends = 0;
                for (index, action) in order.iter().enumerate() {
                    let now = 1000 + index as u64 * 10;
                    let event = match action {
                        0 | 5 => {
                            first_error.get_or_insert(now);
                            Some(Event::Runtime(RuntimeEvent::UpstreamError {
                                session: "running".into(),
                                error: crate::messages::HttpError {
                                    code: Some(400),
                                    status: "INVALID_ARGUMENT".into(),
                                    message: "Synthetic failure".into(),
                                },
                            }))
                        }
                        1 if !forgotten => {
                            saw_text = true;
                            Some(Event::Runtime(RuntimeEvent::Turn {
                                session: "running".into(),
                                update: TurnUpdate::Text(text.into()),
                            }))
                        }
                        2 if !forgotten => Some(Event::Runtime(RuntimeEvent::Turn {
                            session: "running".into(),
                            update: TurnUpdate::PromptEnded {
                                stop_reason: Some(StopReason::EndTurn),
                            },
                        })),
                        3 => {
                            if let Some(id) = turn.core.work.by_session("running") {
                                if turn.core.work[&id].reply().is_some() {
                                    disconnected = true;
                                    Some(Event::Disconnected(turn.core.work[&id].key().to_owned()))
                                } else {
                                    owner_answer.take();
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        4 if !forgotten => Some(Event::Runtime(RuntimeEvent::Turn {
                            session: "running".into(),
                            update: TurnUpdate::Permission {
                                request: crate::runtime_driver::PermissionRequest::new(
                                    serde_json::json!(1),
                                ),
                                call: crate::conversation::ToolCall {
                                    name: "read".into(),
                                    arguments: "{}".into(),
                                    kind: crate::conversation::ToolKind::Function,
                                    namespace: None,
                                },
                            },
                        })),
                        _ => None,
                    };
                    let commands = event
                        .map(|event| turn.core.handle(event, now, std::time::UNIX_EPOCH))
                        .unwrap_or_default();
                    for command in commands {
                        match command {
                            Command::Forget(session) if session == "running" => forgotten = true,
                            Command::Emit(_, TurnEvent::End(result)) => {
                                ends += 1;
                                assert!(
                                    result.is_err(),
                                    "no End(Ok) after failure: {path} {order:?}"
                                );
                            }
                            _ => {}
                        }
                    }
                    assert!(ends <= 1, "no double end: {path} {order:?}");
                    if text == "Terms of Service" && saw_text && first_error.is_some() {
                        assert!(
                            turn.core.lockout.is_some(),
                            "signal in every read order: {path} {order:?}, action={action}"
                        );
                    }
                    if text == "ordinary reply" {
                        assert!(
                            turn.core.lockout.is_none(),
                            "ordinary text is not a lockout: {path} {order:?}"
                        );
                    }
                }
                let bound = first_error.unwrap() + 200;
                let commands = turn
                    .core
                    .handle(Event::Deadline, bound, std::time::UNIX_EPOCH);
                for command in commands {
                    if let Command::Emit(_, TurnEvent::End(result)) = command {
                        ends += 1;
                        assert!(result.is_err(), "no successful deadline: {path} {order:?}");
                    }
                }
                assert!(ends <= 1, "no double deadline end: {path} {order:?}");
                if path != "refresh" && !disconnected {
                    assert_eq!(ends, 1, "response bounded at {bound}: {path} {order:?}");
                }
                if path == "refresh" {
                    if let Some(mut answer) = owner_answer {
                        assert!(
                            matches!(answer.try_recv(), Ok(Err(_))),
                            "usage response is bounded: {order:?}"
                        );
                    } else {
                        assert!(
                            !turn.core.work.values().any(|work| matches!(
                                work,
                                Work::QuotaWorking {
                                    query: quota::QuotaQuery {
                                        purpose: quota::QuotaPurpose::Refresh(_),
                                        ..
                                    },
                                    ..
                                }
                            )),
                            "a disconnected owner response is no longer pending: {order:?}"
                        );
                    }
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 5760);
}

#[test]
fn disconnected_ended_reply_evidence_expires_and_does_not_cross_runtime_replacement() {
    let mut locked = Vec::new();
    for (error_at, replacement) in [(219, false), (221, false), (219, true)] {
        let mut turn = Turn::new();
        turn.text("Terms of Service", 10);
        turn.end(20);
        turn.disconnect(30);
        if replacement {
            turn.core.sessions.replaced(None);
        }
        turn.error(400, error_at);
        locked.push(turn.core.lockout.is_some());
    }
    assert_eq!(
        locked,
        [true, false, false],
        "FR-028 retains ended disconnected evidence only in its runtime and 200 ms window"
    );
}
