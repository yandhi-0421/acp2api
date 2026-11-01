use super::*;
use crate::messages::{HttpError, Message};

const USAGE: &str = "### Quota Limits\n- **G**\n  *(Models within this group: M, N)*\n  - **Weekly Limit Remaining:** 0% remaining (resets 1970-01-01T00:05:00Z)\n";

struct PromptTurns {
    core: Core,
    journal: crate::journal::Journal,
    root: PathBuf,
    tickets: Vec<mpsc::UnboundedReceiver<(TurnEvent, Hold)>>,
    outputs: std::collections::BTreeMap<String, work::Output>,
    keys: std::collections::BTreeMap<String, String>,
    commands: Vec<Command>,
}

impl PromptTurns {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "acp2api-concurrent-errors-{}",
            crate::random_hex::<8>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&root, std::time::UNIX_EPOCH).unwrap();
        Self {
            core: Core::fixture(8),
            journal,
            root,
            tickets: Vec::new(),
            outputs: Default::default(),
            keys: Default::default(),
            commands: Vec::new(),
        }
    }

    fn start(&mut self, session: &str, model: &str) {
        let body =
            serde_json::json!({"model":model,"messages":[{"role":"user","content":session}]});
        let turn = crate::dialect::CHAT
            .parse(&serde_json::to_vec(&body).unwrap())
            .ok()
            .unwrap()
            .turn;
        let request = self.journal.begin(crate::journal::Api::Chat, 0);
        self.keys.insert(session.into(), request.id.clone());
        let (output, ticket) = mpsc::unbounded_channel();
        self.tickets.push(ticket);
        self.outputs.insert(session.into(), output.clone());
        let record = records::RequestRecord::fixture(
            crate::conversation::record_hashes(&turn.conversation),
            crate::conversation::tool_set_hash(&turn.tools),
            1,
            u64::MAX / 2,
        );
        self.core.work.add(Work::AcceptedWorking(Active::new(
            Accepted {
                turn,
                model: crate::catalog::SelectedModel {
                    model: model.into(),
                    level: None,
                    level_from_name: false,
                },
                request,
                reply: ResponseChannel {
                    output,
                    keep_alive: None,
                },
            },
            Some(session.into()),
            request::Start::Prompt(0),
            record,
            crate::journal::SessionUse::New,
        )));
    }

    fn handle(&mut self, event: Event, at: Moment) {
        self.commands.extend(self.core.handle(
            event,
            at,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(at),
        ));
    }

    fn error(&mut self, session: &str, code: u16, at: Moment) {
        self.handle(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: session.into(),
                error: HttpError {
                    code: Some(code),
                    status: "X".into(),
                    message: "original failure".into(),
                },
            }),
            at,
        );
    }

    fn update(&mut self, session: &str, update: TurnUpdate, at: Moment) {
        self.handle(
            Event::Runtime(RuntimeEvent::Turn {
                session: session.into(),
                update,
            }),
            at,
        );
    }

    fn end(&mut self, session: &str, at: Moment) {
        self.update(
            session,
            TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::EndTurn),
            },
            at,
        );
    }

    fn query_count(&self) -> usize {
        let mut queries: std::collections::BTreeSet<_> = self
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::Usage { work, .. } | Command::Prepare { work, .. }
                    if work.starts_with("quota-") =>
                {
                    Some(work.as_str())
                }
                _ => None,
            })
            .collect();
        for work in self.core.work.values() {
            if let Work::QuotaQueued { query, .. } = work {
                queries.insert(&query.work);
            }
        }
        queries.len()
    }
}

impl Drop for PromptTurns {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn max_turn_requests_keeps_the_last_reply_chunk_during_an_unrelated_state() {
    for condition in [0, 429, 401] {
        for before in [false, true] {
            let mut turns = PromptTurns::new();
            turns.start("first", "m");
            turns.start("second", "m");
            if before && condition != 0 {
                turns.error("second", condition, 1);
            }
            turns.update("first", TurnUpdate::Text("first ".into()), 2);
            turns.update("first", TurnUpdate::Text("last".into()), 3);
            turns.update(
                "first",
                TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::MaxTurnRequests),
                },
                10,
            );
            if !before && condition != 0 {
                turns.error("second", condition, 20);
            }
            turns.handle(Event::Deadline, 210);
            let mut text = String::new();
            let mut errors = Vec::new();
            for command in &turns.commands {
                if let Command::Emit(output, event) = command
                    && output.same_channel(&turns.outputs["first"])
                {
                    match event {
                        TurnEvent::Text(chunk) => text.push_str(chunk),
                        TurnEvent::End(Err(error)) => errors.push(error),
                        _ => {}
                    }
                }
            }
            assert_eq!(
                text, "first last",
                "max_turn_requests keeps the held chunk with condition {condition}, before={before}"
            );
            assert!(matches!(&errors[..], [Message::MaxTurnRequests]));
            assert!(matches!(errors[0].row(), crate::messages::Row::Other502));
        }
    }
}

