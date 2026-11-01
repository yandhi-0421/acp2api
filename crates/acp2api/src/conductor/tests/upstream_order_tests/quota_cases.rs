use super::*;

#[test]
fn quota_and_sign_in_errors_preserve_the_prefix_when_stderr_arrives_first() {
    for code in [429, 401] {
        let mut turn = PromptTurn::new();
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
    let mut turn = PromptTurn::new();
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
    let (mut turn, mut answer) = PromptTurn::quota_query();
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
    let mut turn = PromptTurn::new();
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
        let mut turn = PromptTurn::new();
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
    let (mut turn, _answer) = PromptTurn::quota_query();
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
fn a_failed_usage_keeps_its_first_error_across_stderr_and_rpc_failures() {
    let mut codes = Vec::new();
    for rpc in [false, true] {
        let (mut turn, mut answer) = PromptTurn::quota_query();
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
fn a_repeated_unknown_quota_error_keeps_the_first_stdout_deadline() {
    let mut turn = PromptTurn::new();
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
        let mut turn = PromptTurn::new();
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
    let mut turn = PromptTurn::new();
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
fn replaced_quota_error_keeps_its_reset_query_at_both_stdout_endings() {
    for rpc in [false, true] {
        let mut turn = PromptTurn::new();
        turn.error(429, 0);
        turn.error(503, 20);
        let update = if rpc {
            TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                crate::messages::HttpError {
                    code: Some(400),
                    status: "X".into(),
                    message: "later".into(),
                },
            ))
        } else {
            TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::EndTurn),
            }
        };
        let commands = turn.core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update,
            }),
            30,
            std::time::UNIX_EPOCH,
        );
        let queries = commands
            .iter()
            .filter(|c| match c {
                Command::Usage { .. } => true,
                Command::Prepare { work, .. } => work.starts_with("quota-"),
                _ => false,
            })
            .count()
            + turn
                .core
                .work
                .values()
                .filter(|w| matches!(w, Work::QuotaQueued { .. }))
                .count();
        assert_eq!(
            queries, 1,
            "FR-036 reset query after replaced quota error; rpc={rpc}"
        );
        assert!(commands.iter().any(|c| matches!(c, Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::UpstreamHttp(error)))) if error.code == Some(503))));
    }
}

#[test]
fn a_late_quota_error_does_not_repeat_a_retiring_usage_query() {
    for retired in [false, true] {
        let (mut turn, _answer) = PromptTurn::quota_query();
        turn.error(400, 10);
        if retired {
            turn.handle(Event::Deadline, 210);
        }
        turn.error(429, 220);
        assert_eq!(
            turn.core
                .work
                .values()
                .filter(|w| matches!(w, Work::QuotaQueued { .. }))
                .count(),
            0,
            "an existing usage query must not enqueue another query; retired={retired}"
        );
        assert!(
            turn.core.quota.wait().is_some(),
            "the late quota signal still updates quota wait"
        );
    }
}

#[test]
fn a_queued_usage_query_keeps_its_session_after_a_late_stderr_error() {
    for code in [400, 429] {
        let mut turn = PromptTurn::new();
        turn.core.catalog = None;
        turn.core.model_names.clear();
        let (reply, _answer) = oneshot::channel();
        turn.error(429, 10);
        turn.handle(Event::ReadModelNames(reply), 11);
        turn.end(20);
        assert_eq!(turn.core.work.values().filter(|w| matches!(w, Work::QuotaQueued { session: Some(session), .. } if session == "running")).count(), 1);
        let commands = turn.core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(code),
                    status: "X".into(),
                    message: "late".into(),
                },
            }),
            30,
            std::time::UNIX_EPOCH,
        );
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, Command::Forget(s) if s == "running")),
            "the queued usage query still owns its ACP session"
        );
        let commands = turn.core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: "models-2".into(),
                result: Err(crate::runtime_driver::PreparationError::Rejected(
                    crate::messages::Message::RuntimeFailed,
                )),
            }),
            40,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            commands
                .iter()
                .filter(|c| matches!(c, Command::Usage { session, .. } if session == "running"))
                .count(),
            1
        );
    }
}

#[test]
fn sign_in_required_overrides_quota_at_the_stdout_wait_deadline() {
    let mut turn = PromptTurn::new();
    turn.error(429, 0);
    turn.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "other".into(),
            error: crate::messages::HttpError {
                code: Some(401),
                status: "UNAUTHENTICATED".into(),
                message: "Authentication required".into(),
            },
        }),
        10,
    );
    let commands = turn
        .core
        .handle(Event::Deadline, 200, std::time::UNIX_EPOCH);
    assert!(commands.iter().any(|c| matches!(
        c,
        Command::Emit(
            _,
            TurnEvent::End(Err(crate::messages::Message::SignInRequired(_)))
        )
    )));
}
