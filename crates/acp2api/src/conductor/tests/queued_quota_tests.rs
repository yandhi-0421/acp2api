use super::*;
use crate::messages::Message;
use std::time::{Duration, UNIX_EPOCH};

struct Fixture {
    core: Core,
    folder: PathBuf,
    output: super::super::work::Output,
    _tickets: [mpsc::UnboundedReceiver<(TurnEvent, Hold)>; 2],
}

impl Fixture {
    fn new() -> Self {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-queued-query-{}",
            crate::random_hex::<12>().unwrap()
        ));
        let journal = crate::journal::Journal::open(&folder, UNIX_EPOCH).unwrap();
        let (core, _waiter, tickets) = prompting_with_waiter(&journal);
        let output = core.work[&0].reply().unwrap().output.clone();
        Self {
            core,
            folder,
            output,
            _tickets: tickets,
        }
    }

    fn event(&mut self, event: Event, now: Moment) -> Vec<Command> {
        self.core
            .handle(event, now, UNIX_EPOCH + Duration::from_millis(now))
    }

    fn sign_in_required(&mut self, now: Moment) -> Vec<Command> {
        let mut commands = self.event(Event::StartSignIn, now);
        commands.extend(self.event(
            Event::Runtime(RuntimeEvent::Authenticated(Err(Message::UpstreamHttp(
                crate::messages::HttpError {
                    code: Some(401),
                    status: "UNAUTHENTICATED".into(),
                    message: "Sign in required".into(),
                },
            )))),
            now,
        ));
        commands
    }

    fn prepare(&mut self, work: String, session: &str, now: Moment) -> Vec<Command> {
        let metadata = serde_json::json!({"configOptions":[{"id":"model","name":"Model","type":"select","currentValue":"m","options":[{"value":"m","name":"M"}]}]});
        self.event(
            Event::Runtime(RuntimeEvent::Prepared {
                work,
                result: Ok((
                    session.into(),
                    crate::catalog::Catalog::decode(&metadata, UNIX_EPOCH).unwrap(),
                )),
            }),
            now,
        )
    }

    fn occupy_waiter(&mut self) {
        let commands = self.event(Event::Limit(2), 0);
        let work = commands
            .into_iter()
            .find_map(|command| match command {
                Command::Prepare { work, .. } => Some(work),
                _ => None,
            })
            .expect("the second request takes the second place");
        self.prepare(work, "other", 0);
        self.event(Event::Limit(1), 0);
    }

    fn end_other(&mut self, now: Moment) -> Vec<Command> {
        let mut commands = self.event(
            Event::Runtime(RuntimeEvent::Turn {
                session: "other".into(),
                update: TurnUpdate::PromptEnded {
                    stop_reason: Some(StopReason::EndTurn),
                },
            }),
            now,
        );
        commands.extend(self.event(Event::Deadline, now + 200));
        commands
    }

    fn finish_response(&mut self, reset: bool, now: Moment) -> Vec<Command> {
        let update = if reset {
            TurnUpdate::Failed(Message::UpstreamHttp(quota_error_without_reset_time()))
        } else {
            TurnUpdate::PromptEnded {
                stop_reason: Some(StopReason::Refusal),
            }
        };
        let mut commands = self.event(
            Event::Runtime(RuntimeEvent::Turn {
                session: "running".into(),
                update,
            }),
            now,
        );
        commands.extend(self.event(Event::Deadline, now + 200));
        commands
    }

    fn errors<'a>(&self, commands: &'a [Command]) -> Vec<&'a Message> {
        commands
            .iter()
            .filter_map(|command| match command {
                Command::Emit(output, TurnEvent::End(Err(error)))
                    if output.same_channel(&self.output) =>
                {
                    Some(error)
                }
                _ => None,
            })
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.folder).unwrap();
    }
}

