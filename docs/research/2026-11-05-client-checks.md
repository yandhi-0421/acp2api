# SC-003 client checks: reviewed native evidence

**Latest reviewed results: 17 PASS, 0 FAIL.** These comprise 16 behavior checks and one result record.
The captures ran at `176253e`; this review applies the contract at `f4df342` without a native rerun.

On 2026-11-05, the owner removed advisor from [SC-003](../prd/acp2api.md#L1137).
Claude Code can retry a state refusal once after a capability removal; acp2api must reject both requests without an ACP prompt.
The no-stream check records the 300-second limit tracked in [#243](https://github.com/yandhi-0421/acp2api/issues/243).

## Reviewed results

Run 1: **8 PASS, 9 FAIL**. Run 2: **10 PASS, 2 FAIL**. Run 3: **2 PASS, 0 FAIL**.
The latest result uses the most recent capture for each retained variant.
A dash means that the run did not select the variant.
PASS for the no-stream row means that the record is complete; it does not require the original request to succeed.

| Variant | Run 1 | Run 2 | Run 3 | Latest | Source |
|---|---|---|---|---|---|
| `01-attribution` | PASS | — | — | PASS | [N1](captures/2026-11-05-client-checks/native-run-1/01-attribution/result.json#L9) |
| `03-claude-keep-alive` | PASS | — | — | PASS | [N1](captures/2026-11-05-client-checks/native-run-1/03-claude-keep-alive/result.json#L9) |
| `03-codex-keep-alive` | FAIL | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/03-codex-keep-alive/result.json#L9) |
| `04-effort` | FAIL | FAIL | PASS | PASS | [N3](captures/2026-11-05-client-checks/native-run-3/04-effort/result.json#L9) |
| `05-claude-lockout` | PASS | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/05-claude-lockout/result.json#L9) |
| `05-codex-lockout` | FAIL | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/05-codex-lockout/result.json#L9) |
| `05-claude-sign-in` | PASS | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/05-claude-sign-in/result.json#L9) |
| `05-codex-sign-in` | FAIL | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/05-codex-sign-in/result.json#L9) |
| `06-claude-retry` | PASS | — | — | PASS | [N1](captures/2026-11-05-client-checks/native-run-1/06-claude-retry/result.json#L9) |
| `06-codex-retry` | FAIL | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/06-codex-retry/result.json#L9) |
| `07-print-before` | PASS | — | — | PASS | [N1](captures/2026-11-05-client-checks/native-run-1/07-print-before/result.json#L9) |
| `07-print-after` | FAIL | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/07-print-after/result.json#L9) |
| `07-interactive-before` | FAIL | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/07-interactive-before/result.json#L9) |
| `07-interactive-after` | FAIL | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/07-interactive-after/result.json#L9) |
| `07-subagent` | FAIL | FAIL | PASS | PASS | [N3](captures/2026-11-05-client-checks/native-run-3/07-subagent/result.json#L9) |
| `08-nonstream-310s` | PASS | PASS | — | PASS | [N2](captures/2026-11-05-client-checks/native-run-2/08-nonstream-310s/result.json#L9) |
| `09-openai-sdk` | PASS | — | — | PASS | [N1](captures/2026-11-05-client-checks/native-run-1/09-openai-sdk/result.json#L9) |

[The index](captures/2026-11-05-client-checks/latest-results.json) maps each row to its source.
[The counts](captures/2026-11-05-client-checks/counts.json) distinguish behavior checks from the result record.
Each result preserves `recorded_status` and `recorded_error` from the earlier evaluation.
The review changes verdicts and records no new client execution.

## Recorded API client behavior

**State refusal.** Both API clients show the state message and stop. No ACP prompt starts in any of the four latest state cases.
Claude Code sends two main requests; its second request removes only `thinking.display` and the associated beta.
Both requests receive HTTP 400, `invalid_request_error`, and `x-should-retry: false`.
Codex sends one main request under each state.
Evidence: [Claude lockout](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L2), [Claude sign-in](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L4),
[Codex lockout](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L3), and [Codex sign-in](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L5).

**No-stream record.** Request 3 disconnects after 300.037 seconds with a timeout header of 300.
One replacement, ID 4, succeeds; [the run-2 record](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L6) retains both requests and their times.
The original request did not succeed. This record documents the known limit in #243.

The [run-1 record](captures/2026-11-05-client-checks/native-run-1/evidence.ndjson#L6) also retains a disconnect and replacement.
Its original run hit the 390-second deadline while the replacement was still active.
The reviewed record passes because those observations exist; the earlier wall-time failure remains in `recorded_error`.

**Effort.** Main requests 2 and 4 carry `output_config.effort` values `low` and `high`.
The ACP `session/set_config_option` calls set `thinking_level` to `low` and `high`.
[The effort summary](captures/2026-11-05-client-checks/native-run-3/evidence.ndjson#L2) separates those requests from auxiliary title requests.

**Subagent.** The child receives partial text, an error, and a 503 response with `retry-after: 30`.
Early retry ID 6 arrives after 29.979856 seconds, about 29.97 seconds, and receives 503 with `retry-after: 1`.
ID 7 succeeds after 30.016 seconds. Parent completion does not end the check before child recovery.
[The child sequence](captures/2026-11-05-client-checks/native-run-3/evidence.ndjson#L3) retains every attempt and response.

## Native script versions

Run 1 used earlier versions of both `runner.py` and `research-host.rs`.
The source column above identifies the run for each result; the hashes below identify its executed scripts.
Run 1 supplies five latest results, run 2 supplies ten, and run 3 supplies two.

| Native run | runner.py SHA-256 | research-host.rs SHA-256 |
|---|---|---|
| [N1](captures/2026-11-05-client-checks/native-run-1/evidence.ndjson#L1) | `4f6ba546ac18037fc5324d4f375e635e1ae013e4e3959c4ac34c9e1e987a41ef` | `772bc7e2427a335b26d6554d4e7cf31c17124237766e6625bddcc04e073a1a71` |
| [N2](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L1) | `1850de6d2b231c611badafe312e05c0e6dff03c5563a1f2ee2916dd88c8796e5` | `359b73bd522598b382c4c4b8c45523431e83ab306a8089ba7aab69045c4d699b` |
| [N3](captures/2026-11-05-client-checks/native-run-3/evidence.ndjson#L1) | `375709a01a2718dc6f0156dab76e0bc331801d8f18ff12ead5199c3ebf43d020` | `359b73bd522598b382c4c4b8c45523431e83ab306a8089ba7aab69045c4d699b` |

The nine run-1 failures reflect the earlier disk quota measurement and after-text injection.
The two run-2 failures reflect effort confirmation and early parent completion.
Run 3 supplies the successful effort and subagent captures. Their pass rules did not change.

## Evidence and checks

Claude Code: 2.1.309. Codex: 0.163.0. [The SDK requirements](captures/2026-11-05-client-checks/requirements.txt) pin OpenAI SDK 3.46.0.
All native runs use the acceptance fake ACP agent and production `SystemClock`.
The maintainer keeps the raw captures outside the repository.
SHA-256 values and byte lengths replace client prompts and tool text in the public evidence.

[The repeatable evidence check](captures/2026-11-05-client-checks/verify-evidence.py) checks public links, reviewed results, personal data, credentials, and known instruction prefixes.
Its optional private-input check verifies fingerprints, ACP prompt counts, ACP configuration calls, and the short error lines.
[The scan result](captures/2026-11-05-client-checks/redaction-scan.json) records its outcome.
[The validation summary](captures/2026-11-05-client-checks/validation.json) records the dry run and the retained controls.
[Safety controls](captures/2026-11-05-client-checks/self-check.py), [behavior controls](captures/2026-11-05-client-checks/round3-controls.py), and [contract controls](captures/2026-11-05-client-checks/contract-controls.py) define the retained lever checks.
[The README](captures/2026-11-05-client-checks/README.md) gives the commands and unchanged safety caps.
