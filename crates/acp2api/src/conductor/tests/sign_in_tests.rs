use super::*;

#[test]
fn a_disconnected_sign_in_check_keeps_its_place_until_its_acp_reply() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-check-disconnect-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.work.clear();
    core.sign_in.set_required();
    core.limit = 1;
    let mut replies = Vec::new();
    let mut leases = Vec::new();
    let mut ids = Vec::new();
    for text in ["A", "B"] {
        let body = serde_json::json!({"model":"m","messages":[{"role":"user","content":text}]})
            .to_string();
        let turn = crate::dialect::CHAT
            .parse(body.as_bytes())
            .ok()
            .unwrap()
            .turn;
        let request = journal.begin(crate::journal::Api::Chat, 0);
        ids.push(request.id.clone());
        let (admission, reply) = oneshot::channel();
        replies.push(reply);
        let (connection, lease) = mpsc::unbounded_channel();
        leases.push(lease);
        core.handle(
            Event::CheckPending(Box::new(PendingRequest {
                mode: ResponseMode::Body,
                turn,
                request,
                admission,
                connection,
            })),
            0,
            std::time::UNIX_EPOCH,
        );
    }
    let commands = core.handle(
        Event::Disconnected(ids[0].clone()),
        0,
        std::time::UNIX_EPOCH,
    );
    assert!(
        matches!(
            core.work[&core.work.by_request(&ids[1]).unwrap()],
            Work::CheckQueued(_)
        ),
        "FR-049 the next check waits for the disconnected call reply"
    );
    assert!(
        core.work
            .by_request(&ids[0])
            .is_some_and(|id| matches!(core.work[&id], Work::Retiring(_))),
        "the disconnected check retains its occupied entry"
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Prepare { work, .. } if work == &ids[1])),
        "FR-049 disconnect does not release outstanding ACP work"
    );
    let error = crate::messages::Message::UpstreamAcp(
        serde_json::from_value(
            serde_json::json!({"code":-32000,"message":"Authentication required"}),
        )
        .unwrap(),
    );
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::Prepared {
            work: ids[0].clone(),
            result: Err(crate::runtime_driver::PreparationError::Rejected(error)),
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::Prepare { work, .. } if work == &ids[1])),
        "FR-049 ACP completion releases the place"
    );
    assert!(commands.iter().any(|command| matches!(command, Command::Complete(done) if done.request.id == ids[0] && done.session_use.is_none())), "FR-031 the pending request retains its journal completion");
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_sign_in_required_replacement_refuses_the_queue_before_it_can_dispatch() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-replacement-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
    core.handle(Event::RuntimeFailure, 0, std::time::UNIX_EPOCH);
    let commands = core.handle(
        Event::RuntimeReady {
            sign_in_required: true,
        },
        1,
        std::time::UNIX_EPOCH,
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Prepare { .. } | Command::Prompt { .. })),
        "FR-038 a signed-out runtime starts no queued request"
    );
    assert!(commands.iter().any(|command| matches!(command, Command::Complete(done) if done.request.id == queued && matches!(done.outcome, crate::journal::Outcome::Refused("sign_in_required")))), "FR-037 the queued request gets the condition that started");
    assert_eq!(core.state(), State::SignInRequired);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn retirement_and_unavailability_end_the_sign_in_wait_in_either_order() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-retirement-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for unavailable in [false, true] {
        for wait_ends_first in [false, true] {
            for deadline in [false, true] {
                let (mut core, _, _tickets) = prompting_with_waiter(&journal);
                core.work.clear();
                core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
                let end = |core: &mut Core| {
                    if deadline {
                        core.handle(Event::Deadline, 600_000, std::time::UNIX_EPOCH);
                    } else if core.sign_in.wait().is_some() {
                        core.handle(Event::EndSignInWait, 1, std::time::UNIX_EPOCH);
                    }
                };
                if wait_ends_first {
                    end(&mut core);
                }
                if unavailable {
                    core.handle(
                        Event::RuntimeUnavailable(UnavailableReason {
                            message: crate::messages::Unavailable::no_executable(),
                            cause: None,
                        }),
                        1,
                        std::time::UNIX_EPOCH,
                    );
                } else {
                    core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
                    core.lifecycle.plan(Replacement::Build {
                        reply: oneshot::channel().0,
                        source: None,
                        initial: false,
                    });
                }
                if !wait_ends_first {
                    end(&mut core);
                }
                assert!(
                    core.sign_in.wait().is_none(),
                    "retirement or unavailability ends the sign-in wait"
                );
                if unavailable {
                    assert!(
                        matches!(core.lifecycle.state(), RuntimeState::Unavailable(_)),
                        "FR-040 a wait cannot restart an unavailable runtime"
                    );
                    assert!(
                        core.lifecycle.replacement().is_none(),
                        "FR-040 no automatic replacement remains"
                    );
                } else {
                    assert!(
                        matches!(core.lifecycle.state(), RuntimeState::Retiring { .. }),
                        "a wait cannot overwrite the build retirement"
                    );
                    assert!(
                        core.replacement_ready(0),
                        "the existing build replacement can proceed"
                    );
                    assert!(matches!(
                        core.lifecycle.replacement(),
                        Some(Replacement::Build { .. })
                    ));
                }
            }
        }
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_sign_in_replacement_waits_for_running_prompt_turns() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-active-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
    let commands = core.handle(Event::EndSignInWait, 1, std::time::UNIX_EPOCH);
    assert!(
        core.lifecycle.replacement().is_some(),
        "FR-039 sign-in uses the ordinary replacement path"
    );
    assert!(
        !core.replacement_ready(0),
        "FR-037 the running prompt retains its runtime"
    );
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::Cancel(_) | Command::Emit(_, TurnEvent::End(_))))
    );
    core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded { stop_reason: None },
        }),
        2,
        std::time::UNIX_EPOCH,
    );
    let commands = core.handle(Event::Deadline, 202, std::time::UNIX_EPOCH);
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::Emit(_, TurnEvent::End(Ok(_))))),
        "FR-037 the running prompt ends normally"
    );
    assert!(
        core.replacement_ready(0),
        "FR-039 the replacement proceeds after the prompt ends"
    );
    assert!(
        core.lifecycle.failure_count() == 0,
        "FR-039 planned replacement is not a runtime failure"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn aborting_retirement_replaces_a_runtime_with_an_ended_sign_in_wait() {
    let mut core = Core::fixture(1);
    core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
    core.handle(Event::RetireRuntime, 1, std::time::UNIX_EPOCH);
    assert!(
        core.sign_in.wait().is_none(),
        "retirement ends the owner's wait"
    );
    core.handle(Event::ResumeRuntime, 2, std::time::UNIX_EPOCH);
    assert!(
        matches!(core.lifecycle.replacement(), Some(Replacement::SignIn)),
        "FR-039 an aborted retirement must still stop the runtime's authenticate call"
    );
    assert!(
        core.replacement_ready(0),
        "the planned replacement can proceed"
    );
    assert!(
        core.lifecycle.failure_count() == 0,
        "FR-039 replacement is not a runtime failure"
    );
}

#[test]
fn an_elapsed_quota_reset_completes_a_pending_sign_in_check() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-elapsed-quota-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let wall = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1);
    let journal = crate::journal::Journal::open(&folder, wall).unwrap();
    for (reset, retry_after) in [
        ("1970-01-01T00:00:00Z", 3600),
        ("1970-01-01T00:00:01Z", 0),
        ("1970-01-01T00:00:01.000500Z", 0),
    ] {
        let mut core = Core::fixture(2);
        core.sign_in.set_required();
        let mut checks = Vec::new();
        for _ in 0..2 {
            let turn = crate::dialect::CHAT
                .parse(br#"{"model":"m","messages":[{"role":"user","content":"Check"}]}"#)
                .ok()
                .unwrap()
                .turn;
            let request = journal.begin(crate::journal::Api::Chat, 0);
            checks.push(request.id.clone());
            let (admission, _reply) = oneshot::channel();
            let (connection, _lease) = mpsc::unbounded_channel();
            core.handle(
                Event::CheckPending(Box::new(PendingRequest {
                    mode: ResponseMode::Body,
                    turn,
                    request,
                    admission,
                    connection,
                })),
                0,
                wall,
            );
        }
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, wall).unwrap();
        core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: checks[0].clone(),
                result: Ok(("checked".into(), catalog)),
            }),
            1,
            wall,
        );
        assert!(
            !core.sign_in.required(),
            "the other check cleared sign-in required"
        );
        let error = crate::messages::AcpError {
            code: 429,
            message: format!("RESOURCE_EXHAUSTED; resets {reset}"),
            data: None,
        };
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: checks[1].clone(),
                result: Err(crate::runtime_driver::PreparationError::Rejected(
                    crate::messages::Message::UpstreamAcp(error),
                )),
            }),
            1,
            wall,
        );
        assert!(
            commands.iter().any(|command| matches!(
                command,
                Command::RefusePending {
                    error: crate::messages::Message::QuotaWait { retry_after: actual, .. },
                    ..
                } if *actual == retry_after
            )),
            "FR-036 reset {reset} completes the failed request with 429 (past reset uses the fallback)"
        );
        assert!(
            core.work.by_request(&checks[1]).is_none(),
            "the failed check releases its place"
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_quota_signal_without_a_reset_in_a_sign_in_check_uses_the_fallback() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-quota-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for disconnected in [false, true] {
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.work.clear();
        core.sign_in.set_required();
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"m","messages":[{"role":"user","content":"Check"}]}"#)
            .ok()
            .unwrap()
            .turn;
        let request = journal.begin(crate::journal::Api::Chat, 0);
        let work = request.id.clone();
        let (admission, _reply) = oneshot::channel();
        let (connection, _lease) = mpsc::unbounded_channel();
        core.handle(
            Event::CheckPending(Box::new(PendingRequest {
                mode: ResponseMode::Body,
                turn,
                request,
                admission,
                connection,
            })),
            0,
            std::time::UNIX_EPOCH,
        );
        if disconnected {
            core.handle(Event::Disconnected(work.clone()), 0, std::time::UNIX_EPOCH);
        }
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work: work.clone(),
                result: Err(crate::runtime_driver::PreparationError::Rejected(
                    crate::messages::Message::UpstreamAcp(
                        serde_json::from_value(
                            serde_json::json!({"code":429,"message":"RESOURCE_EXHAUSTED"}),
                        )
                        .unwrap(),
                    ),
                )),
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            core.quota.wait().unwrap().deadline,
            3_600_000,
            "FR-036 no reset time uses the 60-minute fallback"
        );
        assert!(
            core.work
                .values()
                .any(|w| matches!(w, Work::QuotaQueued { .. })),
            "FR-036 the reset query uses the shared queue"
        );
        assert!(
            core.work.by_request(&work).is_none(),
            "the completed check releases its place"
        );
        if disconnected {
            assert!(commands.iter().any(|c| matches!(c, Command::Complete(done) if done.request.id == work && matches!(done.outcome, crate::journal::Outcome::Disconnected))));
        } else {
            assert!(
                commands.iter().any(|c| matches!(
                    c,
                    Command::RefusePending {
                        error: crate::messages::Message::SignInRequired(_),
                        ..
                    }
                )),
                "FR-037 the higher-priority sign-in condition selects the error"
            );
        }
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn starting_a_sign_in_wait_keeps_the_normal_state_and_queue() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-normal-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
    let commands = core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
    assert_eq!(
        core.state(),
        State::Running,
        "FR-039 the wait alone does not require sign-in"
    );
    assert!(
        core.work.by_request(&queued).is_some(),
        "FR-037 starting a wait does not flush the queue"
    );
    assert!(!commands.iter().any(|c| matches!(
        c,
        Command::Emit(_, TurnEvent::End(_)) | Command::RefusePending { .. }
    )));
    assert!(
        core.refusal(0).is_none(),
        "the wait alone does not refuse admission"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_successful_request_check_allows_admission_during_the_owners_wait() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-success-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.work.clear();
    core.sign_in.set_required();
    core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
    let turn = crate::dialect::CHAT
        .parse(br#"{"model":"m","messages":[{"role":"user","content":"Check"}]}"#)
        .ok()
        .unwrap()
        .turn;
    let request = journal.begin(crate::journal::Api::Chat, 0);
    let work = request.id.clone();
    let (admission, _reply) = oneshot::channel();
    let (connection, _lease) = mpsc::unbounded_channel();
    core.handle(
        Event::CheckPending(Box::new(PendingRequest {
            mode: ResponseMode::Body,
            turn,
            request,
            admission,
            connection,
        })),
        0,
        std::time::UNIX_EPOCH,
    );
    let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
    let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::Prepared {
            work,
            result: Ok(("checked".into(), catalog)),
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    assert!(
        core.refusal(1).is_none(),
        "FR-038 the successful request check clears sign-in required"
    );
    assert!(
        core.sign_in.wait().is_some(),
        "the owner still has its independent wait"
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::AdmitChecked { session, .. } if session == "checked"))
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_owners_wait_shows_the_latest_sign_in_address() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-sign-in-address-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.work.clear();
    core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
    for address in [
        "https://accounts.example.com/first",
        "https://accounts.example.com/latest",
    ] {
        core.handle(
            Event::Runtime(RuntimeEvent::SignInAddress(address.into())),
            1,
            std::time::UNIX_EPOCH,
        );
    }
    assert_eq!(
        core.sign_in.wait().unwrap().address.as_deref(),
        Some("https://accounts.example.com/latest")
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_sign_in_error_from_authenticate_starts_the_required_condition() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-authentication-error-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for code in [401, 500] {
        let (mut core, queued, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::StartSignIn, 0, std::time::UNIX_EPOCH);
        let error = crate::messages::Message::UpstreamAcp(
            serde_json::from_value(
                serde_json::json!({"code":code,"message":"Authentication failed"}),
            )
            .unwrap(),
        );
        let commands = core.handle(
            Event::Runtime(RuntimeEvent::Authenticated(Err(error))),
            1,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            core.sign_in.required(),
            code == 401,
            "FR-038 the authentication error selects sign-in required"
        );
        assert_eq!(
            core.work.by_request(&queued).is_some(),
            code != 401,
            "FR-037 the queue ends only when a condition starts"
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Cancel(_))),
            "FR-037 running turns are preserved"
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
}
