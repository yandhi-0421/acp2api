# SC-003 client checks: native results

**Original 18 variants: 14 PASS, 4 FAIL.**
**With the additional experimental advisor variant: 15 PASS, 4 FAIL across 19 variants.**
The third native run passed all three selected checks: experimental advisor, effort, and subagent recovery.
Four failed rows describe three client behaviors. No lever failure remains, and no product defect is established.
The owner decides product or PRD changes.

## Open client findings

1. **Default advisor rejection.** Claude Code rejects the custom main model before HTTP; [the short error line](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L2) records this result.
   The experimental route succeeds with `CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL=1`.
   Both model names remain `gemini-3.8-flash`.
   The client sends an advisor declaration, receives the `Input tag` rejection, removes the advisor, and succeeds.
   See [the two requests and responses](captures/2026-11-05-client-checks/native-run-3/evidence.ndjson#L2).
2. **State capability fallback.** Both Claude Code state variants send two main requests.
   Each response has HTTP 400, `invalid_request_error`, and `x-should-retry: false`.
   After the first state error, the client removes `thinking.display` and its beta, then repeats the request.
   It then shows the state message and exits.
   See [the lockout evidence](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L3) and [the sign-in evidence](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L4).
3. **No-stream disconnect.** Claude Code disconnects the original no-stream request, ID 3, after 300.037 seconds; its timeout header is 300.
   It sends one replacement, ID 4, which succeeds; [the request sequence](captures/2026-11-05-client-checks/native-run-2/evidence.ndjson#L5) retains the times and statuses.
   The original request still fails the check that requires success after more than 300 seconds.

## Latest results

Run 1: **5 PASS, 13 FAIL**. Run 2 selected those failures: **7 PASS, 6 FAIL**.
Run 3: **3 PASS, 0 FAIL**. The latest result uses the most recent native run for each variant.
A dash means that the run did not select the variant.
Class **a** means a lever defect; **b** means client behavior; **c** means a product defect.
The class applies to the latest failure. All four latest failures have class **b**.
The first 18 rows retain the original set. The last row is the additional experimental variant.

| Variant | Run 1 | Run 2 | Run 3 | Latest / class | Evidence |
|---|---|---|---|---|---|
| `01-attribution` | PASS | — | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-1/01-attribution/result.json#L9) |
| `02-advisor` | FAIL | FAIL | — | FAIL / b | [result](captures/2026-11-05-client-checks/native-run-2/02-advisor/result.json#L9) |
| `03-claude-keep-alive` | PASS | — | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-1/03-claude-keep-alive/result.json#L9) |
| `03-codex-keep-alive` | FAIL | PASS | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-2/03-codex-keep-alive/result.json#L9) |
| `04-effort` | FAIL | FAIL | PASS | PASS / — | [result](captures/2026-11-05-client-checks/native-run-3/04-effort/result.json#L9) |
| `05-claude-lockout` | FAIL | FAIL | — | FAIL / b | [result](captures/2026-11-05-client-checks/native-run-2/05-claude-lockout/result.json#L9) |
| `05-codex-lockout` | FAIL | PASS | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-2/05-codex-lockout/result.json#L9) |
| `05-claude-sign-in` | FAIL | FAIL | — | FAIL / b | [result](captures/2026-11-05-client-checks/native-run-2/05-claude-sign-in/result.json#L9) |
| `05-codex-sign-in` | FAIL | PASS | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-2/05-codex-sign-in/result.json#L9) |
| `06-claude-retry` | PASS | — | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-1/06-claude-retry/result.json#L9) |
| `06-codex-retry` | FAIL | PASS | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-2/06-codex-retry/result.json#L9) |
| `07-print-before` | PASS | — | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-1/07-print-before/result.json#L9) |
| `07-print-after` | FAIL | PASS | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-2/07-print-after/result.json#L9) |
| `07-interactive-before` | FAIL | PASS | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-2/07-interactive-before/result.json#L9) |
| `07-interactive-after` | FAIL | PASS | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-2/07-interactive-after/result.json#L9) |
| `07-subagent` | FAIL | FAIL | PASS | PASS / — | [result](captures/2026-11-05-client-checks/native-run-3/07-subagent/result.json#L9) |
| `08-nonstream-310s` | FAIL | FAIL | — | FAIL / b | [result](captures/2026-11-05-client-checks/native-run-2/08-nonstream-310s/result.json#L9) |
| `09-openai-sdk` | PASS | — | — | PASS / — | [result](captures/2026-11-05-client-checks/native-run-1/09-openai-sdk/result.json#L9) |
| `02-advisor-experimental` | — | — | PASS | PASS / — | [result](captures/2026-11-05-client-checks/native-run-3/02-advisor-experimental/result.json#L9) |

[The result index](captures/2026-11-05-client-checks/latest-results.json) retains these 19 rows and their source paths.

## Third native run: direct evidence

- **Experimental advisor:** request 1 declares `advisor_20260301` with the custom model; request 2 omits that tool and receives HTTP 200.
  See [the advisor summary](captures/2026-11-05-client-checks/native-run-3/evidence.ndjson#L2).
  The default advisor variant retains its separate FAIL result.
- **Effort:** main requests 2 and 4 carry `output_config.effort` values `low` and `high`.
  The ACP `session/set_config_option` calls set `thinking_level` to `low` and `high`.
  See [the request and ACP summary](captures/2026-11-05-client-checks/native-run-3/evidence.ndjson#L3).
  The auxiliary title requests do not establish the main request values.
- **Subagent:** the child receives partial text, an error, and a 503 response with `retry-after: 30`.
  Its accepted retry follows the error by 30.016 seconds, then succeeds.
  See [the child and parent request sequence](captures/2026-11-05-client-checks/native-run-3/evidence.ndjson#L4).
  Parent completion no longer ends the check before child recovery.

## Method and retained evidence

Date: 2026-11-05. Issue: #239. HEAD: `176253eb90e546e7e5d1e06fa658d5b9656d0ee0`.
Claude Code: 2.1.309. Codex: 0.163.0. Official Python OpenAI SDK: 3.46.0.
[The native identity record](captures/2026-11-05-client-checks/native-run-3/evidence.ndjson#L1) gives the client versions and executable hashes.
[The SDK requirements](captures/2026-11-05-client-checks/requirements.txt) pin its version.

All three native runs use the acceptance fake ACP agent and production `SystemClock`.
Real clients use isolated configuration, a made-up key, and a local refusal proxy.
No official runtime or external model endpoint serves these checks.
[The lever README](captures/2026-11-05-client-checks/README.md) defines the isolation, caps, and repeat commands.

The maintainer keeps the raw captures outside the repository.
Public summaries retain request order, times, status codes, diagnostic headers, field names, and our test markers.
SHA-256 values and byte lengths replace client prompts, tool definitions, descriptions, and other client instruction text.
[The README](captures/2026-11-05-client-checks/README.md#public-evidence) defines the hash format and the kept evidence.
The original pass rules and the four client failures remain unchanged.