#[test]
fn sign_in_required_ends_a_response_whose_refusal_query_is_queued_later() {
    let mut fixture = Fixture::new();
    fixture.sign_in_required(1);
    let commands = fixture.finish_response(false, 2);
    assert!(
        matches!(fixture.errors(&commands).as_slice(), [Message::SignInRequired(address)]
        if address == "http://127.0.0.1:8329/#sign-in"),
        "FR-037 ends the held response with M-13 when its query cannot start"
    );
    assert_eq!(
        (
            fixture.core.view.borrow().places,
            fixture.core.view.borrow().queued
        ),
        (0, 0)
    );
    assert_eq!(
        commands
            .iter()
            .filter(|c| matches!(c, Command::Forget(session) if session == "running"))
            .count(),
        1
    );
    let later = fixture.event(Event::Deadline, 3_600_202);
    assert!(
        fixture.errors(&later).is_empty(),
        "the response ends only once"
    );
}

#[test]
fn runtime_unavailable_ends_a_response_whose_refusal_query_is_queued_later() {
    let mut fixture = Fixture::new();
    fixture.event(
        Event::RuntimeUnavailable(UnavailableReason {
            message: crate::messages::Unavailable::no_executable(),
            cause: None,
        }),
        1,
    );
    let commands = fixture.finish_response(false, 2);
    assert!(
        matches!(fixture.errors(&commands).as_slice(), [Message::RuntimeUnavailable(reason)]
            if matches!(reason.fault, crate::messages::RuntimeFault::NoExecutable)),
        "FR-037 ends the held response with M-14 when its query cannot start"
    );
    assert_eq!(
        (
            fixture.core.view.borrow().places,
            fixture.core.view.borrow().queued
        ),
        (0, 0)
    );
    assert_eq!(
        commands
            .iter()
            .filter(|c| matches!(c, Command::Forget(session) if session == "running"))
            .count(),
        1
    );
    let later = fixture.event(Event::Deadline, 3_600_202);
    assert!(
        fixture.errors(&later).is_empty(),
        "the response ends only once"
    );
}

#[test]
fn a_late_reset_query_ends_its_response_but_keeps_the_quota_wait() {
    for unavailable in [false, true] {
        let mut fixture = Fixture::new();
        if unavailable {
            fixture.event(
                Event::RuntimeUnavailable(UnavailableReason {
                    message: crate::messages::Unavailable::no_executable(),
                    cause: None,
                }),
                1,
            );
        } else {
            fixture.sign_in_required(1);
        }
        let commands = fixture.finish_response(true, 2);
        assert!(
            match fixture.errors(&commands).as_slice() {
                [Message::SignInRequired(_)] => !unavailable,
                [Message::RuntimeUnavailable(_)] => unavailable,
                _ => false,
            },
            "FR-037 ends a reset query's response with the active condition's error"
        );
        assert_eq!(
            fixture.core.view.borrow().quota_reset.as_deref(),
            Some("1970-01-01T01:00:00Z"),
            "FR-036 keeps the fallback after the blocked query ends"
        );
        assert_eq!(
            (
                fixture.core.view.borrow().places,
                fixture.core.view.borrow().queued
            ),
            (0, 0)
        );
        assert_eq!(
            commands
                .iter()
                .filter(|c| matches!(c, Command::Forget(session) if session == "running"))
                .count(),
            1
        );
    }
}

#[test]
fn sign_in_required_ends_an_already_queued_query_even_when_all_places_are_occupied() {
    for reset in [false, true] {
        let mut fixture = Fixture::new();
        fixture.occupy_waiter();
        fixture.finish_response(reset, 1);
        assert_eq!(
            (
                fixture.core.view.borrow().places,
                fixture.core.view.borrow().queued
            ),
            (1, 1)
        );
        let commands = fixture.sign_in_required(202);
        assert!(
            matches!(
                fixture.errors(&commands).as_slice(),
                [Message::SignInRequired(_)]
            ),
            "FR-037 ends an already queued response without waiting for a free place"
        );
        assert_eq!(
            (
                fixture.core.view.borrow().places,
                fixture.core.view.borrow().queued
            ),
            (1, 0)
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Cancel(_))),
            "FR-037 keeps the other prompt turn running"
        );
        let finished = fixture.end_other(203);
        assert!(
            finished
                .iter()
                .any(|command| matches!(command, Command::Emit(_, TurnEvent::End(Ok(_)))))
        );
        assert_eq!(fixture.core.view.borrow().places, 0);
    }
}

