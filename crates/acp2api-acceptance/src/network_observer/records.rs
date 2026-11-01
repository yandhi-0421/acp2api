use super::{Attempt, Destination, LOG_CAP, RECORD_CAP};
#[cfg(any(windows, test))]
use serde::Deserialize;
#[cfg(any(windows, test))]
use std::{collections::HashMap, net::IpAddr};

#[cfg(any(target_os = "macos", test))]
pub(super) fn macos(bytes: &[u8]) -> Result<Vec<Attempt>, String> {
    if bytes.len() as u64 > LOG_CAP {
        return Err("unified log exceeds 1 MiB".into());
    }
    let events: Vec<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| format!("unified log JSON: {e}"))?;
    if events.len() > RECORD_CAP {
        return Err("unified log exceeds 4096 records".into());
    }
    let mut attempts = Vec::new();
    for event in events {
        let kind = event["eventType"]
            .as_str()
            .ok_or("unified log lacks eventType")?;
        if kind == "lossEvent" {
            return Err("unified log reports lost records".into());
        }
        let message = event["eventMessage"]
            .as_str()
            .ok_or("unified log lacks eventMessage")?;
        let lower = message.to_ascii_lowercase();
        if ["dropped", "suppressed", "lost"]
            .iter()
            .any(|word| lower.contains(word))
        {
            return Err("unified log reports a record gap".into());
        }
        if !message.contains("network-outbound") {
            continue;
        }
        let (prefix, _) = message
            .split_once(" deny(")
            .ok_or("unrecognized sandbox outbound record")?;
        let (_, pid) = prefix.rsplit_once('(').ok_or("sandbox record lacks PID")?;
        let pid = pid
            .strip_suffix(')')
            .ok_or("invalid sandbox PID")?
            .parse()
            .map_err(|_| "invalid sandbox PID")?;
        attempts.push(Attempt {
            pid,
            destination: Destination::SandboxDeny,
        });
    }
    Ok(attempts)
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
pub(super) struct Cursor {
    pub record: u64,
    pub time: String,
}
#[cfg(any(windows, test))]
#[derive(Deserialize)]
struct WindowsCapture {
    anchor: bool,
    oldest: u64,
    newest: u64,
    events: Vec<WindowsEvent>,
}
#[cfg(any(windows, test))]
#[derive(Deserialize)]
struct WindowsEvent {
    record: u64,
    id: u16,
    fields: HashMap<String, String>,
}
#[cfg(any(windows, test))]
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
        attempts.push(Attempt {
            pid,
            destination: Destination::Address(destination),
        });
    }
    Ok(attempts)
}

