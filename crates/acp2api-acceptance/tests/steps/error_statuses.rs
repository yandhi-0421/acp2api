use crate::{
    World, error_support as support, lockout_support as lockout, places_support as queue,
    quota_support as quota, runtime_support as process, upstream_error,
};
use acp2api_acceptance::{TurnEndPlan, TurnPlan, TurnStep, endpoint_check};
use cucumber::{given, then, when};

#[given("acp2api is in lockout")]
#[given("acp2api is in the state \"lockout\"")]
async fn locked(w: &mut World) {
    lockout::send_signal(w, lockout::SIGNAL).await;
}

#[given("acp2api is in the state \"quota wait\"")]
#[given("acp2api is in a quota wait whose reset time is 120 s away")]
async fn quota_wait(w: &mut World) {
    queue::start(w).await;
    quota::seed(w, true).await;
}

#[given("the request waits in the queue when the owner stops acp2api")]
async fn queued_exit(w: &mut World) {
    queue::full_places(w).await;
    support::stop_next_tick(w).await;
}

#[given(
    regex = r"^the next prompt turn gets the upstream error (503 UNAVAILABLE|400 INVALID_ARGUMENT)$"
)]
fn next_upstream(w: &mut World, row: String) {
    let (code, status) = row.split_once(' ').unwrap();
    w.request.plan = Some(support::upstream(code.parse().unwrap(), status));
}

#[given("the next prompt turn sends no update for 5 minutes")]
fn no_update(w: &mut World) {
    w.request.updates.clear();
    w.request.plan = Some(TurnPlan {
        steps: vec![],
        end: TurnEndPlan::Never,
    });
}

#[given("the runtime process exits during the next prompt turn")]
fn process_exit(w: &mut World) {
    w.request.plan = Some(TurnPlan {
        steps: vec![],
        end: TurnEndPlan::Exit(17),
    });
}

#[given("a built-in tool runs with no permission request in the next prompt turn")]
fn built_in(w: &mut World) {
    w.request.plan = Some(TurnPlan {
        steps: vec![TurnStep::BuiltInRuns {
            name: "view_file".into(),
        }],
        end: TurnEndPlan::Never,
    });
}

#[given(
    regex = r#"^a streamed request to "([^"]+)" was accepted before (a lockout|the start of the state "sign-in required"|the start of the state "runtime unavailable"|the exit of acp2api|the start of a quota wait|an error of the 503 row|an error of the 502 row)$"#
)]
async fn accepted_before(w: &mut World, endpoint: String, case: String) {
    if case == "the start of the state \"runtime unavailable\"" {
        support::two_prior_failures(w).await;
        queue::full_places(w).await;
        w.request.plan = Some(TurnPlan::new().chunk("queued stream reply"));
        let live = queue::send(w, endpoint_check::request(&endpoint, "queued stream", true)).await;
        assert_eq!(
            live.status(),
            Some(200),
            "queued stream accepted before third failure"
        );
        assert_eq!(
            queue::prompts(w).len(),
            2,
            "tested stream waits without ACP work"
        );
        w.endpoint.set_live(live);
    } else {
        upstream_error::accepted_stream(w, &endpoint).await;
    }
}

#[given(
    regex = r#"^each place holds a prompt turn before (the stderr error line 429 "RESOURCE_EXHAUSTED"|the stderr error line 401|the third failure of the runtime process in 10 minutes)$"#
)]
async fn occupied_before(w: &mut World, event: String) {
    if event == "the third failure of the runtime process in 10 minutes" {
        support::two_prior_failures(w).await;
    }
    queue::full_places(w).await;
}

#[when(
    regex = r#"^(a lockout|the start of the state "sign-in required"|the start of the state "runtime unavailable"|the exit of acp2api|the start of a quota wait|an error of the 502 row) ends the request$"#
)]
async fn stream_failure(w: &mut World, case: String) {
    match case.as_str() {
        "the exit of acp2api" => {
            support::stop_next_tick(w).await;
        }
        "the start of the state \"runtime unavailable\"" => {
            process::exit(w).await;
        }
        _ => {
            let plan = match case.as_str() {
                "a lockout" => lockout::plan(lockout::SIGNAL, ""),
                "the start of the state \"sign-in required\"" => {
                    w.scenario.fake().set_signed_in(false);
                    support::upstream(401, "UNAUTHENTICATED")
                }
                "the start of a quota wait" => quota::error(429, Some(quota::RESET)),
                "an error of the 502 row" => support::upstream(400, "INVALID_ARGUMENT"),
                _ => unreachable!(),
            };
            w.scenario.fake().finish_gate("upstream-error", plan);
        }
    }
    let live = w.endpoint.take_live();
    w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
}

#[then(expr = "the stream has an {string} event with the {string} {string}")]
fn stream_type(w: &mut World, name: String, path: String, expected: String) {
    assert_eq!(path, "error.type");
    assert_eq!(
        support::event(w, &name)["error"]["type"],
        expected,
        "FR-027 Anthropic stream error.type"
    );
}

#[then(expr = "no {string} event follows the {string} event")]
fn no_terminal(w: &mut World, terminal: String, error: String) {
    let events = &w.scenario.answer.as_ref().unwrap().events;
    let at = events
        .iter()
        .position(|event| event.event == error)
        .unwrap();
    assert!(
        !events[at + 1..].iter().any(|event| event.event == terminal),
        "FR-027 no normal end after error"
    );
}

