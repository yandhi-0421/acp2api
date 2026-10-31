use super::*;

#[test]
fn a_prompt_after_a_quota_signal_keeps_its_no_update_deadline() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-quota-idle-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for with_update in [false, true] {
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "running".into(),
                error: crate::messages::HttpError {
                    code: Some(429),
                    status: "RESOURCE_EXHAUSTED".into(),
                    message: "Quota exhausted".into(),
                },
            }),
            60_000,
            std::time::UNIX_EPOCH,
        );
        if with_update {
            core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "running".into(),
                    update: TurnUpdate::Usage {
                        used: 1000,
                        size: 1_048_576,
                    },
                }),
                240_000,
                std::time::UNIX_EPOCH,
            );
        }
        let deadline = if with_update { 540_000 } else { 300_000 };
        let early = core.handle(Event::Deadline, deadline - 1, std::time::UNIX_EPOCH);
        assert!(
            !early
                .iter()
                .any(|command| matches!(command, Command::Cancel(_)))
        );
        assert_eq!(
            core.next_deadline(),
            Some(deadline),
            "the running prompt still schedules its FR-051 deadline"
        );
        let due = core.handle(Event::Deadline, deadline, std::time::UNIX_EPOCH);
        assert!(
            due.iter()
                .any(|command| matches!(command, Command::Cancel(session) if session == "running")),
            "FR-051 cancels a silent prompt after a quota signal"
        );
        assert!(
            due.iter().any(|command| matches!(
                command,
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::NoUpdate)))
            )),
            "FR-051 ends with M-19 rather than a quota response"
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[tokio::test]
async fn no_update_releases_the_last_chunk_before_the_error() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-no-update-chunk-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for stream in [false, true] {
        let (mut core, _, [events, _queued]) = prompting_with_waiter(&journal);
        let form = crate::dialect::CHAT
            .parse(
                &serde_json::to_vec(&serde_json::json!({
                    "model":"m", "messages":[{"role":"user","content":"First"}], "stream":stream
                }))
                .unwrap(),
            )
            .ok()
            .unwrap()
            .form;
        let mut commands = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::Text("Last reply chunk".into()),
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        commands.extend(core.handle(Event::Deadline, 300_000, std::time::UNIX_EPOCH));
        let emitted: Vec<_> = commands
            .iter()
            .filter_map(|command| match command {
                Command::Emit(_, TurnEvent::Text(text)) => Some(text.as_str()),
                Command::Emit(_, TurnEvent::End(Err(crate::messages::Message::NoUpdate))) => {
                    Some("M-19")
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            emitted,
            ["Last reply chunk", "M-19"],
            "FR-051: stream={stream}"
        );
        let clock = crate::Ports::production().clock;
        for command in commands {
            if let Command::Emit(output, event) = command {
                let _ = output.send((event, clock.hold(HoldKind::TurnEvent)));
            }
        }
        let response = crate::dialect::CHAT
            .respond(
                form,
                Ticket {
                    events,
                    hold: clock.hold(HoldKind::TurnEvent),
                },
                clock,
            )
            .await;
        assert_eq!(response.status().as_u16(), if stream { 200 } else { 503 });
        let body = axum::body::to_bytes(response.into_body(), 65_536)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        if stream {
            assert!(body.find("Last reply chunk").unwrap() < body.find("no_update").unwrap());
        } else {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"]["code"],
                "no_update"
            );
        }
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[tokio::test]
async fn max_turn_requests_releases_the_last_chunk_before_the_error() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-max-turn-chunk-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for stream in [false, true] {
        let (mut core, _, [events, _queued]) = prompting_with_waiter(&journal);
        let form = crate::dialect::CHAT
            .parse(
                &serde_json::to_vec(&serde_json::json!({
                    "model":"m", "messages":[{"role":"user","content":"First"}], "stream":stream
                }))
                .unwrap(),
            )
            .ok()
            .unwrap()
            .form;
        let mut commands = Vec::new();
        for update in [
            TurnUpdate::Text("Last reply chunk".into()),
            TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::MaxTurnRequests),
            },
        ] {
            commands.extend(core.handle(
                Event::Runtime(RuntimeEvent::Turn {
                    session: "running".into(),
                    update,
                }),
                0,
                std::time::UNIX_EPOCH,
            ));
        }
        commands.extend(core.handle(Event::Deadline, 200, std::time::UNIX_EPOCH));
        let emitted: Vec<_> = commands
            .iter()
            .filter_map(|command| match command {
                Command::Emit(_, TurnEvent::Text(text)) => Some(text.as_str()),
                Command::Emit(
                    _,
                    TurnEvent::End(Err(crate::messages::Message::MaxTurnRequests)),
                ) => Some("M-21"),
                _ => None,
            })
            .collect();
        assert_eq!(
            emitted,
            ["Last reply chunk", "M-21"],
            "FR-028: stream={stream}"
        );
        let clock = crate::Ports::production().clock;
        for command in commands {
            if let Command::Emit(output, event) = command {
                let _ = output.send((event, clock.hold(HoldKind::TurnEvent)));
            }
        }
        let response = crate::dialect::CHAT
            .respond(
                form,
                Ticket {
                    events,
                    hold: clock.hold(HoldKind::TurnEvent),
                },
                clock,
            )
            .await;
        assert_eq!(response.status().as_u16(), if stream { 200 } else { 502 });
        let body = axum::body::to_bytes(response.into_body(), 65_536)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        if stream {
            assert!(
                body.find("Last reply chunk").unwrap() < body.find("max_turn_requests").unwrap()
            );
        } else {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&body).unwrap()["error"]["code"],
                "max_turn_requests"
            );
        }
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn disconnect_during_session_load_does_not_fail_another_prompt_after_30_seconds() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-load-disconnect-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, loading, _tickets) = prompting_with_waiter(&journal);
    core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
    core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::Text("Hi".into()),
        }),
        1,
        std::time::UNIX_EPOCH,
    );
    core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "running".into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(agent_client_protocol_schema::v1::StopReason::EndTurn),
            },
        }),
        2,
        std::time::UNIX_EPOCH,
    );
    core.handle(Event::Deadline, 202, std::time::UNIX_EPOCH);
    core.sessions.unload(|_| folder.join("saved-cwd"));
    let id = core.by_request(&loading).unwrap();
    core.work.get_mut(&id).and_then(|work| match work {
        Work::AcceptedQueued(request) => Some(request),
        _ => None,
    }).unwrap().turn = crate::dialect::CHAT.parse(&serde_json::to_vec(&serde_json::json!({"model":"m","messages":[{"role":"user","content":"First"},{"role":"assistant","content":"Hi"},{"role":"user","content":"Next"}]})).unwrap()).ok().unwrap().turn;
    core.replaced(Some("other".into()), None);
    core.limit = 2;
    let preparing = core.handle(Event::ResumeRuntime, 203, std::time::UNIX_EPOCH);
    assert!(preparing.iter().any(|command| matches!(command, Command::Prepare { load: Some((session, _)), .. } if session == "running")));
    let turn = crate::dialect::CHAT
        .parse(br#"{"model":"m","messages":[{"role":"user","content":"Independent"}]}"#)
        .ok()
        .unwrap()
        .turn;
    let (output, _ticket) = mpsc::unbounded_channel();
    core.handle(
        Event::Accepted(Box::new(Accepted {
            turn,
            model: crate::catalog::SelectedModel {
                model: "m".into(),
                level: None,
                level_from_name: false,
            },
            request: journal.begin(crate::journal::Api::Chat, 203),
            reply: ResponseChannel {
                output,
                keep_alive: None,
            },
        })),
        203,
        std::time::UNIX_EPOCH,
    );
    core.handle(
        Event::Disconnected(loading.clone()),
        204,
        std::time::UNIX_EPOCH,
    );
    let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
    let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
    core.handle(
        Event::Runtime(RuntimeEvent::Prepared {
            work: loading,
            result: Ok(("running".into(), catalog)),
        }),
        205,
        std::time::UNIX_EPOCH,
    );
    let mut commands = core.handle(Event::Deadline, 30_205, std::time::UNIX_EPOCH);
    commands.extend(core.handle(
        Event::Runtime(RuntimeEvent::Turn {
            session: "other".into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(agent_client_protocol_schema::v1::StopReason::EndTurn),
            },
        }),
        30_206,
        std::time::UNIX_EPOCH,
    ));
    commands.extend(core.handle(Event::Deadline, 30_406, std::time::UNIX_EPOCH));
    let ends: Vec<_> = commands
        .into_iter()
        .filter_map(|command| match command {
            Command::Emit(_, TurnEvent::End(result)) => Some(
                result
                    .map(|_| "success".into())
                    .unwrap_or_else(|error| error.to_string()),
            ),
            _ => None,
        })
        .collect();
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        ends,
        ["success"],
        "FR-041 a disconnected session/load has no running prompt to time out"
    );
}

