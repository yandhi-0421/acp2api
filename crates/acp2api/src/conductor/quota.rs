use super::{
    Active, Command, Core, Event, Failure, QUOTA_FALLBACK, QuotaResult, SETTLE_DELAY, Work, WorkId,
    quota,
};
use crate::{
    Moment,
    messages::{Message, OwnerMessage},
    runtime_driver::{RuntimeEvent, TurnUpdate},
};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

pub(super) fn has_quota(text: &str, model: &str) -> bool {
    model_group(text, model)
        .is_some_and(|group| !limit_rows(group).any(|(remaining, _)| remaining == 0))
}

pub(super) fn usage_reset(text: &str, model: &str, wall: SystemTime) -> Option<SystemTime> {
    limit_rows(model_group(text, model)?)
        .filter(|(remaining, _)| *remaining == 0)
        .filter_map(|(_, row)| reset_time(row, wall))
        .max()
}

fn model_group<'a>(text: &'a str, model: &str) -> Option<&'a str> {
    text.split("\n- **").find(|group| {
        group.lines().any(|line| {
            line.trim()
                .strip_prefix("*(Models within this group: ")
                .and_then(|line| line.strip_suffix(")*"))
                .is_some_and(|names| {
                    names.split(", ").any(|name| {
                        let mut parts = model.split(|c: char| !c.is_ascii_alphanumeric());
                        name.split(|c: char| !c.is_ascii_alphanumeric())
                            .all(|word| parts.any(|part| part.eq_ignore_ascii_case(word)))
                    })
                })
        })
    })
}

fn limit_rows(group: &str) -> impl Iterator<Item = (u8, &str)> {
    group.lines().filter_map(|line| {
        let (_, value) = line.split_once("Limit Remaining:** ")?;
        let (remaining, _) = value.split_once("% remaining")?;
        Some((remaining.parse().ok()?, line))
    })
}

pub(super) fn error_text(message: &Message) -> Option<&str> {
    if message.lockout_signal().is_some() || message.requires_sign_in() {
        return None;
    }
    match message {
        Message::UpstreamHttp(error)
            if (matches!(error.code, Some(402 | 429)) || error.status == "RESOURCE_EXHAUSTED") =>
        {
            Some(&error.message)
        }
        Message::UpstreamAcp(error)
            if (matches!(error.code, 402 | 429)
                || error.message.contains("RESOURCE_EXHAUSTED")) =>
        {
            Some(&error.message)
        }
        _ => None,
    }
}

#[derive(Clone)]
pub(super) struct Wait {
    pub timer: TimerId,
    pub deadline: Moment,
    until: SystemTime,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct TimerId(pub u64);
impl Wait {
    pub fn new(until: SystemTime, now: Moment, wall: SystemTime, timer: TimerId) -> Self {
        Self {
            timer,
            deadline: now
                .saturating_add(until.duration_since(wall).unwrap_or_default().as_millis() as u64),
            until,
        }
    }
    pub fn message(&self, now: Moment) -> Message {
        Message::QuotaWait {
            until: crate::journal::timestamp(self.until).expect("reset time in UTC"),
            retry_after: self.deadline.saturating_sub(now).div_ceil(1000),
        }
    }
}

pub(super) fn reset_time(text: &str, wall: SystemTime) -> Option<SystemTime> {
    let (_, reset) = text.split_once("resets ")?;
    rfc3339(
        reset
            .split_whitespace()
            .next()?
            .trim_end_matches([')', '"', '\'', ',', ';']),
    )
    .or_else(|| {
        let (date, _) = reset.split_once(" UTC")?;
        let fields: Vec<_> = date.split_whitespace().collect();
        let [month, day, year, time] = fields.as_slice() else {
            return None;
        };
        let month = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ]
        .iter()
        .position(|name| name == month)? as u32
            + 1;
        let day = day.strip_suffix(',')?.parse().ok()?;
        let year = year.strip_suffix(',')?.parse().ok()?;
        let (hour, minute) = time.split_once(':')?;
        if hour.len() != 2 || minute.len() != 2 {
            return None;
        }
        calendar(
            [year, month, day],
            [hour.parse().ok()?, minute.parse().ok()?, 0],
            0,
            0,
        )
    })
    .filter(|until| *until >= wall)
}

