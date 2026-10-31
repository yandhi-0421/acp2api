use super::*;

fn prepare_quota_queries(core: &mut Core, commands: &[Command], now: Moment) -> Vec<Command> {
    let mut prepared = Vec::new();
    for command in commands {
        if let Command::Prepare { work, .. } = command {
            let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
            let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
            prepared.extend(core.handle(
                Event::Runtime(RuntimeEvent::Prepared {
                    work: work.clone(),
                    result: Ok(("quota-query".into(), catalog)),
                }),
                now,
                std::time::UNIX_EPOCH,
            ));
        }
    }
    prepared
}

#[test]
fn a_known_reset_replaces_a_quota_wait_without_another_reset_query() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-replaced-quota-wait-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: quota_error_without_reset_time(),
        }),
        60_000,
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_000),
    );
    assert_eq!(core.quota.wait().unwrap().deadline, 3_660_000);
    let ended = core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: crate::messages::HttpError {
                code: Some(429),
                status: "RESOURCE_EXHAUSTED".into(),
                message: "resets 1970-01-01T00:02:00Z".into(),
            },
        }),
        60_001,
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_001),
    );
    assert_eq!(core.quota.wait().unwrap().deadline, 120_000);
    assert_eq!(
        ended
            .iter()
            .filter(|command| matches!(
                command,
                Command::Emit(
                    _,
                    TurnEvent::End(Err(crate::messages::Message::QuotaWait { .. }))
                )
            ))
            .count(),
        1
    );
    assert_eq!(
        core.work
            .values()
            .filter(|work| matches!(work, Work::QuotaQueued { .. } | Work::QuotaWorking { .. }))
            .count(),
        0,
        "FR-036/FR-052 send no reset query for a replaced quota wait"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_request_settling_after_its_quota_wait_was_replaced_ends_with_the_current_quota_wait() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-settled-replaced-quota-wait-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for (error, until, retry_after, deadline, queries) in [
        (
            quota_error_without_reset_time(),
            "1970-01-01T01:01:00Z",
            3600,
            3_660_001,
            1,
        ),
        (
            crate::messages::HttpError {
                code: Some(429),
                status: "RESOURCE_EXHAUSTED".into(),
                message: "resets 1970-01-01T00:02:00Z".into(),
            },
            "1970-01-01T00:02:00Z",
            60,
            120_000,
            0,
        ),
    ] {
        let (mut core, second, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::Limit(2), 0, std::time::UNIX_EPOCH);
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: second,
                result: Ok(("other".into(), catalog)),
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                    quota_error_without_reset_time(),
                )),
            }),
            60_000,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_000),
        );
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "other".into(),
                error,
            }),
            60_001,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_001),
        );
        let ended = core.handle(
            Event::Deadline,
            60_200,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_200),
        );
        assert_eq!(
            ended
                .iter()
                .filter(|command| matches!(
                    command,
                    Command::Usage { .. } | Command::Prepare { .. }
                ))
                .count(),
            0,
            "FR-036/FR-052 send no reset query for the replaced wait"
        );
        assert_eq!(
            ended.iter().filter(|command| matches!(command,
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::QuotaWait { until: reset, retry_after: seconds })))
                    if reset == until && *seconds == retry_after
            )).count(),
            1,
            "the running request ends once with the current quota wait, without waiting for /usage"
        );
        let refusal = core.model_entries(60_200).err().unwrap();
        assert_eq!(crate::dialect::CHAT.refusal(&refusal).status(), 429);
        assert_eq!(core.quota.wait().unwrap().deadline, deadline);
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "other".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::EndTurn),
                },
            }),
            60_201,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_201),
        );
        let current = core.handle(
            Event::Deadline,
            60_401,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_401),
        );
        assert_eq!(
            current
                .iter()
                .filter(|command| matches!(command,
                    Command::Usage { session, .. } if session == "other"
                ))
                .count(),
            queries,
            "only the current quota wait with no reset time needs a query"
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_request_queries_its_reset_time_after_the_replacing_quota_wait_expires() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-expired-replacing-quota-wait-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    for (session, error, now) in [
        ("running", quota_error_without_reset_time(), 60_000),
        (
            "other",
            crate::messages::HttpError {
                code: Some(429),
                status: "RESOURCE_EXHAUSTED".into(),
                message: "resets 1970-01-01T00:01:01Z".into(),
            },
            60_001,
        ),
    ] {
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: session.into(),
                error,
            }),
            now,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(now),
        );
    }
    core.handle(
        Event::Deadline,
        61_000,
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_000),
    );
    core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::EndTurn),
            },
        }),
        61_001,
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_001),
    );
    let querying = core.handle(
        Event::Deadline,
        61_201,
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_201),
    );
    assert_eq!(
        querying
            .iter()
            .filter(|command| matches!(command,
                Command::Usage { session, .. } if session == "running"
            ))
            .count(),
        1,
        "FR-036 still queries the missing reset time when no quota wait remains"
    );
    assert!(
        !querying
            .iter()
            .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(_)))),
        "the response waits for its reset query"
    );
    for update in [
        TurnUpdate::Text("### Quota Limits\n- **Model M**\n  *(Models within this group: M)*\n  - **Five Hour Limit Remaining:** 0% remaining (resets 1970-01-01T00:02:00Z)".into()),
        TurnUpdate::PromptEnded { stop_reason: Some(StopReason::EndTurn) },
    ] {
        core.handle(
            Event::Runtime(RuntimeEvent::Turn { session: "running".into(), update }),
            61_202,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_202),
        );
    }
    let ended = core.handle(
        Event::Deadline,
        61_402,
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(61_402),
    );
    assert_eq!(ended.iter().filter(|command| matches!(command,
        Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::QuotaWait { until, retry_after: 59 })))
            if until == "1970-01-01T00:02:00Z"
    )).count(), 1, "the running request ends with the queried reset time");
    assert_eq!(core.view.borrow().state, State::Running);
    assert!(
        core.model_entries(61_402).is_ok(),
        "the query does not restart an expired quota wait"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_queued_reset_query_runs_only_without_another_current_quota_wait() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-queued-replaced-quota-wait-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for (release, queries, responses, state) in [
        (60_300, 0, 1, State::QuotaWait),
        (120_001, 1, 0, State::Running),
    ] {
        let (mut core, second, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::Limit(2), 0, std::time::UNIX_EPOCH);
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: second,
                result: Ok(("other".into(), catalog)),
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        core.handle(Event::Limit(1), 1, std::time::UNIX_EPOCH);
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                    quota_error_without_reset_time(),
                )),
            }),
            60_000,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_000),
        );
        let queued = core.handle(
            Event::Deadline,
            60_200,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_200),
        );
        assert!(
            !queued.iter().any(|command| matches!(
                command,
                Command::Usage { .. } | Command::Emit(_, TurnEvent::End(_))
            )),
            "the reset query and its response wait for a place"
        );
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "other".into(),
                error: crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: "resets 1970-01-01T00:02:00Z".into(),
                },
            }),
            60_201,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(60_201),
        );
        let dispatched = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "other".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::EndTurn),
                },
            }),
            release,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(release),
        );
        assert_eq!(
            dispatched
                .iter()
                .filter(|command| matches!(command,
                    Command::Usage { session, .. } if session == "running"
                ))
                .count(),
            queries,
            "FR-036/FR-052 skip the queued reset query only while another quota wait is current"
        );
        assert_eq!(dispatched.iter().filter(|command| matches!(command,
            Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::QuotaWait { until, retry_after: 60 })))
                if until == "1970-01-01T00:02:00Z"
        )).count(), responses, "a skipped query ends its response with the current quota wait");
        assert_eq!(core.view.borrow().state, state);
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_client_disconnect_cancels_a_prompt_awaiting_quota_end() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-disconnect-quota-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    let request = core.work[&0].key().to_owned();
    core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: quota_error_without_reset_time(),
        }),
        60_000,
        std::time::UNIX_EPOCH,
    );
    let disconnected = core.handle(Event::Disconnected(request), 60_001, std::time::UNIX_EPOCH);
    assert_eq!(
        disconnected
            .iter()
            .filter(|command| matches!(command,
                Command::Cancel(session) if session == "running"
            ))
            .count(),
        1,
        "FR-031 cancels the prompt when the API client disconnects during the quota wait"
    );
    assert!(
        !disconnected.iter().any(|command| matches!(
            command,
            Command::Prepare { .. }
                | Command::Usage { .. }
                | Command::Forget(_)
                | Command::Emit(_, _)
        )),
        "the disconnected prompt keeps its place until it ends and sends no response"
    );
    assert_eq!(core.work.values().filter(|work| matches!(work,
        Work::QuotaQueued { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, .. }
            if model == "m" && wait.deadline == 3_660_000
    )).count(), 1, "FR-036/FR-052 preserve exactly one reset query after the disconnect");
    let ended = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::Cancelled),
            },
        }),
        60_002,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        ended
            .iter()
            .filter(|command| matches!(command,
                Command::Forget(session) if session == "running"
            ))
            .count(),
        1,
        "FR-031 clears the ACP session after its prompt ends"
    );
    assert_eq!(
        ended
            .iter()
            .filter(|command| matches!(
                command,
                Command::Complete(crate::journal::Completion {
                    outcome: crate::journal::Outcome::Disconnected,
                    ..
                })
            ))
            .count(),
        1
    );
    assert!(
        !ended
            .iter()
            .any(|command| matches!(command, Command::Emit(_, _)))
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_client_disconnect_after_the_quota_error_ends_the_turn_keeps_the_reset_query() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-disconnect-ended-quota-turn-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    let request = core.work[&0].key().to_owned();
    core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                quota_error_without_reset_time(),
            )),
        }),
        60_000,
        std::time::UNIX_EPOCH,
    );
    let disconnected = core.handle(Event::Disconnected(request), 60_001, std::time::UNIX_EPOCH);
    assert_eq!(
        disconnected
            .iter()
            .filter_map(|command| match command {
                Command::Cancel(_) => Some("cancel"),
                Command::Forget(session) if session == "running" => Some("forget"),
                _ => None,
            })
            .collect::<Vec<_>>(),
        ["forget"],
        "sends no cancel because the prompt turn has ended, then forgets the ACP session"
    );
    assert_eq!(core.work.values().filter(|work| matches!(work,
        Work::QuotaWorking { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, phase: QuotaPhase::Preparing, .. }
            if model == "m" && wait.deadline == 3_660_000
    )).count(), 1, "FR-036/FR-052 preserve exactly one reset query after the disconnect");
    assert_eq!(
        disconnected
            .iter()
            .filter(|command| matches!(
                command,
                Command::Complete(crate::journal::Completion {
                    outcome: crate::journal::Outcome::Disconnected,
                    ..
                })
            ))
            .count(),
        1
    );
    assert!(
        !disconnected
            .iter()
            .any(|command| matches!(command, Command::Emit(_, _)))
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_non_quota_error_after_the_quota_error_ends_the_turn_keeps_the_reset_query() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-quota-settling-error-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(
                quota_error_without_reset_time(),
            )),
        }),
        60_000,
        std::time::UNIX_EPOCH,
    );
    let ended = core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: crate::messages::HttpError {
                code: Some(503),
                status: "UNAVAILABLE".into(),
                message: "Temporary failure".into(),
            },
        }),
        60_001,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        ended
            .iter()
            .filter(|command| matches!(command,
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::UpstreamHttp(error))))
                    if error.code == Some(503) && error.message == "Temporary failure"
            ))
            .count(),
        1,
        "the non-quota error still ends the API request once"
    );
    assert_eq!(core.work.values().filter(|work| matches!(work,
        Work::QuotaWorking { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, phase: QuotaPhase::Preparing, .. }
            if model == "m" && wait.deadline == 3_660_000
    )).count(), 1, "FR-036/FR-052 preserve exactly one reset query after the error");
    assert!(
        !ended
            .iter()
            .any(|command| matches!(command, Command::Cancel(_))),
        "sends no cancel because the prompt turn has ended"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_runtime_failure_while_awaiting_quota_end_keeps_the_reset_query() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-quota-runtime-failure-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: quota_error_without_reset_time(),
        }),
        60_000,
        std::time::UNIX_EPOCH,
    );
    let ended = core.handle(
        Event::Runtime(RuntimeEvent::Failed("runtime failed".into())),
        60_001,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        ended
            .iter()
            .filter(|command| matches!(command,
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::Upstream(error))))
                    if error == "runtime failed"
            ))
            .count(),
        1,
        "the runtime failure still ends the API request once"
    );
    assert_eq!(core.work.values().filter(|work| matches!(work,
        Work::QuotaWorking { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, phase: QuotaPhase::Preparing, .. }
            if model == "m" && wait.deadline == 3_660_000
    )).count(), 1, "FR-036/FR-052 preserve exactly one reset query after the runtime failure");
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_non_quota_error_cancels_a_prompt_awaiting_quota_end() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-quota-error-cancel-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: quota_error_without_reset_time(),
        }),
        60_000,
        std::time::UNIX_EPOCH,
    );
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: crate::messages::HttpError {
                code: Some(503),
                status: "UNAVAILABLE".into(),
                message: "Temporary failure".into(),
            },
        }),
        60_001,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, Command::Cancel(session) if session == "running"))
            .count(),
        1,
        "a non-quota error cancels the prompt while it awaits the quota end"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_no_update_cancel_keeps_the_pending_quota_reset_query() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-idle-quota-reset-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: quota_error_without_reset_time(),
        }),
        60_000,
        std::time::UNIX_EPOCH,
    );
    let timer = core.quota.wait().unwrap().timer;
    let cancelled = core.handle(Event::Deadline, 300_000, std::time::UNIX_EPOCH);
    assert!(
        cancelled
            .iter()
            .any(|command| matches!(command, Command::Cancel(session) if session == "running"))
    );
    assert_eq!(core.work.values().filter(|work| matches!(work,
        Work::QuotaQueued { query: QuotaQuery { purpose: QuotaPurpose::Reset { wait, model: Some(model), response: None }, .. }, session: None }
        if model == "m" && wait.deadline == 3_660_000
    )).count(), 1, "FR-036/FR-052 keep exactly one reset query after the FR-051 cancel");
    assert!(
        !cancelled
            .iter()
            .any(|command| matches!(command, Command::Prepare { .. } | Command::Usage { .. })),
        "the cancelled prompt still occupies its place"
    );
    let ended = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::Cancelled),
            },
        }),
        300_001,
        std::time::UNIX_EPOCH,
    );
    let prepared = prepare_quota_queries(&mut core, &ended, 300_001);
    assert_eq!(prepared.iter().filter(|command| matches!(command, Command::Usage { session, .. } if session == "quota-query")).count(), 1, "the preserved reset query reaches /usage");
    assert_eq!(
        cancelled
            .iter()
            .filter(|command| matches!(
                command,
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::NoUpdate)))
            ))
            .count(),
        1,
        "the API request ends once with M-19"
    );
    assert!(
        !ended
            .iter()
            .chain(&prepared)
            .any(|command| matches!(command, Command::Emit(_, _) | Command::Complete(_))),
        "the reset query holds no second API response"
    );
    let mut query_end = Vec::new();
    for update in [
        TurnUpdate::Text("### Quota Limits\n- **Model M**\n  *(Models within this group: M)*\n  - **Five Hour Limit Remaining:** 0% remaining (resets 1970-01-01T00:10:00Z)".into()),
        TurnUpdate::PromptEnded { stop_reason: Some(StopReason::EndTurn) },
    ] {
        query_end.extend(core.handle(Event::Runtime(RuntimeEvent::Turn {
            session: "quota-query".into(), update,
        }), 300_002, std::time::UNIX_EPOCH + std::time::Duration::from_millis(300_002)));
    }
    query_end.extend(core.handle(
        Event::Deadline,
        300_202,
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(300_202),
    ));
    let wait = core.quota.wait().unwrap();
    assert!(
        wait.timer == timer,
        "the reset result belongs to the original quota wait"
    );
    assert_eq!(
        wait.deadline, 600_000,
        "the reset query replaces the 60-minute fallback"
    );
    assert!(
        !query_end.iter().any(|command| matches!(
            command,
            Command::Emit(_, _) | Command::Complete(_) | Command::Usage { .. }
        )),
        "the reset result neither repeats /usage nor completes the API request again"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn runtime_unavailable_ends_each_kind_of_queued_quota_query() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-queued-quota-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let mut observed = Vec::new();
    for kind in ["reset", "refusal", "refresh"] {
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let active = core.work.remove(&0).unwrap().into_active().unwrap();
        let (reply, mut result) = oneshot::channel();
        let purpose = match kind {
            "reset" => QuotaPurpose::Reset {
                wait: quota::Wait::new(
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(3600),
                    0,
                    std::time::UNIX_EPOCH,
                    quota::TimerId(1),
                ),
                model: Some(active.model.clone()),
                response: Some(active),
            },
            "refusal" => QuotaPurpose::Refusal {
                model: active.model.clone(),
                response: Some(active),
            },
            _ => QuotaPurpose::Refresh(reply),
        };
        core.enqueue_quota(purpose, None);
        let commands = core.handle(
            Event::RuntimeUnavailable(UnavailableReason {
                message: crate::messages::Unavailable::no_executable(),
                cause: None,
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        let answers = commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    Command::Emit(
                        _,
                        TurnEvent::End(Err(crate::messages::Message::RuntimeUnavailable(_)))
                    )
                )
            })
            .count();
        observed.push((
            answers,
            core.work
                .values()
                .any(|work| matches!(work, Work::QuotaQueued { .. })),
            kind != "refresh" || matches!(result.try_recv(), Ok(Err(_))),
        ));
    }
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        observed,
        [(2, false, true), (2, false, true), (1, false, true)],
        "queued Reset/Refusal replies get M-14 and the queued owner action gets an error"
    );
}

