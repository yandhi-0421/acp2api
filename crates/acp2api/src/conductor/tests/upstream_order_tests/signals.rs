use super::*;

#[test]
fn late_reply_text_starts_lockout_during_the_stderr_wait() {
    let mut turn = PromptTurn::new();
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
    let mut turn = PromptTurn::new();
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
fn a_cut_suffix_does_not_turn_order_1403_into_a_lockout_signal() {
    let mut turn = PromptTurn::new();
    turn.error(400, 0);
    turn.text("Order 1403 is shipped to", 20);
    assert!(turn.core.lockout.is_none());
    turn.text(" Boston.", 30);
    assert!(
        turn.core.lockout.is_none(),
        "a cut token head is not a new 403 token"
    );
    turn.end(40);
    assert!(turn.core.lockout.is_none());
}

#[test]
fn an_unfinished_403_token_waits_for_the_next_chunk_or_the_turn_end() {
    for continuation in [Some("50 items"), Some(" items"), None] {
        let mut turn = PromptTurn::new();
        turn.error(400, 0);
        turn.text("Total 403", 20);
        assert!(
            turn.core.lockout.is_none(),
            "the token at a chunk end is incomplete"
        );
        if let Some(text) = continuation {
            turn.text(text, 30);
        } else {
            turn.end(30);
        }
        assert_eq!(
            turn.core.lockout.is_some(),
            continuation != Some("50 items"),
            "FR-034 distinguishes 403 from 40350, and confirms a token at turn end"
        );
    }
}

#[test]
fn failed_reply_signals_survive_a_disconnect_before_the_error() {
    let mut locked = Vec::new();
    for disconnect_first in [false, true] {
        let mut turn = PromptTurn::new();
        if disconnect_first {
            turn.disconnect(10);
        }
        turn.text("Terms of Service", 20);
        if !disconnect_first {
            turn.disconnect(30);
        }
        turn.error(400, 40);
        locked.push(turn.core.lockout.is_some());
    }
    assert_eq!(
        locked,
        [true, true],
        "text before stderr counts on either side of disconnect"
    );
}

#[test]
fn failed_reply_signals_include_text_discarded_after_a_client_tool_call() {
    let mut turn = PromptTurn::new();
    turn.tool(10);
    turn.text("Terms of Service", 20);
    turn.error(400, 30);
    assert!(
        turn.core.lockout.is_some(),
        "the signal scan precedes reply filtering"
    );
}

#[test]
fn a_repeated_error_does_not_reset_a_split_failed_reply_signal() {
    let mut turn = PromptTurn::new();
    turn.tool(0);
    turn.error(400, 10);
    turn.text("Terms of ", 20);
    turn.error(400, 30);
    turn.text("Service", 40);
    assert!(
        turn.core.lockout.is_some(),
        "repeated stderr preserves a split signal"
    );
}

#[test]
fn the_longest_lockout_signal_can_cross_a_chunk_after_sixteen_bytes() {
    let mut turn = PromptTurn::new();
    turn.error(400, 0);
    turn.text("PERMISSION_DENIE", 10);
    assert!(turn.core.lockout.is_none());
    turn.text("D", 20);
    assert!(
        turn.core.lockout.is_some(),
        "retain enough bytes for PERMISSION_DENIED"
    );
}

#[test]
fn a_reply_suffix_cuts_multibyte_text_at_a_character_boundary() {
    let mut turn = PromptTurn::new();
    turn.error(400, 0);
    turn.text(&format!("{}safe", "界".repeat(8)), 10);
    assert!(turn.core.lockout.is_none());
    turn.text(" Terms of ", 20);
    turn.text("Service", 30);
    assert!(
        turn.core.lockout.is_some(),
        "UTF-8 tail trimming preserves the next signal"
    );
}

#[test]
fn a_lockout_signal_can_start_before_the_stderr_error_and_finish_after_it() {
    let mut turn = PromptTurn::new();
    turn.text("Terms of ", 10);
    turn.error(400, 20);
    assert!(turn.core.lockout.is_none());
    turn.text("Service", 30);
    assert!(
        turn.core.lockout.is_some(),
        "the pre-error suffix remains part of this turn"
    );
}

#[test]
fn disconnected_ended_reply_evidence_expires_and_does_not_cross_runtime_replacement() {
    let mut locked = Vec::new();
    for (error_at, replacement) in [(219, false), (220, false), (221, false), (219, true)] {
        let mut turn = PromptTurn::new();
        turn.text("Terms of Service", 10);
        turn.end(20);
        turn.disconnect(30);
        if replacement {
            turn.core.sessions.replaced(None);
        }
        turn.error(400, error_at);
        locked.push(turn.core.lockout.is_some());
    }
    assert_eq!(
        locked,
        [true, true, false, false],
        "FR-028 retains ended disconnected evidence only in its runtime and 200 ms window"
    );
}

#[test]
fn pre_error_text_survives_a_known_quota_rpc_failure() {
    let mut turn = PromptTurn::new();
    turn.text("Terms of Service", 5);
    let error = crate::messages::HttpError {
        code: Some(429),
        status: "RESOURCE_EXHAUSTED".into(),
        message: "resets 1970-01-01T00:02:00Z".into(),
    };
    turn.update(
        TurnUpdate::Failed(crate::messages::Message::UpstreamHttp(error.clone())),
        10,
    );
    assert_eq!(turn.output.len(), 1);
    turn.handle(
        Event::Runtime(RuntimeEvent::UpstreamError {
            session: "running".into(),
            error,
        }),
        20,
    );
    assert!(
        turn.core.lockout.is_some(),
        "FR-034 text before known-quota RPC failure still counts with its stderr error"
    );
}

#[test]
fn a_confirmed_signal_survives_more_text_before_the_stderr_error() {
    for signal in ["403 ", "PERMISSION_DENIED", "Terms of Service"] {
        let mut turn = PromptTurn::new();
        turn.text(signal, 1);
        turn.text(" more than seventeen bytes of ordinary text", 2);
        turn.text(" and another chunk", 3);
        turn.error(400, 4);
        assert!(
            turn.core.lockout.is_some(),
            "confirmed signal {signal:?} must remain sticky"
        );
    }
}