#[cfg(test)]
mod tests {
    use super::super::Observation;
    use super::*;
    use serde_json::{Value, json};
    // Kernel deny text recorded by the orchestrator outside Codex, 2026-11-01.
    const DENY: &str = "Sandbox: Python(72753) deny(1) network-outbound remote:*:9";
    fn mac(message: &str) -> Vec<u8> {
        serde_json::to_vec(&json!([{"eventType":"logEvent","processID":0,"eventMessage":message}]))
            .unwrap()
    }
    // Field names and values follow Microsoft's recorded 5156/5157 XML examples.
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
    fn sandbox_pid_comes_from_the_message_not_the_kernel_pid() {
        let attempts = macos(&mac(DENY)).unwrap();
        assert_eq!(
            attempts,
            vec![Attempt {
                pid: 72753,
                destination: Destination::SandboxDeny
            }]
        );
        assert!(
            Observation::select(72753, attempts)
                .only_loopback()
                .unwrap_err()
                .starts_with("FR-046 outbound attempt")
        );
    }
    #[test]
    fn fake_agent_and_other_child_denials_do_not_count_as_product_connections() {
        assert!(
            Observation::select(100, macos(&mac(DENY)).unwrap())
                .only_loopback()
                .is_ok()
        );
        let capture = win(vec![
            event(11, 5157, "72753", "192.0.2.1"),
            event(12, 5156, "100", "127.0.0.1"),
        ]);
        assert!(
            Observation::select(100, parse(&capture).unwrap())
                .only_loopback()
                .is_ok()
        );
    }
    #[test]
    fn both_windows_success_and_failure_audits_fail_for_the_product_pid() {
        for id in [5156, 5157] {
            let capture = win(vec![event(11, id, "0x11cc", "192.0.2.1")]);
            let observation = Observation::select(4556, parse(&capture).unwrap());
            assert!(
                observation
                    .only_loopback()
                    .unwrap_err()
                    .starts_with("FR-046 outbound attempt")
            );
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
            let observation = Observation::select(
                4556,
                parse(&win(vec![event(11, 5156, "4556", address)])).unwrap(),
            );
            assert_eq!(observation.only_loopback().is_ok(), allowed, "{address}");
        }
    }
    #[test]
    fn inbound_wfp_records_are_not_outbound_attempts() {
        let mut record = event(11, 5156, "4556", "192.0.2.1");
        record["fields"]["Direction"] = json!("%%14592");
        assert!(parse(&win(vec![record])).unwrap().is_empty());
    }
    #[test]
    fn loss_and_suppression_in_unified_logs_fail_closed() {
        for value in [
            json!([{"eventType":"lossEvent"}]),
            json!([{"eventType":"logEvent","eventMessage":"Sandbox: 12 messages suppressed"}]),
            json!([{"eventType":"logEvent","eventMessage":"3 network-outbound messages dropped"}]),
        ] {
            assert!(macos(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
    #[test]
    fn malformed_or_incomplete_sandbox_records_fail_closed() {
        for message in [
            "network-outbound",
            "Sandbox: Python(?) deny(1) network-outbound",
            "Sandbox: Python72753 deny(1) network-outbound",
        ] {
            assert!(macos(&mac(message)).is_err(), "{message}");
        }
        for bytes in [b"".as_slice(), b"[", b"[{}]"] {
            assert!(macos(bytes).is_err());
        }
    }
    #[test]
    fn audit_loss_clear_full_service_stop_and_policy_changes_fail_closed() {
        for id in [1100, 1101, 1102, 1104, 1108, 4719] {
            assert!(
                parse(&win(vec![json!({"record":11,"id":id,"fields":{}})])).is_err(),
                "{id}"
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
            assert!(parse(&capture).is_err(), "{name}");
        }
    }
    #[test]
    fn security_records_must_be_ordered_and_inside_the_window() {
        for records in [vec![10], vec![21], vec![12, 11], vec![11, 11]] {
            let capture = win(records
                .into_iter()
                .map(|n| event(n, 5156, "4556", "127.0.0.1"))
                .collect());
            assert!(parse(&capture).is_err());
        }
    }
    #[test]
    fn malformed_windows_fields_fail_instead_of_disappearing() {
        for (name, value) in [
            ("ProcessID", json!("bad")),
            ("Direction", json!("Outbound")),
            ("DestAddress", json!("not-an-IP")),
        ] {
            let mut record = event(11, 5156, "4556", "127.0.0.1");
            record["fields"][name] = value;
            assert!(parse(&win(vec![record])).is_err(), "{name}");
        }
        for name in ["ProcessID", "Direction", "DestAddress"] {
            let mut record = event(11, 5156, "4556", "127.0.0.1");
            record["fields"].as_object_mut().unwrap().remove(name);
            assert!(parse(&win(vec![record])).is_err(), "{name}");
        }
        assert!(windows(b"{", 10).is_err());
    }
    #[test]
    fn log_byte_and_record_limits_fail_instead_of_truncating() {
        let oversized = vec![b' '; LOG_CAP as usize + 1];
        assert!(macos(&oversized).is_err());
        assert!(windows(&oversized, 10).is_err());
        let events = vec![event(11, 5156, "4556", "127.0.0.1"); RECORD_CAP + 1];
        assert!(parse(&win(events)).is_err());
        let events = vec![json!({"eventType":"logEvent","eventMessage":DENY}); RECORD_CAP + 1];
        assert!(macos(&serde_json::to_vec(&events).unwrap()).is_err());
    }
}