#[when(
    regex = r#"^(an absent local API key|a prompt turn with no update for 5 minutes in a stream|a failure of the runtime process in a stream|the stop reason "max_turn_requests" in a stream) ends a request to "([^"]+)"$"#
)]
async fn message_case(w: &mut World, case: String, endpoint: String) {
    queue::start(w).await;
    if case == "an absent local API key" {
        w.request.queue_next(&w.scenario);
        w.scenario.answer = Some(
            w.scenario
                .client()
                .await
                .without_local_api_key()
                .send(endpoint_check::request(
                    &endpoint,
                    "error message probe",
                    false,
                ))
                .finish(&mut w.scenario.time)
                .await,
        );
        return;
    }
    w.request.plan = Some(match case.as_str() {
        "a prompt turn with no update for 5 minutes in a stream" => {
            w.request.updates.clear();
            TurnPlan {
                steps: vec![],
                end: TurnEndPlan::Never,
            }
        }
        "a failure of the runtime process in a stream" => TurnPlan {
            steps: vec![],
            end: TurnEndPlan::Exit(17),
        },
        _ => TurnPlan::new().end("max_turn_requests"),
    });
    support::send(
        w,
        endpoint_check::request(&endpoint, "error message probe", true),
    )
    .await;
}

#[then(
    regex = r#"^(the error body|the "error" event|the "response.failed" event|the "error" object) holds the message "(acp2api: (?:the local API key is absent or wrong\.|the official runtime sent no update for 5 minutes, so acp2api cancelled the prompt turn\.|the runtime process failed, so acp2api ended the prompt turn\.|the prompt turn reached the limit of model requests of the official runtime\.))"$"#
)]
fn case_message(w: &mut World, place: String, expected: String) {
    let actual = match place.as_str() {
        "the error body" => w.scenario.answer.as_ref().unwrap().json()["error"]["message"].clone(),
        "the \"error\" event" => support::event(w, "error")["error"]["message"].clone(),
        "the \"response.failed\" event" => {
            support::event(w, "response.failed")["response"]["error"]["message"].clone()
        }
        _ => {
            let events = &w.scenario.answer.as_ref().unwrap().events;
            assert_eq!(events.len(), 1, "FR-027 Chat error is one data line");
            let body: serde_json::Value = serde_json::from_str(&events[0].data).unwrap();
            body["error"]["message"].clone()
        }
    };
    assert_eq!(actual, expected, "FR-032 exact message of the case");
}

#[then(expr = "acp2api refuses the request with HTTP {int} and the message of the state {string}")]
fn model_refusal(w: &mut World, status: u16, state: String) {
    let answer = w.scenario.answer.as_ref().unwrap();
    if state == "lockout" {
        lockout::refusal(
            w,
            answer,
            "acp2api: lockout since <start time> after <signal>; acp2api sends no request to the official runtime until the owner ends the lockout at <address of the management panel>.",
        );
    } else {
        support::state_refusal(w, answer, status, &state);
    }
}

#[given("two requests with no stream wait in the queue")]
async fn queued_bodies(w: &mut World) {
    for name in ["Q0", "Q1"] {
        w.request.plan = Some(TurnPlan::new().chunk("queued reply"));
        let live = queue::send(
            w,
            endpoint_check::request("POST /v1/messages", &queue::marker(name), false),
        )
        .await;
        assert_eq!(live.status(), None, "queued body waits for its result");
        w.places.calls.push((name.into(), Some(live)));
    }
    assert_eq!(
        queue::prompts(w).len(),
        2,
        "queued requests have no ACP work"
    );
}

#[when(
    regex = r#"^(the stderr error line 429 "RESOURCE_EXHAUSTED"|the stderr error line 401|the third failure of the runtime process in 10 minutes) starts the state "([^"]+)"$"#
)]
async fn start_condition(w: &mut World, event: String, state: String) {
    if state == "runtime unavailable" {
        assert_eq!(
            event,
            "the third failure of the runtime process in 10 minutes"
        );
        process::exit(w).await;
        return;
    }
    let plan = match state.as_str() {
        "quota wait" => quota::error(429, Some(quota::RESET)),
        "sign-in required" => {
            w.scenario.fake().set_signed_in(false);
            support::upstream(401, "UNAUTHENTICATED")
        }
        _ => unreachable!(),
    };
    w.scenario.fake().finish_gate("queue-A", plan);
    w.scenario.time.settle().await;
}

#[then(expr = "each of the two requests gets HTTP {int} with the message of the state {string}")]
async fn queued_refusals(w: &mut World, status: u16, state: String) {
    for name in ["Q0", "Q1"] {
        let answer = queue::take(w, name)
            .finish_current(&mut w.scenario.time)
            .await;
        support::state_refusal(w, &answer, status, &state);
        assert!(
            !queue::has_prompt(w, name),
            "FR-037 flushed request starts no ACP call"
        );
    }
    if state == "runtime unavailable" {
        for name in ["A", "B"] {
            let answer = queue::take(w, name)
                .finish_current(&mut w.scenario.time)
                .await;
            assert_eq!(
                answer.status, 200,
                "active stream was accepted before failure"
            );
            let errors: Vec<_> = answer
                .events
                .iter()
                .filter(|event| event.event == "error")
                .collect();
            assert_eq!(
                errors.len(),
                1,
                "runtime failure ends each active stream once"
            );
            let error: serde_json::Value = serde_json::from_str(&errors[0].data).unwrap();
            assert_eq!(error["error"]["type"], "api_error");
            assert_eq!(
                error["error"]["message"],
                "acp2api: the runtime process failed, so acp2api ended the prompt turn."
            );
        }
        return;
    }
    let survivor = &w.prompt_sessions["B"];
    assert!(
        w.scenario
            .fake()
            .record()
            .cancels()
            .iter()
            .all(|call| call["params"]["sessionId"] != *survivor),
        "FR-037 state change preserves the other active turn"
    );
    queue::end(w, "B").await;
}
