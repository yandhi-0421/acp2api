use super::*;

#[test]
fn an_idle_prompt_failure_starts_the_existing_retry_wait() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-idle-retry-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _tickets) = prompting_with_waiter(&journal);
    let turn = crate::dialect::CHAT
        .parse(br#"{"model":"m","messages":[{"role":"user","content":"First"}]}"#)
        .ok()
        .unwrap()
        .turn;
    core.handle(Event::Deadline, 300_000, std::time::UNIX_EPOCH);
    let actual = core.check(&turn, 305_000).err().map(|error| {
        let seconds = match error.row() {
            crate::messages::Row::Temporary503(seconds) => Some(seconds),
            _ => None,
        };
        (seconds, error.message.to_string())
    });
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(actual, Some((Some(25), "acp2api: a request of this conversation failed less than 30 s ago. Send the request again in 25 s. The error was: the official runtime sent no update for 5 minutes, so acp2api cancelled the prompt turn.".into())), "FR-029: M-19 uses the existing 503-row retry wait");
}

#[test]
fn runtime_failure_keeps_retry_model_resolution_after_clearing_the_catalog() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-retry-catalog-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    for catalog_at_admission in [false, true] {
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        core.work.clear();
        core.sessions.replaced(None);
        let metadata = serde_json::json!({
            "configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}],
            "models":{"availableModels":[{"modelId":"m-low"},{"modelId":"m-high"}]}
        });
        core.catalog = catalog_at_admission
            .then(|| crate::catalog::Catalog::decode(&metadata, std::time::UNIX_EPOCH).unwrap());
        let turn = |model: &str, effort: &str| {
            crate::dialect::CHAT
                .parse(
                    &serde_json::to_vec(
                        &serde_json::json!({"model":model,"reasoning_effort":effort,
                "messages":[{"role":"user","content":"First"}]}),
                    )
                    .unwrap(),
                )
                .ok()
                .unwrap()
                .turn
        };
        let clock = crate::Ports::production().clock;
        let request = journal.begin(crate::journal::Api::Chat, 0);
        let work = request.id.clone();
        let (admission, mut admitted) = oneshot::channel();
        let (accepted, _) = core
            .admit(
                TurnWork {
                    pending: PendingRequest {
                        mode: ResponseMode::Body,
                        turn: turn("m", "high"),
                        request,
                        admission,
                        connection: mpsc::unbounded_channel().0,
                    },
                    hold: clock.hold(HoldKind::TurnEvent),
                },
                &journal,
                clock.as_ref(),
            )
            .unwrap();
        let _seed = admitted.try_recv().unwrap().ok().unwrap();
        core.handle(accepted, 0, std::time::UNIX_EPOCH);
        core.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work,
                result: Ok((
                    "failed".into(),
                    crate::catalog::Catalog::decode(&metadata, std::time::UNIX_EPOCH).unwrap(),
                )),
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        assert!(
            core.catalog.is_some(),
            "catalog loads before the M-20 failure"
        );
        core.handle(Event::RuntimeFailure, 0, std::time::UNIX_EPOCH);
        assert!(core.catalog.is_none(), "M-20 clears the live catalog");
        let actual: Vec<_> = [("m", "high"), ("m-high", "low"), ("m", "low")]
            .into_iter()
            .map(|(model, effort)| {
                let (admission, mut answer) = oneshot::channel();
                let _ = core.admit(
                    TurnWork {
                        pending: PendingRequest {
                            mode: ResponseMode::Body,
                            turn: turn(model, effort),
                            request: journal.begin(crate::journal::Api::Chat, 0),
                            admission,
                            connection: mpsc::unbounded_channel().0,
                        },
                        hold: clock.hold(HoldKind::TurnEvent),
                    },
                    &journal,
                    clock.as_ref(),
                );
                match answer.try_recv().unwrap() {
                    Err(error)
                        if matches!(error.message, crate::messages::Message::RetryWait { .. }) =>
                    {
                        "retry_wait"
                    }
                    Err(_) => "other error",
                    Ok(_) => "accepted",
                }
            })
            .collect();
        assert_eq!(
            actual,
            ["retry_wait", "retry_wait", "accepted"],
            "FR-029 resolves the same model ID without the live catalog; catalog at admission: {catalog_at_admission}"
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn retry_wait_keeps_the_client_tool_set_when_tool_calls_are_disabled() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-retry-tools-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let (mut core, _, _) = prompting_with_waiter(&journal);
    core.work.clear();
    let mut tickets = Vec::new();
    let mut request = |tool: &str| {
        let body = serde_json::json!({
            "model":"m", "tool_choice":"none",
            "tools":[{"type":"function","function":{"name":tool,"parameters":{}}}],
            "messages":[{"role":"user","content":"Read a file"}]
        });
        let turn = crate::dialect::CHAT
            .parse(&serde_json::to_vec(&body).unwrap())
            .ok()
            .unwrap()
            .turn;
        let (output, ticket) = mpsc::unbounded_channel();
        tickets.push(ticket);
        Accepted {
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
        }
    };
    let seed = request("read_file");
    let seed_id = seed.request.id.clone();
    core.handle(Event::Accepted(Box::new(seed)), 0, std::time::UNIX_EPOCH);
    core.handle(
        Event::StartFailed(
            seed_id,
            crate::messages::Message::Upstream("acp2api: session creation failed".into()),
        ),
        0,
        std::time::UNIX_EPOCH,
    );
    let same = core.handle(
        Event::Accepted(Box::new(request("read_file"))),
        5000,
        std::time::UNIX_EPOCH,
    );
    let changed = core.handle(
        Event::Accepted(Box::new(request("list_files"))),
        5000,
        std::time::UNIX_EPOCH,
    );
    assert_eq!(
        (
            same.iter()
                .filter(|command| matches!(
                    command,
                    Command::Emit(
                        _,
                        TurnEvent::End(Err(crate::messages::Message::RetryWait { .. }))
                    )
                ))
                .count(),
            changed
                .iter()
                .filter(|command| matches!(command, Command::Prepare { .. }))
                .count(),
        ),
        (1, 1),
        "FR-029 keeps the request's client tool set in the retry match"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_call_id_failure_starts_a_502_retry_wait_before_completion() {
    let folder = std::env::temp_dir().join(format!(
        "acp2api-call-id-retry-{}",
        crate::random_hex::<12>().unwrap()
    ));
    let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
    let turn = crate::dialect::CHAT
        .parse(br#"{"model":"m","messages":[{"role":"user","content":"First"}]}"#)
        .ok()
        .unwrap()
        .turn;
    let mut actual = Vec::new();
    for fail in [false, true] {
        let (mut core, _, _tickets) = prompting_with_waiter(&journal);
        let Work::AcceptedWorking(active) = core.work.get_mut(&0).unwrap() else {
            panic!("the first request has an active prompt")
        };
        assert!(matches!(
            active.client_tool_called(crate::conversation::ToolCall {
                name: "read".into(),
                arguments: "{}".into(),
                kind: crate::conversation::ToolKind::Function,
                namespace: None,
            }),
            request::PromptTurnAction::Cancel
        ));
        core.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::EndTurn),
                },
            }),
            0,
            std::time::UNIX_EPOCH,
        );
        let Work::AcceptedWorking(active) = core.work.remove(&0).unwrap() else {
            panic!("the completed prompt awaits finalization")
        };
        let (commands, wait, record) = active.finish(Ok(()), 200, None, |_| {
            if fail {
                Err("random-source failure".into())
            } else {
                Ok("call-id".into())
            }
        });
        let waits: Vec<_> = wait.into_iter().collect();
        let seconds = match retry::refusal(&waits, &turn, 5_200) {
            Some(crate::messages::Message::RetryWait { seconds, error })
                if matches!(error.row(), crate::messages::Row::Other502) =>
            {
                Some(seconds)
            }
            Some(_) => panic!("the random-source error must retain its 502 row"),
            None => None,
        };
        let tool_calls = commands
            .iter()
            .filter(|command| matches!(command, Command::Emit(_, TurnEvent::ToolCall { .. })))
            .count();
        let error = commands.iter().find_map(|command| match command {
            Command::Emit(_, TurnEvent::End(Err(error))) => Some(error.to_string()),
            _ => None,
        });
        actual.push((seconds, record.is_some(), tool_calls, error));
        assert!(
            retry::refusal(&waits, &turn, 30_200).is_none(),
            "the wait expires after 30 s"
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
    assert_eq!(
        actual,
        [
            (None, true, 1, None),
            (Some(25), false, 0, Some("random-source failure".into())),
        ],
        "FR-029 includes a 502 failure while making the call ID"
    );
}
