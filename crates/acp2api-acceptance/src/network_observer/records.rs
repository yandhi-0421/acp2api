use super::{Attempt, LOG_CAP, RECORD_CAP};
use serde::Deserialize;
use std::{collections::HashMap, net::IpAddr};

#[derive(Deserialize)]
struct WindowsCapture {
    anchor: bool,
    oldest: u64,
    newest: u64,
    events: Vec<WindowsEvent>,
}
#[derive(Deserialize)]
struct WindowsEvent {
    record: u64,
    id: u16,
    fields: HashMap<String, String>,
}
pub(super) fn windows(bytes: &[u8], start: u64) -> Result<Vec<Attempt>, String> {
    if bytes.len() as u64 > LOG_CAP {
        return Err("Security log exceeds 1 MiB".into());
    }
    let capture: WindowsCapture =
        serde_json::from_slice(bytes).map_err(|e| format!("Security log JSON: {e}"))?;
    if !capture.anchor || capture.oldest > start || capture.newest < start {
        return Err("Security log window was cleared or overwritten".into());
    }
    if capture.events.len() > RECORD_CAP {
        return Err("Security log exceeds 4096 records".into());
    }
    let mut attempts = Vec::new();
    let mut previous = start;
    for event in capture.events {
        if event.record <= previous || event.record > capture.newest {
            return Err("Security log record order/window gap".into());
        }
        previous = event.record;
        if ![5156, 5157].contains(&event.id) {
            return Err(format!("Security log/audit gap event {}", event.id));
        }
        let field = |name: &str| {
            event
                .fields
                .get(name)
                .ok_or_else(|| format!("Security event lacks {name}"))
        };
        let raw = field("ProcessID")?;
        let pid = if let Some(hex) = raw.strip_prefix("0x") {
            u32::from_str_radix(hex, 16)
        } else {
            raw.parse()
        }
        .map_err(|_| "invalid Security event PID")?;
        match field("Direction")?.as_str() {
            "%%14592" => continue,
            "%%14593" => {}
            _ => return Err("unknown Security event direction".into()),
        }
        let destination: IpAddr = field("DestAddress")?
            .parse()
            .map_err(|_| "invalid Security event destination")?;
        attempts.push(Attempt { pid, destination });
    }
    Ok(attempts)
}

#[cfg(test)]
mod tests {
    use super::super::only_loopback;
    use super::*;
    use serde_json::{Value, json};

