use super::*;

#[test]
fn a_process_end_keeps_its_error_for_work_without_a_stored_error() {
    for path in ["prompt", "refresh", "checking", "models"] {
        for condition in [0, 429, 401] {
            for failed in [false, true] {
                let (mut turn, mut owner) = if path == "refresh" {
                    let (turn, owner) = PromptTurn::quota_query();
                    (turn, Some(owner))
                } else {
                    (PromptTurn::new(), None)
                };
                let (admission, _answer) = oneshot::channel();
                let (connection, _lease) = mpsc::unbounded_channel();
                if matches!(path, "checking" | "models") {
                    turn.core = Core::fixture(2);
                    if path == "checking" {
                        let journal =
                            crate::journal::Journal::open(&turn.root, std::time::UNIX_EPOCH)
                                .unwrap();
                        let parsed = crate::dialect::CHAT
                            .parse(
                                br#"{"model":"m","messages":[{"role":"user","content":"check"}]}"#,
                            )
                            .ok()
                            .unwrap()
                            .turn;
                        turn.handle(
                            Event::CheckPending(Box::new(PendingRequest {
                                mode: ResponseMode::Body,
                                turn: parsed,
                                request: journal.begin(crate::journal::Api::Chat, 0),
                                admission,
                                connection,
                            })),
                            0,
                        );
                        assert!(
                            turn.core
                                .work
                                .values()
                                .any(|work| matches!(work, Work::Checking(_)))
                        );
                    } else {
                        let (reply, _answer) = oneshot::channel();
                        turn.handle(Event::ReadModelNames(reply), 0);
                        assert!(
                            turn.core
                                .work
                                .values()
                                .any(|work| matches!(work, Work::ModelsReading(_)))
                        );
                    }
                }
                if condition != 0 {
                    turn.handle(
                        Event::Runtime(RuntimeEvent::UpstreamError {
                            session: "other".into(),
                            error: crate::messages::HttpError {
                                code: Some(condition),
                                status: "X".into(),
                                message: "resets 1970-01-01T00:02:00Z".into(),
                            },
                        }),
                        10,
                    );
                }
                let commands = turn.core.handle(
                    if failed {
                        Event::Runtime(RuntimeEvent::Failed("process failure".into()))
                    } else {
                        Event::Runtime(RuntimeEvent::Closed)
                    },
                    60,
                    std::time::UNIX_EPOCH,
                );
                let mut errors: Vec<_> = commands
                    .iter()
                    .filter_map(|command| match command {
                        Command::Emit(_, TurnEvent::End(Err(error)))
                        | Command::RefusePending { error, .. }
                        | Command::ModelsRead {
                            result: Err(crate::messages::OwnerMessage::Api(error)),
                            ..
                        } => Some(error.clone()),
                        _ => None,
                    })
                    .collect();
                if let Some(owner) = owner.as_mut() {
                    let error = owner.try_recv().unwrap().unwrap_err();
                    if let crate::messages::OwnerMessage::Api(error) = error {
                        errors.push(error);
                    } else {
                        panic!("unexpected refresh error: {error:?}");
                    }
                }
                assert_eq!(
                    errors.len(),
                    1,
                    "{path}, condition={condition}, failed={failed}"
                );
                match &errors[0] {
                    crate::messages::Message::RuntimeFailed => assert!(!failed),
                    crate::messages::Message::Upstream(text) => {
                        assert!(failed);
                        assert_eq!(text, "process failure");
                    }
                    other => panic!("{path}, condition={condition}, failed={failed}: {other:?}"),
                }
                if path == "prompt" {
                    assert_eq!(turn.core.retry_waits.len(), 1);
                    assert_eq!(turn.core.retry_waits[0].deadline, 30_060);
                } else {
                    assert!(turn.core.retry_waits.is_empty());
                }
            }
        }
    }
}

