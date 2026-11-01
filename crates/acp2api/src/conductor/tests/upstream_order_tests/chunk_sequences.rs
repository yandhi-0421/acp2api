use super::*;

// Fixed sequence from the second review; no entropy or real time.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }
}

#[test]
fn deterministic_chunks_errors_and_disconnects_match_whole_text() {
    const P: &[&str] = &[
        "403",
        "1403",
        "40350",
        " ",
        " ",
        "\n",
        ".",
        ",",
        "a",
        "中",
        "—",
        "Terms of Service",
        "Terms of ",
        "Service",
        "PERMISSION_DENIED",
        "Total ",
        "ok",
        "bbbbbbbbbbbbbbbbbbbbbbbbbb",
    ];
    let mut rng = Lcg(77);
    let (mut positive, mut negative) = (0, 0);
    let mut r = Turn::new();
    let journal = crate::journal::Journal::open(&r.root, std::time::UNIX_EPOCH).unwrap();
    for _ in 0..4000 {
        let mut text = String::new();
        for _ in 0..1 + rng.next() % 8 {
            text.push_str(P[rng.next() % P.len()]);
        }
        let want = crate::messages::lockout_signal(None, &text, None).is_some();
        let bounds: Vec<usize> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        let mut cuts = vec![0, bounds.len() - 1];
        for _ in 0..rng.next() % 5 {
            cuts.push(rng.next() % bounds.len());
        }
        cuts.sort();
        cuts.dedup();
        let chunks: Vec<&str> = cuts
            .windows(2)
            .map(|w| &text[bounds[w[0]]..bounds[w[1]]])
            .collect();
        let n = chunks.len();
        // error position: before chunk i, or after the end (n+1 means after prompt end)
        let error_at = rng.next() % (n + 2);
        let disconnect_at = rng.next() % (n + 3);
        r.reset(&journal);
        let mut t = 1;
        let mut gone = false;
        for (i, chunk) in chunks.iter().copied().map(Some).chain([None]).enumerate() {
            if i == error_at {
                t += 1;
                r.error(400, t);
            }
            if i == disconnect_at && r.core.work.by_session("running").is_some() {
                t += 1;
                r.disconnect(t);
                gone = true;
            }
            if let Some(chunk) = chunk {
                t += 1;
                r.text(chunk, t);
            }
        }
        if r.core.lockout.is_none() {
            t += 1;
            if gone {
                r.update(
                    TurnUpdate::PromptEnded {
                        stop_reason: Some(StopReason::Cancelled),
                    },
                    t,
                )
            } else {
                r.end(t)
            }
            if error_at == n + 1 && r.core.lockout.is_none() {
                t += 1;
                r.error(400, t);
            }
            if disconnect_at > n
                && !gone
                && r.core.lockout.is_none()
                && r.core.work.by_session("running").is_some()
            {
                t += 1;
                r.disconnect(t);
            }
        }
        assert_eq!(
            r.core.lockout.is_some(),
            want,
            "text={text:?} chunks={chunks:?} error_at={error_at} disconnect_at={disconnect_at}"
        );
        if want { positive += 1 } else { negative += 1 }
    }
    println!("deterministic Core cases positive={positive} negative={negative}");
}
