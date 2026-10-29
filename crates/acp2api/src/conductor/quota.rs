use crate::{Moment, messages::Message};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) fn error_text(message: &Message) -> Option<&str> {
    match message {
        Message::UpstreamHttp(error)
            if !matches!(error.code, 401 | 403)
                && !error.message.contains("Terms of Service")
                && (matches!(error.code, 402 | 429) || error.status == "RESOURCE_EXHAUSTED") =>
        {
            Some(&error.message)
        }
        Message::UpstreamAcp(error)
            if !matches!(error.code, 401 | 403)
                && !error.message.contains("Terms of Service")
                && (matches!(error.code, 402 | 429)
                    || error.message.contains("RESOURCE_EXHAUSTED")) =>
        {
            Some(&error.message)
        }
        _ => None,
    }
}

#[derive(Clone)]
pub(super) struct Wait {
    pub deadline: Moment,
    until: SystemTime,
}
impl Wait {
    pub fn new(until: SystemTime, now: Moment, wall: SystemTime) -> Self {
        Self {
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

pub(super) fn reset_time(text: &str) -> Option<SystemTime> {
    text.split_whitespace()
        .find_map(|word| {
            rfc3339(word.trim_matches(|c| matches!(c, '(' | ')' | '"' | '\'' | ',' | ';')))
        })
        .or_else(|| {
            let (_, rest) = text.split_once("resets ")?;
            let (date, _) = rest.split_once(" UTC")?;
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
    let leap = |y: u32| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
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
    if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(seconds as u64, nanos))
    } else {
        UNIX_EPOCH
            .checked_sub(Duration::from_secs(seconds.unsigned_abs()))?
            .checked_add(Duration::from_nanos(u64::from(nanos)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_expired_rfc3339_reset_before_the_epoch_is_still_usable() {
        let until = reset_time("resets 1969-12-31T23:59:59Z").unwrap();
        let wait = Wait::new(until, 0, UNIX_EPOCH);
        assert_eq!(
            wait.message(0).to_string(),
            "acp2api: the Google quota is used up until 1969-12-31T23:59:59Z. acp2api starts no prompt turn before that time."
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
                reset_time(text).map(|at| at.duration_since(UNIX_EPOCH).unwrap().as_secs()),
                seconds,
                "{text}"
            );
        }
    }
}
