use super::*;

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