#[test]
fn a_process_failure_during_the_stdout_wait_preserves_the_upstream_error() {
    for rpc in [false, true] {
        for ending in ["closed", "failed", "timeout", "expired"] {
            for (code, message) in [
                (400, "plain"),
                (500, "plain"),
                (503, "plain"),
                (429, "resets 1970-01-01T00:02:00Z"),
                (429, "no reset time"),
            ] {
                let mut turn = PromptTurn::new();
                let error = crate::messages::HttpError {
                    code: Some(code),
                    status: "UNAVAILABLE".into(),
                    message: message.into(),
                };
                if rpc {
                    turn.update(
                        TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(error)),
                        10,
                    );
                } else {
                    turn.handle(
                        Event::Runtime(RuntimeEvent::UpstreamError {
                            session: "running".into(),
                            error,
                        }),
                        10,
                    );
                }
                let event = match ending {
                    "closed" | "expired" => Event::Runtime(RuntimeEvent::Closed),
                    "failed" => {
                        Event::Runtime(RuntimeEvent::Failed("runtime process failed".into()))
                    }
                    _ => Event::RuntimeFailure,
                };
                turn.handle(event, if ending == "expired" { 3_600_011 } else { 60 });
                assert!(
                    turn.ends.iter().any(|(_, result)| match result {
                        Err(crate::messages::Message::UpstreamHttp(error)) =>
                            code != 429 && error.code == Some(code),
                        Err(crate::messages::Message::QuotaWait { .. }) => code == 429,
                        _ => false,
                    }),
                    "stored stderr {code} {message} must survive {ending}"
                );
                if code == 429 {
                    assert!(
                        turn.core.retry_waits.is_empty(),
                        "quota has no conversation retry wait"
                    );
                } else if code != 400 {
                    assert_eq!(turn.core.retry_waits.last().unwrap().deadline, 30_010);
                }
            }
        }
    }
}

#[test]
fn waiting_errors_follow_the_current_state_priority_at_each_ending() {
    for code in [400, 500, 503, 429] {
        for rpc in [false, true] {
            for replacement in [401, 429] {
                for ending in ["deadline", "closed", "end"] {
                    if rpc && ending == "end" {
                        continue;
                    }
                    if code == 429
                        && replacement == 401
                        && (ending == "end" || rpc && ending == "deadline")
                    {
                        continue;
                    }
                    let mut turn = PromptTurn::new();
                    let error = crate::messages::HttpError {
                        code: Some(code),
                        status: "X".into(),
                        message: "No reset time".into(),
                    };
                    if rpc {
                        turn.update(
                            TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(error)),
                            10,
                        );
                    } else {
                        turn.handle(
                            Event::Runtime(RuntimeEvent::UpstreamError {
                                session: "running".into(),
                                error,
                            }),
                            10,
                        );
                    }
                    turn.handle(
                        Event::Runtime(RuntimeEvent::UpstreamError {
                            session: "other".into(),
                            error: crate::messages::HttpError {
                                code: Some(replacement),
                                status: "X".into(),
                                message: "resets 1970-01-01T00:02:00Z".into(),
                            },
                        }),
                        20,
                    );
                    match ending {
                        "deadline" => turn.handle(Event::Deadline, 210),
                        "closed" => turn.handle(Event::Runtime(RuntimeEvent::Closed), 60),
                        _ => turn.end(60),
                    }
                    turn.handle(Event::Deadline, 210);
                    assert_eq!(
                        turn.ends.len(),
                        1,
                        "code={code}, rpc={rpc}, replacement={replacement}, {ending}"
                    );
                    match &turn.ends[0].1 {
                        Err(crate::messages::Message::UpstreamHttp(error)) => {
                            assert_eq!(error.code, Some(code))
                        }
                        Err(crate::messages::Message::SignInRequired(_)) => {
                            assert_eq!(code, 429);
                            assert_eq!(replacement, 401);
                        }
                        Err(crate::messages::Message::QuotaWait { retry_after, .. }) => {
                            assert_eq!(code, 429);
                            assert_eq!(replacement, 429);
                            assert_eq!(*retry_after, 120);
                        }
                        other => panic!(
                            "code={code}, rpc={rpc}, replacement={replacement}, {ending}: {other:?}"
                        ),
                    }
                    if code == 429 {
                        assert!(turn.core.retry_waits.is_empty());
                    } else {
                        assert!(matches!(
                            &turn.ends[0].1,
                            Err(crate::messages::Message::UpstreamHttp(_))
                        ));
                        assert_eq!(turn.core.retry_waits.len(), 1);
                        assert_eq!(turn.core.retry_waits[0].deadline, 30_010);
                    }
                }
            }
        }
    }
}

#[test]
fn stop_and_lockout_still_override_a_pending_upstream_error() {
    for lockout in [false, true] {
        let mut turn = PromptTurn::new();
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
            "lockout and stop keep their priority"
        );
    }
}

