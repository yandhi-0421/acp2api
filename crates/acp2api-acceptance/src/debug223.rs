pub fn trace(event: &str, fields: impl std::fmt::Display) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LINES: AtomicUsize = AtomicUsize::new(0);
    if LINES.fetch_add(1, Ordering::Relaxed) >= 4096 {
        return;
    }
    let us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros();
    let mut line = format!(
        "[DEBUG-223] us={us} pid={} {event} {fields}",
        std::process::id()
    );
    while line.len() > 500 {
        line.pop();
    }
    eprintln!("{line}");
}
