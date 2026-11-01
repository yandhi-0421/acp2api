use super::*;

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
    let mut turn = PromptTurn::new();
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
fn errors_and_process_endings_keep_the_response_and_usage_query_bounded() {
    fn orders(items: &mut [u8; 4], at: usize, output: &mut Vec<[u8; 4]>) {
        if at == 4 {
            output.push(*items);
            return;
        }
        for next in at..4 {
            items.swap(at, next);
            orders(items, at + 1, output);
            items.swap(at, next);
        }
    }
    let mut permutations = Vec::new();
    orders(&mut [0, 1, 2, 3], 0, &mut permutations);
    let mut turn = PromptTurn::new();
    let journal = crate::journal::Journal::open(&turn.root, std::time::UNIX_EPOCH).unwrap();
    let mut checked = 0;
    for rpc_first in [false, true] {
        for (code, message, second) in [
            (400, "plain", 400),
            (500, "plain", 500),
            (503, "plain", 503),
            (429, "resets 1970-01-01T00:02:00Z", 429),
            (429, "no reset time", 429),
            (429, "no reset time", 503),
        ] {
            for path in ["prompt", "refresh", "reset", "refusal"] {
                for text in ["Terms of Service", "ordinary reply"] {
                    for ending in ["end", "rpc", "closed", "failed", "timeout"] {
                        for disconnect in [false, true] {
                            if rpc_first && code == 429 && message == "no reset time" {
                                continue;
                            }
                            for order in &permutations {
                                let text_at = order.iter().position(|action| *action == 1).unwrap();
                                let first_at = order
                                    .iter()
                                    .position(|action| matches!(action, 0 | 3))
                                    .unwrap();
                                if rpc_first && text_at > first_at {
                                    continue;
                                }
                                turn.reset(&journal);
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
                                let mut first_error = None;
                                let mut gone = false;
                                for (i, action) in order.iter().enumerate() {
                                    let now = 1000 + i as u64 * 10;
                                    match action {
                                        0 | 3 => {
                                            let first = first_error.is_none();
                                            first_error.get_or_insert(now);
                                            let code = if *action == 3 { second } else { code };
                                            let error = crate::messages::HttpError {
                                                code: Some(code),
                                                status: "X".into(),
                                                message: message.into(),
                                            };
                                            if rpc_first && first {
                                                turn.update(
                                                    TurnUpdate::Failed(
                                                        crate::messages::Message::UpstreamHttp(
                                                            error,
                                                        ),
                                                    ),
                                                    now,
                                                );
                                            } else {
                                                turn.handle(
                                                    Event::Runtime(RuntimeEvent::UpstreamError {
                                                        session: "running".into(),
                                                        error,
                                                    }),
                                                    now,
                                                );
                                            }
                                        }
                                        1 => turn.text(text, now),
                                        _ if disconnect => {
                                            if turn.core.work.by_session("running").is_some() {
                                                if path == "refresh" {
                                                    owner_answer.take();
                                                } else {
                                                    turn.disconnect(now);
                                                    gone = true;
                                                }
                                            }
                                        }
                                        _ => turn.tool(now),
                                    }
                                }
                                let first_error = first_error.unwrap();
                                match ending {
                                    "end" => turn.end(1100),
                                    "rpc" => turn.update(
                                        TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                                            crate::messages::HttpError {
                                                code: Some(400),
                                                status: "X".into(),
                                                message: "RPC end".into(),
                                            },
                                        )),
                                        1100,
                                    ),
                                    "closed" => {
                                        turn.handle(Event::Runtime(RuntimeEvent::Closed), 1100)
                                    }
                                    "failed" => turn.handle(
                                        Event::Runtime(RuntimeEvent::Failed("gone".into())),
                                        1100,
                                    ),
                                    _ => {
                                        turn.handle(Event::Deadline, first_error + 200);
                                        turn.handle(Event::Deadline, first_error + 30_200);
                                    }
                                }
                                if ending != "timeout" && turn.core.work.values().any(|work| matches!(work,
                                Work::QuotaWorking { phase: QuotaPhase::Prompt(session), query: QuotaQuery { purpose: QuotaPurpose::Reset { response: Some(_), .. }, .. }, .. } if session == "running"
                            )) {
                                turn.error(500, 1110);
                                turn.end(1120);
                            }
                                turn.handle(
                                    Event::Deadline,
                                    first_error + if ending == "timeout" { 30_201 } else { 200 },
                                );
                                let context = || {
                                    format!(
                                        "rpc_first={rpc_first} {code}/{second} {message} {path} {text} {ending} disconnect={disconnect} {order:?}"
                                    )
                                };
                                assert!(turn.ends.len() <= 1, "double response: {}", context());
                                assert!(
                                    turn.ends.iter().all(|(_, result)| result.is_err()),
                                    "successful failed response: {}",
                                    context()
                                );
                                let last_is_second = order.iter().position(|a| *a == 3).unwrap()
                                    > order.iter().position(|a| *a == 0).unwrap();
                                let expected_kind = if rpc_first
                                    && path == "prompt"
                                    && code == 429
                                    && message != "no reset time"
                                {
                                    ("quota", 429)
                                } else if text == "Terms of Service" {
                                    ("lockout", 403)
                                } else if path == "reset" || path == "refusal" {
                                    if !rpc_first && matches!(ending, "closed" | "failed") {
                                        if ending == "failed" {
                                            ("process", 503)
                                        } else {
                                            ("runtime", 503)
                                        }
                                    } else {
                                        ("quota", 429)
                                    }
                                } else if code == 429 {
                                    if second == 503 && last_is_second {
                                        ("upstream", 503)
                                    } else {
                                        ("quota", 429)
                                    }
                                } else {
                                    ("upstream", code)
                                };
                                for (_, result) in &turn.ends {
                                    let actual = match result {
                                        Err(crate::messages::Message::Lockout(_)) => {
                                            ("lockout", 403)
                                        }
                                        Err(crate::messages::Message::QuotaWait { .. }) => {
                                            ("quota", 429)
                                        }
                                        Err(crate::messages::Message::UpstreamHttp(error)) => {
                                            ("upstream", error.code.unwrap())
                                        }
                                        Err(crate::messages::Message::RuntimeFailed) => {
                                            ("runtime", 503)
                                        }
                                        Err(crate::messages::Message::Upstream(_)) => {
                                            ("process", 503)
                                        }
                                        other => {
                                            panic!("unexpected error kind {other:?}: {}", context())
                                        }
                                    };
                                    assert_eq!(actual, expected_kind, "error kind: {}", context());
                                }
                                assert!(
                                    turn.ends.iter().all(|(at, _)| *at <= first_error + 200),
                                    "response exceeded wait: {}",
                                    context()
                                );
                                if path != "refresh" && !gone {
                                    assert_eq!(
                                        turn.ends.len(),
                                        1,
                                        "missing response: {}",
                                        context()
                                    );
                                }
                                assert_eq!(
                                    turn.core.lockout.is_some(),
                                    text == "Terms of Service",
                                    "signal outcome: {}",
                                    context()
                                );
                                let expected = if path != "prompt"
                                    || (code == 429
                                        && message == "no reset time"
                                        && text != "Terms of Service")
                                {
                                    1
                                } else {
                                    0
                                };
                                assert_eq!(
                                    turn.query_count(),
                                    expected,
                                    "usage count: {}",
                                    context()
                                );
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(checked, 14_080);
}
