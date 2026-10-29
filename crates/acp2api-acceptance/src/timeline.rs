use crate::hub::{Hub, HubLink, send};
use crate::real_limit;
use crate::time_limits::SETTLE_CAP;
use serde_json::json;
use std::time::Instant;
#[derive(Clone, Debug)]
pub struct TimeLine {
    pub hub: Hub,
}
impl TimeLine {
    pub fn new(hub: Hub) -> Self {
        Self { hub }
    }
    pub async fn settle(&mut self) {
        self.settle_timers(Vec::new()).await;
    }
    async fn settle_timers(&mut self, timers: Vec<u64>) {
        let hub = self.hub.clone();
        tokio::task::spawn_blocking(move || settle_blocking(&hub, &timers))
            .await
            .unwrap();
    }
    pub fn next_deadline(&self) -> Option<u64> {
        let s = self.hub.shared.0.lock().unwrap();
        s.sleepers
            .values()
            .map(|s| s.at)
            .chain(s.external.keys().copied())
            .filter(|at| *at != u64::MAX)
            .min()
    }
    pub async fn advance_to(&mut self, target: u64) {
        assert!(target >= self.hub.now(), "virtual time cannot reverse");
        self.settle().await;
        loop {
            let at = self
                .next_deadline()
                .filter(|at| *at <= target)
                .unwrap_or(target);
            let has_external_inputs = {
                let mut s = self.hub.shared.0.lock().unwrap();
                s.now = at;
                let due: Vec<_> = s.external.range(..=at).map(|(at, _)| *at).collect();
                let has_external_inputs = !due.is_empty();
                for key in due {
                    for event in s.external.remove(&key).unwrap() {
                        crate::fake::dispatch(&mut s, event);
                    }
                }
                has_external_inputs
            };
            if has_external_inputs {
                self.settle().await;
            }
            let timers = {
                let mut s = self.hub.shared.0.lock().unwrap();
                let due: Vec<_> = s
                    .sleepers
                    .iter()
                    .filter(|(id, sleep)| sleep.at <= at && !s.wake_pending.contains(id))
                    .map(|(id, sleep)| (*id, sleep.writer.clone()))
                    .collect();
                let mut timers = Vec::new();
                for (id, writer) in due {
                    timers.push(id);
                    s.wake_pending.push(id);
                    if let Some(writer) = writer {
                        send(&mut writer.lock().unwrap(), &json!({"wake":id,"now":at}))
                            .expect("timer wake");
                    }
                }
                s.counts.wake_pending = s.wake_pending.len() as u64;
                timers
            };
            self.settle_timers(timers).await;
            if at == target && self.next_deadline().is_none_or(|next| next > target) {
                break;
            }
        }
    }
}
fn settle_blocking(hub: &Hub, timers: &[u64]) {
    let deadline = Instant::now() + real_limit(SETTLE_CAP);
    let link = HubLink::new(hub);
    let mut previous = None;
    let mut drained_and_waited = false;
    loop {
        if let Some(failure) = hub.failure() {
            panic!("{failure}");
        }
        let counts = hub.counts();
        let unchanged = previous == Some(counts);
        // Completion plus a confirmed new wait from the same task is a processing barrier.
        let rearmed = !timers.is_empty() && {
            let state = hub.shared.0.lock().unwrap();
            timers.iter().all(|id| {
                state.completed_timers.get(id).is_some_and(|task| {
                    state.sleepers.iter().any(|(next, sleeper)| {
                        next > id && sleeper.task.as_ref() == Some(task) && sleeper.writer.is_some()
                    })
                })
            })
        };
        if counts.balanced()
            && unchanged
            && (drained_and_waited || rearmed && hub.counts() == counts)
        {
            break;
        }
        previous = Some(counts);
        let _ = link.call(json!({"op":"drain"}));
        if Instant::now() >= deadline {
            panic!(
                "settle {} s cap: open channels {counts:?}",
                SETTLE_CAP.as_secs()
            );
        }
        drained_and_waited = timers.is_empty() || unchanged && (!rearmed || !counts.balanced());
        if drained_and_waited {
            hub.wait_change(deadline);
        }
    }
    let mut state = hub.shared.0.lock().unwrap();
    for id in timers {
        state.completed_timers.remove(id);
    }
}
