use super::*;

struct PromptTurn {
    core: Core,
    output: Vec<(Moment, String)>,
    queries: std::collections::BTreeSet<String>,
    ends: Vec<(Moment, Result<(), crate::messages::Message>)>,
    root: PathBuf,
    _tickets: [mpsc::UnboundedReceiver<(TurnEvent, Hold)>; 2],
}

impl PromptTurn {
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
            queries: Default::default(),
            ends: Vec::new(),
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
                Command::Emit(_, TurnEvent::End(result)) => {
                    if let Err(error) = &result {
                        self.output.push((at, error.to_string()));
                    }
                    self.ends.push((at, result.map(|_| ())));
                }
                Command::Usage { work, .. } | Command::Prepare { work, .. }
                    if work.starts_with("quota-") =>
                {
                    self.queries.insert(work);
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

    fn query_count(&self) -> usize {
        let mut queries = self.queries.clone();
        for work in self.core.work.values() {
            if let Work::QuotaQueued { query, .. } = work {
                queries.insert(query.work.clone());
            }
        }
        queries.len()
    }

    fn reset(&mut self, journal: &crate::journal::Journal) {
        let (mut core, queued, tickets) = prompting_with_waiter(journal);
        core.handle(Event::Disconnected(queued), 0, std::time::UNIX_EPOCH);
        self.core = core;
        self._tickets = tickets;
        self.output.clear();
        self.queries.clear();
        self.ends.clear();
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

impl Drop for PromptTurn {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

mod chunk_sequences;
mod concurrent;
mod interleavings;
mod quota_cases;
mod runtime_end;
mod signals;
mod stderr_wait;
