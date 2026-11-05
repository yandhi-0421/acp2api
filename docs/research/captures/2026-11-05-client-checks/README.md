# SC-003 client check levers

[The research note](../../2026-11-05-client-checks.md) gives the latest totals, all 19 variants, and three open client findings.
The third native run confirms the experimental advisor route, effort changes, and subagent recovery.
The sandbox dry run does not establish a Claude Code or Codex result.

## Public evidence

The maintainer keeps the raw captures outside the repository.
Each native run has one `evidence.ndjson` file with its identity and the cases that the research note cites.
Each line is one JSON record. A case record contains its ordered requests, response events, ACP configuration calls, and short client errors.
The summaries retain the original request IDs, times, status codes, and diagnostic headers.
`direct_user_markers` distinguishes the child prompt from markers inside Agent arguments or tool results.

SHA-256 values and byte lengths replace client prompts, tool definitions, descriptions, and other client instruction text.
A string fingerprint covers its exact UTF-8 bytes.
An object fingerprint covers JSON with sorted keys, UTF-8, no ASCII escapes, and separators `,` and `:` without spaces.

`field_names` lists the object's top-level field names. `test_markers` preserves our marker names without client prose.
Each tool has a `_definition` fingerprint; its original fields retain identifiers or fingerprints.
The `type`, `name`, and `model` fields identify the advisor declaration.
`source_files` contains hashes and byte lengths of the private evidence files.

The public set retains every `results.json` and `result.json` from the earlier unit.
The attribution text and advisor source excerpts in those results also have fingerprints.
[The result index](latest-results.json) identifies the latest native result for each variant.
The three summaries are [run 1](native-run-1/evidence.ndjson), [run 2](native-run-2/evidence.ndjson), and [run 3](native-run-3/evidence.ndjson).
[The scan result](redaction-scan.json) records the public file scan.
The public set contains no raw request log, full request index, full host transcript, or client transcript.

## Run outside the Codex sandbox

From this worktree, run:

```sh
sh docs/research/captures/2026-11-05-client-checks/run-all.sh
```

The default private output is `~/acp2api-build/239-native/<timestamp>/`.
Use `--output <new-folder>` to select another output folder.
The script refuses an existing output folder. Keep its raw output outside the repository.
Do not return the private scratch folder that its identity file names.

Allow about 20 to 30 minutes for the full set with the build cache.
The hard total cap is 3,000 seconds, with preparation.

Each variant prints one result line. Exit 0 means every selected check passed.
`FAIL` retains evidence. `BLOCKED` means that preparation or isolation prevented the check.

To repeat the third native selection, run:

```sh
sh docs/research/captures/2026-11-05-client-checks/run-all.sh --only 02-advisor-experimental 04-effort 07-subagent
```

`--only` accepts one or more names from the table below.
`--failed-from <private-results-file>` selects FAIL/BLOCKED cases in catalog order, with the same pass rules.
It cannot combine with `--only` or `--dry-run`.
The second native results file selects six cases; it does not select the experimental variant.

Preparation builds the workspace with `--locked`, then links [the research host](research-host.rs) against Cargo's reported library artifacts.
It uses `~/acp2api-build/target-239`. No build runs during a client check.
It installs [the pinned official SDK](requirements.txt) into `~/acp2api-build/tmp/239-client-checks/sdk-venv/`.

It then runs nine safety controls and three behavior controls.
[The behavior controls](round3-controls.py) use our synthetic records and a local terminal; they require no private capture fixture.
They preserve the parent/child completion check, Responses markers, and effort confirmation.

## Isolation and bounds

- Each real API client runs under `sandbox-exec`, then `env -i`, with fresh home and configuration folders.
- Each API client can connect only to the local HTTP recorder and refusal proxy.
- The proxy refuses every CONNECT. It opens no upstream connection.
- The host permits loopback connections only. Its launcher checks the fake executable's hash before each launch.
- The host calls `acp2api::run` with the acceptance adapters and production `SystemClock`.
- The host uses isolated account, data, and runtime home folders. It searches no real editor installation.
- The profile denies access to the real `.codex`, `.claude`, `.claude.json`, `.gemini`, and keychain folders.
- Codex configuration comes from literals. The lever reads neither real configuration nor `auth.json`.
- The advisor checks copy only `cachedGrowthBookFeatures` from `.claude.json`.
- Each check permits 32 HTTP requests, with auxiliary requests included. Request 33 receives a terminal refusal without a log entry.
- Each fake process permits 300 ACP lines. Its watchdog uses the case wall cap.
- The acceptance time factor is 2 for fake watchdogs only. It changes neither the product clock nor the client clock.
- The hub retains its 20,000-event and 16 MiB memory caps. Each log and child file has a 4 MiB cap.
- Each case scratch folder has a 128 MiB cap. Allocation counts use `lstat`; installed symlink targets do not count.
- The combined runtime scratch and capture cap is 3 GiB, below 4 GB. Shared Cargo and SDK caches are outside this budget.
- Each request body and response body has a 1 MiB cap. The relay forwards client disconnects to acp2api.
- Each case deadline stops its API client. Cleanup stops the host, then kills its process group if necessary.
- The total watchdog kills all registered process groups at 3,000 seconds. There is no shell EXIT trap.

