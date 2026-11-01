use super::*;

mod build_wait_tests;
mod review_tests;
mod walk_tests;

struct Fixture {
    core: Core,
    journal: crate::journal::Journal,
    folder: PathBuf,
    tickets: Vec<mpsc::UnboundedReceiver<(TurnEvent, Hold)>>,
}

impl Fixture {
    fn new() -> Self {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-session-limit-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, std::time::UNIX_EPOCH).unwrap();
        let mut fixture = Self {
            core: Core::fixture(3),
            journal,
            folder,
            tickets: Vec::new(),
        };
        fixture.core.replaced(Some("startup".into()), None);
        // The startup ACP session plus six session/new calls leave one place below N = 8.
        let (_, commands) = fixture.request("startup", false);
        assert!(
            commands
                .iter()
                .any(|c| matches!(c, Command::Prompt { session, .. } if session == "startup"))
        );
        fixture.end("startup");
        for index in 0..6 {
            let (work, _) = fixture.request(&format!("fill-{index}"), true);
            let session = format!("session-{index}");
            fixture.prepared(&work, Ok(&session));
            fixture.end(&session);
        }
        fixture
    }

    fn event(&mut self, event: Event) -> Vec<Command> {
        self.core.handle(event, 0, std::time::UNIX_EPOCH)
    }

    fn request(&mut self, text: &str, system: bool) -> (String, Vec<Command>) {
        let mut messages = vec![serde_json::json!({"role":"user", "content":text})];
        if system {
            messages.insert(
                0,
                serde_json::json!({"role":"system", "content":"session limit test"}),
            );
        }
        let turn = crate::dialect::CHAT
            .parse(
                &serde_json::to_vec(&serde_json::json!({
                    "model":"m", "messages":messages
                }))
                .unwrap(),
            )
            .ok()
            .unwrap()
            .turn;
        let request = self.journal.begin(crate::journal::Api::Chat, 0);
        let work = request.id.clone();
        let (output, ticket) = mpsc::unbounded_channel();
        self.tickets.push(ticket);
        let commands = self.event(Event::Accepted(Box::new(Accepted {
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
        })));
        (work, commands)
    }

    fn prepared(
        &mut self,
        work: &str,
        result: Result<&str, crate::messages::Message>,
    ) -> Vec<Command> {
        let metadata = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        self.event(Event::Runtime(RuntimeEvent::Prepared {
            work: work.into(),
            result: result
                .map(|session| {
                    (
                        session.into(),
                        crate::catalog::Catalog::decode(&metadata, std::time::UNIX_EPOCH).unwrap(),
                    )
                })
                .map_err(Into::into),
        }))
    }

    fn end(&mut self, session: &str) {
        self.event(Event::Runtime(RuntimeEvent::Turn {
            session: session.into(),
            update: TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::EndTurn),
            },
        }));
        self.core
            .handle(Event::Deadline, 200, std::time::UNIX_EPOCH);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.folder).unwrap();
    }
}

#[test]
fn concurrent_dispatches_reserve_the_last_acp_session_before_prepared() {
    let mut f = Fixture::new();
    let mut commands = f.request("eighth", true).1;
    commands.extend(f.request("ninth", true).1);
    commands.extend(f.request("tenth", true).1);
    assert_eq!(
        (
            commands
                .iter()
                .filter(|c| matches!(c, Command::Prepare { .. }))
                .count(),
            f.core.work.queued()
        ),
        (1, 2),
        "FR-041 pending session/new calls must not pass N = 8"
    );
}

#[test]
fn a_failed_prepared_releases_capacity_for_the_waiting_request() {
    let mut f = Fixture::new();
    let (failed, _) = f.request("fails", true);
    let (waiting, commands) = f.request("waits", true);
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, Command::Prepare { .. })),
        "the ninth ACP session must first wait"
    );
    let commands = f.prepared(
        &failed,
        Err(crate::messages::Message::Upstream(
            "session/new failed".into(),
        )),
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::Prepare { work, .. } if work == &waiting)),
        "a failed Prepared must let the waiter use the released place without a replacement"
    );
    assert!(!f.core.lifecycle.replacement_pending());
}