#[test]
fn a_new_quota_error_uses_a_new_query_after_the_previous_query_failed() {
    for failure in ["stderr", "rpc"] {
        let mut turns = PromptTurns::new();
        turns.core.sessions = records::AcpSessions::fixture(Vec::new(), Some("startup".into()), 0);
        turns.start("first", "m");
        turns.start("second", "m");
        turns.error("first", 429, 5);
        turns.handle(Event::Disconnected(turns.keys["first"].clone()), 6);
        turns.error("second", 400, 7);
        turns.handle(Event::Deadline, 207);
        if failure == "stderr" {
            turns.error("startup", 500, 210);
        } else {
            turns.update(
                "startup",
                TurnUpdate::Failed(Message::UpstreamHttp(HttpError {
                    code: Some(500),
                    status: "X".into(),
                    message: "failed usage".into(),
                })),
                210,
            );
        }
        turns.error("second", 429, 220);
        assert_eq!(
            turns.query_count(),
            2,
            "a failed {failure} usage query cannot accept the new wait"
        );
        let work = turns
            .core
            .work
            .values()
            .find_map(|work| match work {
                Work::QuotaWorking {
                    query,
                    phase: QuotaPhase::Preparing,
                    ..
                } => Some(query.work.clone()),
                _ => None,
            })
            .unwrap();
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"},{"value":"n","name":"N"}]}]});
        let catalog = crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap();
        turns.handle(
            Event::Runtime(RuntimeEvent::Prepared {
                work,
                result: Ok(("new-usage".into(), catalog)),
            }),
            221,
        );
        turns.update("new-usage", TurnUpdate::Text(USAGE.into()), 230);
        turns.end("new-usage", 240);
        turns.end("startup", 250);
        turns.handle(Event::Deadline, 470);
        assert!(
            matches!(
                turns.core.refusal(470),
                Some(Message::QuotaWait {
                    retry_after: 300,
                    ..
                })
            ),
            "the second usage result replaces the 60-minute fallback"
        );
    }
}

#[test]
fn reset_queries_merge_only_without_a_response_and_for_the_same_model() {
    for held_response in [false, true] {
        for second_model in ["m", "n"] {
            for read_text in [false, true] {
                let mut turns = PromptTurns::new();
                turns.core.sessions =
                    records::AcpSessions::fixture(Vec::new(), Some("startup".into()), 0);
                turns.start("first", "m");
                turns.start("second", second_model);
                turns.error("first", 429, 5);
                let usage_session = if held_response {
                    turns.end("first", 6);
                    "first"
                } else {
                    turns.handle(Event::Disconnected(turns.keys["first"].clone()), 6);
                    "startup"
                };
                if read_text {
                    turns.update(usage_session, TurnUpdate::Text(USAGE.into()), 7);
                }
                turns.handle(Event::Disconnected(turns.keys["second"].clone()), 8);
                turns.error("second", 429, 9);
                assert_eq!(
                    turns.query_count(),
                    if !held_response && second_model == "m" {
                        1
                    } else {
                        2
                    }
                );
                turns.update(
                    usage_session,
                    TurnUpdate::Text(if read_text { "" } else { USAGE }.into()),
                    20,
                );
                turns.end(usage_session, 30);
                turns.handle(Event::Deadline, 230);
                if held_response {
                    assert_eq!(turns.commands.iter().filter(|c| matches!(c,
                        Command::Emit(output, TurnEvent::End(Err(Message::QuotaWait { .. }))) if output.same_channel(&turns.outputs["first"])
                    )).count(), 1, "the query still completes its own API response");
                } else if second_model == "m" {
                    assert!(
                        matches!(
                            turns.core.refusal(230),
                            Some(Message::QuotaWait {
                                retry_after: 300,
                                ..
                            })
                        ),
                        "a merged query uses the new wait and its usage result"
                    );
                }
            }
        }
    }
}

#[test]
fn process_end_scans_each_reply_and_gives_the_original_error_to_its_response() {
    for ending in ["closed", "failed", "timeout"] {
        let mut turns = PromptTurns::new();
        let options = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        turns.core.catalog =
            Some(crate::catalog::Catalog::decode(&options, std::time::UNIX_EPOCH).unwrap());
        turns.start("ordinary", "m");
        turns.start("failed", "m");
        turns.update("ordinary", TurnUpdate::Text("Total 403".into()), 5);
        turns.update("failed", TurnUpdate::Text("Error 403".into()), 6);
        turns.error("failed", 500, 10);
        turns.handle(
            match ending {
                "closed" => Event::Runtime(RuntimeEvent::Closed),
                "failed" => Event::Runtime(RuntimeEvent::Failed("gone".into())),
                _ => Event::RuntimeFailure,
            },
            50,
        );
        assert_eq!(turns.core.state(), State::Lockout);
        let mut responses = 0;
        for command in &turns.commands {
            if let Command::Emit(output, TurnEvent::End(Err(Message::Lockout(text)))) = command {
                responses += 1;
                assert_eq!(
                    text.original
                        .as_ref()
                        .is_some_and(|original| original.contains("original failure")),
                    output.same_channel(&turns.outputs["failed"]),
                    "only the signalling response gets the original error at {ending}"
                );
            }
        }
        assert_eq!(responses, 2);
        let (reply, mut answer) = oneshot::channel();
        turns.handle(Event::ReadModelNames(reply), 60);
        assert!(
            matches!(answer.try_recv(), Ok(Ok(()))),
            "the owner can still read the cached model names during lockout"
        );
    }
}
