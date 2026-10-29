use crate::hub::{Hub, HubLink, send};
use crate::real_limit;
use crate::time_limits::REAL_TIME_CAP;
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
        let hub = self.hub.clone();
        tokio::task::spawn_blocking(move || settle_blocking(&hub))
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
        loop {
            self.settle().await;
            let at = self
                .next_deadline()
                .filter(|at| *at <= target)
                .unwrap_or(target);
            {
                let mut s = self.hub.shared.0.lock().unwrap();
                s.now = at;
                let due: Vec<_> = s.external.range(..=at).map(|(at, _)| *at).collect();
                for key in due {
                    for event in s.external.remove(&key).unwrap() {
                        crate::fake::dispatch(&mut s, event);
                    }
                }
            }
            self.settle().await;
            {
                let mut s = self.hub.shared.0.lock().unwrap();
                let due: Vec<_> = s
                    .sleepers
                    .iter()
                    .filter(|(id, sleep)| sleep.at <= at && !s.wake_pending.contains(id))
                    .map(|(id, sleep)| (*id, sleep.writer.clone()))
                    .collect();
                for (id, writer) in due {
                    s.wake_pending.push(id);
                    if let Some(writer) = writer {
                        send(&mut writer.lock().unwrap(), &json!({"wake":id,"now":at}))
                            .expect("timer wake");
                    }
                }
                s.counts.wake_pending = s.wake_pending.len() as u64;
            }
            self.settle().await;
            if at == target && self.next_deadline().is_none_or(|next| next > target) {
                break;
            }
        }
    }
}
fn settle_blocking(hub: &Hub) {
    let deadline = Instant::now() + real_limit(REAL_TIME_CAP);
    let link = HubLink::new(hub);
    let mut previous = None;
    loop {
        if let Some(failure) = hub.failure() {
            panic!("{failure}");
        }
        let counts = hub.counts();
        if counts.balanced() && previous == Some(counts) {
            return;
        }
        previous = Some(counts);
        let _ = link.call(json!({"op":"drain"}));
        if Instant::now() >= deadline {
            panic!(
                "settle {} s cap: open channels {counts:?}",
                REAL_TIME_CAP.as_secs()
            );
        }
        hub.wait_change(deadline);
    }
}
