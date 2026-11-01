use super::*;

struct Fixture {
    core: Core,
    journal: crate::journal::Journal,
    folder: PathBuf,
    tickets: Vec<mpsc::UnboundedReceiver<(TurnEvent, Hold)>>,
}

impl Fixture {
    fn new() -> Self {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-call-limit-{}",
            crate::random_hex::<12>().unwrap()
        ));
        Self {
            core: Core::fixture(3),
            journal: crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap(),
            folder,
            tickets: Vec::new(),
        }
    }

    fn event(&mut self, event: Event, now: Moment) -> Vec<Command> {
        self.core.handle(
            event,
            now,
            std::time::UNIX_EPOCH + std::time::Duration::from_millis(now),
        )
    }

    fn request(&mut self, messages: serde_json::Value, now: Moment) -> String {
        let turn = crate::dialect::CHAT
            .parse(
                &serde_json::to_vec(&serde_json::json!({"model":"m", "messages":messages}))
                    .unwrap(),
            )
            .ok()
            .unwrap()
            .turn;
        let request = self.journal.begin(crate::journal::Api::Chat, now);
        let work = request.id.clone();
        let (output, ticket) = mpsc::unbounded_channel();
        self.tickets.push(ticket);
        let commands = self.event(
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
            now,
        );
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, Command::Prepare { work: key, .. } if key == &work))
        );
        work
    }

    fn prepare(&mut self, work: &str, session: &str, now: Moment) -> Vec<Command> {
        let metadata = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        self.event(
            Event::Runtime(RuntimeEvent::Prepared {
                work: work.into(),
                result: Ok((
                    session.into(),
                    crate::catalog::Catalog::decode(&metadata, std::time::UNIX_EPOCH).unwrap(),
                )),
            }),
            now,
        )
    }

    fn update(&mut self, session: &str, update: TurnUpdate, now: Moment) -> Vec<Command> {
        self.event(
            Event::Runtime(RuntimeEvent::Turn {
                session: session.into(),
                update,
            }),
            now,
        )
    }

    fn end(&mut self, session: &str, now: Moment) {
        self.update(
            session,
            TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::EndTurn),
            },
            now,
        );
        self.event(Event::Deadline, now + 200);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.folder).unwrap();
    }
}

fn runtime_error(commands: &[Command]) -> bool {
    commands.iter().any(|c| {
        matches!(
            c,
            Command::Emit(
                _,
                TurnEvent::End(Err(crate::messages::Message::RuntimeFailed))
            )
        )
    })
}

#[test]
fn preparing_fails_at_five_minutes_from_send() {
    let mut f = Fixture::new();
    f.request(
        serde_json::json!([{"role":"user","content":"Prepare"}]),
        10_000,
    );
    let early = f.event(Event::Deadline, 309_999);
    assert!(!runtime_error(&early));
    assert!(matches!(f.core.lifecycle.state(), RuntimeState::Serving));
    let due = f.event(Event::Deadline, 310_000);
    assert!(
        runtime_error(&due),
        "FR-041: a preparation ends with M-20 at its call limit"
    );
    assert!(matches!(f.core.lifecycle.state(), RuntimeState::Restarting));
    assert_eq!(
        f.core.lifecycle.failure_count(),
        1,
        "the call limit counts as an FR-041 failure"
    );
}