fn rfc3339(text: &str) -> Option<SystemTime> {
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let number = |start, end| {
        let digits = text.get(start..end)?;
        digits.bytes().all(|b| b.is_ascii_digit()).then_some(())?;
        digits.parse::<u32>().ok()
    };
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    let mut at = 19;
    let mut nanos = 0;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            if at - start < 9 {
                nanos += u32::from(bytes[at] - b'0') * 10_u32.pow(8 - (at - start) as u32);
            }
            at += 1;
        }
        if at == start {
            return None;
        }
    }
    let offset = match bytes.get(at..)? {
        [b'Z' | b'z'] => 0,
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2]
            if [h1, h2, m1, m2].into_iter().all(u8::is_ascii_digit) =>
        {
            let hours = u32::from(h1 - b'0') * 10 + u32::from(h2 - b'0');
            let minutes = u32::from(m1 - b'0') * 10 + u32::from(m2 - b'0');
            if hours > 23 || minutes > 59 {
                return None;
            }
            let seconds = i64::from(hours * 3600 + minutes * 60);
            if *sign == b'-' { -seconds } else { seconds }
        }
        _ => return None,
    };
    calendar([year, month, day], [hour, minute, second], nanos, offset)
}

fn calendar(date: [u32; 3], time: [u32; 3], nanos: u32, offset: i64) -> Option<SystemTime> {
    let [year, month, day] = date;
    let [hour, minute, second] = time;
    let months = [
        31,
        if crate::journal::is_leap_year(u64::from(year)) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month)
        || day == 0
        || day > months[(month - 1) as usize]
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days_before = |y: u32| {
        let y = i64::from(y);
        365 * y + (y + 3) / 4 - (y + 99) / 100 + (y + 399) / 400
    };
    let days = days_before(year) - days_before(1970)
        + i64::from(months[..(month - 1) as usize].iter().sum::<u32>() + day - 1);
    let seconds = days * 86400 + i64::from(hour * 3600 + minute * 60 + second) - offset;
    UNIX_EPOCH.checked_add(Duration::new(u64::try_from(seconds).ok()?, nanos))
}