#[test]
fn disconnect_during_session_creation_retires_without_a_prompt() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-disconnected-prepare-{}",
        std::process::id()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let turn = crate::dialect::CHAT
        .parse(br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#)
        .ok()
        .unwrap()
        .turn;
    let request = journal.begin(crate::journal::Api::Chat, 0);
    let work = request.id.clone();
    let (output, ticket) = mpsc::unbounded_channel();
    let mut core = Core::fixture(1);
    let commands = core.handle(
        Event::Accepted(Box::new(Accepted {
            turn,
            model: crate::catalog::SelectedModel {
                model: "m".into(),
                level: None,
                level_from_name: false,
            },
            request,
            reply: ResponseChannel {
                output,
                keep_alive: None,
            },
        })),
        0,
        std::time::UNIX_EPOCH,
    );
    assert!(matches!(&commands[..], [Command::Prepare { .. }]));
    drop(ticket);
    core.handle(Event::Disconnected(work.clone()), 1, std::time::UNIX_EPOCH);
    assert_eq!(
        core.work.len(),
        1,
        "an outstanding ACP call retains its place"
    );
    let turn = crate::dialect::CHAT
        .parse(br#"{"model":"m","messages":[{"role":"user","content":"Queued"}]}"#)
        .ok()
        .unwrap()
        .turn;
    let queued = journal.begin(crate::journal::Api::Chat, 1);
    let queued_id = queued.id.clone();
    let (output, _queued_ticket) = mpsc::unbounded_channel();
    assert!(
        core.handle(
            Event::Accepted(Box::new(Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: "m".into(),
                    level: None,
                    level_from_name: false
                },
                request: queued,
                reply: ResponseChannel {
                    output,
                    keep_alive: None
                },
            })),
            1,
            std::time::UNIX_EPOCH
        )
        .is_empty(),
        "the second request waits for the outstanding ACP call"
    );
    let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
    let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::Prepared {
            work: work.clone(),
            result: Ok(("disconnected-session".into(), catalog)),
        }),
        2,
        std::time::UNIX_EPOCH,
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Prompt { .. })),
        "FR-031 sends no prompt for a disconnected request"
    );
    assert!(commands.iter().any(|command| matches!(command, Command::Forget(session) if session == "disconnected-session")), "FR-031 clears the created ACP session");
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::Prepare { work, .. } if work == &queued_id)),
        "the completed ACP call dispatches the next queued request"
    );
    assert!(
        core.by_request(&work).is_none(),
        "the completed ACP call releases its entry"
    );
    assert!(
        core.sessions.records().is_empty(),
        "the disconnected session cannot continue"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_prompt_rpc_error_waits_for_the_http_error_lines() {
    let folder = std::env::temp_dir().join(format!("acp2api-error-wait-{}", std::process::id()));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for with_http in [false, true] {
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#)
            .ok()
            .unwrap()
            .turn;
        let (output, _ticket) = mpsc::unbounded_channel();
        let mut core = Core {
            sessions: records::AcpSessions::fixture(Vec::new(), Some("session".into()), 0),
            ..Core::fixture(1)
        };
        core.handle(
            Event::Accepted(Box::new(Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: "m".into(),
                    level: None,
                    level_from_name: false,
                },
                request: journal.begin(crate::journal::Api::Chat, 0),
                reply: ResponseChannel {
                    output,
                    keep_alive: None,
                },
            })),
            0,
            std::time::UNIX_EPOCH,
        );
        let ended = core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "session".into(),
                update: TurnUpdate::Failed(crate::messages::Message::UpstreamAcp(
                    crate::messages::AcpError {
                        data: None,
                        code: -32099,
                        message: "RPC failure".into(),
                    },
                )),
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        assert!(
            !ended
                .iter()
                .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(_)))),
            "FR-028 waits for error lines after a prompt RPC failure"
        );
        assert!(
            core.handle(Event::Deadline, 199, std::time::UNIX_EPOCH)
                .is_empty()
        );
        let ended = if with_http {
            core.handle(
                Event::Runtime(RuntimeEvent::UpstreamError {
                    session: "session".into(),
                    error: crate::messages::HttpError {
                        code: Some(503),
                        status: "UNAVAILABLE".into(),
                        message: "HTTP failure".into(),
                    },
                }),
                200,
                std::time::UNIX_EPOCH,
            )
        } else {
            core.handle(Event::Deadline, 200, std::time::UNIX_EPOCH)
        };
        let message = ended
            .into_iter()
            .find_map(|command| match command {
                Command::Emit(_, TurnEvent::End(Err(error))) => Some(error.to_string()),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            message,
            if with_http {
                "acp2api: upstream error 503 UNAVAILABLE: HTTP failure"
            } else {
                "acp2api: upstream ACP error -32099: RPC failure"
            }
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
}