#[test]
fn checking_fails_before_acceptance_at_its_call_limit() {
    let mut f = Fixture::new();
    f.core.sign_in.set_required();
    let turn = crate::dialect::CHAT
        .parse(br#"{"model":"m","messages":[{"role":"user","content":"Check"}]}"#)
        .ok()
        .unwrap()
        .turn;
    let (admission, _reply) = oneshot::channel();
    let (connection, _lease) = mpsc::unbounded_channel();
    f.event(
        Event::CheckPending(Box::new(PendingRequest {
            mode: ResponseMode::Body,
            turn,
            request: f.journal.begin(crate::journal::Api::Chat, 10_000),
            admission,
            connection,
        })),
        10_000,
    );
    let due = f.event(Event::Deadline, 310_000);
    assert!(
        due.iter().any(|c| matches!(
            c,
            Command::RefusePending {
                error: crate::messages::Message::RuntimeFailed,
                ..
            }
        )),
        "FR-041: a check with no answer gets M-20 before acceptance"
    );
    assert!(matches!(f.core.lifecycle.state(), RuntimeState::Restarting));
}

#[test]
fn models_reading_reports_m20_at_its_call_limit() {
    let mut f = Fixture::new();
    let (reply, _result) = oneshot::channel();
    f.event(Event::ReadModelNames(reply), 10_000);
    let due = f.event(Event::Deadline, 310_000);
    assert!(
        due.iter().any(|c| matches!(
            c,
            Command::ModelsRead {
                result: Err(crate::messages::OwnerMessage::Api(
                    crate::messages::Message::RuntimeFailed
                )),
                ..
            }
        )),
        "FR-041: the owner gets M-20 for a model read with no answer"
    );
    assert!(matches!(f.core.lifecycle.state(), RuntimeState::Restarting));
}

#[test]
fn quota_working_gives_preparation_and_usage_separate_limits() {
    let mut f = Fixture::new();
    let (reply, mut result) = oneshot::channel();
    let commands = f.event(Event::RefreshQuota(reply), 10_000);
    let work = commands
        .iter()
        .find_map(|c| match c {
            Command::Prepare { work, .. } => Some(work.clone()),
            _ => None,
        })
        .unwrap();
    let commands = f.prepare(&work, "usage", 309_999);
    assert!(commands.iter().any(|c| matches!(c, Command::Usage { .. })));
    f.update("usage", TurnUpdate::Activity, 549_999);
    f.event(Event::Deadline, 609_998);
    assert!(matches!(f.core.lifecycle.state(), RuntimeState::Serving));
    f.event(Event::Deadline, 609_999);
    let error = result
        .try_recv()
        .ok()
        .and_then(Result::err)
        .map(|error| error.to_string());
    assert_eq!(
        error.as_deref(),
        Some("acp2api: the runtime process failed, so acp2api ended the prompt turn.")
    );
    assert!(matches!(f.core.lifecycle.state(), RuntimeState::Restarting));
}

#[test]
fn retiring_keeps_a_disconnected_preparations_limit() {
    let mut f = Fixture::new();
    let work = f.request(
        serde_json::json!([{"role":"user","content":"Disconnect"}]),
        10_000,
    );
    f.tickets.clear();
    f.event(Event::Disconnected(work.clone()), 20_000);
    assert!(matches!(
        f.core.work[&f.core.work.by_request(&work).unwrap()],
        Work::Retiring(_)
    ));
    f.event(Event::Deadline, 310_000);
    assert!(
        matches!(f.core.lifecycle.state(), RuntimeState::Restarting),
        "FR-041: a disconnect cannot remove the outstanding call limit"
    );
    assert_eq!(f.core.work.places(), 0);
}

#[test]
fn awaiting_quota_preparation_keeps_its_call_limit_and_stored_quota_error() {
    let mut f = Fixture::new();
    let work = f.request(serde_json::json!([{"role":"user","content":"First"}]), 0);
    f.prepare(&work, "saved", 1);
    f.update("saved", TurnUpdate::Text("Reply".into()), 2);
    f.end("saved", 3);
    f.core.sessions.unload(|_| PathBuf::from("saved-cwd"));
    f.core.replaced(None, None);
    f.request(
        serde_json::json!([
            {"role":"user","content":"First"},
            {"role":"assistant","content":"Reply"},
            {"role":"user","content":"Next"}
        ]),
        10_000,
    );
    assert!(
        f.core.work.by_session("saved").is_some(),
        "the call loads the saved ACP session"
    );
    f.event(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "saved".into(),
            error: quota_error_without_reset_time(),
        }),
        20_000,
    );
    let due = f.event(Event::Deadline, 310_000);
    assert!(
        matches!(f.core.lifecycle.state(), RuntimeState::Restarting),
        "FR-041: AwaitingQuotaPreparation must not hold a place forever"
    );
    assert!(
        due.iter().any(|c| matches!(
            c,
            Command::Emit(
                _,
                TurnEvent::End(Err(crate::messages::Message::QuotaWait {
                    retry_after: 3310,
                    ..
                }))
            )
        )),
        "FR-037: the stored quota error keeps the 60-minute fallback from the signal"
    );
}

#[test]
fn an_answer_clears_the_preparation_limit_for_success_and_failure() {
    for success in [false, true] {
        let mut f = Fixture::new();
        let work = f.request(serde_json::json!([{"role":"user","content":"Answer"}]), 0);
        if success {
            f.prepare(&work, "answered", 299_999);
        } else {
            f.event(
                Event::StartFailed(work, crate::messages::Message::RuntimeFailed),
                299_999,
            );
        }
        let commands = f.event(Event::Deadline, 360_000);
        assert!(!runtime_error(&commands));
        assert!(
            matches!(f.core.lifecycle.state(), RuntimeState::Serving),
            "the answered call must not fail later"
        );
    }
}

#[test]
fn a_usage_answer_clears_its_limit() {
    let mut f = Fixture::new();
    f.core.replaced(Some("usage".into()), None);
    let (reply, _result) = oneshot::channel();
    f.event(Event::RefreshQuota(reply), 0);
    f.end("usage", 299_999);
    f.event(Event::Deadline, 360_000);
    assert!(matches!(f.core.lifecycle.state(), RuntimeState::Serving));
    assert_eq!(f.core.next_deadline(), None);
}

#[test]
fn a_replacement_clears_old_limits_and_ignores_a_late_preparation() {
    let mut f = Fixture::new();
    let old = f.request(serde_json::json!([{"role":"user","content":"Old"}]), 0);
    f.event(Event::Runtime(RuntimeEvent::Closed), 10_000);
    assert_eq!(
        f.core.next_deadline(),
        Some(40_000),
        "only the retry wait remains after failure"
    );
    f.core.replaced(None, None);
    f.event(
        Event::RuntimeReady {
            sign_in_required: false,
        },
        20_000,
    );
    f.event(Event::Deadline, 40_000);
    assert_eq!(f.core.next_deadline(), None);
    f.request(serde_json::json!([{"role":"user","content":"New"}]), 40_000);
    let late = f.prepare(&old, "late", 50_000);
    assert!(matches!(&late[..], [Command::Forget(session)] if session == "late"));
    assert!(f.core.sessions.records().is_empty());
    assert!(f.core.catalog.is_none());
    assert_eq!(f.core.next_deadline(), Some(340_000));
    f.event(Event::Deadline, 300_000);
    assert!(matches!(f.core.lifecycle.state(), RuntimeState::Serving));
    assert!(runtime_error(&f.event(Event::Deadline, 340_000)));
}
