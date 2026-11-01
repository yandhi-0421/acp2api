use super::*;
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}
const P: &[&str] = &[
    "403",
    "403",
    "403",
    "1403",
    "40350",
    "4",
    "03",
    "40",
    "3",
    ".",
    "..",
    "-",
    "_",
    " ",
    " ",
    ",",
    "\n",
    ")",
    "(",
    ":",
    "a",
    "A",
    "9",
    "é",
    "中",
    "—",
    "。",
    "😀",
    "\u{a0}",
    "aaaaaaaaaaaaaaaaaaaaaaaa",
    "bbbbbbbbbbbbbbbb",
    "Terms of Service",
    "Terms of ",
    "Service",
    "PERMISSION_DENIED",
    "PERMISSION",
    "_DENIED",
    "Total ",
    "403.",
    "403-",
    "-403",
    ".403",
    "403_",
];
fn old(message: &str) -> Option<&'static str> {
    if message
        .split(|c: char| !c.is_alphanumeric() && !matches!(c, '_' | '.' | '-'))
        .any(|token| token.strip_suffix('.').unwrap_or(token) == "403")
    {
        Some("403")
    } else if message.contains("PERMISSION_DENIED") {
        Some("PERMISSION_DENIED")
    } else if message.contains("Terms of Service") {
        Some("Terms of Service")
    } else {
        None
    }
}
#[test]
fn deterministic_chunk_splits_keep_the_status_token_rules() {
    let mut rng = Lcg(0x203);
    let mut bad: Vec<String> = Vec::new();
    for _ in 0..2_000 {
        let mut text = String::new();
        for _ in 0..1 + rng.below(12) {
            text.push_str(P[rng.below(P.len())]);
        }
        let whole = crate::messages::lockout_signal(None, &text, None);
        assert_eq!(whole, old(&text), "classifier moved on {text:?}");
        let bounds: Vec<usize> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        for _ in 0..8 {
            let max = 1 + rng.below(9);
            let mut at = 0;
            let mut chunks = Vec::new();
            while at + 1 < bounds.len() {
                let step = rng.below(max + 1);
                let next = (at + step).min(bounds.len() - 1);
                chunks.push(&text[bounds[at]..bounds[next]]);
                at = next;
            }
            let mut s = ReplySignals::default();
            let mut early = None;
            for chunk in &chunks {
                if let Some(found) = s.observe(chunk, false) {
                    early.get_or_insert(found);
                }
                assert!(s.tail.len() <= 17, "tail bound");
            }
            let end = s.observe("", true);
            if early.is_some() && whole.is_none() {
                bad.push(format!("EARLY FALSE {text:?} {chunks:?}"));
            }
            if end.is_some() != whole.is_some() {
                bad.push(format!(
                    "DIFF whole={whole:?} stream={end:?} {text:?} {chunks:?}"
                ));
            }
        }
        if bad.len() >= 5 {
            break;
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