#[test]
fn a_queued_query_without_a_response_forgets_its_session_once_and_releases_the_queue() {
    for (reset, unavailable) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut fixture = Fixture::new();
        fixture.occupy_waiter();
        let request = fixture.core.work[&0].key().to_owned();
        let mut commands = fixture.finish_response(reset, 1);
        let query = fixture
            .core
            .work
            .values()
            .find(|work| matches!(work, Work::QuotaQueued { .. }))
            .unwrap()
            .key()
            .to_owned();
        fixture._tickets[0].close();
        commands.extend(fixture.event(Event::Disconnected(query), 202));
        if unavailable {
            commands.extend(fixture.event(
                Event::RuntimeUnavailable(UnavailableReason {
                    message: crate::messages::Unavailable::no_executable(),
                    cause: None,
                }),
                203,
            ));
        } else {
            commands.extend(fixture.sign_in_required(203));
        }
        commands.extend(fixture.end_other(204));
        commands.extend(fixture.event(Event::Deadline, 3_600_204));
        assert_eq!(
            (
                commands
                    .iter()
                    .filter(|c| matches!(c, Command::Forget(session) if session == "running"))
                    .count(),
                commands
                    .iter()
                    .filter(|c| matches!(c, Command::Complete(done) if done.request.id == request))
                    .count(),
                fixture.core.view.borrow().places,
                fixture.core.view.borrow().queued,
            ),
            (1, 1, 0, 0),
            "a detached query releases its ACP session once, without a second completion or occupied place"
        );
        assert!(
            fixture.errors(&commands).is_empty(),
            "the disconnected response gets no error event"
        );
    }
}

#[test]
fn overlapping_conditions_select_runtime_unavailable_for_a_held_quota_response() {
    let mut fixture = Fixture::new();
    fixture.sign_in_required(1);
    fixture.event(
        Event::RuntimeUnavailable(UnavailableReason {
            message: crate::messages::Unavailable::no_executable(),
            cause: None,
        }),
        2,
    );
    let commands = fixture.finish_response(true, 3);
    assert!(
        matches!(fixture.errors(&commands).as_slice(), [Message::RuntimeUnavailable(reason)]
        if matches!(reason.fault, crate::messages::RuntimeFault::NoExecutable)),
        "FR-037 selects runtime unavailable above sign-in required and quota wait"
    );
    assert_eq!(fixture.core.view.borrow().state, State::RuntimeUnavailable);
    fixture.event(Event::ResumeRuntime, 204);
    assert_eq!(fixture.core.view.borrow().state, State::SignInRequired);
    assert_eq!(
        fixture.core.view.borrow().quota_reset.as_deref(),
        Some("1970-01-01T01:00:00Z")
    );
}

#[test]
fn a_condition_that_ends_before_the_query_is_queued_does_not_refuse_it() {
    for (unavailable, reset) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut fixture = Fixture::new();
        if unavailable {
            fixture.event(
                Event::RuntimeUnavailable(UnavailableReason {
                    message: crate::messages::Unavailable::no_executable(),
                    cause: None,
                }),
                1,
            );
        } else {
            fixture.sign_in_required(1);
        }
        fixture.event(
            Event::RuntimeReady {
                sign_in_required: false,
            },
            2,
        );
        let commands = fixture.finish_response(reset, 3);
        assert!(
            commands.iter().any(
                |command| matches!(command, Command::Usage { session, .. } if session == "running")
            ),
            "a cleared condition permits the query, including a reset query during quota wait"
        );
        assert!(
            fixture.errors(&commands).is_empty(),
            "the response waits for its running query"
        );
        assert_eq!(
            (
                fixture.core.view.borrow().places,
                fixture.core.view.borrow().queued
            ),
            (1, 0)
        );
    }
}

