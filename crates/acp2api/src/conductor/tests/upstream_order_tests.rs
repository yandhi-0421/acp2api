use super::*;

struct Turn {
    core: Core,
    output: Vec<(Moment, String)>,
    root: PathBuf,
    _tickets: [mpsc::UnboundedReceiver<(TurnEvent, Hold)>; 2],
}

impl Turn {
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
                Command::Emit(_, TurnEvent::End(Err(error))) => {
                    self.output.push((at, error.to_string()));
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

impl Drop for Turn {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn stderr_first_preserves_text_before_the_held_error_chunk() {
    let mut turn = Turn::new();
    turn.error(503, 0);
    turn.text("Sure, ", 50);
    turn.text("503 The service is unavailable.", 100);
    turn.end(110);
    assert_eq!(
        turn.output,
        [
            (100, "Sure, ".into()),
            (
                110,
                "acp2api: upstream error 503 UNAVAILABLE: Synthetic failure".into()
            ),
        ],
        "FR-028 stderr must not overtake unread reply chunks"
    );
}

#[test]
fn stderr_wait_ends_at_the_prompt_end_without_another_200_ms() {
    let mut turn = Turn::new();
    turn.error(503, 10);
    turn.handle(Event::Deadline, 109);
    assert!(turn.output.is_empty(), "FR-028 must wait for unread stdout");
    turn.end(110);
    assert_eq!(
        turn.output,
        [(
            110,
            "acp2api: upstream error 503 UNAVAILABLE: Synthetic failure".into()
        ),],
        "FR-028 the stdout result ends the error wait immediately"
    );
}

#[test]
fn stderr_wait_ends_at_200_ms_even_when_stdout_never_ends() {
    let mut turn = Turn::new();
    turn.error(503, 10);
    turn.text("Sure, ", 20);
    turn.text("Synthetic failure", 30);
    turn.update(TurnUpdate::Activity, 150);
    turn.handle(Event::Deadline, 209);
    assert_eq!(
        turn.output,
        [(30, "Sure, ".into())],
        "FR-028 the error wait lasts 200 ms"
    );
    turn.handle(Event::Deadline, 210);
    assert_eq!(
        turn.output,
        [
            (30, "Sure, ".into()),
            (
                210,
                "acp2api: upstream error 503 UNAVAILABLE: Synthetic failure".into()
            ),
        ],
        "FR-028 later activity must not extend the error wait"
    );
    assert_eq!(
        turn.core.work.places(),
        1,
        "FR-049 an unfinished turn still holds its place"
    );
}

#[test]
fn late_reply_text_starts_lockout_during_the_stderr_wait() {
    let mut turn = Turn::new();
    turn.error(400, 0);
    turn.text("Your account violates the Terms of ", 50);
    turn.text("Service.", 100);
    assert!(
        turn.core.lockout.is_some(),
        "FR-034 failed-turn text after stderr must start lockout during the wait"
    );
}

#[test]
fn late_reply_text_starts_lockout_after_the_response_ended() {
    let mut turn = Turn::new();
    turn.error(400, 0);
    turn.handle(Event::Deadline, 200);
    assert_eq!(
        turn.output.len(),
        1,
        "the response ended before the late text"
    );
    turn.text("Your account violates the Terms of ", 250);
    turn.text("Service.", 260);
    assert!(
        turn.core.lockout.is_some(),
        "FR-034 retiring failed-turn text must still start lockout"
    );
}

#[test]
fn quota_and_sign_in_errors_preserve_the_prefix_when_stderr_arrives_first() {
    for code in [429, 401] {
        let mut turn = Turn::new();
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
    let mut turn = Turn::new();
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
    let (mut turn, mut answer) = Turn::quota_query();
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