#[test]
fn model_lists_use_the_quota_row_until_the_deadline_without_waiting_for_its_timer() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-quota-models-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    let status =
        |result: Result<Vec<crate::catalog::ModelEntry>, crate::messages::Refusal>| match result {
            Ok(_) => 200,
            Err(error) => crate::dialect::CHAT.refusal(&error).status().as_u16(),
        };
    let normal = status(core.model_entries(0));
    core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: crate::messages::HttpError {
                code: Some(429),
                status: "RESOURCE_EXHAUSTED".into(),
                message: "resets 1970-01-01T00:02:00Z".into(),
            },
        }),
        0,
        std::time::UNIX_EPOCH,
    );
    let results = [
        normal,
        status(core.model_entries(1)),
        status(core.model_entries(120_000)),
    ];
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        results,
        [200, 429, 200],
        "FR-012/FR-036 GET /v1/models uses the state's row, including the exact quota deadline"
    );
}

#[test]
fn a_quota_error_after_disconnect_still_queries_the_reset_time() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-retiring-quota-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    let request = core.work[&0].key().to_owned();
    core.handle(Event::Disconnected(request), 0, std::time::UNIX_EPOCH);
    core.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error: quota_error_without_reset_time(),
        }),
        0,
        std::time::UNIX_EPOCH,
    );
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::Cancelled),
            },
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    let prepared = prepare_quota_queries(&mut core, &commands, 1);
    let queries = prepared
        .iter()
        .filter(|command| matches!(command, Command::Usage { .. }))
        .count();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        queries, 1,
        "FR-036/FR-052 a lost API reply does not discard a global quota signal or its reset query"
    );
}

#[test]
fn acp_quota_error_refuses_the_current_and_queued_requests() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-acp-quota-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    let mut commands = core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Failed(crate::messages::Message::UpstreamAcp(
                crate::messages::AcpError {
                    data: None,
                    code: 429,
                    message: "RESOURCE_EXHAUSTED; resets 1970-01-01T00:02:00Z".into(),
                },
            )),
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    commands.extend(core.handle(Event::Deadline, 201, std::time::UNIX_EPOCH));
    let statuses: Vec<_> = commands
        .into_iter()
        .filter_map(|command| match command {
            Command::Emit(_, TurnEvent::End(Err(message))) => Some(
                crate::dialect::MESSAGES
                    .refusal(&message.into())
                    .status()
                    .as_u16(),
            ),
            _ => None,
        })
        .collect();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        statuses,
        [429, 429],
        "FR-033/FR-036 an ACP quota error has the same state row as a stderr quota error"
    );
}
