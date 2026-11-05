# SC-003 client check levers

On 2026-11-05, the owner removed advisor from [SC-003](../../../prd/acp2api.md#L1137).
Claude Code can retry a state refusal once after a capability removal; acp2api must reject both requests without an ACP prompt.
The no-stream check records the 300-second limit tracked in [#243](https://github.com/yandhi-0421/acp2api/issues/243).

[The research note](../../2026-11-05-client-checks.md) reports 17 reviewed PASS results: 16 behavior checks and one result record.
The review uses retained captures. It starts no native API client.

## Run the checks

From this worktree, run:

```sh
sh docs/research/captures/2026-11-05-client-checks/run-all.sh
```

The default private output is `~/acp2api-build/239-native/<timestamp>/`.
Use `--output <new-folder>` to select another folder.
The script refuses an existing output folder. Keep raw output outside the repository.
Allow about 20 to 30 minutes for the full set with the build cache.

To repeat the two retained variants from native run 3, run:

```sh
sh docs/research/captures/2026-11-05-client-checks/run-all.sh --only 04-effort 07-subagent
```

`--only` selects catalog names. `--failed-from <reviewed-results-file>` selects FAIL/BLOCKED names in catalog order.
These options cannot combine with `--dry-run`.
An empty selection or a name outside the current catalog fails before preparation.
Exit 0 means that every selected check has PASS or DRY-PASS.

[The runner](runner.py) builds the workspace with `--locked`, then links [the research host](research-host.rs) against Cargo's reported library artifacts.
It uses `~/acp2api-build/target-239`. No build runs during an API client check.
[The official SDK driver](sdk-check.py) uses [the pinned dependencies](requirements.txt) in `~/acp2api-build/tmp/239-client-checks/sdk-venv/`.

## Current pass rules

The model name is `gemini-3.8-flash`. The fake advertises low, medium, and high thinking levels.
Direct user markers distinguish parent, child, and auxiliary requests.
A fault on an auxiliary request fails the check.

| Variants | Required observation | Wall cap |
|---|---|---|
| `01-attribution` | The capture retains the first system text. ACP omits its billing prefix. The reply succeeds. | 90 s |
| `03-claude-keep-alive`, `03-codex-keep-alive` | A 330-second pause has pings or response.in_progress events. Every event gap stays below 300 seconds. The reply succeeds. | 390 s each |
| `04-effort` | Main output_config.effort values and ACP thinking_level values change from low to high after /effort high. | 90 s |
| `05-claude-lockout`, `05-claude-sign-in` | At most two main requests receive the state refusal. Only a capability removal permits the second request. No ACP prompt starts. | 45 s each |
| `05-codex-lockout`, `05-codex-sign-in` | One main request receives the state refusal. No ACP prompt starts. | 45 s each |
| `06-claude-retry`, `06-codex-retry` | No prompt succeeds before 30 seconds. The later retry succeeds. Codex sends no earlier retry. | 120 s each |
| `07-print-before`, `07-print-after` | The fallback or resume receives the retry wait, then succeeds. After text, a partial delta and closed block precede the error. | 120 s each |
| `07-interactive-before` | The fallback receives a refusal during the retry wait, then succeeds after 30 seconds. | 120 s |
| `07-interactive-after` | No automatic resume occurs. A user follow-up receives a refusal during the retry wait, then succeeds. | 120 s |
| `07-subagent` | The child resumes, receives refusals before 30 seconds, then succeeds. Parent success does not end the check. | 150 s |
| `08-nonstream-310s` | The capture records the original no-stream request, its end or disconnect, and every replacement. Client success is not a behavior gate. | 390 s |
| `09-openai-sdk` | The SDK parses both Chat Completions forms with SC239-OK and stop. The stream contains usage. | 45 s |

Every state refusal must have HTTP 400, invalid_request_error, and x-should-retry false.
Both API clients must show the state message and stop.
For Claude Code 2.1.309, the observed capability removal is thinking.display=updates and its associated beta.
The lever rejects any other body change or unrelated diagnostic header change.

Long pauses include an ACP usage update without reply text, as FR-051 requires.
After-text faults emit two ACP chunks and await the relay's post-flush acknowledgement before the error.
The long no-stream case delays the first accepted no-stream prompt only; a replacement receives an immediate normal reply.
It retains the API client's default timeout and retry options.

## Safety and retained controls

[The safety code](safety.py) supplies redaction, process cleanup, deadlines, and bounded logs.
[The safety controls](self-check.py) check the HTTP cap, refusal proxy, redaction, timeout, log cap, terminal, file cap, and scratch allocation.
[The behavior controls](round3-controls.py) check parent/child completion, Responses markers, and effort confirmation with our synthetic records.
[The contract controls](contract-controls.py) check state refusals, capability removal, zero ACP prompts, the no-stream record, and the 17-variant catalog.
Preparation runs all three control scripts before any API client prompt.
[The validation summary](validation.json) retains their results and the dry-run results.

- Real API clients run under sandbox-exec and env -i, with fresh home and configuration folders.
- API clients can reach only the local recorder and refusal proxy. The proxy refuses every CONNECT without an upstream connection.
- The host permits loopback connections only. Its launcher checks the fake executable's hash before each launch.
- The profile denies access to real configuration, credentials, and keychain folders. No check reads the real Claude or Codex configuration.
- Each case permits 32 HTTP requests, with auxiliary requests included. Each request and response body has a 1 MiB cap.
- Each fake process permits 300 ACP lines. Its watchdog uses the case wall cap; the acceptance factor changes fake watchdogs only.
- The hub retains 20,000 events and a 16 MiB cap. Every log and child file has a 4 MiB cap.
- Each case scratch folder has a 128 MiB cap. Allocation counts use lstat and exclude installed symlink targets.
- Combined runtime scratch and captures have a 3 GiB cap, below 4 GB. Shared Cargo and SDK caches are outside this budget.
- Case deadlines stop API clients and hosts. The total watchdog kills registered process groups at 3,000 seconds, with preparation included.

A TEST-NET denial check precedes every native prompt.
If sandbox-exec cannot apply its profile, every selected check is BLOCKED.
The script never retries a real API client outside that profile.

## Reproduce the dry run and evidence checks

To exercise the local product path without native prompts, run:

```sh
sh docs/research/captures/2026-11-05-client-checks/run-all.sh --dry-run --output /private/tmp/sc239-dry
```

This mode runs 12 checks with synthetic requests and two real official SDK requests.
It uses short pauses, real 30-second retry waits, and disconnect controls.
DRY-PASS does not establish a native API client result.

To repeat the reviewed verdicts without a native rerun, run:

```sh
python3 -B docs/research/captures/2026-11-05-client-checks/summarize-rounds.py
```

[The review script](summarize-rounds.py) shares the state and record predicates with the runner.
Its default mode compares the reviewed files byte-for-byte. `--write` regenerates them.
It preserves original status and error fields as recorded_status and recorded_error.
Unchanged behavior rules retain their original verdicts.

To repeat the public scan and link checks, run:

```sh
python3 -B docs/research/captures/2026-11-05-client-checks/verify-evidence.py
```

[The evidence checker](verify-evidence.py) uses [hashed instruction signatures](prompt-signatures.json); it contains no client prompt text.
The public scan checks known instruction prefixes, personal data, credentials, local links, and reviewed result consistency.
For a private fingerprint and ACP transcript comparison, add `--raw-root <private-239-native-folder>`.
The command never copies the private input.
[The scan result](redaction-scan.json) records the full check with the maintainer's private input.

## Evidence format

The maintainer keeps raw captures outside the repository.
The public set keeps native result files and one summary per run: [N1](native-run-1/evidence.ndjson), [N2](native-run-2/evidence.ndjson), and [N3](native-run-3/evidence.ndjson).
Each summary starts with identity and script hashes. Each later line contains one cited case.
It retains ordered requests, times, status codes, diagnostic headers, ACP counts, configuration calls, and short errors.
[The result index](latest-results.json) and [counts](counts.json) identify the reviewed sources and totals.

Client prompts, tool definitions, descriptions, and other client instruction text have SHA-256 values and byte lengths.
String hashes cover exact UTF-8 bytes. Object hashes cover sorted-key JSON without ASCII escapes or spaces after commas and colons.
field_names lists top-level keys; test_markers retains our marker names; each tool has a complete _definition fingerprint.
source_files identifies the private evidence by hash and byte length.
The public set contains no raw request log, full request index, host transcript, or client transcript.

[The private analysis lever](analyze-native.py) remains available for scratch metadata and the original disk-cap diagnosis.
It requires private raw inputs and writes its output outside the repository.