#[test]
fn a_quota_error_while_a_known_session_loads_does_not_start_a_prompt_cancel_timer() {
    for ending in [
        "prepared",
        "repeat",
        "disconnect",
        "closed",
        "failed",
        "expired",
    ] {
        let mut turn = PromptTurn::new();
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
        if ending == "repeat" {
            turn.error(429, 20);
        }
        if ending == "disconnect" {
            let commands = turn.core.handle(
                Event::Disconnected(request_id.clone()),
                20,
                std::time::UNIX_EPOCH,
            );
            assert!(
                !commands.iter().any(|c| matches!(c, Command::Cancel(_))),
                "a pending session/load has no prompt turn to cancel"
            );
        }
        if matches!(ending, "closed" | "failed" | "expired") {
            turn.handle(
                if ending != "failed" {
                    Event::Runtime(RuntimeEvent::Closed)
                } else {
                    Event::Runtime(RuntimeEvent::Failed("gone".into()))
                },
                if ending == "expired" { 3_600_011 } else { 20 },
            );
            assert!(matches!(
                &turn.ends[..],
                [(_, Err(crate::messages::Message::QuotaWait { .. }))]
            ));
            continue;
        }
        let commands = turn
            .core
            .handle(Event::Deadline, 230, std::time::UNIX_EPOCH);
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Cancel(_))),
            "session/load is not a running prompt with a 200 ms cancel bound"
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
            !turn.core.sessions.runtime_failure_due(30_230),
            "a completed load leaves no stale cancellation"
        );
        turn.handle(Event::Deadline, 30_230);
        assert!(matches!(turn.core.lifecycle.state(), RuntimeState::Serving));
    }
}

#[test]
fn a_failed_usage_cancel_has_a_thirty_second_runtime_failure_bound() {
    let (mut turn, _answer) = PromptTurn::quota_query();
    turn.error(400, 0);
    let expired = turn
        .core
        .handle(Event::Deadline, 200, std::time::UNIX_EPOCH);
    assert!(
        expired.iter().any(|command| matches!(command,
            Command::Cancel(session) if session == "running"
        )),
        "a failed usage prompt must actually be cancelled"
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
fn pending_403_is_confirmed_by_each_runtime_process_ending() {
    for text in ["Error 403", "Error 403."] {
        for path in ["waiting", "retiring", "usage", "quota-retiring"] {
            for ending in ["closed", "failed", "timeout"] {
                let (mut turn, _answer) = if path.contains("usage") || path == "quota-retiring" {
                    let (turn, answer) = PromptTurn::quota_query();
                    (turn, Some(answer))
                } else {
                    (PromptTurn::new(), None)
                };
                turn.text(text, 5);
                turn.error(500, 10);
                assert!(turn.core.lockout.is_none());
                if path.ends_with("retiring") || ending == "timeout" {
                    turn.handle(Event::Deadline, 210);
                }
                let event = match ending {
                    "closed" => Event::Runtime(RuntimeEvent::Closed),
                    "failed" => Event::Runtime(RuntimeEvent::Failed("gone".into())),
                    _ => Event::Deadline,
                };
                let at = if ending == "timeout" {
                    30_210
                } else if path.ends_with("retiring") {
                    220
                } else {
                    100
                };
                let commands = turn.core.handle(event, at, std::time::UNIX_EPOCH);
                assert!(turn.core.lockout.is_some(), "{text:?} {path} {ending}");
                assert!(commands.iter().any(|c| matches!(c, Command::Lockout(_))));
                turn.handle(
                    Event::RuntimeReady {
                        sign_in_required: false,
                    },
                    at + 1,
                );
                assert_eq!(turn.core.state(), State::Lockout);
                assert_eq!(turn.core.work.places(), 0);
            }
        }
    }
}

#[test]
fn a_runtime_failure_discards_an_ended_disconnected_summary() {
    let mut turn = PromptTurn::new();
    turn.text("Terms of Service", 10);
    turn.end(20);
    turn.disconnect(30);
    turn.handle(Event::Runtime(RuntimeEvent::Closed), 40);
    turn.error(400, 50);
    assert!(
        turn.core.lockout.is_none(),
        "signal evidence cannot cross a runtime process failure"
    );
}

#[test]
fn lockout_cancels_a_usage_prompt_that_is_in_its_error_wait() {
    let (mut turn, _answer) = PromptTurn::quota_query();
    turn.error(400, 10);
    let commands = turn.core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Text("Terms of Service".into()),
        }),
        20,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        commands
            .iter()
            .filter(|c| matches!(c, Command::Cancel(s) if s == "running"))
            .count(),
        1
    );
    let cancel = commands
        .iter()
        .position(|c| matches!(c, Command::Cancel(_)))
        .unwrap();
    let lockout = commands
        .iter()
        .position(|c| matches!(c, Command::Lockout(_)))
        .unwrap();
    assert!(cancel < lockout);
}