    fn event(record: u64, id: u16, pid: &str, address: &str) -> Value {
        json!({"record":record,"id":id,"fields":{"ProcessID":pid,"Direction":"%%14593","DestAddress":address}})
    }
    fn win(events: Vec<Value>) -> Value {
        json!({"anchor":true,"oldest":1,"newest":20,"events":events})
    }
    fn parse(value: &Value) -> Result<Vec<Attempt>, String> {
        windows(&serde_json::to_vec(value).unwrap(), 10)
    }
    #[test]
    fn other_child_connections_do_not_count_as_product_connections() {
        let capture = win(vec![
            event(11, 5157, "72753", "192.0.2.1"),
            event(12, 5156, "100", "127.0.0.1"),
        ]);
        assert_eq!(only_loopback(100, parse(&capture).unwrap()), Ok(()));
        assert_eq!(only_loopback(72753, parse(&capture).unwrap()),
            Err("FR-046 outbound attempt outside 127.0.0.1: Attempt { pid: 72753, destination: 192.0.2.1 }".into()));
    }
    #[test]
    fn both_windows_success_and_failure_audits_fail_for_the_product_pid() {
        for id in [5156, 5157] {
            let capture = win(vec![event(11, id, "0x11cc", "192.0.2.1")]);
            assert_eq!(only_loopback(4556, parse(&capture).unwrap()),
                Err("FR-046 outbound attempt outside 127.0.0.1: Attempt { pid: 4556, destination: 192.0.2.1 }".into()));
        }
    }
    #[test]
    fn only_the_exact_ipv4_destination_is_allowed() {
        for (address, allowed) in [
            ("127.0.0.1", true),
            ("127.0.0.2", false),
            ("::1", false),
            ("::ffff:127.0.0.1", false),
        ] {
            let result = only_loopback(
                4556,
                parse(&win(vec![event(11, 5156, "4556", address)])).unwrap(),
            );
            let expected = if allowed {
                Ok(())
            } else {
                Err(format!(
                    "FR-046 outbound attempt outside 127.0.0.1: Attempt {{ pid: 4556, destination: {address} }}"
                ))
            };
            assert_eq!(result, expected, "{address}");
        }
    }
    #[test]
    fn inbound_wfp_records_are_not_outbound_attempts() {
        let mut record = event(11, 5156, "4556", "192.0.2.1");
        record["fields"]["Direction"] = json!("%%14592");
        assert_eq!(parse(&win(vec![record])), Ok(vec![]));
    }
    #[test]
    fn audit_loss_clear_full_service_stop_and_policy_changes_fail_closed() {
        for id in [1100, 1101, 1102, 1104, 1108, 4612, 4719] {
            assert_eq!(
                parse(&win(vec![json!({"record":11,"id":id,"fields":{}})])),
                Err(format!("Security log/audit gap event {id}"))
            );
        }
    }
    #[test]
    fn overwritten_or_reused_security_anchor_fails_closed() {
        for (name, value) in [
            ("anchor", json!(false)),
            ("oldest", json!(11)),
            ("newest", json!(9)),
        ] {
            let mut capture = win(vec![]);
            capture[name] = value;
            assert_eq!(
                parse(&capture),
                Err("Security log window was cleared or overwritten".into()),
                "{name}"
            );
        }
    }
    #[test]
    fn security_records_must_be_ordered_and_inside_the_window() {
        for records in [vec![10], vec![21], vec![12, 11], vec![11, 11]] {
            let capture = win(records
                .into_iter()
                .map(|n| event(n, 5156, "4556", "127.0.0.1"))
                .collect());
            assert_eq!(
                parse(&capture),
                Err("Security log record order/window gap".into())
            );
        }
    }
    #[test]
    fn malformed_windows_fields_fail_instead_of_disappearing() {
        for (name, value, expected) in [
            ("ProcessID", json!("bad"), "invalid Security event PID"),
            (
                "Direction",
                json!("Outbound"),
                "unknown Security event direction",
            ),
            (
                "DestAddress",
                json!("not-an-IP"),
                "invalid Security event destination",
            ),
        ] {
            let mut record = event(11, 5156, "4556", "127.0.0.1");
            record["fields"][name] = value;
            assert_eq!(parse(&win(vec![record])), Err(expected.into()), "{name}");
        }
        for name in ["ProcessID", "Direction", "DestAddress"] {
            let mut record = event(11, 5156, "4556", "127.0.0.1");
            record["fields"].as_object_mut().unwrap().remove(name);
            assert_eq!(
                parse(&win(vec![record])),
                Err(format!("Security event lacks {name}"))
            );
        }
        assert_eq!(
            windows(b"{", 10),
            Err("Security log JSON: EOF while parsing an object at line 1 column 1".into())
        );
    }
    #[test]
    fn log_byte_and_record_limits_fail_instead_of_truncating() {
        let mut oversized = serde_json::to_vec(&win(vec![])).unwrap();
        oversized.resize(LOG_CAP as usize + 1, b' ');
        assert_eq!(
            windows(&oversized, 10),
            Err("Security log exceeds 1 MiB".into())
        );
        let events = (11..=11 + RECORD_CAP as u64)
            .map(|record| event(record, 5156, "4556", "127.0.0.1"))
            .collect();
        let mut capture = win(events);
        capture["newest"] = json!(11 + RECORD_CAP as u64);
        assert_eq!(
            parse(&capture),
            Err("Security log exceeds 4096 records".into())
        );
    }
}