#[test]
fn a_disconnected_waiter_cancels_session_limit_replacement() {
    let mut f = Fixture::new();
    let (running, _) = f.request("running", true);
    f.prepared(&running, Ok("eighth"));
    let (waiting, _) = f.request("waits", true);
    assert!(
        f.core.lifecycle.replacement_pending(),
        "the new ACP session must request a replacement"
    );
    f.event(Event::Disconnected(waiting));
    f.end("eighth");
    assert!(
        !f.core.lifecycle.replacement_pending()
            && matches!(f.core.lifecycle.state(), RuntimeState::Serving),
        "decision 1: no waiter means no idle stop"
    );
    let (_, commands) = f.request("a later waiter", true);
    assert!(
        f.core.lifecycle.replacement_pending()
            && !commands
                .iter()
                .any(|c| matches!(c, Command::Prepare { .. })),
        "a later new ACP session still needs a replacement"
    );
    assert!(f.core.replacement_ready(0));
    f.tickets.pop();
    assert!(
        !f.core.replacement_ready(0),
        "decision 1: a closed waiter must prevent the stop before its disconnect event arrives"
    );
}

#[test]
fn usage_can_pass_n_while_an_api_request_waits_for_replacement() {
    let mut f = Fixture::new();
    let (running, _) = f.request("running", true);
    f.prepared(&running, Ok("eighth"));
    f.request("waits", true);
    let (reply, _result) = oneshot::channel();
    let commands = f.event(Event::RefreshQuota(reply));
    let work = commands
        .iter()
        .find_map(|c| match c {
            Command::Prepare { work, .. } if work.starts_with("quota-") => Some(work.clone()),
            _ => None,
        })
        .expect(
            "decision 2: /usage must get a session/new beyond N, also during the replacement wait",
        );
    let commands = f.prepared(&work, Ok("usage"));
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, Command::Usage { session, .. } if session == "usage"))
            && f.core.lifecycle.replacement_pending(),
        "the quota query runs while the API request stays queued"
    );
}

#[test]
fn session_limit_replacement_preserves_the_owners_running_sign_in_wait() {
    let mut f = Fixture::new();
    let (running, _) = f.request("running", true);
    f.prepared(&running, Ok("eighth"));
    f.event(Event::StartSignIn);
    f.event(Event::Runtime(RuntimeEvent::SignInAddress(
        "https://example.test/sign-in".into(),
    )));
    f.request("waits", true);
    f.end("eighth");
    assert!(
        f.core
            .sign_in
            .wait()
            .is_some_and(|wait| wait.deadline == 600_000
                && wait.address.as_deref() == Some("https://example.test/sign-in"))
            && !f.core.replacement_ready(0),
        "decision 3: replacement must preserve the owner wait, address, and deadline"
    );
    f.event(Event::Runtime(RuntimeEvent::Authenticated(Ok(()))));
    assert!(
        f.core.replacement_ready(0),
        "the replacement can proceed after sign-in ends"
    );
}

#[test]
fn auxiliary_preparations_count_and_failed_ones_release_their_reservations() {
    for kind in ["models", "sign-in", "usage"] {
        for succeeds in [false, true] {
            let mut f = Fixture::new();
            let (reply, _answer) = oneshot::channel();
            let (connection, _connected) = mpsc::unbounded_channel();
            let commands = match kind {
                "models" => {
                    f.core.catalog = None;
                    f.core.model_names.clear();
                    f.event(Event::ReadModelNames(reply))
                }
                "usage" => f.event(Event::RefreshQuota(reply)),
                "sign-in" => {
                    f.core.sign_in.set_required();
                    let (admission, _pending) = oneshot::channel();
                    let turn = crate::dialect::CHAT.parse(br#"{"model":"m","messages":[{"role":"user","content":"sign-in check"}]}"#).ok().unwrap().turn;
                    f.event(Event::CheckPending(Box::new(PendingRequest {
                        mode: ResponseMode::Body,
                        turn,
                        request: f.journal.begin(crate::journal::Api::Chat, 0),
                        admission,
                        connection,
                    })))
                }
                _ => unreachable!(),
            };
            let work = commands
                .iter()
                .find_map(|command| match command {
                    Command::Prepare { work, .. } => Some(work.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{kind} must dispatch a session/new"));
            f.prepared(
                &work,
                if succeeds {
                    Ok("auxiliary")
                } else {
                    Err(crate::messages::Message::Upstream(
                        "session/new failed".into(),
                    ))
                },
            );
            // A failed ordinary check leaves the independent sign-in condition intact.
            f.core.sign_in.clear_required();
            let (_, commands) = f.request("after auxiliary work", true);
            assert_eq!(
                (
                    commands
                        .iter()
                        .any(|command| matches!(command, Command::Prepare { .. })),
                    f.core.lifecycle.replacement_pending()
                ),
                (!succeeds, succeeds),
                "decision 2: {kind}, success={succeeds}, must count toward N and release only on failure"
            );
        }
    }
}