The native preflight checks a denied TEST-NET connection before any client prompt.
If `sandbox-exec` cannot apply its profile, every selected check is `BLOCKED`.
The script never retries a real API client outside that profile.

## Checks and pass rules

The model is `gemini-3.8-flash`. The fake advertises low, medium, and high thinking levels.
Explicit prompt markers select the fault target by ACP session and call ID.
Auxiliary requests remain in private captures. A fault on an auxiliary request fails the check.

| Variant | Required result | Wall cap |
|---|---|---|
| `01-attribution` | The capture retains the first system text. ACP omits its billing prefix. The reply succeeds. | 90 s |
| `02-advisor` | The custom models produce an advisor declaration, Input tag rejection, advisor removal, and successful reply. | 90 s |
| `02-advisor-experimental` | The same models and pass rule apply, with CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL=1. | 90 s |
| `03-claude-keep-alive` | Pings continue through a 330-second text pause. Every event gap stays below 300 seconds. The reply succeeds. | 390 s |
| `03-codex-keep-alive` | response.in_progress events satisfy the same pause, gap, and completion rules. | 390 s |
| `04-effort` | The main output_config.effort values and ACP thinking_level values change from low to high after /effort high. | 90 s |
| `05-claude-lockout`, `05-codex-lockout` | One main request receives HTTP 400 and x-should-retry false. The client shows the state message and stops. | 45 s each |
| `05-claude-sign-in`, `05-codex-sign-in` | The same rule applies after the fake rejects the startup sign-in check. | 45 s each |
| `06-claude-retry`, `06-codex-retry` | The immediate Claude fallback receives 503. No prompt succeeds before 30 seconds. The later retry succeeds. Codex sends no earlier retry. | 120 s each |
| `07-print-before`, `07-print-after` | A fallback or resume receives the retry wait, then succeeds. After text, a partial delta and closed block precede the error. | 120 s each |
| `07-interactive-before` | The fallback receives a refusal during the retry wait, then succeeds after 30 seconds. | 120 s |
| `07-interactive-after` | No automatic resume occurs. A user follow-up receives a refusal during the retry wait, then succeeds. | 120 s |
| `07-subagent` | A real child resumes, receives the retry wait, and succeeds. Parent success does not end the check. | 150 s |
| `08-nonstream-310s` | The original no-stream request exceeds 300 seconds and succeeds. A disconnect or replacement fails the check. | 390 s |
| `09-openai-sdk` | The SDK parses both Chat Completions forms with SC239-OK and stop. The stream contains usage. | 45 s |

The long pauses include one ACP usage update without text. This prevents the FR-051 no-update cancellation.
After-text faults emit two ACP chunks and await the relay's post-flush acknowledgement before the error.
The 310-second case delays the first accepted no-stream prompt only. A replacement receives an immediate normal reply.
It retains the client's default timeout and retry options; its cap includes the preceding 30-second retry wait.
The terminal driver waits for actual effort confirmation and the saved-high message before the next prompt.

## Dry run and private analysis

To check preparation in the sandbox, run:

```sh
sh docs/research/captures/2026-11-05-client-checks/run-all.sh --dry-run --output /private/tmp/sc239-dry
```

This mode sends synthetic requests and two official SDK requests through the local product path.
It uses short pauses, real 30-second retry waits, and disconnect controls.
It sends no Claude Code or Codex prompt. `DRY-PASS` does not establish a native PASS.

[The native analysis lever](analyze-native.py) reads private captures and scratch metadata:

```sh
python3 -B docs/research/captures/2026-11-05-client-checks/analyze-native.py <private-run> <private-scratch> <new-private-analysis>
```

[The two-run comparison lever](summarize-rounds.py) also requires private inputs:

```sh
python3 -B docs/research/captures/2026-11-05-client-checks/summarize-rounds.py <first-private-run> <second-private-run> <new-private-analysis>
```

[The offline advisor lever](inspect-advisor.py) reads the installed executable and sends zero HTTP requests:

```sh
python3 -B docs/research/captures/2026-11-05-client-checks/safety.py <new-log> 60 python3 -B docs/research/captures/2026-11-05-client-checks/inspect-advisor.py <local-claude-executable> <new-private-output>
```

Its output contains private source excerpts. [The retained inspection result](advisor-source/result.json) replaces those excerpts with fingerprints.
The [native advisor summary](native-run-3/evidence.ndjson#L2) supplies the executed evidence for the experimental route.
The default advisor variant still fails before HTTP.