#[test]
fn quota_queries_keep_their_arrival_order_with_a_response_behind_an_owner_query() {
    let dispatches = |commands: &[Command]| -> Vec<String> {
        commands
            .iter()
            .filter_map(|command| match command {
                Command::Prepare { .. } => Some("prepare".into()),
                Command::Usage { session, .. } => Some(format!("usage:{session}")),
                _ => None,
            })
            .collect()
    };
    let mut fixture = Fixture::new();
    fixture.occupy_waiter();
    let (reply, _result) = oneshot::channel();
    fixture.event(Event::RefreshQuota(reply), 1);
    fixture.finish_response(false, 2);
    assert_eq!(
        (
            fixture.core.view.borrow().places,
            fixture.core.view.borrow().queued
        ),
        (1, 2)
    );
    let first = fixture.event(Event::Limit(2), 203);
    assert_eq!(
        dispatches(&first),
        ["prepare"],
        "FR-050 dispatches the older owner query first"
    );
    let work = first
        .into_iter()
        .find_map(|command| match command {
            Command::Prepare { work, .. } => Some(work),
            _ => None,
        })
        .unwrap();
    let mut commands = fixture.prepare(work, "refresh", 204);
    commands.extend(fixture.end_other(205));
    assert_eq!(
        dispatches(&commands),
        ["usage:refresh", "usage:running"],
        "the response's query follows the older query when a place becomes free"
    );
}

#[test]
fn a_reset_query_still_runs_during_quota_wait_and_updates_the_response_reset_time() {
    let mut fixture = Fixture::new();
    fixture.occupy_waiter();
    fixture.finish_response(true, 1);
    assert_eq!(
        (
            fixture.core.view.borrow().places,
            fixture.core.view.borrow().queued
        ),
        (1, 1)
    );
    let mut commands = fixture.end_other(202);
    assert!(
        commands.iter().any(
            |command| matches!(command, Command::Usage { session, .. } if session == "running")
        ),
        "FR-036 permits the reset query during quota wait"
    );
    for (now, update) in [
        (403, TurnUpdate::Text("### Quota Limits\n- **M**\n  *(Models within this group: M)*\n  - **Five Hour Limit Remaining:** 0% remaining (resets 1970-01-01T00:10:00Z)".into())),
        (404, TurnUpdate::PromptEnded { stop_reason: Some(StopReason::EndTurn) }),
    ] {
        commands.extend(fixture.event(Event::Runtime(RuntimeEvent::Turn { session: "running".into(), update }), now));
    }
    commands.extend(fixture.event(Event::Deadline, 604));
    assert!(
        matches!(fixture.errors(&commands).as_slice(), [Message::QuotaWait { until, retry_after: 600 }]
        if until == "1970-01-01T00:10:00Z"),
        "FR-036 uses the queried reset time in the held response"
    );
    assert_eq!(
        fixture.core.view.borrow().quota_reset.as_deref(),
        Some("1970-01-01T00:10:00Z")
    );
    assert_eq!(
        (
            fixture.core.view.borrow().places,
            fixture.core.view.borrow().queued
        ),
        (0, 0)
    );
    fixture.event(Event::Deadline, 600_000);
    assert_eq!(fixture.core.view.borrow().state, State::Running);
}

#[test]
fn lockout_still_ends_queued_quota_responses_and_cancels_the_running_prompt() {
    for reset in [false, true] {
        let mut fixture = Fixture::new();
        fixture.occupy_waiter();
        fixture.finish_response(reset, 1);
        let mut commands = fixture.event(
            Event::Runtime(RuntimeEvent::UpstreamError {
                session: "other".into(),
                error: crate::messages::HttpError {
                    code: Some(403),
                    status: "PERMISSION_DENIED".into(),
                    message: "TOS_VIOLATION".into(),
                },
            }),
            202,
        );
        assert!(
            matches!(fixture.errors(&commands).as_slice(), [Message::Lockout(_)]),
            "FR-035 selects lockout for the queued response, including during quota wait"
        );
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::Cancel(session) if session == "other"))
        );
        assert_eq!(
            (
                fixture.core.view.borrow().places,
                fixture.core.view.borrow().queued
            ),
            (0, 0)
        );
        commands.extend(fixture.event(Event::Deadline, 3_600_202));
        assert_eq!(fixture.core.view.borrow().state, State::Lockout);
        assert_eq!(fixture.errors(&commands).len(), 1);
        assert_eq!(
            commands
                .iter()
                .filter(|c| matches!(c, Command::Forget(session) if session == "running"))
                .count(),
            1
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::Usage { .. } | Command::Prepare { .. }))
        );
    }
}