pub(super) struct QuotaQuery {
    pub(super) work: String,
    pub(super) purpose: QuotaPurpose,
}
impl QuotaQuery {
    pub(super) fn disconnect(&mut self, commands: &mut Vec<Command>) {
        let response = match &mut self.purpose {
            QuotaPurpose::Reset { response, .. } | QuotaPurpose::Refusal { response, .. } => {
                response
            }
            QuotaPurpose::Refresh(_) => unreachable!(),
        };
        let active = response.take().unwrap();
        commands.push(active.disconnected());
    }
}
pub(super) enum QuotaPurpose {
    Refresh(oneshot::Sender<Result<(), OwnerMessage>>),
    Reset {
        wait: quota::Wait,
        model: Option<String>,
        response: Option<Active>,
    },
    Refusal {
        model: String,
        response: Option<Active>,
    },
}
pub(super) enum QuotaPhase {
    Preparing,
    Prompt(String),
    Settling { session: String, at: Moment },
}
impl QuotaPhase {
    pub(super) fn session(&self) -> Option<&str> {
        match self {
            Self::Preparing => None,
            Self::Prompt(session) | Self::Settling { session, .. } => Some(session),
        }
    }
}
impl Core {
    pub(super) fn start_quota(
        &mut self,
        until: std::time::SystemTime,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) -> quota::Wait {
        let started = self
            .quota_wait
            .as_ref()
            .is_none_or(|wait| wait.deadline <= now);
        self.next_quota.0 += 1;
        let wait = quota::Wait::new(until, now, wall, self.next_quota);
        self.quota_wait = Some(wait.clone());
        if started {
            let message = self.refusal(now).unwrap_or_else(|| wait.message(now));
            self.refuse_queued(
                crate::journal::Outcome::Refused("quota_wait"),
                message,
                commands,
            );
        }
        wait
    }
    pub(super) fn quota_signal(
        &mut self,
        error: &crate::messages::Message,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) -> Option<(quota::Wait, bool)> {
        let reset = quota::reset_time(quota::error_text(error)?, wall);
        let known = reset.is_some();
        let wait = self.start_quota(reset.unwrap_or(wall + QUOTA_FALLBACK), now, wall, commands);
        Some((wait, known))
    }
    pub(super) fn quota_event(
        &mut self,
        id: WorkId,
        event: Event,
        upstream_error: Option<crate::messages::Message>,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        match event {
            Event::StartFailed(_, error) => self.finish_quota(id, Err(error), now, wall, commands),
            Event::Runtime(RuntimeEvent::Prepared { work, result }) => match result {
                Ok((session, catalog)) => {
                    self.catalog = Some(catalog);
                    let Work::QuotaWorking { phase, .. } = self.work.get_mut(&id).unwrap() else {
                        unreachable!()
                    };
                    *phase = QuotaPhase::Prompt(session.clone());
                    commands.push(Command::Usage { work, session });
                }
                Err(error) => self.finish_quota(id, Err(error), now, wall, commands),
            },
            Event::Runtime(RuntimeEvent::Turn { session, update }) => {
                self.update_quota(id, session, update, now, commands)
            }
            Event::Runtime(RuntimeEvent::UpstreamError { .. }) => {
                let message = upstream_error.unwrap();
                let _ = self.quota_signal(&message, now, wall, commands);
                let Work::QuotaWorking { result, .. } = self.work.get_mut(&id).unwrap() else {
                    unreachable!()
                };
                *result = Err(message);
            }
            Event::Deadline => self.finish_quota(id, Ok(()), now, wall, commands),
            _ => unreachable!(),
        }
    }
    pub(super) fn enqueue_reset_query(&mut self, wait: quota::Wait, model: Option<String>) {
        self.enqueue_quota(
            QuotaPurpose::Reset {
                wait,
                model,
                response: None,
            },
            None,
        );
    }
    pub(super) fn enqueue_quota(&mut self, purpose: QuotaPurpose, session: Option<String>) {
        let id = self.next_work;
        self.next_work += 1;
        let query = QuotaQuery {
            work: format!("quota-{id}"),
            purpose,
        };
        self.work.insert(id, Work::QuotaQueued { query, session });
    }
    fn update_quota(
        &mut self,
        id: WorkId,
        session: String,
        update: TurnUpdate,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        let Work::QuotaWorking {
            phase,
            text,
            result,
            ..
        } = self.work.get_mut(&id).unwrap()
        else {
            unreachable!()
        };
        match update {
            TurnUpdate::Text(chunk) => text.push_str(&chunk),
            TurnUpdate::PromptEnded { .. } => {
                *phase = QuotaPhase::Settling {
                    session,
                    at: now.saturating_add(SETTLE_DELAY),
                }
            }
            TurnUpdate::Failed(error) => {
                *result = Err(error);
                *phase = QuotaPhase::Settling {
                    session,
                    at: now.saturating_add(SETTLE_DELAY),
                };
            }
            TurnUpdate::Permission { request, .. }
            | TurnUpdate::RefusedPermission { request, .. } => {
                commands.push(Command::Permission(request))
            }
            TurnUpdate::UnpermittedTool(name) => {
                *result = Err(crate::messages::Message::BuiltInToolRan(name));
                commands.push(self.sessions.cancel(session, now));
            }
            TurnUpdate::Activity | TurnUpdate::Usage { .. } | TurnUpdate::ThoughtDropped => {}
        }
    }
    fn finish_quota(
        &mut self,
        id: WorkId,
        result: Result<(), crate::messages::Message>,
        now: Moment,
        wall: std::time::SystemTime,
        commands: &mut Vec<Command>,
    ) {
        let Work::QuotaWorking {
            mut query,
            phase,
            text,
            result: recorded,
        } = self.work.remove(&id).unwrap()
        else {
            unreachable!()
        };
        let result = result.and(recorded);
        self.quota_result.send_replace(Arc::new(Some(QuotaResult {
            text: match &result {
                Ok(()) => Ok(text.clone()),
                Err(error) => Err(error.clone()),
            },
            time: crate::journal::timestamp(wall).expect("quota result time in UTC"),
        })));
        let end = match &mut query.purpose {
            QuotaPurpose::Refresh(_) => result,
            QuotaPurpose::Reset { wait, model, .. } => {
                if result.is_ok()
                    && let Some(until) = model
                        .as_deref()
                        .and_then(|model| quota::usage_reset(&text, model, wall))
                {
                    *wait = quota::Wait::new(until, now, wall, wait.timer);
                    if self.is_current_quota_wait(wait) {
                        self.quota_wait = Some(wait.clone());
                    }
                }
                Err(self.refusal(now).unwrap_or_else(|| wait.message(now)))
            }
            QuotaPurpose::Refusal { model, .. } => {
                if result.is_ok() && quota::has_quota(&text, model) {
                    Ok(())
                } else {
                    let until = if result.is_ok() {
                        quota::usage_reset(&text, model, wall)
                    } else {
                        None
                    };
                    let wait = self.start_quota(
                        until.unwrap_or(wall + QUOTA_FALLBACK),
                        now,
                        wall,
                        commands,
                    );
                    Err(self.refusal(now).unwrap_or_else(|| wait.message(now)))
                }
            }
        };
        self.complete_quota(
            query,
            phase.session().map(str::to_owned),
            end.map_err(|message| Failure::at(message, now)),
            now,
            commands,
        );
    }
    pub(super) fn complete_quota(
        &mut self,
        query: QuotaQuery,
        session: Option<String>,
        result: Result<(), Failure>,
        now: Moment,
        commands: &mut Vec<Command>,
    ) {
        let response = match query.purpose {
            QuotaPurpose::Refresh(reply) => {
                let _ = reply.send(result.map_err(|failure| OwnerMessage::Api(failure.message)));
                commands.extend(session.map(Command::Forget));
                return;
            }
            QuotaPurpose::Reset { response, .. } | QuotaPurpose::Refusal { response, .. } => {
                response
            }
        };
        if let Some(active) = response {
            let (forget, completion) = self.complete_request(active, result, now);
            commands.extend(completion);
            commands.extend(
                session
                    .filter(|session| Some(session) != forget.as_ref())
                    .map(Command::Forget),
            );
            commands.extend(forget.map(Command::Forget));
        } else {
            commands.extend(session.map(Command::Forget));
        }
    }
    fn is_current_quota_wait(&self, wait: &quota::Wait) -> bool {
        self.quota_wait
            .as_ref()
            .is_some_and(|current| current.timer == wait.timer)
    }
    pub(super) fn can_query_quota_reset(&self, wait: &quota::Wait) -> bool {
        self.quota_wait.is_none() || self.is_current_quota_wait(wait)
    }
    /// A request that leaves a quota wait without its usage check hands the reset query on.
    pub(super) fn keep_quota_wait_reset_query(&mut self, active: &Active) {
        if let Some(wait) = active.quota_wait()
            && self.is_current_quota_wait(wait)
        {
            self.enqueue_reset_query(wait.clone(), Some(active.model.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usage_reset_waits_for_the_latest_exhausted_limit() {
        let text = GROUPS.replace(
            "75% remaining (resets Oct 24, 2026, 14:03 UTC)",
            "0% remaining (resets Oct 18, 2026, 00:05 UTC)",
        );
        assert_eq!(
            usage_reset(&text, "gemini-3.8-flash", UNIX_EPOCH),
            reset_time("resets Oct 18, 2026, 00:10 UTC", UNIX_EPOCH)
        );
    }
    #[test]
    fn quota_requires_the_request_model_in_the_group_model_line() {
        let text = GROUPS
            .replace("Gemini Flash, ", "")
            .replace("0% remaining", "50% remaining");
        assert!(
            !has_quota(&text, "gemini-3.8-flash"),
            "FR-034 no group names the requested model"
        );
    }
    #[test]
    fn reset_time_requires_a_reset_marker_and_a_nonpast_time() {
        let wall = UNIX_EPOCH + Duration::from_secs(1_792_281_600);
        for text in [
            "observed at 2026-10-18T00:01:00Z; reset tomorrow",
            "resets 2026-10-17T23:59:59Z",
            "resets 1969-12-31T23:59:59Z",
        ] {
            assert_eq!(reset_time(text, wall), None, "{text}");
        }
    }
    const GROUPS: &str = "### Session Usage
- **Session total:** data not available yet

### Quota Limits
- **Claude and GPT models**
  *(Models within this group: Claude Opus, Claude Sonnet, GPT-OSS)*
  - **Weekly Limit Remaining:** 0% remaining (resets Oct 24, 2026, 12:34 UTC)
  - **Five Hour Limit Remaining:** 75% remaining (resets Oct 18, 2026, 00:20 UTC)
- **Gemini Models**
  *(Models within this group: Gemini Flash, Gemini Pro)*
  - **Weekly Limit Remaining:** 75% remaining (resets Oct 24, 2026, 14:03 UTC)
  - **Five Hour Limit Remaining:** 0% remaining (resets Oct 18, 2026, 00:10 UTC)";

    #[test]
    fn usage_reset_uses_the_exhausted_rows_of_the_named_model_group() {
        assert_eq!(
            usage_reset(GROUPS, "gemini-3.8-flash", UNIX_EPOCH),
            reset_time("resets Oct 18, 2026, 00:10 UTC", UNIX_EPOCH),
            "FR-036 ignores another group and the non-exhausted weekly row"
        );
    }
    #[test]
    fn quota_is_available_only_when_all_limits_of_the_model_group_are_positive() {
        let text = "### Quota Limits\n- **Gemini Models**\n  *(Models within this group: Gemini Flash, Gemini Pro)*\n  - **Weekly Limit Remaining:** 75% remaining (resets Oct 24, 2026, 12:34 UTC)\n  - **Five Hour Limit Remaining:** 0% remaining (resets Oct 17, 2026, 17:34 UTC)\n- **Claude and GPT models**\n  *(Models within this group: Claude Opus, Claude Sonnet, GPT-OSS)*\n  - **Weekly Limit Remaining:** 100% remaining (resets Oct 24, 2026, 14:03 UTC)\n  - **Five Hour Limit Remaining:** 25% remaining (resets Oct 17, 2026, 19:03 UTC)";
        assert_eq!(
            ["gemini-3.8-flash", "claude-sonnet", "unknown"].map(|model| has_quota(text, model)),
            [false, true, false]
        );
    }
    #[test]
    fn reset_grammar_rejects_unknown_and_invalid_dates() {
        for (text, seconds) in [
            ("resets Oct 24, 2026, 12:34 UTC", Some(1792845240)),
            ("resets 2026-10-24T14:34:00+02:00", Some(1792845240)),
            ("resets 2026-10-24t12:34:00z", Some(1792845240)),
            ("resets Feb 29, 2025, 12:34 UTC", None),
            ("resets Oct 24, 2026, 24:34 UTC", None),
            ("resets 2026-02-30T12:34:00Z", None),
            ("resets 2026-10-24T12:34:00+24:00", None),
            ("reset tomorrow", None),
            ("resets in 2 minutes", None),
            ("resets 10/24/2026 12:34", None),
        ] {
            assert_eq!(
                reset_time(text, UNIX_EPOCH)
                    .map(|at| at.duration_since(UNIX_EPOCH).unwrap().as_secs()),
                seconds,
                "{text}"
            );
        }
    }
}
