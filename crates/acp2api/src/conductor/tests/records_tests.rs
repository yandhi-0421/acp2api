use super::*;

fn answer_quota(core: &mut Core, mut commands: Vec<Command>, now: Moment) -> Vec<Command> {
    let sessions: Vec<_> = commands
        .iter()
        .filter_map(|command| match command {
            Command::Usage { session, .. } => Some(session.clone()),
            _ => None,
        })
        .collect();
    for session in sessions {
        commands.extend(core.handle(Event::Runtime(RuntimeEvent::Turn { session:session.clone(), update:TurnUpdate::Text("### Quota Limits\n- **Gemini Models**\n  *(Models within this group: Gemini Flash, Gemini Pro)*\n  - **Weekly Limit Remaining:** 75% remaining (resets Oct 24, 2026, 12:34 UTC)".into()) }), now + 1, std::time::UNIX_EPOCH));
        commands.extend(core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session,
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::EndTurn),
                },
            }),
            now + 1,
            std::time::UNIX_EPOCH,
        ));
        commands.extend(core.handle(Event::Deadline, now + 201, std::time::UNIX_EPOCH));
    }
    commands
}

#[test]
fn a_checked_session_is_forgotten_only_when_its_request_does_not_start() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-checked-session-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for (content, closed, forgotten) in [
        ("First", true, true),
        ("First", false, true),
        ("Other", false, false),
    ] {
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.handle(Event::Deadline, 300_000, std::time::UNIX_EPOCH);
        let body = serde_json::json!({"model":"m","messages":[{"role":"user","content":content}]});
        let turn = crate::dialect::CHAT
            .parse(&serde_json::to_vec(&body).unwrap())
            .ok()
            .unwrap()
            .turn;
        let (output, ticket) = mpsc::unbounded_channel();
        let _ticket = if closed {
            drop(ticket);
            None
        } else {
            Some(ticket)
        };
        let accepted = Accepted {
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
        };
        let commands = core.handle(
            Event::Checked {
                id: 99,
                accepted: Some(Box::new(accepted)),
                session: "checked".into(),
            },
            305_000,
            std::time::UNIX_EPOCH,
        );
        assert_eq!(
            commands
                .iter()
                .any(|command| matches!(command, Command::Forget(session) if session == "checked")),
            forgotten,
            "row {content} closed={closed}"
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_session_prepared_after_runtime_failure_is_forgotten() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-late-prepared-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    core.handle(Event::RetireRuntime, 0, std::time::UNIX_EPOCH);
    let work = core.work[&0].key().to_owned();
    core.handle(
        Event::Runtime(RuntimeEvent::Failed("broken pipe".into())),
        1,
        std::time::UNIX_EPOCH,
    );
    let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
    let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
    let commands = core.handle(
        Event::Runtime(RuntimeEvent::Prepared {
            work,
            result: Ok(("late-session".into(), catalog)),
        }),
        2,
        std::time::UNIX_EPOCH,
    );
    std::fs::remove_dir_all(folder).unwrap();
    assert!(matches!(&commands[..], [Command::Forget(session)] if session == "late-session"));
}

#[test]
fn a_tool_call_does_not_make_a_refused_turn_reusable() {
    let folder = std::env::temp_dir().join(format!("acp2api-tool-refusal-{}", std::process::id()));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let mut selected = Vec::new();
    for stop_reason in [
        Some(StopReason::Refusal),
        Some(StopReason::EndTurn),
        Some(StopReason::Cancelled),
        None,
    ] {
        let turn = crate::dialect::CHAT
            .parse(br#"{"model":"gemini-3.8-flash","tools":[{"type":"function","function":{"name":"read","parameters":{}}}],"messages":[{"role":"user","content":"First"}]}"#)
                .ok()
                .unwrap()
                .turn;
        let (output, _ticket) = mpsc::unbounded_channel();
        let record = records::RequestRecord::fixture(
            crate::conversation::record_hashes(&turn.conversation),
            crate::conversation::tool_set_hash(&turn.tools),
            1,
            100_000,
        );
        let accepted = Accepted {
            turn,
            model: crate::catalog::SelectedModel {
                model: "gemini-3.8-flash".into(),
                level: None,
                level_from_name: false,
            },
            request: journal.begin(crate::journal::Api::Chat, 0),
            reply: ResponseChannel {
                output: output.clone(),
                keep_alive: None,
            },
        };
        let mut active = Active::new(
            accepted,
            Some("existing".into()),
            request::Start::Prompt(0),
            record,
            crate::journal::SessionUse::New,
        );
        assert!(matches!(
            active.text_received("Before".into(), 0, &mut Vec::new()),
            request::PromptTurnAction::Continue
        ));
        assert!(matches!(
            active.client_tool_called(crate::conversation::ToolCall {
                name: "read".into(),
                arguments: "{}".into(),
                kind: crate::conversation::ToolKind::Function,
                namespace: None,
            }),
            request::PromptTurnAction::Cancel
        ));
        let mut work = WorkTable::default();
        work.add(Work::AcceptedWorking(active));
        let mut core = Core {
            work,
            sessions: records::AcpSessions::fixture(Vec::new(), None, 1),
            ..Core::fixture(1)
        };
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "existing".into(),
                update: TurnUpdate::PromptEnded { stop_reason },
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        let commands = core.handle(Event::Deadline, 200, std::time::UNIX_EPOCH);
        let call_id = commands
            .iter()
            .find_map(|command| match command {
                Command::Emit(_, crate::conversation::TurnEvent::ToolCall { id, .. }) => {
                    Some(id.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| "unreturned-call".into());
        answer_quota(&mut core, commands, 200);
        let body = r#"{"model":"gemini-3.8-flash","tools":[{"type":"function","function":{"name":"read","parameters":{}}}],"messages":[
                {"role":"user","content":"First"},
                {"role":"assistant","content":"Before","tool_calls":[{"id":"call-id","function":{"name":"read","arguments":"{}"}}]},
                {"role":"tool","tool_call_id":"call-id","content":"Done"},
                {"role":"user","content":"Next"}
        ]}"#.replace("call-id", &call_id);
        let next = crate::dialect::CHAT
            .parse(body.as_bytes())
            .ok()
            .unwrap()
            .turn;
        selected.extend(
            core.handle(
                Event::Accepted(Box::new(Accepted {
                    model: crate::catalog::SelectedModel {
                        model: "gemini-3.8-flash".into(),
                        level: None,
                        level_from_name: false,
                    },
                    turn: next,
                    request: journal.begin(crate::journal::Api::Chat, 200),
                    reply: ResponseChannel {
                        output,
                        keep_alive: None,
                    },
                })),
                200,
                std::time::UNIX_EPOCH,
            )
            .into_iter()
            .filter_map(|command| match command {
                Command::Prepare { .. } => Some("rebuild".to_owned()),
                Command::Prompt { session, .. } => Some(session),
                _ => None,
            }),
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        selected,
        ["rebuild", "existing", "existing", "rebuild"],
        "ADR-0003 excludes a refused turn even when it returned a tool call"
    );
}

#[test]
fn a_session_that_leaves_for_good_is_forgotten() {
    let folder = std::env::temp_dir().join(format!("acp2api-forget-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
    let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
    let mut core = Core::fixture(1);
    let mut outputs = Vec::new();
    let mut forgotten = Vec::new();
    let mut turn = |core: &mut Core, system: &str, session: &str, ends: TurnUpdate| {
        let body = format!(
            r#"{{"model":"gemini-3.8-flash","messages":[{{"role":"system","content":"{system}"}},{{"role":"user","content":"Hi"}}]}}"#
        );
        let turn = crate::dialect::CHAT
            .parse(body.as_bytes())
            .ok()
            .unwrap()
            .turn;
        let request = journal.begin(crate::journal::Api::Chat, 0);
        let work = request.id.clone();
        let (output, ticket) = mpsc::unbounded_channel();
        outputs.push(ticket);
        let mut commands = core.handle(
            Event::Accepted(Box::new(Accepted {
                model: crate::catalog::SelectedModel {
                    model: "gemini-3.8-flash".into(),
                    level: None,
                    level_from_name: false,
                },
                turn,
                request,
                reply: ResponseChannel {
                    output,
                    keep_alive: None,
                },
            })),
            0,
            std::time::UNIX_EPOCH,
        );
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"gemini-3.8-flash","options":[{"value":"gemini-3.8-flash","name":"M"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, clock.wall()).unwrap();
        let prepared = Ok((session.to_owned(), catalog));
        commands.extend(core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work,
                result: prepared,
            }),
            0,
            std::time::UNIX_EPOCH,
        ));
        let session = session.to_owned();
        commands.extend(core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session,
                update: ends,
            }),
            0,
            std::time::UNIX_EPOCH,
        ));
        commands.extend(core.handle(Event::Deadline, 1000, std::time::UNIX_EPOCH));
        commands = answer_quota(core, commands, 1000);
        forgotten.extend(commands.into_iter().filter_map(|command| match command {
            Command::Forget(session) => Some(session),
            _ => None,
        }));
    };
    let ended = |end_turn| TurnUpdate::PromptEnded {
        stop_reason: Some(if end_turn {
            agent_client_protocol_schema::v1::StopReason::EndTurn
        } else {
            agent_client_protocol_schema::v1::StopReason::Refusal
        }),
    };
    turn(
        &mut core,
        "A",
        "s1",
        TurnUpdate::Failed(crate::messages::Message::Upstream("x".into())),
    );
    turn(&mut core, "B", "s2", ended(false));
    turn(&mut core, "C", "s3", ended(true));
    turn(&mut core, "D", "s4", ended(true));
    std::fs::remove_dir_all(&folder).unwrap();
    assert_eq!(
        forgotten,
        ["s1", "s2"],
        "a failed session and a session with no record; records stay above the limit"
    );
}

#[test]
fn an_idle_session_is_forgotten_by_its_24_hour_wake_up() {
    let body = br#"{"model":"m","messages":[{"role":"user","content":"Hi"}]}"#;
    let turn = crate::dialect::CHAT.parse(body).ok().unwrap().turn;
    let mut core = Core {
        sessions: records::AcpSessions::fixture(
            vec![records::Record {
                location: records::Location::Loaded,
                session: "idle".into(),
                record: records::RequestRecord::fixture(
                    crate::conversation::record_hashes(&turn.conversation),
                    crate::conversation::tool_set_hash(&[]),
                    1,
                    24 * 60 * 60 * 1000,
                ),
            }],
            None,
            1,
        ),
        ..Core::fixture(1)
    };
    let mut forgotten = Vec::new();
    if let Some(at) = core.next_deadline()
        && at <= 24 * 60 * 60 * 1000
    {
        for command in core.handle(Event::Deadline, at, std::time::UNIX_EPOCH) {
            if let Command::Forget(session) = command {
                forgotten.push((at, session));
            }
        }
    }
    assert_eq!(
        forgotten,
        [(86_400_000, "idle".to_owned())],
        "an idle session is forgotten at 24 hours with only its scheduled wake-up"
    );
}

#[test]
fn a_failed_session_creation_still_logs_its_session_use() {
    let folder = std::env::temp_dir().join(format!("acp2api-use-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let clock: Arc<dyn Clock> = Arc::new(crate::os::SystemClock::default());
    let journal = crate::journal::Journal::open(&folder, clock.wall()).unwrap();
    let mut core = Core::fixture(1);
    let body = br#"{"model":"m","messages":[{"role":"user","content":"Hi"},{"role":"assistant","content":"Hello"},{"role":"user","content":"Again"}]}"#;
    let turn = crate::dialect::CHAT.parse(body).ok().unwrap().turn;
    let request = journal.begin(crate::journal::Api::Chat, 0);
    let work = request.id.clone();
    let (output, _ticket) = mpsc::unbounded_channel();
    let accepted = Event::Accepted(Box::new(Accepted {
        model: crate::catalog::SelectedModel {
            model: "m".into(),
            level: None,
            level_from_name: false,
        },
        turn,
        request,
        reply: ResponseChannel {
            output,
            keep_alive: None,
        },
    }));
    assert!(matches!(
        &core.handle(accepted, 0, std::time::UNIX_EPOCH)[..],
        [Command::Prepare { .. }]
    ));
    let failed = Event::Runtime(RuntimeEvent::Prepared {
        work,
        result: Err(crate::runtime_driver::PreparationError::Rejected(
            crate::messages::Message::Upstream("session/new failed".into()),
        )),
    });
    for command in core.handle(failed, 0, std::time::UNIX_EPOCH) {
        if let Command::Complete(completion) = command {
            journal.complete(completion, clock.as_ref());
        }
    }
    let lines: String = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    std::fs::remove_dir_all(&folder).unwrap();
    let line: serde_json::Value = serde_json::from_str(lines.trim()).unwrap();
    assert_eq!(line["result"], "failed");
    assert_eq!(line["session"], serde_json::Value::Null);
    assert_eq!(line["session_use"], "rebuild", "FR-056 and FR-021");
}
