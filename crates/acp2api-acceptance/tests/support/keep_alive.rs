use crate::{World, places_support as queue};
use acp2api_acceptance::{TurnEndPlan, TurnPlan, client::SseBlock as Block, endpoint_check};
use serde_json::Value;
use std::time::Duration;

const MAX_EVENT_GAP: u64 = 5 * 60 * 1000;
pub const NO_UPDATE_LIMIT: u64 = 5 * 60 * 1000;
const QUEUE_WAIT: u64 = 330 * 1000;
pub const UPDATE_GAP: Duration = Duration::from_secs(4 * 60);

#[derive(Debug, Default)]
pub struct KeepAliveState {
    pub queued: bool,
    pub observed: Option<Observation>,
}

#[derive(Debug)]
pub struct Observation {
    pub accepted: u64,
    pub end: Option<u64>,
    pub checked_until: u64,
    pub wait_end: u64,
    pub blocks: Vec<Block>,
}

pub async fn occupy(w: &mut World, seconds: u64) {
    assert_eq!(seconds * 1000, QUEUE_WAIT, "bounded queue fixture");
    queue::start(w).await;
    for name in ["A", "B"] {
        // An update before five minutes keeps the occupied turns alive for all 330 s.
        w.request.plan = Some(
            TurnPlan::new()
                .after(UPDATE_GAP)
                .usage(1000, 1_048_576)
                .after(Duration::from_secs(seconds) - UPDATE_GAP)
                .chunk(&format!("reply {name}")),
        );
        let live = queue::send(
            w,
            endpoint_check::request("POST /v1/messages", &queue::marker(name), false),
        )
        .await;
        w.places.calls.push((name.into(), Some(live)));
    }
    assert_eq!(queue::prompts(w).len(), 2, "both places hold a prompt");
    w.keep_alive.queued = true;
    w.endpoint.defer();
}

pub fn silent(w: &mut World, first: &str, last: &str) {
    w.request.plan = Some(TurnPlan {
        end: TurnEndPlan::Never,
        ..TurnPlan::new().chunk(first).chunk(last)
    });
    w.endpoint.defer();
}

pub async fn observe(w: &mut World) -> &Observation {
    if w.keep_alive.observed.is_none() {
        let live = w.endpoint.take_live();
        let wire = live.wire();
        let accepted = wire
            .lock()
            .unwrap()
            .head_at
            .expect("accepted stream HTTP head");
        assert_eq!(live.status(), Some(200), "accepted HTTP stream");
        let waiting = w.keep_alive.queued;
        let wait_end = accepted + if waiting { QUEUE_WAIT } else { NO_UPDATE_LIMIT };
        let target = wait_end + 200;
        w.scenario.time.advance_to(target).await;

        if waiting {
            let prompts = w.scenario.fake().record().prompts();
            assert_eq!(prompts.len(), 3, "queued request dispatches after release");
            assert!(
                prompts[2].at >= accepted + QUEUE_WAIT,
                "request actually waits 330 s for a place"
            );
        }
        let ended = wire.lock().unwrap().end_at.is_some();
        if waiting || ended {
            w.scenario.answer = Some(live.finish(&mut w.scenario.time).await);
        }
        let trace = wire.lock().unwrap().clone();
        assert!(!trace.incomplete, "complete SSE blocks at observation");
        w.keep_alive.observed = Some(Observation {
            accepted,
            end: trace.end_at,
            checked_until: target,
            wait_end,
            blocks: trace.blocks,
        });
    }
    w.keep_alive.observed.as_ref().unwrap()
}

pub fn adjacent(observation: &Observation) {
    for block in &observation.blocks {
        if !block.comments.is_empty() {
            assert_eq!(block.comments, ["keep-alive"], "Chat keep-alive comment");
            assert!(
                block.data.is_empty() && block.event.is_empty(),
                "comment-only keep-alive block"
            );
        }
    }
    let mut largest = observation
        .blocks
        .windows(2)
        .map(|pair| pair[1].at - pair[0].at)
        .max()
        .unwrap_or(0);
    if observation.end.is_none() {
        let last = observation
            .blocks
            .last()
            .map_or(observation.accepted, |event| event.at);
        largest = largest.max(observation.checked_until - last);
    }
    assert!(
        largest < MAX_EVENT_GAP,
        "FR-011 adjacent-event deadline: observed gap {largest} ms must be < {MAX_EVENT_GAP} ms"
    );
}

pub fn first(observation: &Observation) {
    let first = observation.blocks.first().expect("FR-011 first event");
    let delay = first.at - observation.accepted;
    assert!(
        delay < MAX_EVENT_GAP,
        "FR-011 first-event deadline: {delay} ms must be < {MAX_EVENT_GAP} ms"
    );
}

pub fn end(observation: &Observation) {
    let end = observation.end.expect("FR-011 response must end");
    let last = observation.blocks.last().expect("FR-011 last event");
    let delay = end - last.at;
    assert!(
        delay < MAX_EVENT_GAP,
        "FR-011 response-end deadline: {delay} ms must be < {MAX_EVENT_GAP} ms"
    );
}

pub fn responses_keep_alive(observation: &Observation) {
    let waiting: Vec<_> = observation
        .blocks
        .iter()
        .filter(|block| block.at < observation.wait_end)
        .collect();
    let progress: Vec<_> = waiting
        .iter()
        .filter(|block| block.event == "response.in_progress")
        .collect();
    assert!(
        progress.iter().any(|block| block.at > observation.accepted),
        "FR-011 Responses keep-alive must recur during the queue wait"
    );
    let created: Value = serde_json::from_str(&waiting[0].data).unwrap();
    assert_eq!(waiting[0].event, "response.created");
    for (index, block) in waiting.iter().enumerate() {
        assert!(block.comments.is_empty(), "Responses uses real SSE events");
        let value: Value = serde_json::from_str(&block.data).unwrap();
        assert_eq!(value["type"], block.event);
        assert_eq!(value["sequence_number"], index as u64);
        assert_eq!(value["response"]["id"], created["response"]["id"]);
        assert_eq!(value["response"]["object"], "response");
        assert_eq!(value["response"]["status"], "in_progress");
        assert!(
            ["response.created", "response.in_progress"].contains(&block.event.as_str()),
            "no output events before the queued prompt starts"
        );
    }
}
