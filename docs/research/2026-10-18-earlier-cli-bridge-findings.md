# Findings of the earlier CLI bridge that matter for acp2api

Date checked: 2026-10-18 (Asia/Taipei).

Provenance: This note collects facts from the notes, the plans, and the issues of an earlier project of the maintainer. That project was a bridge from the same three APIs to the Antigravity CLI (`agy`). It will be deleted, so its facts that matter for acp2api moved here on 2026-10-18. The note keeps only facts that touch the official runtime, the Gemini models, the Google account, the API clients, or a probe method. Each fact names its source in plain text. The sources are these old files and issues:

- the research notes `2026-10-03-antigravity-bridge-prior-art.md`, `2026-10-03-antigravity-local-engine-interface.md`, `2026-10-04-antigravity-cli-live-probe.md`, `2026-10-04-antigravity-quota-signals.md`, `2026-10-04-antigravity-tool-contract.md`, `2026-10-05-antigravity-cli-auto-update.md`, `2026-10-05-antigravity-mcp-passthrough-prototype.md`, the first live acceptance note (2026-10-06), and the second live acceptance note (2026-10-06);
- the two ADRs `0001-native-engine-bridge.md` and `0002-native-engine-harness-tools.md`, the glossary, the specification `mvp-spec.md`, and the code at commit `7bc620a`;
- the plan `acp-engine-plan.md` of 2026-10-15, its issues 41, 81, 84, and 86 to 94, and the draft note of its pull request 95 (`2026-10-16-windows-exploration.md`, never merged).

Four notes of the same project moved as full copies: [2026-10-03-antigravity-proxy-landscape.md](2026-10-03-antigravity-proxy-landscape.md), [2026-10-05-api-endpoint-contract.md](2026-10-05-api-endpoint-contract.md), [2026-10-13-gemini-thinking-controls.md](2026-10-13-gemini-thinking-controls.md), and [2026-10-15-official-runtime-acp-probe.md](2026-10-15-official-runtime-acp-probe.md). The results of its last probe are in [2026-10-15-official-runtime-probe-p.md](2026-10-15-official-runtime-probe-p.md). Its test ideas are in [2026-10-18-test-ideas-from-the-earlier-cli-bridge.md](2026-10-18-test-ideas-from-the-earlier-cli-bridge.md).

Words of the earlier project:

- The earlier bridge: the earlier CLI bridge of the maintainer.
- The maintainer: the person who ran the earlier bridge and who maintains acp2api. The old word was "operator".
- Harness: an API client, for example Claude Code or Codex. In acp2api, a harness process is a different thing.
- Engine: the Antigravity CLI in headless mode, or later the official runtime.
- Cascade: a conversation of the CLI engine. It is not an ACP session.
- CLI turn: the work of the CLI for one input, up to its `result` event.
- Agent file, locked profile: a file of the CLI that gives an agent its system text and its tools. The locked profile had no tools. The official runtime ignores agent files.
- Tool catalog, catalog update: the text in the agent file that listed the client tools, and the text that announced a new tool set.
- Tool step, planner response, step check: a unit of the stream of the CLI for one tool call; a unit for one model response; the check of the earlier bridge that stopped a CLI turn at an unexpected tool step.
- Held call: a `tools/call` that the earlier bridge kept open until the API client sent the tool result.
- Fail-stop: the stop of the earlier bridge after an account-level signal. It is near to the acp2api lockout, but its signals differ.
- Quota stop: the refusal of the earlier bridge for a model group at a quota threshold. It is near to the acp2api quota wait, but it had a threshold.
- Model group: a set of models that share one quota: the Gemini models, and the Claude and GPT models.
- Version test: the old word was "acceptance test". The earlier bridge ran it for each new version of its engine. acp2api has the compatibility check instead.
- Information session: an ACP session with no tools that the plan used for `/usage`. The old word was "info session".
- Place: one of the prompt turns that can run at the same time. The old word was "slot".
- Gen 1, Gen 2, Gen 3: the three patterns of Antigravity proxies, see part 2.
- Egress: the network path and the IP address of the traffic to Google.
- Probe P: the last probe of the earlier bridge before its build, see [2026-10-15-official-runtime-probe-p.md](2026-10-15-official-runtime-probe-p.md).
- Run and probe IDs such as E1, L1 to L4, P1, I1, M1, H1, C1 to C5, and findings F2 to F13 are names in the old notes. They are not the F and W findings of the acp2api issues 8 and 16.

The note has three parts:

- Part 1: the CLI engine, the models, and the API clients.
- Part 2: the Antigravity proxies, the bans, and the reasons of the earlier bridge for its architecture.
- Part 3: the Windows exploration and the ACP engine plan. Its IDs WIN-, RT-, Q-W, Q-, PRD-, and CON- are new. Each PRD- row is a choice of the earlier bridge and an open point for the PRD of acp2api.

## Part 1: the CLI engine, the models, and the API clients

### How to read part 1

Part 1 collects facts from seven notes of the earlier CLI bridge. The notes are from 2026-10-04 to 2026-10-08. Each fact has these lines:

- **Source:** the old file name and the section heading. A heading in quotation marks is quoted data.
- **Evidence:** one of these marks.
  - live run: a run of the Antigravity CLI `agy`, with the CLI version and the date (Asia/Taipei).
  - static: the strings of the CLI program, or the spec or the code of the earlier bridge.
  - documentation: an official page of Google, or the changelog of the CLI.
  - user report: a statement of the maintainer, or a report in a public tracker.
  - inference: a conclusion of the source note that no run proved.
- **Touches:** the acp2api issue or ADR that the fact touches.
- **acp2api:** a line only where acp2api decided or found something different.

The CLI is a different program from the official runtime. A fact about the CLI is evidence for the official runtime only where this part says so. A part of a fact that this distillation adds is marked "(inference of this part)".

### The rebuild after a model change to the other model group

- **The rebuild passed in a live run.** In the first live acceptance session, a request changed the model of a Gemini Cascade to a Claude model. The result row says: "Pass. Rebuild with the full transcript. The Claude model gave the first answer of the Gemini Cascade." The log reason was `no Cascade record matches this conversation history`.
  - Source: the first live acceptance note (2026-10-06), section "Results", row "Model change to the other model group".
  - Evidence: live run (CLI 1.2.17, 2026-10-06, code of the earlier bridge at commit `7bc620a`). The row does not name the Claude model. The note does not give the transcript, so it does not show if the conversation had tool calls, images, or a system prompt.
  - Touches: [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23), [Which ACP session does acp2api use for each API request?](https://github.com/yandhi-0421/acp2api/issues/10), [How does a client tool call pause and continue a prompt turn?](https://github.com/yandhi-0421/acp2api/issues/11).
- **The rebuild text and its role labels.** At commit `7bc620a`, a rebuild of the earlier bridge started a new Cascade. It sent the full conversation as one user message, in this order:
  1. the neutral line `The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.`;
  2. one blank line;
  3. one line for each item, with the role label and the text on the same line: `[user] <text>`, `[assistant] <text>`, `[tool call <name>] <arguments>`, and `[tool result <name>] <text>`.
  - The user text held one line `[Image: <path>]` for each image. The spec says that an image in a tool result becomes an attachment path line.
  - The rebuild text had no item for the system prompt. The earlier bridge wrote the system prompt into the agent file of each new Cascade, under the heading `# Harness instructions`.
  - When no earlier tool call had the ID of a tool result, the label of the result kept the call ID of the API client.
  - Source: the spec of the earlier bridge, `mvp-spec.md`, sections "Conversations" (item "Rebuild") and "Workspace and permissions"; the code at commit `7bc620a`, function `renderRebuildTranscript`. These files are not among the seven notes. The row of the live note does not give the labels.
  - Evidence: static.
  - acp2api: each role label is a line of its own, a tool call and a tool result are sentences, and the system prompt stays out of the rebuild text; the neutral line is the same. See [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23) and [How does a client tool call pause and continue a prompt turn?](https://github.com/yandhi-0421/acp2api/issues/11).
- **When the earlier bridge did a rebuild.** The earlier bridge did a rebuild in these cases:
  - no record matched;
  - an edited or removed message;
  - a compaction;
  - a tool result of a held call that ended;
  - a model change to the other model group.

  A model change in the same group continued the Cascade. A change of the system prompt alone gave no rebuild, and the agent file of a Cascade did not change.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Recommended passthrough design", item "Model change"; the spec, section "Conversations", items "Rebuild cases" and "System text change".
  - Evidence: static (decisions of the earlier bridge).
  - acp2api: a new system prompt or a new set of client tools causes a rebuild, and a new model keeps the ACP session, see ADR-0003; Claude and GPT model names get a rejection, see [Which model names does the API of acp2api offer?](https://github.com/yandhi-0421/acp2api/issues/13).

### Google quota windows

- **Two model groups, two windows each.** The model selector in the official documentation shows two groups: Gemini models, and Claude and GPT models. Each group has a weekly remainder and a five-hour remainder. The labels give the percentage left, not the percentage used.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Quota evidence".
  - Evidence: documentation (the Models page, read 2026-10-04).
  - Touches: [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15). The slash command `/usage` of the official runtime gives the same groups and windows ([Probe the token count, the PDF block, and the slash commands of the official runtime on macOS](https://github.com/yandhi-0421/acp2api/issues/26)).
- **The plans page.** Paid plans have five-hour refreshes and weekly limits. For Pro, the five-hour allowance refreshes until the weekly limit applies. Quota cost depends on agent work, so prompt counts alone cannot establish quota use. Google can change the limits.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Quota evidence".
  - Evidence: documentation (the Plans page).
- **No stable schema for the quota output.** The documentation of `/usage` (also named `/quota`) gives a TUI panel and no stable JSON schema.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Quota evidence".
  - Evidence: documentation.
  - Touches: [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15), decision 5: acp2api reads the reset time from the error text or from `/usage`.
- **A five-hour window probably starts at the first use of its group.**
  - On 2026-10-04, the first `/usage` query gave the five-hour reset time 2026-10-04T11:11:15Z for both groups.
  - At 15:45, after the model probes with the default model, the Gemini five-hour reset time stayed at 11:11Z. The reset time of the Claude and GPT group moved to the time of the query plus five hours.
  - On 2026-10-05, the Gemini five-hour reset time moved to 11:20Z, five hours after the first request of the day.
  - On 2026-10-05 at 15:31, the five-hour reset time of the Claude and GPT group was again the time of the query plus five hours (2026-10-05T12:31:24Z).
  - The weekly reset times stayed the same on both days: 2026-10-10T08:22:48Z for the Gemini group, and 2026-10-10T08:24:13Z for the Claude and GPT group.
  - Source: `2026-10-04-antigravity-cli-live-probe.md`, sections "Quota output", "Quota after sets B and C", and "Project permissions"; `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Requests and quota".
  - Evidence: live run (CLI 1.2.16 and 1.2.17). The notes call the start at the first use a hypothesis that these observations agree with. They also conclude that the default model of the CLI was in the Gemini group (inference).
- **Quota left during the probes.** All values are from `/usage`.

  | Time (Asia/Taipei) | CLI | Gemini weekly | Gemini five-hour | Claude and GPT weekly | Claude and GPT five-hour |
  | --- | --- | --- | --- | --- | --- |
  | 2026-10-04, before the model probes | 1.2.16 | 100% | 100% | 100% | 100% |
  | 2026-10-04 15:45 and 16:16 | 1.2.16 | 100% | 100% | 100% | 100% |
  | 2026-10-05 14:50 | 1.2.17 | 100% | 100% | 100% | 100% |
  | 2026-10-05 15:31, after 16 CLI requests in the Gemini group | 1.2.17 | 100% | 99% | 100% | 100% |
  | 2026-10-06 13:44 | 1.2.17 | 100% | 100% | 99% | 97% |
  | 2026-10-06 17:57, after approximately 26 model requests; the 13 of part 3 used `claude-sonnet-4-6` | 1.3.0 | 100% | 100% | 96% | 92% |
  | 2026-10-06 18:58 | 1.3.0 | 100% | 100% | 96% | 99%, after its reset |
  | 2026-10-08, before and after the isolation check | 1.3.0, then 1.3.1 | 100% | 100% | 96% | 100% |

  - Source: `2026-10-04-antigravity-cli-live-probe.md`, sections "Quota output" and "Quota after sets B and C"; `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Requests and quota"; the first live acceptance note (2026-10-06), section "After the session"; the second live acceptance note (2026-10-06), sections "After the session" and "Memory files and safety".
  - Evidence: live run.
- **A quota threshold cannot be a hard limit.** One active request can cross a threshold before the next observation.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Quota policy".
  - Evidence: inference.
- **The quota policy of the earlier bridge.** The maintainer chose these rules for the earlier bridge:
  - A quota stop at 75% used (25% left), in each model group and each window separately. A stop of one group did not stop the other group.
  - A refusal for a model with no known group, and for quota data that is absent, stale, or ambiguous.
  - `agy -p /usage` at most one time in 15 minutes while the bridge served requests, and no refresh while it was idle. Data older than 30 minutes caused a refresh before the request.
  - After a stop, one refresh after the reset time of that group.
  - Accepted risk: in heavy use, the real stop can be approximately 10 percentage points above 75%.
  - An error about a daily cap, a spend cap, or a low AI credits balance also started a quota stop of the group of the requested model. With the 75% threshold, such an error must not occur, so the bridge logged it as a quota stop that came too late.
  - Amendment 2026-10-08: the threshold became a setting from 1 through 90. When the maintainer turned the quota stop off, a fixed 90% backstop applied.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Quota policy".
  - Evidence: static (decisions of the earlier bridge).
  - acp2api: no threshold; a quota wait starts only after a quota signal (429 `RESOURCE_EXHAUSTED`, 402, or the stop reason `refusal`), see [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15), decisions 2 and 5.

### Google account signals and error kinds

- **An account block can come as a sign-in error.** The CLI changelog gives these facts:
  - 1.2.14 shows a sign-in error to accounts that Google blocked for a Terms of Service violation. The error contains a link to an appeal.
  - 1.2.5 signs the CLI out and clears the stale tokens when the sign-in server definitively rejects the credentials. Temporary network failures, server errors, and rate limits do not sign the CLI out.
  - 1.2.15 stops at once with `Your AI credits balance is too low to continue.` when the plan quota is empty and the AI credits cannot pay for the request. The note concludes that the CLI uses AI credits after the plan quota is empty.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Error evidence and limits".
  - Evidence: documentation (CLI changelog). The use of AI credits is an inference of the note.
  - Touches: [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15). Its table starts a lockout for a sign-in error that also has a 403 or a Terms of Service text.
  - acp2api: the table of signals names no AI credits text, see [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15), decision 2.
- **Error kinds that the CLI changelog names.** The CLI retries transient errors, for example 502, 503, 504, and a per-minute 429. Version 1.2.13 respects the retry delays of the service. It stops for delays above 30 seconds, and for daily caps and spend caps.
  - No reviewed source guarantees that the CLI never retries a 403 before it shows an error. A stop after an observable error does not prove an immediate stop in the CLI.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Error evidence and limits".
  - Evidence: documentation (CLI changelog, 1.2.1 and 1.2.13).
  - Touches: [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15).
- **The fail-stop and quota-stop signals of the earlier bridge.** The maintainer chose these signals on 2026-10-05. The reason: a false stop costs one action of the maintainer, but a missed stop can cost the account.

  | Signal in a CLI output | The earlier bridge | acp2api |
  | --- | --- | --- |
  | HTTP status `403` | fail-stop | lockout |
  | Canonical status `PERMISSION_DENIED` | fail-stop | lockout |
  | A text about a Terms of Service block, an appeal, or a suspension | fail-stop | lockout for the text "Terms of Service" in an error channel; the reply text alone starts no lockout |
  | An unexpected sign-out: not signed in, `401`, `UNAUTHENTICATED`, or invalid or expired stored credentials | fail-stop | the state "sign-in required", not a lockout |
  | A daily cap, a spend cap, or a low AI credits balance | quota stop of the model group | quota wait for 429 `RESOURCE_EXHAUSTED`, 402, or the stop reason `refusal` |
  | Other errors, for example a retryable `5xx` or a per-minute `429` | error to the API client, no stop | `400`, `5xx`, a timeout, or an exit: no lockout; 429 `RESOURCE_EXHAUSTED`: quota wait |

  - Other rules of the earlier bridge: the fail-stop stayed after a restart, and only the maintainer removed it, after a check of the account. Its error had a status code that API clients do not retry automatically. The bridge did no retry of its own. The only alert was the error to the API client, with no notification to an external service.
  - Source: `2026-10-04-antigravity-quota-signals.md`, sections "Fail-stop policy" and "Alert".
  - Evidence: static (decisions of the earlier bridge).
  - Touches: [How does acp2api report an upstream error and a cancelled prompt turn to each API?](https://github.com/yandhi-0421/acp2api/issues/25) (how acp2api stops the retries of Claude Code and Codex is open).
  - acp2api: a sign-in error starts the state "sign-in required", and only an error channel can start a lockout, see [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15).
- **The host of the model requests.** The CLI log shows that the CLI sends its model requests to `daily-cloudcode-pa.googleapis.com`.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Other facts".
  - Evidence: live run (CLI 1.2.17 log, 2026-10-05).
  - Touches: [What do the Google terms and the February 2026 account actions say about third-party ACP Clients?](https://github.com/yandhi-0421/acp2api/issues/4).
- **An idle CLI process still calls Google.** During a hold of 26 minutes with no model request, the CLI log shows only `loadCodeAssist` and `fetchAvailableModels` calls, every 6 minutes. It shows no `streamGenerateContent` call. The model request after the wait succeeded.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "2. 30-minute hold".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15) (a live process can still connect to Google), ADR-0004 (no idle stop). No acp2api record shows if the official runtime makes such periodic calls.

### What the models did

- **The system prompt in the agent file reached the model.** The system message of the API client was `You are a terse assistant. End every reply with the token ZEBRA-19 on its own line.` The question was `What is 2 + 3?`. The reply was `2 + 3 is 5.` and then `ZEBRA-19` on its own line.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "7. System prompt in the agent file".
  - Evidence: live run (CLI 1.2.17, default model, 2026-10-05).
  - Touches: [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23).
- **The CLI fixes the system prompt when a Cascade starts.** A new agent file, or a different agent at a resume, did not change it. In run L3b, a new agent body asked for the token `OKAPI-42` and added the tool `get_tide(lookup_key_q7)`. The reply had no `OKAPI-42`, and the model called `get_tide` with the argument `harbor`.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, sections "3. Tool set changes" and "7. System prompt in the agent file".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - acp2api: the system prompt goes in `_meta.antigravity.systemPrompt.append` at `session/new` and at each `session/load`, and a new system prompt causes a rebuild, see [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23) and ADR-0003.
- **Without the tool schemas, the model guesses names.** In runs E1 and E2, the agent body had no tool catalog. The model called `ServerName: "weather"` with `Arguments: {"location": "Lisbon"}`. The schema had the parameter `city`, and the server name was `bridge-probe`.
  - E1 ended with `TOOL_ERROR`: `tool get_weather is not enabled for server weather`. E2 made the same call, then an `unknown` step with `ERROR`, then `list_resources` with an empty `ServerName`.
  - The token counts show that the schemas were not in the context. E1 and E2 had 1211 input tokens with two tools. Set C had 1230 with one tool and a longer prompt.
  - With the tool catalog in the agent body, the count was 1445. Run E3 then called `ServerName: "bridge-probe"` with `Arguments: {"city": "Lisbon"}`.
  - In runs L2 and L3a, the model called a tool that the new catalog removed, or called `get_time` with `zone`, not `tz_name`. L3a is not conclusive, because the history held a successful call with `zone`.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, sections "Tool catalog" and "3. Tool set changes".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: ADR-0002 (without `eagerMcpServers`, the model of the official runtime reads the MCP tool schemas with `view_file`, and the tool filter removes `view_file`).
- **Tool definitions in a user message reach the model. In a tool result, they did not.**
  - Run L4: the earlier bridge put a catalog update at the start of a user message. The update added `get_moon(iso_day_k9)`. The model called `get_moon` with `iso_day_k9`.
  - Finding F3: the catalog update went as one more text part of a tool result. After the update added `get_time`, the model listed only `get_weather`. In the next request, it said that it had no time tool.
  - Fix item 11, on `gemini-3.8-flash-low` through `/v1/chat/completions`: the update went into the tool result, and again at the start of the next user message (`[Tool update]` with the full new catalog). The model then called `get_time`.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "3. Tool set changes"; the first live acceptance note (2026-10-06), section "Findings", F3; the second live acceptance note (2026-10-06), section "Catalog update (item 11)".
  - Evidence: live run (CLI 1.2.17 for L4 and F3, CLI 1.3.0 for item 11). The note of F3 does not name the model.
  - acp2api: a new set of client tools causes a rebuild, and acp2api sends no catalog update, see ADR-0003.
- **The model can write a command output with no tool.** In the version test of the earlier bridge, the command check asked `Run the shell command "echo agy-bridge" and show its output.` On `gemini-3.8-flash-low`, the check had no tool step, and the model wrote `agy-bridge` without a tool.
  - Source: the first live acceptance note (2026-10-06), section "Results", row "Acceptance test on `1.2.17` with `gemini-3.8-flash-low`".
  - Evidence: live run (CLI 1.2.17, 2026-10-06).
  - Touches: [How does the project prove that v1 works on Windows and macOS?](https://github.com/yandhi-0421/acp2api/issues/17).
- **A model with a file tool tries paths outside the request.** An agent with the tool `view_file` got a relative path, and the tool rejected it. With no instruction, the model then tried four other paths: `/workspace/probe-canary.txt`, `/proc/self/cwd/probe-canary.txt`, `/proc/self/environ`, and `/etc/passwd`. The permission check of the CLI refused `/etc/passwd`, because it is outside the workspace.
  - Source: `2026-10-04-antigravity-cli-live-probe.md`, section "Tool exclusion canary".
  - Evidence: live run (CLI 1.2.16, default model, 2026-10-04).
  - Touches: [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9) (the tool filter), ADR-0002.
- **The model reported an unknown tool to the user.** For a call to a tool that was not in the tool set, the earlier bridge returned an MCP result with `isError: true` and a text. The tool step ended as `ERROR` with this text. The model continued and told the user that the tool was not available.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "1. End-to-end pending tool call".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9), decision 3 (`reject_once` for a permission request that is not for a client tool).
- **Images in a tool result reach the model.**
  - Run I1: the user message had one line with the path of a 128 × 128 PNG, with a green top half and a magenta bottom half. The model called the client tool `Read` with the path. The earlier bridge sent the image as an MCP content block `{"type": "image", "data": <base64>, "mimeType": "image/png"}`.
  - The input tokens increased from 1468 to 2966. The reply was `The image is split horizontally into two halves, with bright green on the top half and magenta on the bottom half.`
  - On 2026-10-06, an image in a tool result through `/v1/messages` also passed. In the next request, the model gave the correct color.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "5. Images in tool results"; the first live acceptance note (2026-10-06), section "Results", row "Image in a tool result (`/v1/messages`)".
  - Evidence: live run (CLI 1.2.17).
  - Touches: [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), F6 (an image result passed under cancel at permission).
  - acp2api: each image goes as an ACP `image` block, not as a file with a path line, see ADR-0003.
- **A model change in the same group kept the context.** Run M1 continued the Cascade of run E3 in a new process with `--conversation` and `--model gemini-3.1-pro-low`.
  - The CLI log said `Propagating selected model override to backend: label="Gemini 3.1 Pro (Low)"`. Without `--model`, the label was `Gemini 3.8 Flash (High)`.
  - The Cascade ID stayed the same. The reply was `You asked about Lisbon, and the tool returned station code W-8486.`
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "6. Model change inside one Cascade".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: ADR-0003, rule "Model" (a new model keeps the ACP session).
- **No thought text.** The stream-json output of the CLI had no thought text. It had only `thinking_tokens` in the step usage.
  - Source: the first live acceptance note (2026-10-06), section "Other probes".
  - Evidence: live run (CLI 1.2.17, 2026-10-06).
  - Touches: [What does acp2api return for `thinking` blocks and `reasoning` items?](https://github.com/yandhi-0421/acp2api/issues/24). The official runtime also sent no thought text ([Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), F10).
- **Cache reads in the token counts of the CLI.**
  - A Cascade of `claude-sonnet-4-6` had a system text of approximately 8 000 tokens. CLI turn 1 gave `input_tokens` 8255 and `cache_read_tokens` 0. CLI turn 2 (continue) gave 573 and 7781. Thus `input_tokens` does not contain `cache_read_tokens`.
  - `gemini-3.8-flash-low` gave `cache_read_tokens: 0` on both CLI turns of the same test (8728 and 8843 input tokens).
  - In probe 1c of the second session (`claude-sonnet-4-6`), the first request had 9597 input tokens and 0 cached tokens. On the second request, the CLI values were 586 and 9119.
  - Source: the first live acceptance note (2026-10-06), section "Does `input_tokens` contain `cache_read_tokens`?"; the second live acceptance note (2026-10-06), section "Part 1: live check of the fix list", row 1c, item 8.
  - Evidence: live run (CLI 1.2.17 and 1.3.0).
  - acp2api: the official runtime gives no cache counts, so acp2api reports the last `usage_update.used` as the input tokens, with no cache tokens, see [Which token usage does acp2api report to each API?](https://github.com/yandhi-0421/acp2api/issues/20).

### MCP behavior of the CLI

The MCP client of the CLI and that of the official runtime both send the `User-Agent` `Go-http-client/1.1`. The facts below are for the CLI. Each one says what acp2api knows for the official runtime.

- **The handshake over HTTP.** The CLI sent `server/discover` with the header `Mcp-Protocol-Version: 2026-07-28`, then `initialize` with `2025-11-25`. It sent `GET /mcp` for an event stream, and two `GET` requests to `/.well-known/oauth-protected-resource` paths. A 405 response to each had no bad effect. The CLI sent back the `Mcp-Session-Id` of the server, and it sent `DELETE /mcp` at the end. Plain JSON responses were sufficient.
  - Over stdio, the CLI sent `server/discover`, then `initialize` with protocol version `2025-11-25`, then `tools/list`.
  - Source: `2026-10-04-antigravity-cli-live-probe.md`, section "MCP server of the bridge" and its subsection "Positive path", probe C5.
  - Evidence: live run (CLI 1.2.16, 2026-10-04).
  - Touches: [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), F4: the official runtime 1.4.0 also sent `server/discover` with `2026-07-28`, then `initialize` with `2025-11-25`, a `GET` for an event stream, and two `GET` requests to `/.well-known/oauth-protected-resource` paths.
- **The MCP connection opens only after the first user message.** In run E2, the earlier bridge sent the user message 30 s after the process start and 23 s after the `init` event. No `initialize` came before the user message.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "1. End-to-end pending tool call".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), F4: the official runtime opens the connection only at the first prompt turn of an ACP session.
- **One MCP session for each process, and `_meta` on each call.** Each CLI process opened its own MCP session and sent the `Mcp-Session-Id` of the earlier bridge in each request. Each `tools/call` contained `_meta` with `antigravity.google/conversation_id`, `antigravity.google/artifacts_dir`, and `progressToken`. Thus the earlier bridge could find the Cascade of each call.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "1. End-to-end pending tool call".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: ADR-0002: under cancel at permission, no `tools/call` reaches the MCP server of acp2api. The probe of 2026-10-15 saw `_meta["antigravity.google/conversation_id"]` on a `tools/call` of the official runtime ([2026-10-15-official-runtime-acp-probe.md](2026-10-15-official-runtime-acp-probe.md), section 9.1).
- **The CLI read `tools/list` again after `notifications/tools/list_changed`, but the model did not see the new tool.** These are the five runs of the tool set change:

  | Run | Method | CLI | Model |
  | --- | --- | --- | --- |
  | L1 | Same process. Turn 2 adds `get_time(tz_name)`. The earlier bridge sends `notifications/tools/list_changed` on the `GET /mcp` event stream and writes the agent file again. | `tools/list` again after 10 ms | Reply `NO-TOOL`. The model did not see the new tool. |
  | L2 | New process with `--conversation` and the same agent name. The new catalog removes `get_weather` and adds `get_time(tz_name)`. | New MCP session and `tools/list` | Called the removed `get_weather`. Called `get_time` with `zone`, not `tz_name`. |
  | L3a | New process with `--conversation` and a new agent name. | `init.agent` showed the new name | Called `get_time` with `zone` again. This run is not conclusive, because the history contains a successful call with `zone`. |
  | L3b | New process with `--conversation` and one more new agent name. The new body asks for the token `OKAPI-42` and adds `get_tide(lookup_key_q7)`. | `init.agent` showed the new name | No `OKAPI-42`. Called `get_tide` with `harbor`. |
  | L4 | Same process as L3b. The earlier bridge puts a catalog update at the start of the user message. The update adds `get_moon(iso_day_k9)`. | No new `tools/list`. The CLI sent the `get_moon` call. | Called `get_moon` with `iso_day_k9`. Success. |

  - In L1, the earlier bridge answered the `GET /mcp` request with a `text/event-stream` response and kept it open. The CLI also sent `notifications/roots/list_changed` at the start of CLI turn 2.
  - The note concludes: the CLI obeys `notifications/tools/list_changed` in one process when the server sends it on the `GET /mcp` event stream.
  - Limit (inference of this part): in this design, the model got the tool schemas only from the catalog text in the agent file (see "Without the tool schemas, the model guesses names"). Thus L1 shows the second read of the CLI. It does not show that a tool from a second read reaches the model.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "3. Tool set changes".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9), [Which ACP session does acp2api use for each API request?](https://github.com/yandhi-0421/acp2api/issues/10), ADR-0002. In the macOS probe of acp2api, the test MCP server answered the `GET` with 405. The official runtime sent 5 `notifications/roots/list_changed` at each later prompt of a process ([Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), F4).
  - acp2api: a new set of client tools causes a rebuild, and acp2api sends no `notifications/tools/list_changed`, see ADR-0003.
  - The official runtime 1.4.0 read `tools/list` again after `notifications/tools/list_changed`, and its model called the new tool, in one run of probe P on 2026-10-15. See [2026-10-15-official-runtime-probe-p.md](2026-10-15-official-runtime-probe-p.md), section 5.
- **The CLI does not use its tool list to block a call.** The CLI sent a `tools/call` also for a tool name that was not in its last `tools/list` result (runs L2 and L4). It did not compare the arguments with the input schema (run L2). Thus the earlier bridge checked the tool name, and the API client checked the arguments.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, sections "1. End-to-end pending tool call" and "3. Tool set changes".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9), decision 3 (acp2api answers `reject_once` when a permission request is not for a client tool of the request).
- **The schemas of MCP tools are files for a file tool.** The CLI writes each schema to `~/.gemini/antigravity-cli/mcp/<server>/<tool>.json`. It deleted the file of the tool `echo` of set C. The CLI program contains the prompt text `Each MCP server has a directory %s containing tool schemas`. Another prompt text refers to a `<mcp_servers>` system prompt block.
  - A possible explanation of the note: the native method is a default prompt section and schema files that the model reads with a file tool. The locked profile removed both. The agent did not verify this explanation.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Tool catalog".
  - Evidence: live run and static (CLI 1.2.17). The explanation is an inference.
  - Touches: ADR-0002. The static inspection of the official runtime 1.4.0 found the same method: the model reads the schemas with `view_file`, unless `_meta.antigravity.eagerMcpServers` names the server ([2026-10-15-official-runtime-acp-probe.md](2026-10-15-official-runtime-acp-probe.md), section 3.3, row "MCP").
- **`list_resources` stays callable.** The model can call `list_resources` with `tools: []` when the workspace has an MCP server. In run E2, the CLI refused the call, because its argument was empty.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Tool catalog".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9), decision 3: the tool filter of the official runtime keeps `list_resources` and `read_resource`.
- **The time limit of a tool call.**
  - The default time limit of an MCP tool call is 3 minutes. In probe C3, the CLI stopped the call after 180 s with `MCP tool call to server "bridge-probe" timed out after 3m0s`. The model replied `UNAVAILABLE`.
  - The server field `timeoutSeconds` in `mcp_config.json` changes the limit. Probe C4 held a call for 240.07 s with `timeoutSeconds: 600`.
  - Run H1 held a call for 1563.02 s (26 min 3 s) with `timeoutSeconds: 1800`. The model then used the result.
  - During the hold of H1, the CLI sent no `ping` and no other MCP request.
  - At the time limit, the CLI stopped the call with an error and sent no `notifications/cancelled`. The model then continued without the tool result.
  - Source: `2026-10-04-antigravity-cli-live-probe.md`, section "Positive path"; `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "2. 30-minute hold".
  - Evidence: live run (CLI 1.2.16 on 2026-10-04, CLI 1.2.17 on 2026-10-05).
  - acp2api: the official runtime ignores `timeoutSeconds`, ends a held call after 180 s, and sends `notifications/cancelled` ([2026-10-15-official-runtime-acp-probe.md](2026-10-15-official-runtime-acp-probe.md), sections 3.3 and 9.3); acp2api holds no call, see ADR-0002.
- **Two tool calls from one model response, one `tools/call` at a time.** Run P1 asked for two independent calls at the same time: `get_weather` for Oslo and `read_note` for `n-17`.
  - One planner response (step 1) made two tool steps, `get_weather` (step 2) and `read_note` (step 3). No planner response came between them.
  - The CLI sent the second `tools/call` 10 ms after the earlier bridge answered the first. The stream showed step 3 as `ACTIVE` only after step 2 was `DONE`. Thus the earlier bridge could not see the second call in advance.
  - Step 3 reported `duration_seconds: 16.08`. This time starts at the planner response, before the result of step 2.
  - The API client received two responses with one tool call each. The second call needed no new model request. The final text contained both codes: `Weather station code: W-BCC4; Note reference code: N-01B0.`
  - The prototype waited 8 s for more calls before each response. This wait only added time.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "4. More than one tool call".
  - Evidence: live run (CLI 1.2.17, default model, 2026-10-05).
  - Touches: ADR-0002 (each response of acp2api has no more than one tool call, and no probe tested parallel calls under cancel at permission).
- **The CLI keeps a copy of each image of a tool result.** The tool step output was `[Resource offloaded to file:///.../brain/<conversation>/.system_generated/steps/2/media_0.png]`. The model still saw the image.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "5. Images in tool results".
  - Evidence: live run (CLI 1.2.17, 2026-10-05).
  - Touches: ADR-0005 (the files of the official runtime in its runtime home).
- **The CLI adds two lines to a tool output.** The model received two more lines with the tool output, `Created At:` and `Completed At:`. The model copied both lines into its response.
  - Source: `2026-10-04-antigravity-cli-live-probe.md`, section "Positive path".
  - Evidence: live run (CLI 1.2.16, 2026-10-04).
  - Touches: ADR-0002. The official runtime added the same two lines to an MCP result ([2026-10-15-official-runtime-acp-probe.md](2026-10-15-official-runtime-acp-probe.md), section 9.2). Under cancel at permission, the tool result goes as text in a prompt.
- **The MCP design of the earlier bridge.** The earlier bridge chose a held call. Its MCP server on the loopback address held each `tools/call` until the next API request brought the tool result, for up to 30 minutes. It wrote the system prompt and a tool catalog into the agent file of each Cascade. For a new tool set, it kept the Cascade and put a catalog update at the start of the next user message.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Recommended passthrough design".
  - Evidence: static (decisions of the earlier bridge).
  - acp2api: cancel at permission with the tool filter and `eagerMcpServers`, see ADR-0002; a new set of client tools causes a rebuild, see ADR-0003.

### Built-in tools of the CLI

- **`claude-sonnet-4-6` called the built-in tool `manage_task` on its own (finding F9).** On 2026-10-06 at 13:40:45, the version test of the earlier bridge ran on `claude-sonnet-4-6`, because a synthetic quota file had stopped the Gemini group.
  - In the command check, the model wrote `I'll start by listing any running background tasks…`. Then the stream had a `tool` step with `tool_name: manage_task` and `parameters: {"Action": "list"}`.
  - The step check sent SIGINT on the `ACTIVE` step, but the CLI had already run the tool. The `DONE` step had `duration_seconds: 0.21` and `output: "No background tasks are currently running."`.
  - The agent profile had no tools, and the project file had the deny rules of the earlier bridge.
  - On 2026-10-04 and on 2026-10-06, `gemini-3.8-flash-low` made no built-in tool call in the same check. Thus the result of the version test depends on the model.
  - Source: the first live acceptance note (2026-10-06), section "Built-in tool in the command check (F9)".
  - Evidence: live run (CLI 1.2.17, 2026-10-06).
- **No control of the CLI blocked `manage_task`.** The research of F9 on CLI 1.3.0 found these facts:
  - The locked agent profile does not remove `manage_task` from the tools of `claude-sonnet-4-6`. When asked to list its tools, the model named only `manage_task` (4 of 4 runs with the normal catalog text).
  - The CLI ran `manage_task` with no approval, in 0.07 to 0.22 s (3 of 4 runs).
  - A permission rule cannot name `manage_task`. The CLI log said `ignoring invalid deny entry "manage_task(*)": unknown action "manage_task"`. It said the same for `browser`, `search_web`, `send_message`, and the other tool names.
  - `--mode plan` and `--sandbox` did not block `manage_task`.
  - With the command prompt and the base configuration, 3 runs had no tool step. Two replies said that `manage_task` is the only tool of the model. One reply said that the model has no tools.
  - When asked to list its tools, no model named another built-in tool. The other 59 names of `init.tools` did not occur in a tool step in this session.
  - Source: the second live acceptance note (2026-10-06), sections "Part 3: research of F9", "Results", and "Facts".
  - Evidence: live run (CLI 1.3.0, 2026-10-06).
- **What `manage_task` can do.** `manage_task` has the actions list, kill, status, and send input (the reply of base run 1). The scope of these actions is not known: the tasks of this Cascade, or of each CLI session on the Mac.
  - Source: the second live acceptance note (2026-10-06), section "Facts".
  - Evidence: live run (CLI 1.3.0). The scope is unknown.
  - Touches: [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9). The official runtime has `manage_task` in its Go program `localharness_external`. With no tool filter, it ran with no permission request and no `tool_call` update ([2026-10-15-official-runtime-acp-probe.md](2026-10-15-official-runtime-acp-probe.md), sections 3.3 and 9.1).
- **Only an instruction to the model stopped the call.** A stronger instruction in the tool catalog said: `This conversation has no tools. Never call a tool. This rule includes each built-in tool, for example manage_task, ...`. With it, the model replied that it cannot do it in this conversation (1 of 1 run). This defense acts on the model, not on the CLI.
  - The earlier bridge chose this instruction, with the step check as the backstop (option A, 2026-10-06).
  - A check on the final code had no tool step in 5 of 5 runs on `claude-sonnet-4-6`. On `gemini-3.8-flash-low`, the request to list the tools gave `NONE`. The note calls this a small sample and not proof.
  - Source: the second live acceptance note (2026-10-06), sections "Results" and "F9 decision and live check on the final code".
  - Evidence: live run (CLI 1.3.0, 2026-10-06).
  - acp2api: the tool filter `_meta.antigravity.enabledTools: []` and the version check are the controls for `manage_task`, and a built-in tool that runs with no permission request makes acp2api cancel the prompt turn, see [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9) and ADR-0002.

### API clients

- **Codex and `reasoning.effort`.** Finding F2 says: "Codex always sends `reasoning.effort`."
  - Source: the first live acceptance note (2026-10-06), section "Findings", F2.
  - Evidence: a statement of the note, with no run and no code location.
  - acp2api: Codex sends `reasoning.effort` only when the user sets `model_reasoning_effort` (`codex-rs/core/src/client.rs`, lines 934 to 943, at `rust-v0.163.0`), see [Which model names does the API of acp2api offer?](https://github.com/yandhi-0421/acp2api/issues/13).
- **The OpenAI input counts contain the cached tokens.** The input field of the Chat Completions API (`prompt_tokens`) and of the Responses API (`input_tokens`) contains the cached tokens. After the fix, the second request of probe 1c got `input_tokens: 9705` and `cached_tokens: 9119` from `/v1/responses`. The CLI values were 586 and 9119.
  - Source: the first live acceptance note (2026-10-06), section "Findings", F4; the second live acceptance note (2026-10-06), section "Part 1: live check of the fix list", row 1c, item 8.
  - Evidence: static (the API definition, as the note gives it) and live run (CLI 1.3.0, 2026-10-06).
  - Touches: [Which token usage does acp2api report to each API?](https://github.com/yandhi-0421/acp2api/issues/20).
- **A Responses input item with no `type`.** The OpenAI Responses API makes `type` optional for a message item, for example `{"role":"user","content":"..."}`. The earlier bridge refused this item with `bridge_unsupported: input item type null is not supported.` (finding F11). The same item with `"type":"message"` worked.
  - Source: the second live acceptance note (2026-10-06), section "Responses input item with no `type` (F11)".
  - Evidence: live run (CLI 1.3.0, 2026-10-06).
  - Touches: [Which request fields and content blocks does acp2api use, ignore, or reject?](https://github.com/yandhi-0421/acp2api/issues/21): acp2api uses such an item as a message item.

### Isolation of conversations

- **The CLI has no public switch for memories.** The CLI program has internal fields such as `disable_auto_generate_memories`, but no document describes them. A rule of the earlier bridge forbade a write of a global setting. Thus the maintainer split the cross-Cascade isolation test into a seed on 2026-10-06 and a check later.
  - Source: the second live acceptance note (2026-10-06), section "Isolation test, part 1: the seed".
  - Evidence: static (CLI program strings, CLI 1.3.0).
  - Touches: ADR-0005 (acp2api runs the official runtime in its own runtime home), [What does acp2api do with the conversation files of the official runtime?](https://github.com/yandhi-0421/acp2api/issues/31).
- **An earlier isolation test on one day.** On 2026-10-05, on CLI 1.2.17, one Cascade got a codename, and a different Cascade got the question. The captures are `iso-plant.ndjson` and `iso-ask.ndjson`. The seven notes do not give its result. The test was to run again after some days, when the CLI has more Cascades.
  - Source: `captures/README.md`, section "`2026-10-04-mcp-probe/`"; `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "8. Deferred check".
  - Evidence: live run (no result in these notes).
- **The seed.** On 2026-10-06 at 18:52:50, on CLI 1.3.0, a new Cascade on `gemini-3.8-flash-low` got the system prompt `You are a helpful assistant.`. The user message gave a random codename and the preference `From now on, end every answer with KIWI.`
  - The reply was `OK. KIWI`, with 676 input tokens and 0 cache reads.
  - The process of the seed closed after the idle time of 5 minutes, with exit code 0.
  - The memory files, names and metadata only: `~/.gemini/antigravity-cli/implicit/` had 13 files before and after the seed, and no file name contained the seed Cascade ID. `conversation_summaries.db` kept its size (106 496 bytes). Its time changed from 18:52 to 18:53, but it also changed after the earlier runs of the session.
  - Source: the second live acceptance note (2026-10-06), section "Isolation test, part 1: the seed".
  - Evidence: live run (CLI 1.3.0, 2026-10-06).
- **The check.** On 2026-10-08, from 11:33 to 12:10, the same CLI project, data directory, workspace, system prompt, and model ran two requests. The seed codename was `AMBER-FALCON-2085`.
  - C1 asked a new Cascade for the codename from an earlier conversation, with `UNKNOWN` as the reply if it knew none. C1 ran on CLI 1.3.1 at 12:07:35, about 41 hours and 15 minutes after the seed.
  - C2 continued the Cascade of C1 with the full conversation of C1, and asked for the capital of France in one sentence.

  | Request | CLI | Cascade | Reply | Codename | KIWI | Input tokens | Cache reads |
  | --- | --- | --- | --- | --- | --- | --- | --- |
  | Seed | 1.3.0 | the seed Cascade | `OK. KIWI` | Given in the prompt | Yes | 676 | 0 |
  | C1 | 1.3.1 | new | `UNKNOWN` | No | No | 667 | 0 |
  | C2 | 1.3.1 | the Cascade of C1, continued | `The capital of France is Paris.` | No | No | 803 | 0 |

  - The input tokens of C1 were 9 fewer than those of the seed, below the failure threshold of 776 (the seed plus 100). C2 holds the conversation of C1, so its 803 input tokens are not the comparison.
  - Before the check, `implicit/` had 15 files. Their names, sizes, and times matched the list that the agent saved after the seed process closed, and they stayed unchanged. No name contained the seed Cascade ID. The note does not explain the change from 13 to 15 files.
  - `conversation_summaries.db` grew from 106 496 to 110 592 bytes after the version test of 1.3.1 (time 11:37:20), and it kept that size after the check. The agent did not read the database. The note says that these facts alone do not show a memory leak.
  - Result: pass for the planned check. The new Cascade did not give the seed codename or the `KIWI` preference.
  - Limits: one sample of one workspace and one model. It does not prove that the CLI enforces isolation for all Cascades, that it never makes a memory, or that no leak can occur later. The CLI version changed between the seed and the check. The maintainer advanced the check from its planned time, so it was not a full 48-hour check.
  - Source: the second live acceptance note (2026-10-06), sections "Isolation test (part 2, cross-Cascade)", "Method and replies", "Memory files and safety", and "Result and limits".
  - Evidence: live run (CLI 1.3.0 and 1.3.1).
  - Touches: ADR-0005, [What does acp2api do with the conversation files of the official runtime?](https://github.com/yandhi-0421/acp2api/issues/31).

### Probe methods and safety rules

- **The safety rules of each live probe.** Each probe used one Google account. Each model request started after the previous request ended. The agent did not read credential files, the keychain, or `state.vscdb`, and it read only non-secret lines of the CLI logs. The probes did not disable telemetry, bypass permissions, or cause a 403 on purpose.
  - The maintainer approved the changes of configuration files in sets C and D. The agent copied a global file before a change, put the copy back after the probe, and verified that the file was byte-identical.
  - After each session, the agent compared the global CLI files by SHA-256, counted the Cascade directories (the CLI calls them conversation directories), and checked that no CLI process stayed.
  - Source: `2026-10-04-antigravity-cli-live-probe.md`, sections "Scope and evidence" and "Positive path"; `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Scope and evidence"; the first live acceptance note (2026-10-06), sections "Scope and safety" and "After the session".
  - Evidence: live run (the method of the probes).
  - Touches: [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8) (the same rules, and a guard for lockout signals).
  - acp2api: acp2api runs 2 prompt turns at the same time by default, and its macOS probe sent two prompts at the same moment, see [How many prompt turns run at the same time, and what does a queued request get?](https://github.com/yandhi-0421/acp2api/issues/14).
- **Never cause a real account block.** Never provoke a real account block to test the fail-stop. Synthetic error fixtures can test the behavior of the bridge, but they give no proof of the behavior of the CLI. The first live acceptance session tested the fail-stop and the quota stop with synthetic files.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Fail-stop policy", rule 8; the first live acceptance note (2026-10-06), section "Results".
  - Evidence: static (a rule of the earlier bridge) and live run.
  - Touches: [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15), "Deferred points and tests".
- **Control the request budget.** In the first live acceptance session, the agent sent two requests that were not in the plan, by mistake. In the second session, the agent wrote each request to a file before it sent the request. The maintainer authorized each probe set, and later each extra probe.
  - A CLI turn can contain more than one model request, for example before and after a tool call.
  - On 2026-10-08, three checks of the version test sent four model requests, because the MCP check had two completed `agent_response` steps.
  - Source: the first live acceptance note (2026-10-06), section "Scope and safety"; the second live acceptance note (2026-10-06), sections "Scope and safety" and "Version change and request count"; `2026-10-05-antigravity-mcp-passthrough-prototype.md`, section "Requests and quota".
  - Evidence: live run.
  - Touches: ADR-0006 (the compatibility check sends 3 to 4 model requests).
- **A wrapper keeps the raw evidence.** A wrapper script ran the real CLI. It kept a copy of the arguments, the input, the output, and the error output of each run. It sent each signal on to the CLI, and it changed no byte.
  - Source: the second live acceptance note (2026-10-06), section "Scope and safety".
  - Evidence: live run (the method).
- **Markers that prove what reached the model.**
  - A value that only the fake tool knew: `W-8486`, `W-8C0C`, `W-BCC4`, and `N-01B0`. The value in the reply proves that the tool result reached the model.
  - A token that an instruction asks for: `ZEBRA-19` and `OKAPI-42`.
  - A parameter name that a model cannot guess: `tz_name`, `iso_day_k9`, and `lookup_key_q7`. A guessed name such as `zone` or `harbor` shows that the schema did not reach the model.
  - The input token count: 1211 tokens with two tools and 1445 with the tool catalog showed that the schemas were not in the context.
  - Source: `2026-10-05-antigravity-mcp-passthrough-prototype.md`, sections "Tool catalog", "Results", and "3. Tool set changes".
  - Evidence: live run (the method).
- **A canary for tool exclusion.** Put a file with a random value in the workspace. Send the same request to an agent that has the read tool (the positive control) and to an agent with no tools. Compare the stream steps and the real side effects, not only the reply text. A file write request also needs a check for new files.
  - Source: `2026-10-04-antigravity-cli-live-probe.md`, section "Tool exclusion canary"; `2026-10-04-antigravity-tool-contract.md`, section "Hook isolation".
  - Evidence: live run (CLI 1.2.16, 2026-10-04).
- **A reply is not proof of tool exclusion.** A response with no tool steps proves only that the observed request had no reported tool execution. A model statement about absent tools does not prove enforced exclusion. A list of tool names that the program reports does not show the effective tools: the CLI listed the same 60 names in `init.tools` with each agent profile.
  - Source: `2026-10-04-antigravity-quota-signals.md`, section "Tool exclusion ambiguity"; `2026-10-04-antigravity-cli-live-probe.md`, section "Tool exclusion canary".
  - Evidence: inference and live run (CLI 1.2.16).
- **Compare defenses with two prompts.** Change one item of the configuration for each probe. Prompt A: list the name of each tool that you can call, do not call a tool, and reply `NONE` if you have no tool. Prompt B: use the built-in tool and show the result. Stop rule: when the base runs make no call, the prompt cannot compare the defenses.
  - Source: the second live acceptance note (2026-10-06), sections "Method" and "Results".
  - Evidence: live run (the method).
- **Check the hooks before a model probe.** Hooks of the CLI can come from a workspace `.agents/hooks.json`, the global `~/.gemini/config/hooks.json`, the CLI settings, and plugins. Each named hook accepts `enabled: false`. The documentation names no switch that disables all hook sources, so an empty workspace hook file is not proof of isolation. The note gives these steps before a model probe:
  1. Inspect the effective hook list with the official CLI command.
  2. Verify that no unexpected hook can execute.
  3. Verify the selected agent with a unique harmless profile marker.
  4. Test one harmless declared tool as a positive control.
  5. Test the zero-tool profile against the same operation.
  6. Compare stream steps and actual side effects, not only response text.
  - Source: `2026-10-04-antigravity-tool-contract.md`, section "Hook isolation"; `2026-10-04-antigravity-cli-live-probe.md`, section "Hooks and configuration".
  - Evidence: documentation (the hooks page) and live run (`agy -p /hooks` printed nothing on 2026-10-04).
  - Touches: ADR-0005 (the official runtime reads `config/hooks.json` of its runtime home).
- **Keep the workspace out of other `.agents/` trees.** The 1.2.16 changelog says that the CLI loads manifests from each `.agents/` directory between the working directory and the project root. Thus the workspace of the earlier bridge must not be in a directory tree with other `.agents/` directories. The local clone of the earlier bridge had one, for Codex skills. The CLI settings file listed the home directory of the maintainer as a trusted workspace.
  - Source: `2026-10-04-antigravity-cli-live-probe.md`, section "Hooks and configuration".
  - Evidence: documentation (CLI changelog 1.2.16) and live run.
  - Touches: ADR-0005 (the `cwd` of each ACP session is in the system temporary folder, and on Windows the acceptance test of a version checks that the official runtime reads nothing from the parent folders).
- **A file that the CLI added after an update.** At 17:26 on 2026-10-06, after the update to 1.3.0, the CLI added `~/.gemini/antigravity-cli/bin/webm_encoder`. This is in the data directory of the CLI, not in a configuration file.
  - Source: the second live acceptance note (2026-10-06), section "CLI version change during the session".
  - Evidence: live run (CLI 1.3.0).
  - Touches: [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), F5: each harness process of the official runtime logged an install of `webm_encoder` to `~/.gemini/antigravity/bin/webm_encoder`, and when it does so is unknown.
- **The context overflow was never probed.** The maintainer did not authorize the context overflow probe in either session. How the CLI reports a context overflow is not known.
  - Source: the first live acceptance note (2026-10-06), section "How does the engine report a context overflow?"; the second live acceptance note (2026-10-06), section "Not done in this session".
  - Evidence: none (a gap).
  - Touches: [How does acp2api report an upstream error and a cancelled prompt turn to each API?](https://github.com/yandhi-0421/acp2api/issues/25).

### Versions and the version test

- **The CLI updated itself often.** The version went from 1.2.16 (2026-10-04) to 1.2.17 (2026-10-05, 14:20:48) to 1.3.0 (2026-10-06, 17:24) to 1.3.1 (by 2026-10-08). The agent did not start these updates. On 2026-10-06, the direct commands `--version`, `--help`, and `-p /usage` caused the update to 1.3.0. The changelog had 18 versions in 26 days (1.2.0 to 1.2.17, 2026-09-10 to 2026-10-05).
  - Source: `2026-10-05-antigravity-cli-auto-update.md`, section "Evidence"; the second live acceptance note (2026-10-06), sections "CLI version change during the session" and "Version change and request count".
  - Evidence: live run, and documentation (the commit history of the CLI changelog).
  - Touches: ADR-0006. The official runtime has no updater ([2026-10-15-official-runtime-acp-probe.md](2026-10-15-official-runtime-acp-probe.md), section 3.3, row "Updates").
- **The version test of the earlier bridge.** The earlier bridge chose an automatic test before the first request with a new CLI version. The test had three checks: one text request, one MCP round trip with a test tool, and one request for a command that must give no tool step. A failed test made the bridge refuse each request until a new test passed.
  - The note estimated two or three small model requests and a wait of 30 to 60 seconds. In live runs on `gemini-3.8-flash-low`, the test took 32 s (CLI 1.2.17) and 31 s (CLI 1.3.0). On 2026-10-08, it sent four model requests.
  - On `claude-sonnet-4-6`, the test failed at the command check (F9).
  - Source: `2026-10-05-antigravity-cli-auto-update.md`, sections "Options" and "Decision"; the first live acceptance note (2026-10-06), section "Results"; the second live acceptance note (2026-10-06), sections "CLI version change during the session" and "Version change and request count".
  - Evidence: static (decisions of the earlier bridge) and live run.
  - acp2api: acp2api identifies a build by the SHA-256 values of its executables, and only the user starts a compatibility check, see ADR-0006.

## Part 2: the Antigravity proxies, the bans, and the reasons of the earlier bridge

### Evidence levels in this part

Each fact below has one level of the scale of the acp2api research notes: **[Direct]**, **[Supported]**, **[Inferred]**, **[Speculative]**, or **[Unknown]**. The old notes read their sources on 2026-10-03, with a static read of source code and of official pages. The move to acp2api opened no source again. Thus each level shows what the old note read, and it is not stronger than the old note. "Cites" gives the original source in the form that the old note gives. "Touches" names the acp2api issue or ADR that the fact bears on.

The landscape note also moved as a full copy: [2026-10-03-antigravity-proxy-landscape.md](2026-10-03-antigravity-proxy-landscape.md). It has the exact words of the forum posts and of the README warnings that this part only summarizes.

### The three patterns of Antigravity proxies

The earlier bridge sorted the third-party tools for Antigravity into three patterns. These are definitions, with no evidence level. Source: GLOSSARY.md (earlier bridge), section "Architecture"; 2026-10-03-antigravity-proxy-landscape.md, section "1. Three implementation patterns".

- **Gen 1**: a third-party program uses the account credentials to send its own requests directly to the Google API.
- **Gen 2**: the bridge runs the language server from the official installer on the local machine.
- **Gen 3**: the bridge drives a real Antigravity client program while it runs, for example the Antigravity CLI or the desktop app.

Facts:

- **[Direct]** The earlier bridge put the official runtime into Gen 3. Its ADR-0001 lists `agy_acp_server` under "Engine choice inside Gen 3", and it rejects it only for its MVP.
  - Source: 0001-native-engine-bridge.md, section "Considered Options".
  - Cites: none.
  - Touches: [#2](https://github.com/yandhi-0421/acp2api/issues/2).
- **[Direct]** Gen 1 is the most common pattern. On 2026-10-03, its largest projects were CLIProxyAPI (53.9k GitHub stars), opencode-antigravity-auth (11k), AIClient2API (8.8k), and antigravity-claude-proxy (4k).
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Gen 1: credential replay and direct API emulation".
  - Cites: https://github.com/router-for-me/CLIProxyAPI, https://github.com/NoeFabris/opencode-antigravity-auth, https://github.com/justlovemaki/AIClient2API, https://github.com/badrisnarayanan/antigravity-claude-proxy.
  - Touches: the rules of the map that acp2api only translates and never connects to Google itself ([#1](https://github.com/yandhi-0421/acp2api/issues/1)).
- **[Direct]** A static read of CLIProxyAPI and AIClient2API shows how a Gen 1 project calls Google:
  - It uses the public OAuth client of the Antigravity desktop client, which comes from Codeium, with the callback port 51121. This note leaves out the client ID.
  - Its scopes are `cloud-platform`, `userinfo.email`, `userinfo.profile`, `cclog`, and `experimentsandconfigs`.
  - As an alternative, it reads the sign-in session of the user from `state.vscdb`, the local database of Antigravity.
  - It calls `v1internal` methods on `https://cloudcode-pa.googleapis.com`, or on the preview channel `https://daily-cloudcode-pa.googleapis.com`.
  - The methods are `:streamGenerateContent`, `:generateContent`, `:countTokens`, `:onboardUser`, `:loadCodeAssist`, and `:fetchAvailableModels`.
  - It puts the Gemini body into an envelope with a project ID. It cleans the thought signatures, changes `parameters` to `parametersJsonSchema`, and adds the default `safetySettings`.
  - It removes each header of the caller. It sets only `Content-Type`, `Authorization: Bearer`, and a copied `User-Agent`, for example `antigravity/2.8.1 darwin/arm64`.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Gen 1: credential replay and direct API emulation".
  - Cites: the source code of CLIProxyAPI and AIClient2API "as of 2026-10". The old note gives no commit.
  - Touches: [#1](https://github.com/yandhi-0421/acp2api/issues/1); [#4](https://github.com/yandhi-0421/acp2api/issues/4), C4, which cites `internal/auth/antigravity/constants.go` of CLIProxyAPI at commit 911f49c for the direct calls.
- **[Inferred]** The Google server gives new models only to clients of version 2.9.0 or later. For this reason, CLIProxyAPI reads the Google update manifest every 3 hours and copies its version number. Its fallback version is 2.9.1.
  - Level: [Direct] for the code of CLIProxyAPI. [Inferred] for the rule of the server, because only the code of a third-party proxy shows it.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Gen 1: credential replay and direct API emulation", paragraph "Version gate".
  - Cites: the source code of CLIProxyAPI, as of 2026-10, with no commit. The manifest path is `antigravity-hub-auto-updater-…run.app/manifest/latest-arm64-mac.yml`.
  - Touches: [#1](https://github.com/yandhi-0421/acp2api/issues/1).
- **[Direct]** The Gen 1 code of CLIProxyAPI asks for the OAuth scope `cclog` (Cloud Code log). But it has no `onUserEvent` call and no other telemetry call. The old note calls this difference "the most constant signal of a client that is not real", because a proxy can copy a `User-Agent` but not a behavior stream.
  - Level: [Direct] for the code read. The "signal" is an inference of the old note. [#4](https://github.com/yandhi-0421/acp2api/issues/4) rates missing telemetry as a ban signal only [Speculative] (CLIProxyAPI #1814).
  - Source: 2026-10-03-antigravity-proxy-landscape.md, paragraph "Primary defect: no telemetry".
  - Cites: the source code of CLIProxyAPI, as of 2026-10.
  - Touches: [#4](https://github.com/yandhi-0421/acp2api/issues/4); [#8](https://github.com/yandhi-0421/acp2api/issues/8), see the section "Telemetry" below.
- **[Direct]** The Gen 2 example Antigravity-Tools-LS (428 stars, Rust) runs the real language server binary from the official installer packages, under the name `ls_core`. It emulates the extension server and gives the OAuth token to the language server. A source check found facts against its README:
  - It sets the Cloud Code endpoint of the language server to its own local proxy. The proxy sends each Cloud Code request again with its own HTTP client.
  - The tool itself sends OAuth, quota, and update requests to Google.
  - It gives the language server a false `device_fingerprint`. At an account switch, it writes new telemetry IDs and writes to `state.vscdb`.
  - Thus the old note says that it is not a true Gen 2 implementation.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Gen 2: native language server bridge", paragraph "Correction (2026-10-03)"; 2026-10-03-antigravity-local-engine-interface.md, sections "2.6 Does the bridge send a request to a Google endpoint? Yes." and "5.2 Antigravity-Tools-LS".
  - Cites: https://github.com/lbjlaq/Antigravity-Tools-LS at commit `d312237af83820ca15a636e81c5e61660bdf13f4` (2026-03-27), for example `apps/cli-server/src/handlers/proxy.rs` lines 23–54 and `ls-orchestrator/src/native.rs` line 432.
  - Touches: the rule of the map that the official runtime sends each request to Google ([#1](https://github.com/yandhi-0421/acp2api/issues/1)).
- **[Inferred]** A bridge that starts only the language server of the IDE does not send the traffic of the IDE host to Google. That traffic is Clearcut events to `https://play.googleapis.com/log` and `v1internal:recordCodeAssistMetrics`. Thus the telemetry stream of such a bridge differs from a real IDE session, also with no filter.
  - Level: [Direct] for the host traffic, from a static read of the code of the IDE 2.5.5. [Inferred] for the conclusion.
  - Source: 2026-10-03-antigravity-local-engine-interface.md, sections "3.5 Cloud endpoint hostnames" and "5.2 Antigravity-Tools-LS".
  - Cites: `out/main.js`, positions `4622:56138` and `4624:75257`, in `/Applications/Antigravity IDE.app/Contents/Resources/app` (IDE 2.5.5).
  - Touches: item 1 of the section "Reasons of the earlier bridge for its architecture" below. No acp2api record says which traffic an editor sends to Google when it runs the official runtime.
- **[Direct]** The Gen 3 example open-antigravity (207 stars) needs the desktop app with one open workspace. It creates or reuses a Cascade through a local interface, with the headers `x-workspace` and `x-conversation-id`. All traffic to Google comes from the real client. It also reads an `apiKey` from `state.vscdb`.
  - Its costs: the desktop app must stay alive, the bridge gets a Cascade and not a raw model API, and one Cascade limits the throughput.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Gen 3: drive the real app while it runs"; 2026-10-03-antigravity-local-engine-interface.md, sections "1.1 Process discovery and connection" and "1.7 Does the bridge send a request to a Google endpoint? No.".
  - Cites: https://github.com/jackwener/open-antigravity at commit `fab75833ea57a9d9050549d7a61ab93d90118ca2` (2026-03-21).
  - Touches: the rule of the map that acp2api never reads the credentials of the official runtime ([#1](https://github.com/yandhi-0421/acp2api/issues/1)).
- **[Inferred]** linux.do users say about the Gen 3 pattern that it does not cause bans. Their words are "据说" ("it is said").
  - Level: [Inferred], because the report is hearsay from one forum.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Gen 3: drive the real app while it runs".
  - Cites: linux.do, with no URL.
  - Touches: [#4](https://github.com/yandhi-0421/acp2api/issues/4), section "Contradictions", item 1. That item gives reports of a 403 `TOS_VIOLATION` for users of only the official `agy`.
- **[Inferred]** The issue tracker of Antigravity-Manager, an account and quota manager, is the best place to monitor ban signals.
  - Level: an assessment of the old note.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Related tools".
  - Cites: https://github.com/lbjlaq/Antigravity-Manager.
  - Touches: [#15](https://github.com/yandhi-0421/acp2api/issues/15).

### The account actions of February 2026 and later

[#4](https://github.com/yandhi-0421/acp2api/issues/4) holds the Google statements, the history of the terms, the error texts, and the CLIProxyAPI reports. The facts below add the reports that issue 4 does not hold. The old note says that each item of its timeline has more than one source. But it names one source for most items, and it gives a URL only for Antigravity-Manager #2143.

- **[Supported]** The ban wave of 2026-02 hit mainly Gen 1 users: credential replay directly to the Google API.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, sections "Summary" and "Gen 1: credential replay and direct API emulation".
  - Cites: community reports from linux.do, V2EX, and discuss.ai.google.dev, with no URL.
  - Touches: [#4](https://github.com/yandhi-0421/acp2api/issues/4), C4, which agrees: the actions hit tools that call the Google backend with OAuth tokens.
- **[Supported]** In 2026-02, Google sent many 403 Terms of Service bans, also to paid subscribers. Some bans hit normal users with no proxy (false positives). The old note names three posts on discuss.ai.google.dev:
  - "Code Assist Disabled (403 ToS), No Email Response for 8 Days" (2026-02-26);
  - "Prevent don't scold as general guidance — and how about not banning paying users" (2026-03-08);
  - "Eligibility check failed: PERMISSION_DENIED (code 403) appeared while using Antigravity CLI" (2026-06-24).
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Timeline", item "2026-02, the ban wave".
  - Cites: the three post titles, with no URL.
  - Touches: [#4](https://github.com/yandhi-0421/acp2api/issues/4), C6 and "Contradictions" item 1; [#15](https://github.com/yandhi-0421/acp2api/issues/15). Issue 4 cites forum topic 172630 (2026-06-25) for the same error text from the official `agy` 1.0.12.
- **[Inferred]** A V2EX post of 2026-02-13, "谷歌封禁大量 Antigravity OAuth 登录账号" ("Google bans many Antigravity OAuth login accounts"), reports these points:
  - Some users only switched accounts with Antigravity Tools, and they also got a ban.
  - Google considers an account switch as a bypass of the quota defense.
  - An unban depends on luck and on an email appeal.
  - Level: [Inferred], because the old note names only this one post.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Timeline", item "2026-02-13, V2EX", and section "Ban factors", factor 2.
  - Cites: the V2EX post title, with no URL.
  - Touches: the scope of the map, which excludes more than one Google account ([#1](https://github.com/yandhi-0421/acp2api/issues/1)).
- **[Supported]** About 2026-02-26/27, Google removed the bans of many accounts at the same time (the "amnesty"). **[Inferred]**: many accounts lost the subscription. Old accounts usually kept the membership, and new accounts usually lost it.
  - Level: [Supported] for the unban, because issue 4 has a separate source for it. [Inferred] for the date and for the lost subscriptions, from one issue thread.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Timeline", item "2026-02-26/27, the "amnesty"".
  - Cites: https://github.com/lbjlaq/Antigravity-Manager/issues/2143.
  - Touches: [#4](https://github.com/yandhi-0421/acp2api/issues/4), C4. Conflict about the date: issue 4 gives 2026-03-02 for the announcement of an automated unban by Google forum staff. [#2](https://github.com/yandhi-0421/acp2api/issues/2), C6, gives 2026-03-03 (forum 128101, post 2).
- **[Inferred]** After the amnesty, some users started the proxy again, and Google banned their accounts again in a few days. One user lost two PRO accounts just after that user used all of the weekly limit. The same thread has free accounts that used the proxy all the time with no ban. The old note concludes that PRO accounts are the primary target, and that free accounts have a lower risk.
  - Level: [Inferred], from a few user reports in one issue thread.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Timeline", item "After the amnesty, a second ban wave", and section "Ban factors", factor 3.
  - Cites: https://github.com/lbjlaq/Antigravity-Manager/issues/2143.
  - Touches: [#15](https://github.com/yandhi-0421/acp2api/issues/15), decision 5 (quota wait). The account of the macOS probe has the tier `g1-pro-tier`, Google AI Pro ([#8](https://github.com/yandhi-0421/acp2api/issues/8), F3).
- **[Direct]** On 2026-10-03, the READMEs of antigravity-claude-proxy and opencode-antigravity-auth warned that Google banned accounts that used these projects. Both projects told users not to use their primary account.
  - Source: 2026-10-03-antigravity-proxy-landscape.md, section "Timeline", item "Current status".
  - Cites: the README files of https://github.com/badrisnarayanan/antigravity-claude-proxy and https://github.com/NoeFabris/opencode-antigravity-auth.
  - Touches: [#4](https://github.com/yandhi-0421/acp2api/issues/4), which names the risk text of the README of acp2api as a decision that waits for its facts.

### Ban factors

The landscape note lists seven ban factors. It says that the list comes from community evidence and is not an official statement from Google. [#4](https://github.com/yandhi-0421/acp2api/issues/4) found no Google source that names a detection signal ([Unknown]), and it gives the community hypotheses as [Speculative]. Thus each factor below is at most [Inferred] as a cause of a ban. The source of each factor is 2026-10-03-antigravity-proxy-landscape.md, section "Ban factors".

1. **[Speculative] No telemetry.** Gen 1 asks for the `cclog` scope but never sends logs. Thus its behavior stream differs from the stream of the real client.
   - Evidence: the code read of CLIProxyAPI above. Issue 4 gives missing telemetry as a community hypothesis (CLIProxyAPI #1814).
   - Touches: [#8](https://github.com/yandhi-0421/acp2api/issues/8), see the section "Telemetry" below.
2. **[Inferred] Many accounts and account switches.** An account switch at the `state.vscdb` level causes a ban.
   - Evidence: the V2EX report of 2026-02-13. Issue 4: the Antigravity FAQ names automated account rotation as not supported ([Direct]). Multiple accounts that share projects or proxies are a hypothesis (CLIProxyAPI #1637).
   - Touches: [#1](https://github.com/yandhi-0421/acp2api/issues/1), section "Out of scope".
3. **[Inferred] Quota exhaustion with a proxy.** Google banned an account just after the user used all of its weekly limit.
   - Evidence: the second ban wave in Antigravity-Manager #2143. Issue 4 gives repeated quota exhaustion ("shadow bans") as a hypothesis (opencode-antigravity-auth #202).
   - Touches: [#15](https://github.com/yandhi-0421/acp2api/issues/15), decision 5; [#14](https://github.com/yandhi-0421/acp2api/issues/14).
4. **[Speculative] Egress IP.** Data-center IP addresses and IP addresses that rotate have a higher risk than residential IP addresses.
   - Evidence: the old note names no source for this factor.
   - Touches: no acp2api decision. [#2](https://github.com/yandhi-0421/acp2api/issues/2), C1: on macOS with a system SOCKS proxy, the session setup of the official runtime fails ([Supported]).
5. **[Speculative] Request shape and content.** One linux.do user wrote that Google can ban an account from its prompts. Google can use the content to tell a raw API session from a Cascade task.
   - Evidence: one forum quote. Issue 4: CLIProxyAPI collaborators think that the system prompts and the tool lists of third-party harnesses are a signal. A search snippet that nobody opened says that a proxy overrode the system instruction.
   - Touches: [#23](https://github.com/yandhi-0421/acp2api/issues/23), [#11](https://github.com/yandhi-0421/acp2api/issues/11), [#19](https://github.com/yandhi-0421/acp2api/issues/19), ADR-0003. Item 3 of the section "Reasons of the earlier bridge for its architecture" gives the amendment of the earlier bridge.
6. **[Speculative] Concurrency and rhythm.** A bot can send requests 24 hours a day, 7 days a week, and in parallel. A human user does not.
   - Evidence: the old note names no source. Issue 4 gives scheduled `agy -p` calls as a hypothesis (forum 173928).
   - Touches: [#14](https://github.com/yandhi-0421/acp2api/issues/14).
7. **[Supported] Rough enforcement.** The ban waves also hit normal paid users (false positives). The responses to appeals are slow. In one case, no reply came after 8 days.
   - Evidence: the post "Code Assist Disabled (403 ToS), No Email Response for 8 Days". Issue 4, "Contradictions" item 1: reports of a 403 with only the official `agy` or only the official IDE.
   - Touches: [#15](https://github.com/yandhi-0421/acp2api/issues/15), decision 9: the notice keeps the original error text of Google, which can contain the link to an appeal.

The earlier bridge drew these conclusions for its scenario: one person, one account, a paid subscription, home broadband, and personal use only. Source: 2026-10-03-antigravity-proxy-landscape.md, section "Conclusions for this project" (in the adapted copy: "Conclusions for the earlier bridge").

- Factors 2, 3, and 4 do not apply to this scenario.
- The architecture choice controls factors 1, 5, and 6.
- Gen 2 and Gen 3 remove factors 1 and 5, because the real client binary sends its own telemetry and makes requests with the real shape.
- Gen 1 shows all of these factors to Google.

The earlier bridge corrected the point about factor 5 on 2026-10-07 (item 3 of the section "Reasons of the earlier bridge for its architecture"). acp2api records bear on three more points:

- Factor 1: [#8](https://github.com/yandhi-0421/acp2api/issues/8) saw that the official runtime drops some telemetry batches (section "Telemetry" below).
- Factor 3: acp2api starts a quota wait only after a quota signal ([#15](https://github.com/yandhi-0421/acp2api/issues/15)).
- Factor 6: acp2api runs 2 requests at the same time by default ([#14](https://github.com/yandhi-0421/acp2api/issues/14)).

### Sign-in, data folders, and settings of the Antigravity products

The old note read the official pages of the Antigravity CLI on 2026-10-03. These facts are about the CLI and the desktop app. They bear on the official runtime, because all three products keep their data under `~/.gemini`, which is also the default runtime home.

- **[Direct]** Google says that Antigravity has one authentication system for all IDE extensions, the Antigravity CLI, and the desktop app (Antigravity 2.0). The old note reads this as a statement about the account system.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, section "3.5 Sign-in and credential location".
  - Cites: https://antigravity.google/docs/ide/extensions/, section "Authentication and licensing", fetched 2026-10-03.
  - Touches: [#2](https://github.com/yandhi-0421/acp2api/issues/2), "Contradictions" item 3 (shared sign-in); ADR-0005 (the user signs in through the management panel). Issue 2 cites another Google page for the same claim. It also cites the Pi provider, which says that the sign-in of the CLI does not sign in the official runtime.
- **[Direct]** The desktop app keeps its data in `~/.gemini/antigravity`. The CLI keeps its data in `~/.gemini/antigravity-cli`. [#2](https://github.com/yandhi-0421/acp2api/issues/2) gives `~/.gemini/antigravity-acp` for the official runtime.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, section "3.5 Sign-in and credential location"; 2026-10-03-antigravity-local-engine-interface.md, section "3.6 How the desktop app starts the language server".
  - Cites: https://antigravity.google/docs/cli/install/; `dist/paths.js` lines 17–25 in `app.asar` of the desktop app 2.19.1.
  - Touches: ADR-0005. ADR-0005 also says that the official runtime reads the skills in `antigravity-cli/skills/` of its runtime home.
- **[Direct]** The settings, the permissions, and the security configuration of the CLI and of the desktop app synchronize automatically. A conversation of the CLI can move to the desktop app. The old note warns that a bridge that writes global deny rules can change the real desktop app.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, sections "3.1 Product" and "3.4 Tools, custom agents, MCP, and permissions" (paragraph "Risk").
  - Cites: https://antigravity.google/docs/cli/overview/, section "Integration features".
  - Touches: ADR-0005, which gives acp2api its own runtime home.
- **[Direct]** CLI 1.2.7 fixed a defect: a start of the CLI unpinned conversations or marked them unread in the desktop app. Thus the two products share some local state.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, section "3.5 Sign-in and credential location".
  - Cites: google-antigravity/antigravity-cli, `CHANGELOG.md` line 135, at commit `65a3c69e388148c9327f307efe82ddb1c0c8d7d4` (2026-10-03).
  - Touches: ADR-0005.
- **[Direct]** The CLI keeps its session tokens in the keyring of the operating system: Apple Keychain, Linux Secret Service, or Windows Credential Manager. `/logout` removes the authentication profiles from the keyring. CLI 1.1.26 mentions file storage when the keyring is bypassed or unreachable.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, section "3.5 Sign-in and credential location".
  - Cites: https://antigravity.google/docs/cli/install/, sections "Local silent keyring sign-in" and "Logging out"; https://antigravity.google/docs/cli/troubleshooting/, section "Authorize keyring permissions"; `CHANGELOG.md` line 261 at the commit above.
  - Touches: ADR-0005.
- **[Inferred]** On macOS, the CLI 1.1.7 and later keeps its sign-in in the Keychain, with no token file. The Keychain item has the service `gemini` and the account `antigravity`. ADR-0005 gives the service `gemini` and the account `antigravity-acp` for the official runtime.
  - Level: [Inferred], because one third-party source gives each part. The old note opened no credential.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, section "3.5 Sign-in and credential location", list "Credential paths".
  - Cites: omnigent-ai/omnigent at commit `f00d5a0ef17a304a6bf54ca650e965ac357938b3`, `omnigent/onboarding/gemini_auth.py` lines 23–25; sodre90/agy-proxy at commit `194cc6d0846091fe77d14ac43aed7a5952728483`, `README.md` lines 9–10.
  - Touches: ADR-0005, which says that nobody tested what a sign-in in the runtime home of acp2api does with the keychain entry.

### Telemetry

- **[Direct]** The CLI has the key `enableTelemetry` in its `settings.json`, with the default `true`.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, section "3.8 Telemetry".
  - Cites: https://antigravity.google/docs/cli/reference/, section "Configuration keys".
  - Touches: [#3](https://github.com/yandhi-0421/acp2api/issues/3), addendum: a static read found that the `settings.json` of the official runtime holds only the sign-in type and the Google Cloud project.
- **[Direct]** Two wrappers of the CLI set `DO_NOT_TRACK=1`, `DISABLE_TELEMETRY=1`, `GOOGLE_CLOUD_DISABLE_TELEMETRY=1`, and `ANTIGRAVITY_DISABLE_TELEMETRY=1` by default. **[Unknown]**: if the CLI obeys these variables. **[Unknown]**: if the official runtime reads them. No source covers the official runtime.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, section "3.8 Telemetry".
  - Cites: Sakiko15/agy-proxy at commit `4a56e0126546980d78a27c7bb86f1a5a33a44d32`, `src/common/types.ts` line 149 and `src/host/engine.ts` lines 786–793; tinytinycn/agy-proxy-rs at commit `6dec3a047b4a1e589bae7652bc801b033f30ca8b`, `src/config.rs` line 65 and `src/engine/runner.rs` lines 99–104.
  - Touches: ADR-0004 and ADR-0005, which set the environment of each runtime process.
- The earlier bridge decided a rule for telemetry. Its glossary says about telemetry: "The bridge never removes, filters, or fakes them." Its open question 6 about the variables above adds: "The bridge must not set them in any case."
  - Source: GLOSSARY.md (earlier bridge), section "Account safety", entry "Telemetry"; 2026-10-03-antigravity-bridge-prior-art.md, section "7. Open questions", item 6.
  - Touches: a search of the 32 acp2api issues and the six ADRs for "telemetry" found no acp2api rule about telemetry.
- An acp2api record bears on the telemetry reason of the earlier bridge. **[Direct]** The macOS probe of 2026-10-17, with `oauth-personal`, saw the stderr line `No business auth manager configured, dropping telemetry batch of 1 events` once at each permission request (8 times).
  - Source: [#8](https://github.com/yandhi-0421/acp2api/issues/8), the list of the stderr lines of the probe.
  - Conflict: the earlier bridge expected that the real client sends its own telemetry (landscape note, section "Conclusions for this project"; ADR-0001, Gen 3 option). The line shows that the official runtime drops some telemetry batches. [#15](https://github.com/yandhi-0421/acp2api/issues/15) notes that a probe cannot see what a process of the official runtime sends to Google.

### Retries and errors of the Antigravity CLI

The old note found these facts in the changelog of the CLI, at commit `65a3c69e388148c9327f307efe82ddb1c0c8d7d4` of google-antigravity/antigravity-cli. Google says that the CLI "shares the same agent harness" as the desktop app (prior-art note, section "3.1 Product"). The archive of the official runtime holds a Go program, `localharness_external` ([#2](https://github.com/yandhi-0421/acp2api/issues/2), addendum). **[Unknown]**: no source shows that the official runtime has the same retry rules as the CLI. The source of each item is 2026-10-03-antigravity-bridge-prior-art.md, section "3.6 Retries and errors".

- **[Direct]** CLI 1.2.1: the CLI retries 502, 503, 504, a per-minute 429, and stream interruptions itself, with exponential backoff. Cites: `CHANGELOG.md` line 199.
- **[Direct]** CLI 1.2.13: for a rate limit, the CLI waits for the retry delay of the server. It stops at once when the delay is longer than 30 seconds, or when the quota is a daily or billing cap. Cites: `CHANGELOG.md` line 54.
- **[Direct]** CLI 1.2.14: the CLI shows a special sign-in error, with an appeal link, for accounts that Google blocked for a Terms of Service violation. Cites: `CHANGELOG.md` line 46.
- **[Unknown]** If the CLI retries a 403 `PERMISSION_DENIED`. The old note found no source.

Touches: [#15](https://github.com/yandhi-0421/acp2api/issues/15), decision 2: a 429 starts a quota wait, and a sign-in error with a 403 or a Terms of Service text starts a lockout. [#25](https://github.com/yandhi-0421/acp2api/issues/25): the report of upstream errors. [#4](https://github.com/yandhi-0421/acp2api/issues/4), C6: the official `agy` shows a ban as `Eligibility check failed: PERMISSION_DENIED (code 403)`.

### The Pi provider for the official runtime

On 2026-10-05, a reviewer agent of the earlier bridge read `zacbemis/pi-antigravity-acp-provider` (alias `piacp`) by a static read. The old note added it as section 8 on 2026-10-07. The commit is `07e369b4c2798ee88f864ef6df36fd34e515ea5b` (version 0.1.12, last commit 2026-10-03, 32 stars, MIT). piacp reports live checks with `agy_acp_server` 1.1.1 on Linux (`docs/ANTIGRAVITY-MIGRATION.md` lines 28–38), and the earlier bridge did not do them again.

[#2](https://github.com/yandhi-0421/acp2api/issues/2) and [#3](https://github.com/yandhi-0421/acp2api/issues/3) read a later commit of the same project, `ba84539a`, and they hold most of its facts. The facts below are the ones that issues 2 and 3 do not hold. The source of each fact is 2026-10-03-antigravity-bridge-prior-art.md, section "8. Addendum 2026-10-07: a Pi provider for the Antigravity ACP server", with its subsections "8.1 What the project is", "8.2 Facts about `agy_acp_server`", "8.3 Evaluation", and "8.5 Effect on ADR-0001".

- **[Direct]** piacp installs the official runtime itself. It downloads the archive from `dl.google.com`. It checks the archive against a signed catalog of the project, because the ACP Registry gives no checksums.
  - Cites: `docs/RUNTIME-UPDATES.md` line 7; `src/acp/runtime-manifest.ts` line 151.
  - acp2api: acp2api never downloads the official runtime, and it identifies each build by the SHA-256 values of its two executables, see ADR-0006.
- **[Direct]** piacp starts `agy_acp_server.par` as a child process below a watchdog process.
  - Cites: `src/acp/supervisor.mjs`.
  - Touches: ADR-0004, rule "Crash of acp2api".
- **[Direct]** piacp reads `acp_token.json` to check the refresh token, and it deletes the file at logout.
  - Cites: `src/acp/antigravity.ts` lines 62, 80–81, and 103.
  - acp2api: acp2api never reads, changes, or deletes the login files of the official runtime, see ADR-0005.
- **[Direct]** piacp has a logout command and an account switch command.
  - Cites: `README.md` lines 16 and 82.
  - acp2api: more than one Google account is out of scope, because account rotation looks like abuse, see [Map: PRD of acp2api v1](https://github.com/yandhi-0421/acp2api/issues/1).
- **[Direct]** piacp starts one process of the official runtime for each Pi session, with no global limit.
  - Cites: `src/runtime.ts` line 564.
  - acp2api: one runtime process holds all ACP sessions, see ADR-0004. By default 2 requests run at the same time, see [How many prompt turns run at the same time, and what does a queued request get?](https://github.com/yandhi-0421/acp2api/issues/14).
- **[Direct]** piacp finds the ACP session of a request by the Pi session ID and by fingerprints of the messages. A new ACP session gets only the last 32,000 characters of the history.
  - Cites: `src/stream/context.ts` lines 6 and 130.
  - acp2api: a match by the hashes of all items, and a rebuild that holds the earlier part of the conversation, see ADR-0003.
- **[Direct]** piacp puts a fixed framing text into the first prompt of a new ACP session.
  - Cites: `src/stream/context.ts` lines 57 and 80.
  - Touches: ADR-0003, rule "Rebuild" (one fixed neutral line).
- **[Inferred]** The official runtime sends `agent_thought_chunk` updates. The old note also lists thought text as a function that the official runtime gives and the CLI does not give (section "8.5 Effect on ADR-0001").
  - Level: [Direct] that the code of piacp handles the update. [Inferred] that the official runtime sends it, because code that handles an update does not show that it arrives.
  - Cites: `src/acp/events.ts` line 17.
  - Conflict: no `agent_thought_chunk` came from version 1.4.0 on macOS, also with `high` ([#8](https://github.com/yandhi-0421/acp2api/issues/8), and the addendum of [#2](https://github.com/yandhi-0421/acp2api/issues/2)).
  - acp2api: acp2api returns no thought text and drops `agent_thought_chunk`, see [What does acp2api return for `thinking` blocks and `reasoning` items?](https://github.com/yandhi-0421/acp2api/issues/24).
- **[Inferred]** For the official runtime, the earlier bridge found partial equivalents of its tool layers 2 to 4. They are: refuse each permission request in the mode `default`, give an empty working directory, and cancel the ACP session with `session/cancel` at a `tool_call` update. It found no equivalent of layer 1, the agent profile.
  - Level: an assessment of the earlier bridge, from the code of piacp.
  - Source: 2026-10-03-antigravity-bridge-prior-art.md, section "8.5 Effect on ADR-0001".
  - Touches: [#3](https://github.com/yandhi-0421/acp2api/issues/3), addendum: the tool filter `_meta.antigravity.enabledTools: []` removed the built-in tools, and an agent file with a locked profile had no effect. Thus the tool filter is the equivalent of layer 1 that the earlier bridge did not find.

### Reasons of the earlier bridge for its architecture

This section gives the reasons of two ADRs of the earlier bridge. Both have the status `accepted`. They are not acp2api decisions.

- ADR-0001: "Use a native engine bridge, not direct API replay, for one Antigravity account". Decision of 2026-10-04, with amendments of 2026-10-07 and 2026-10-08.
- ADR-0002: "Run the engine natively on the computer of the operator, disable the Cascade tools, and pass the harness tools through". Amendment of 2026-10-06.

Text in quotation marks is the exact wording of the ADR. In the quotes, "the operator" is the maintainer, "the harness" is an API client, "the engine" is the Antigravity CLI, and "Cascade" is a conversation of the CLI. "Still applies" names an acp2api record with the same rule. An "acp2api:" line names a different decision of acp2api.

#### 1. Drive a real Google client, and never build a request to Google

The decision of ADR-0001 (source: 0001-native-engine-bridge.md, section "Decision"):

- "The engine is always a real Antigravity client artifact."
- "The bridge never reads or uses the credentials of the CLI."
- "The bridge only translates the protocol. It converts the engine stream to the OpenAI API and the Anthropic API, and back."
- "The bridge never builds its own HTTP request to Google."

The old glossary also defines the bridge with the sentence "It never sends a request to Google." (GLOSSARY.md, entry "Bridge").

The reasons (source: 0001-native-engine-bridge.md, sections "Context" and "Considered Options"):

- "The ban wave of 2026-02 hit one pattern: credential replay directly to `cloudcode-pa.googleapis.com`. This pattern sends no telemetry, copies a static `User-Agent`, and has no real behavior stream."
- "There are no reports of mass bans for the pattern that drives a real client."
- Gen 1, rejected: "Problems: it sends no telemetry, it uses a static copied `User-Agent`, and its requests have an abnormal shape. The confirmed bans hit exactly this group of users."
- Gen 1, rejected: "Bans continued after the amnesty. One PRO account got a ban when its weekly limit was empty. Thus the enforcement continues."
- Gen 3, accepted: "Advantages: it has the highest fidelity. The real client sends the requests, the telemetry, and the sign-in traffic."

Still applies: the rules of the map say that acp2api only translates, and that it never reads the credentials, the keychain entries, or the login files of the official runtime. They also say that the official runtime sends each request to Google, and that acp2api never connects to Google itself ([#1](https://github.com/yandhi-0421/acp2api/issues/1)).

acp2api: the real client is the official runtime `agy_acp_server`, not the Antigravity CLI, see [What does the official runtime support over ACP on Windows and macOS?](https://github.com/yandhi-0421/acp2api/issues/2).

Conflicts with acp2api records:

- "There are no reports of mass bans for the pattern that drives a real client." [#4](https://github.com/yandhi-0421/acp2api/issues/4), "Contradictions" item 1, gives single reports of a 403 `TOS_VIOLATION` for users of only the official `agy` (forum topics 172630 and 173928). It gives one more report for a user of only the official IDE (topic 122778, post 48). Google confirmed no cause.
- "The real client sends [...] the telemetry". [#8](https://github.com/yandhi-0421/acp2api/issues/8) saw that the official runtime drops some telemetry batches (section "Telemetry" above).

#### 2. Never add direct API replay as a fallback path

The rules (source: 0001-native-engine-bridge.md, section "Consequences"; 0002-native-engine-harness-tools.md, section "Consequences"):

- ADR-0001: "Never add direct API replay as a fallback path. If we add it, all of the constraints above become invalid."
- ADR-0001: "When the CLI is not available, the bridge must show an error. It must not silently give a degraded result."
- ADR-0001: "The bridge sends requests only with a CLI version that passed the acceptance test."
- ADR-0001: "Do not use open-antigravity or Antigravity-Tools-LS as a dependency. Use them only as references for the RPC names and message formats."
- ADR-0002: "If a later CLI version stops the passthrough, the bridge must refuse each request that contains tools. It must show an error. It must not silently drop the tools."

"The constraints above" are the six constraints of ADR-0001. Items 4 to 9 below give them.

Still applies: the rule of the map that acp2api never connects to Google itself ([#1](https://github.com/yandhi-0421/acp2api/issues/1)); the state "runtime unavailable" (ADR-0004); a start only of an accepted version (ADR-0006); a rejection of each feature that acp2api cannot give ([#21](https://github.com/yandhi-0421/acp2api/issues/21)).

#### 3. The prompt content risk (ban factor 5)

Factor 5 of the landscape note is "Request shape and content". Its conclusions said that Gen 2 and Gen 3 remove this factor. The amendment of 2026-10-07 to ADR-0001 corrected this (source: 0001-native-engine-bridge.md, section "Amendment 2026-10-07: the prompt content (ban factor 5)"):

- Facts: "This is true for the request shape, because the engine makes each request."
- Facts: "It is not fully true for the content. The agent file of each Cascade contains the system text and the tool catalog of the harness. The user messages come from the harness. Thus the content of a bridge Cascade is different from the content of a Cascade in the IDE."
- Decision: "The operator accepts this difference as a part of the account risk of this ADR. The bridge preserves the prompt content of the harness. It does not rewrite that content to imitate a Cascade in the IDE."
- Decision: "The bridge still adds the tool catalog and the tool-control instructions that ADR-0002 requires. Those instructions control the Cascade tools; they do not make the harness content the same as IDE content."
- Consequence: "No technical solution in the bridge removes this risk."

These acp2api records bear on the same risk. No acp2api issue or ADR names ban factor 5.

- The system prompt of a request goes in `_meta.antigravity.systemPrompt.append` ([#23](https://github.com/yandhi-0421/acp2api/issues/23)).
- A tool result goes to the model as user text that starts with `Result of your tool call <name> <arguments>:` ([#11](https://github.com/yandhi-0421/acp2api/issues/11)).
- The first prompt of a rebuild holds a neutral line and the earlier part of the conversation with role labels (ADR-0003).
- The client tools are the tools of an MCP server of acp2api (ADR-0002).
- The official runtime puts the `clientInfo` of acp2api into the `User-Agent` of its requests, as `host_path=acp2api/<version>`. The user accepts that Google can find the requests of acp2api ([#19](https://github.com/yandhi-0421/acp2api/issues/19)).

#### 4. One account, with no account switch and no rotation

- ADR-0001, constraint 1: "Use one account. Never switch the account. Do not read or write `state.vscdb`."
- ADR-0001, section "Context": "The operator has one Google account, a paid Antigravity subscription, home broadband, and a Mac."
- Old glossary, entry "Account switch": "A change of the Google account that is signed in to the engine. This project never does an account switch."
- Reason: ban factor 2, with the V2EX report of 2026-02-13 (section "Ban factors" above).
- Reason in ADR-0002, against a VM with a copied identity: "Emulation is necessary only when an instance is not the real computer, for example in an account pool."

Still applies: the map says in "Out of scope": "More than one Google account: account rotation looks like abuse and raises the ban risk." ([#1](https://github.com/yandhi-0421/acp2api/issues/1)). [#4](https://github.com/yandhi-0421/acp2api/issues/4) adds that the Antigravity FAQ names automated account rotation as not supported under consumer plans. The file `state.vscdb` belongs to the desktop app and the IDE. The wider acp2api rule is in ADR-0005: acp2api never reads, changes, or deletes the login files of the official runtime.

#### 5. One egress, with no rotation

- ADR-0001, constraint 2: "Use the direct network of the computer that runs the bridge as the egress. The normal egress is home broadband. If a proxy is necessary, use one sticky node. Do not rotate it."
- ADR-0002, section "Decision": "The egress stays as ADR-0001 says."
- ADR-0001, section "Decision": "On 2026-10-05, the operator added a possible second computer: a Windows computer of the operator. It can use a different network. The operator accepts the risk of a second device and a second IP address. The operator never runs two bridges at the same time."
- ADR-0001, section "Consequences": "The bridge cannot find a bridge on a different computer. The operator keeps the rule of one bridge at a time."
- Old glossary, entry "Egress": "The network path and the IP address that the engine traffic uses to go to Google."
- Reason: ban factor 4, which the old note gives with no named source ([Speculative]).
- Reason in ADR-0002: "If a VM copies the host identifiers while the host also runs Antigravity, two clients have the same device identity. With a separate SOCKS5 egress, they also have two IP addresses."

acp2api records: v1 supports Windows and macOS, and each instance serves only its owner ([#1](https://github.com/yandhi-0421/acp2api/issues/1)). A search of the issues and ADRs found no acp2api decision about the egress or about one instance at a time for one account. [#2](https://github.com/yandhi-0421/acp2api/issues/2), C1: on macOS with a system SOCKS proxy, the session setup of the official runtime fails, because its Python lacks SOCKS support.

#### 6. One model request at a time (ban factor 6)

- ADR-0001, constraint 3: "Run one bridge at a time for the account. Send one model request at a time. Only one Cascade generates at a time. A Cascade with a pending tool call sends no model request, so it does not count against this limit".
- Landscape note, section 3: "The concurrency limit is 1, in series, as a real IDE user works."

acp2api: by default 2 requests run on the official runtime at the same time, and the user can set 1 to 3, because subagent calls are real behavior, see [How many prompt turns run at the same time, and what does a queued request get?](https://github.com/yandhi-0421/acp2api/issues/14).

#### 7. A quota stop before the quota is exhausted (ban factor 3)

- ADR-0001, constraint 4: "Apply the quota stop to each model group separately. When a model group reaches 75% used in its weekly or five-hour quota, refuse all new requests for the models of that group. This means 25% or less remains. The other model group continues to accept requests."
- The amendment of 2026-10-08 made the quota stop a setting: on by default at 75% used, one integer threshold from 1 through 90, and a fixed backstop of 90% when the operator turns it off. A stop from a `quota_cap` signal stays until its reset rule permits admission.
- Old glossary, entry "Model group": "Antigravity has two model groups: the Gemini models, and the Claude and GPT models."
- Reason: ban factor 3, with the second ban wave in Antigravity-Manager #2143.

acp2api: a quota signal starts a quota wait until the reset time, see [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15).

#### 8. A stop at an account-level signal

- ADR-0001, constraint 5: "Apply fail-stop on an account-level signal: an HTTP 403, a `PERMISSION_DENIED` status, a Terms of Service block, or an unexpected sign-out. [...] Stop immediately. Give the reason in each later error response to the harness. The bridge sends no other alert. Keep the stop until the operator removes it on the status page or with a bridge command. Never retry automatically. Never change to a different account."

acp2api: a 403, a `PERMISSION_DENIED` status, or a Terms of Service text starts a lockout, which only the management password ends. A sign-in error with none of these puts acp2api into the state "sign-in required", not into a lockout. See [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15).

#### 9. Real activity of the account

- ADR-0001, constraint 6: "Keep the real Antigravity IDE installed. Use it for real work sometimes, so that the account continues to show real activity signals."

A search of the acp2api issues and ADRs found no record about this point.

#### 10. The real computer, with no VM and no fake hardware identity

The decision of ADR-0002 (source: 0002-native-engine-harness-tools.md, section "Decision"):

- "The engine runs natively on a computer of the operator. The MVP uses the Mac. A Windows computer is possible later (ADR-0001). We do not put the engine in a VM or a container."
- "We do not fake, copy, or change a hardware identifier. This includes SMBIOS data, MAC addresses, and machine IDs."

The reasons against a VM that copies the hardware identity (source: 0002-native-engine-harness-tools.md, section "Considered Options"):

- "The Mac is already the real environment. Emulation is necessary only when an instance is not the real computer, for example in an account pool."
- "On Apple silicon, a macOS VM has no SMBIOS. It shows the model `VirtualMac2,1` and a machine identifier that Virtualization.framework makes. A full copy of the host identity is not possible."
- "Emulation copies only the static fingerprint. It cannot copy the behavior stream. This is the same defect as Gen 1."
- "The ban factors in the landscape research do not include the hardware fingerprint." The landscape research is [2026-10-03-antigravity-proxy-landscape.md](2026-10-03-antigravity-proxy-landscape.md).

The reason against a VM with its default identity: "Costs: the account gets a second device, Google can see VM artifacts, and the operation work increases. This option is necessary only if the Cascade keeps its own tools."

The accepted option: "Advantages: the fingerprint is real with no emulation. The Cascade cannot change the Mac. The harness gets the same tool contract as from a model API."

acp2api records: v1 runs on the computer of the user, on Windows and macOS ([#1](https://github.com/yandhi-0421/acp2api/issues/1)). The Windows checks with the real official runtime run in a Windows 11 ARM VM on the Mac of the user ([#1](https://github.com/yandhi-0421/acp2api/issues/1), [#16](https://github.com/yandhi-0421/acp2api/issues/16)).

Conflict: [#4](https://github.com/yandhi-0421/acp2api/issues/4) lists changes of the hardware ID as a community hypothesis of a detection signal (forum 122778, post 48, [Speculative]). The last reason above says that the ban factors do not include the hardware fingerprint.

#### 11. No built-in tools on the computer, and the API client owns the tools

The decision of ADR-0002 (source: 0002-native-engine-harness-tools.md, section "Decision", and its amendment of 2026-10-06):

- "The bridge disables the built-in tools of the Cascade. The Cascade must not run commands, change files, or use a browser on the Mac."
- "On 2026-10-04, the operator accepted a defense-in-depth standard for the tool exclusion. All four layers are mandatory". The layers are an agent profile with no tools, deny rules for each action, an empty workspace, and a check of each step that stops the Cascade.
- "The operator accepts the residual risk that a black-box probe cannot exclude."
- "The harness owns the tools."
- Amendment: "The instruction acts on the model. It does not block the tool in the engine."

Still applies: the rules of the map say that acp2api declares no file system capability and no terminal capability, and allows no permission request. The `cwd` of each ACP session is an empty temporary folder. If a built-in tool runs with no permission request, acp2api cancels the prompt turn and tells the user ([#1](https://github.com/yandhi-0421/acp2api/issues/1)). The tool filter removes the built-in tools in the official runtime, not only through the model (ADR-0002; [#3](https://github.com/yandhi-0421/acp2api/issues/3), addendum).

acp2api: acp2api cancels the prompt turn at the permission request of each client tool, and it holds no `tools/call` open, see ADR-0002.

acp2api: a new set of client tools causes a rebuild, not an update of a tool catalog in the same ACP session, see ADR-0003.

#### 12. The terms

- ADR-0001, section "Consequences": "The terms name third-party access to the Service as a breach (see Context). This bridge gives third-party harnesses access to the Service. Thus each use of the bridge is a breach of the terms, with each engine. No technical solution can change this. This architecture decreases the risk that Google finds the bridge. It does not make the use agree with the terms."
- ADR-0001, section "Consequences": "The worst case has a precedent. Google can remove a ban (the amnesty), but it can also delete the subscription and give no refund."
- ADR-0001, section "Decision": "On 2026-10-04, the operator accepted the account risk and authorized continued work."

Still applies: [#4](https://github.com/yandhi-0421/acp2api/issues/4) gives the current terms clause, its history, and the FAQ entries.

#### Reasons of the earlier bridge that do not carry over

- ADR-0001 rejected the official runtime for its MVP: "It is a separate binary with its own sign-in, and only a community registry documents it." The prior-art note, section "8.5 Effect on ADR-0001", confirmed the first point. It found no change for the second point, and it added that the registry gives no checksums. acp2api uses the official runtime ([#2](https://github.com/yandhi-0421/acp2api/issues/2)), and it identifies each build by its hashes (ADR-0006).
- ADR-0001 chose the headless mode of the CLI, because "It is the only documented and versioned interface." [#2](https://github.com/yandhi-0421/acp2api/issues/2), C1, says that Google documents the official runtime only through the install steps of editors.
- The prior-art note, section "8.5 Effect on ADR-0001", gives three functions of the official runtime that the CLI does not give: thought text, a model change in a running ACP session, and a standard protocol with an official SDK. acp2api records: no thought text came from version 1.4.0 ([#8](https://github.com/yandhi-0421/acp2api/issues/8)), and a model change uses `session/set_config_option` (ADR-0003).

## Part 3: the Windows exploration and the ACP engine plan

### Windows exploration of the earlier bridge, 2026-10-16

The earlier bridge planned a Windows exploration on a borrowed computer, on 2026-10-16 from about 16:16 to 21:16 (UTC+8). Issue 86 of the earlier project gave the plan, and issues 87 to 94 gave the steps. The plan expected Windows 11. The computer was Windows 10 Home, x64.

Only issue 87, the facts of the computer and of the user profile, has a result. The draft note of the work, in draft pull request 95 of the earlier project, also records the download and the unpack of the archive. The work sent 0 model prompts, 0 `/usage` commands, and 0 `/logout` commands. No coding agent started the official runtime, and the sources have no result of a start by the maintainer.

Thus the sources give no Windows result for these items:

- the start, the sign-in, the process tree, and the stop of the official runtime;
- the console events (issue 90 of the earlier project);
- the experiments with fake child processes (issue 91);
- the fake test suite of the earlier bridge (issue 92). The only measured test result is the line-end failure of WIN-8.

Later, [issue 16](https://github.com/yandhi-0421/acp2api/issues/16) ran the official runtime 1.4.0 on Windows 11 ARM in a VM. This section gives only the facts that its answer does not hold. In this section, the evidence "live run" means a measurement with commands on the real computer. No such measurement started the official runtime.

#### Measured facts

| ID | Fact | Source | Evidence | Touches |
|---|---|---|---|---|
| WIN-1 | The computer was Windows 10 Home, version 10.0.19045, x64. The system locale and the UI language were zh-CN. The ANSI and OEM code pages were 936. The option "Use Unicode UTF-8 for worldwide language support" was off. | Issue 87 of the earlier project, machine capture | live run | [issue 16](https://github.com/yandhi-0421/acp2api/issues/16) (Windows 11 ARM, also zh-CN and code page 936); [issue 17](https://github.com/yandhi-0421/acp2api/issues/17) |
| WIN-2 | The user was a standard user. The user was not in the Administrators group, and the shell was not elevated. `LongPathsEnabled` was 0, and Developer Mode had no enable value. Microsoft Defender real-time protection was on, controlled folder access was off, and Defender was the only antivirus product. Windows Terminal was not installed, and PowerShell 7.6.5 and Windows PowerShell 5.1.19041.5007 were present. | Issue 87 | live run | [issue 17](https://github.com/yandhi-0421/acp2api/issues/17); ADR-0004 (the `TEMP` folder of each runtime process) |
| WIN-3 | The profile was `C:\Users\<user>`, and the temporary folder was `C:\Users\<user>\AppData\Local\Temp`. Neither path had a space, a non-ASCII character, or an 8.3 short name. In Node.js, `os.tmpdir()`, `fs.realpathSync`, and `fs.realpathSync.native` gave the same temporary path. The Desktop, the Documents folder, and the clone were not in OneDrive. | Issue 87 | live run | ADR-0004; ADR-0005 |
| WIN-4 | No parent folder of the profile or of the temporary folder, up to `C:\`, had a `.agents` entry. The check looked only for `.agents`. It did not look for `.gemini`, `.git`, `AGENTS.md`, or `GEMINI.md`. | Issue 87 | live run | ADR-0005; [issue 31](https://github.com/yandhi-0421/acp2api/issues/31): on Windows, no probe checked the parent folders of the `cwd` of an ACP session |
| WIN-5 | Port 47120 was not in an excluded range. `netsh interface ipv4 show excludedportrange protocol=tcp` gave the excluded ports 5357 and 5426. A Node.js listener on `127.0.0.1:47120` succeeded and closed at once. The capture does not say if Windows Firewall showed a message. The earlier bridge proposed to keep an error path for an excluded port on other computers. | Issue 87; draft pull request 95 | live run | [issue 18](https://github.com/yandhi-0421/acp2api/issues/18) (the port of acp2api); [issue 9](https://github.com/yandhi-0421/acp2api/issues/9) (the MCP server of acp2api on 127.0.0.1) |
| WIN-6 | `icacls` on the profile folder gave `(OI)(CI)(F)` to `NT AUTHORITY\SYSTEM`, to `BUILTIN\Administrators`, and to the user. It gave no other identity access. `Get-Acl` showed the three rules as not inherited. | Issue 87 | live run | [issue 7](https://github.com/yandhi-0421/acp2api/issues/7); [issue 18](https://github.com/yandhi-0421/acp2api/issues/18); PRD-20 |
| WIN-7 | The maintainer said that the computer needs a proxy to reach Google. The user system proxy was on, with an address on 127.0.0.1. The shell of the coding agent had no `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, or `NO_PROXY` variable, and WinHTTP was direct. HEAD requests from PowerShell to `https://dl.google.com` and `https://accounts.google.com` gave HTTP 200. The earlier bridge noted that this does not prove that the official runtime uses the proxy. | Issue 87; draft pull request 95; comment of 2026-10-16 on issue 84 of the earlier project | user report (a proxy is necessary); live run (the settings and the HEAD requests) | [issue 22](https://github.com/yandhi-0421/acp2api/issues/22); [issue 17](https://github.com/yandhi-0421/acp2api/issues/17); ADR-0004 (the environment of the runtime process); [issue 2](https://github.com/yandhi-0421/acp2api/issues/2), C1 (a system SOCKS proxy on macOS) |
| WIN-8 | The Git in the program folder of Codex on that computer set `core.autocrlf=true` in its own system `gitconfig`. A checkout with this setting gave CRLF line ends, and a documentation test that expects LF failed. A clone with a local `core.autocrlf=false` passed the documentation tests. The earlier bridge proposed an explicit checkout policy in its first slice. | Issue 87; draft pull request 95 | live run | [issue 17](https://github.com/yandhi-0421/acp2api/issues/17) (the checks and the builds on Windows) |
| WIN-9 | The maintainer downloaded `agy-acp-server-1.4.0-windows-x86_64.zip` from `https://dl.google.com/agy-extensions/releases/windows/` in a browser, and saw no warning. The archive had a `Zone.Identifier` stream, and the files that PowerShell unpacked had none. Both `.exe` files had a valid Authenticode signature that names Google LLC. Nobody measured an unpack with Explorer. The source does not name the browser. | Draft pull request 95; issue 88 of the earlier project (the address) | user report (no warning); live run (the streams and the signatures) | [issue 16](https://github.com/yandhi-0421/acp2api/issues/16), Gaps (the x64 archive); [issue 22](https://github.com/yandhi-0421/acp2api/issues/22); ADR-0006 (the copy); [issue 32](https://github.com/yandhi-0421/acp2api/issues/32) |

#### Static findings

Issue 81 of the earlier project reviewed the Node.js documentation and the code of the earlier bridge for a native Windows port, on 2026-10-13. These of its findings also apply to acp2api.

| ID | Fact | Source | Evidence | Touches |
|---|---|---|---|---|
| WIN-10 | The file mode `0600` does not give owner-only access rules on Windows. | Issue 81 of the earlier project | static | [issue 7](https://github.com/yandhi-0421/acp2api/issues/7); [issue 18](https://github.com/yandhi-0421/acp2api/issues/18); PRD-20 |
| WIN-11 | An environment override can hold one name two times with a different letter case, for example `Path` and `PATH`. Nobody measured which value the child process then sees. The finding is about the `env` object of Node.js. No source checked a Rust program. | Issue 81; issue 91 of the earlier project (the plan of the measurement) | static | ADR-0004 (`TEMP` and `TMP`); ADR-0005 (`GEMINI_HOME`); PRD-17 |
| WIN-12 | Issue 81 named these Windows cases as untested, and nobody measured them later: a walk of the parent folders across drive roots, UNC paths, and directory junctions; a rename over a file that another program holds open; and a rename over a read-only file. | Issue 81; issue 91 | static | ADR-0005; ADR-0006 (the state file); [issue 15](https://github.com/yandhi-0421/acp2api/issues/15) (the state file of a lockout); [issue 17](https://github.com/yandhi-0421/acp2api/issues/17) |

#### Questions that the exploration asked and did not answer

The evidence level of each question is plan.

| ID | Question | Source | What acp2api holds |
|---|---|---|---|
| Q-W1 | What does a Windows console do at Ctrl+C, at Ctrl+Break, and at the close of a tab or a window? Does Ctrl+C reach a child process that the parent started with `detached: false`, and with `detached: true`? How long does the parent process live after a window close? The plan of issue 90 says that the console sends its events to each process that is attached to it, also to the child processes. | Issue 90 of the earlier project | ADR-0004 gives the stop at the exit of acp2api. [Issue 6](https://github.com/yandhi-0421/acp2api/issues/6) gives the rules of `GenerateConsoleCtrlEvent`. No acp2api record names the console events that end acp2api. No record says if the runtime process shares the console of acp2api. |
| Q-W2 | When the parent hides the window of the child process, does a console window open for the official runtime or for one of its child processes? | Issue 88 of the earlier project, run B5 | [Issue 16](https://github.com/yandhi-0421/acp2api/issues/16), W2: the root process had a child `conhost.exe` when SSH started the probe. [Issue 6](https://github.com/yandhi-0421/acp2api/issues/6): the Rust ACP SDK sets `CREATE_NO_WINDOW` on Windows. |
| Q-W3 | Do the stdout lines and the stderr lines of the official runtime end with CRLF on Windows? | Issue 88 | [Issue 16](https://github.com/yandhi-0421/acp2api/issues/16), W7, gives 0 stdout lines that are not JSON. It gives no count of the lines that end with CR. |
| Q-W4 | Does SmartScreen, Microsoft Defender, or Windows Firewall show a message at the first start of the official runtime? | Issue 88, part A | [Issue 16](https://github.com/yandhi-0421/acp2api/issues/16) ran with UAC off and Windows Firewall off. ADR-0006: acp2api starts its own copy of the two executables. |
| Q-W5 | What is the longest path of the unpacked files? | Issue 88, part A | ADR-0004 puts the `TEMP` folder of each runtime process into the folder `acp2api` of the system temporary folder. [Issue 16](https://github.com/yandhi-0421/acp2api/issues/16), W1: each start unpacks 9,778 files into a `_MEI*` folder. On the computer of WIN-2, `LongPathsEnabled` was 0. |
| Q-W6 | Does an ACP session load after `taskkill /T /F` of the process tree? | Issue 88, run B3 | [Issue 16](https://github.com/yandhi-0421/acp2api/issues/16) loaded ACP sessions in later runtime processes. Its answer does not name the stop before each load. ADR-0004 stops the process tree with a Job Object. |
| Q-W7 | Does the official runtime use the system proxy of Windows? | Comment of 2026-10-16 on issue 84 of the earlier project; draft pull request 95 | No acp2api record. [Issue 2](https://github.com/yandhi-0421/acp2api/issues/2), C1: with a system SOCKS proxy on macOS, the setup of an ACP session fails, because the bundled Python has no SOCKS support. |

### Facts about the official runtime in the ACP engine plan

The ACP engine plan of the earlier bridge has the date 2026-10-15. Its section 3 gives facts from the captures of the probe of 2026-10-15 and from the static inspection of version 1.4.0. The plan says that [the probe note](2026-10-15-official-runtime-acp-probe.md) does not state them. The captures are in [`captures/2026-10-15-acp-probe/`](captures/2026-10-15-acp-probe/).

The probe ran on macOS arm64 with one account and `oauth-personal`, and it sent no `clientInfo`. This section gives only the facts that acp2api has not recorded.

| ID | Fact | Source | Evidence | Touches |
|---|---|---|---|---|
| RT-1 | The stderr of the official runtime is a verbose log. Each line starts with a level letter and a time, for example `I1015 11:26:07.556780`. In the probe runs, the log did not contain the text of prompts, replies, or tool arguments. The plan also says that the log contains account data, for example the Google Cloud project ID and the tier. CON-1 gives a conflict about the project ID. | The ACP engine plan, section 3, fact 2 | live run | The map, "Not yet specified" (logs); [issue 15](https://github.com/yandhi-0421/acp2api/issues/15); [issue 18](https://github.com/yandhi-0421/acp2api/issues/18); PRD-9; PRD-12 |
| RT-2 | The Python layer logs each stderr line of a harness process at the level `I`, after the text `harness stderr:`. An upstream model error gives such a line: `harness stderr: received model response error: Error <code>, Message: <text>, Status: <STATUS>, Details: [...]`. A line of a harness process can carry its own Go level mark, for example `ERROR: logging before google.Init: E1015 11:26:07.556773`. | The ACP engine plan, section 3, fact 3, and section 6.7 | live run (`locked-meta-bad-image.stderr.txt`) | [issue 15](https://github.com/yandhi-0421/acp2api/issues/15), decision 1; [issue 25](https://github.com/yandhi-0421/acp2api/issues/25); PRD-9 |
| RT-3 | A built-in tool that runs with no permission request gives a `tool_call` update with the title `Running <tool>` and the status `in_progress`. An example is `Running view_file` with `kind: read`, in an ACP session with no tool filter. A tool of an MCP server of the ACP Client gives a `tool_call` update with the title `Run <tool>?` and the status `pending`. A permission request with the same title follows. In [issue 8](https://github.com/yandhi-0421/acp2api/issues/8), F9, the history replay of a `session/load` had `tool_call` updates with the same title form, for example `Running mcp_probe_probe_big_text`, and `status: "completed"`. | The ACP engine plan, section 3, fact 4 | live run (`read.transcript.ndjson`) | [issue 9](https://github.com/yandhi-0421/acp2api/issues/9), decision 3; ADR-0002; PRD-1 |
| RT-4 | The permission request of the built-in tool `run_command` has the command line itself as its title, with `kind: execute`. It does not have the form `Run <tool>?`. | The ACP engine plan, section 3, fact 7 | live run (`command.transcript.ndjson`, no tool filter) | ADR-0002; [issue 9](https://github.com/yandhi-0421/acp2api/issues/9) |
| RT-5 | At a call of `session/set_model`, the official runtime logs `Superseded session/set_model used by client 'unknown'; clients on session config options send session/set_config_option instead.` The plan concludes that a later version can remove `session/set_model`. The probe sent no `clientInfo`, and the line names the client `'unknown'`. | The ACP engine plan, section 3, fact 8 | live run (for example `command.stderr.txt`) | [issue 2](https://github.com/yandhi-0421/acp2api/issues/2), C4 (`session/set_model` also answers); ADR-0003; [issue 19](https://github.com/yandhi-0421/acp2api/issues/19) |
| RT-6 | The Python layer sends telemetry only with a business sign-in. With `oauth-personal`, it logs `No business auth manager configured, dropping telemetry batch of 1 events`. A personal sign-in in an editor gives the same result. No source tells if the harness processes, which are Go programs, send telemetry of their own. | The ACP engine plan, section 3, fact 9 | static (the rule); live run (the line, also in [issue 8](https://github.com/yandhi-0421/acp2api/issues/8), F11) | [issue 15](https://github.com/yandhi-0421/acp2api/issues/15) (a live runtime process can connect to Google); [issue 4](https://github.com/yandhi-0421/acp2api/issues/4) |
| RT-7 | The reply of `/usage` is Markdown. A section `### Session Usage` comes first. For each group, a line `- **<group>**` comes first, then one italic line that names the models of the group, for example `  *(Models within this group: Gemini Flash, Gemini Pro)*`. Then come the rows `  - **Weekly Limit Remaining:** <N>% remaining (resets <Mon> <D>, <YYYY>, <HH:MM> UTC)` and `  - **Five Hour Limit Remaining:** <N>% remaining (resets ...)`. The groups are `Gemini Models` and `Claude and GPT models`. | The ACP engine plan, section 3, fact 10 | live run (`usage-cmd.transcript.ndjson`) | [issue 15](https://github.com/yandhi-0421/acp2api/issues/15), decision 5 (the reset time from `/usage`); [issue 18](https://github.com/yandhi-0421/acp2api/issues/18); [issue 26](https://github.com/yandhi-0421/acp2api/issues/26); PRD-11 |
| RT-8 | The result of `session/new` has `models.availableModels`, with a `modelId` and a `name` for each model, for example `gemini-3.8-flash-low` and `Gemini 3.8 Flash (Low)`. | The ACP engine plan, section 3, fact 11 | live run | [issue 13](https://github.com/yandhi-0421/acp2api/issues/13), decision 8 (`display_name`) |
| RT-9 | A new ACP session and a loaded ACP session start on the model `gemini-3.8-flash-high` (`currentModelId`). This is also true when the ACP session used another model before the load. The probe selected the model before each prompt. | The ACP engine plan, section 3, fact 12 | live run | ADR-0003 (a change of the model); [issue 13](https://github.com/yandhi-0421/acp2api/issues/13); [issue 2](https://github.com/yandhi-0421/acp2api/issues/2), C4 (the same result on Linux, at the level Inferred); PRD-7 |
| RT-10 | The `available_commands_update` of the official runtime describes the command `logout` as `Log out and clear stored credentials.` | Issue 94 of the earlier project; the captures, for example `bad-model.transcript.ndjson` | live run (2026-10-15, macOS) | [issue 21](https://github.com/yandhi-0421/acp2api/issues/21) (a prompt that starts with `/logout`); [issue 18](https://github.com/yandhi-0421/acp2api/issues/18); ADR-0005 (the token in the macOS keychain) |
| RT-11 | Issue 86 of the earlier project says that the probe P of the plan ran on macOS on 2026-10-15 with 14 prompts and passed its gates. The plan gives two gates. In `tool-task`, the model gives the correct answer after 6 or more tool calls, with a result of about 20,000 characters, an error result, and a valid PNG result. In `load-model`, the tool filter holds after a `session/load` and a change to `gemini-3.8-flash-medium`: no file read and no built-in tool. The results of the other steps are not in the sources, and they were not in the repository of the earlier bridge. | Issue 86 of the earlier project; the ACP engine plan, section 5 | user report (no capture in the sources) | ADR-0002; ADR-0003; [issue 17](https://github.com/yandhi-0421/acp2api/issues/17) |

#### Open questions of the plan with no acp2api answer

The ACP engine plan, section 12, gives the open questions of the earlier bridge. The probe P was to answer most of them, but its results are not in the sources (RT-11). The evidence level of each question is plan.

| ID | Question | Source | What acp2api holds |
|---|---|---|---|
| Q-1 | Does a change of the model keep the context of the ACP session, also after a `session/load`? | The ACP engine plan, section 5 (step `load-model`) and section 12 | ADR-0003 keeps the ACP session at a change of the model or of the thinking level. [Issue 8](https://github.com/yandhi-0421/acp2api/issues/8), F3: a change of `thinking_level` to `low` logged `Selected model gemini-3.8-flash-low …; the agent is rebuilt on the next turn.` No acp2api record checks the context after the change. |
| Q-2 | Which comes first: the error line on stderr, or the result of `session/prompt`? | The ACP engine plan, section 5 (step `error-order`) and section 12 | [Issue 25](https://github.com/yandhi-0421/acp2api/issues/25) asks what acp2api does with the error text that it can stream before stderr shows the error. No measurement gives the order. PRD-10 gives the choice of the earlier bridge. |
| Q-3 | Does macOS start the official runtime after a download in a browser, with no Gatekeeper block? The probe of 2026-10-15 downloaded the archive with a command-line tool. | The ACP engine plan, section 12 | [Issue 8](https://github.com/yandhi-0421/acp2api/issues/8), F1, used the archive of that probe. [Issue 2](https://github.com/yandhi-0421/acp2api/issues/2), C1: one provider removes `com.apple.quarantine` after the unpack, and no source says that macOS blocks the executable without this step (level Inferred). WIN-9 gives the Windows case. |

### Rules of the earlier bridge for the official runtime that acp2api has not decided

The ACP engine plan, section 6, gives the target rules of the earlier bridge for the official runtime. Sections 2.2 and 7 add some. Each row below is a choice of the earlier bridge, not an acp2api decision. acp2api has not decided the point yet, so each row is an open point for the PRD. The evidence level of each row is plan.

This section leaves out each rule that acp2api already decided, the same way or another way. Where a row touches an acp2api decision, the last column names it.

#### The ACP messages

| ID | The choice of the earlier bridge | Source | acp2api now |
|---|---|---|---|
| PRD-1 | The earlier bridge treated each update before the result of `session/load` as a history replay. It sent none of these updates to an API client, and its check of the tool events ignored them. | The ACP engine plan, section 6.1 | No rule. [Issue 9](https://github.com/yandhi-0421/acp2api/issues/9), decision 3, cancels a prompt turn at a `tool_call` with `in_progress` or `completed` and no permission request. RT-3: a replay holds `tool_call` updates with `completed`. Touches ADR-0003. |
| PRD-2 | After the earlier bridge sent `session/cancel` for a prompt, it dropped each later text chunk of that prompt. A dropped chunk went to no API client, and it did not count in the usage estimate. The official runtime sends the chunk `The request was cancelled by the client.` at this time. | The ACP engine plan, section 6.1 | ADR-0002 records that the text goes into the history of the ACP session. No rule says if the API client gets it. Touches [issue 11](https://github.com/yandhi-0421/acp2api/issues/11), [issue 20](https://github.com/yandhi-0421/acp2api/issues/20), and [issue 25](https://github.com/yandhi-0421/acp2api/issues/25). |
| PRD-3 | The earlier bridge accepted only `session/request_permission` from the official runtime. It answered each other request with the JSON-RPC error `-32601`, and it stopped the prompt turn. | The ACP engine plan, sections 6.1 and 6.6 | The map declares no file system capability and no terminal capability. No rule gives the answer to another request. Touches [issue 9](https://github.com/yandhi-0421/acp2api/issues/9) and [issue 25](https://github.com/yandhi-0421/acp2api/issues/25). |
| PRD-4 | The earlier bridge never sent `session/set_mode`. After each `session/new` and `session/load`, `modes.currentModeId` had to be `default`. Else the earlier bridge did not use the ACP session. A `current_mode_update` to another mode stopped the prompt turn. | The ACP engine plan, sections 6.1, 6.3, and 6.6 | No rule about the modes. [Issue 16](https://github.com/yandhi-0421/acp2api/issues/16), W8, records the modes `default`, `auto_edit`, and `yolo`. Touches ADR-0002. |
| PRD-5 | The tool event check of the earlier bridge (its old word was "step check") was fail-closed. It allowed only these events: a `tool_call` with `status: pending` and the title `Run <name>?`, then its permission request; and a `tool_call_update` for a known `toolCallId`, with `pending`, or with `failed` after an answer. Each other tool event stopped the prompt turn and closed the runtime process. | The ACP engine plan, section 6.6 | acp2api: a prompt turn stops only at a tool with `in_progress` or `completed` and no permission request, and no new state starts, see [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9). No rule for the other events. |
| PRD-6 | The MCP server of the earlier bridge answered each `tools/call` with `isError: true` and the text `Tool calls go through the permission request.`, and the prompt turn stopped. A permission request with no option of `kind: reject_once` got `cancelled`, and the prompt turn stopped. | The ACP engine plan, sections 6.5 and 6.6 | ADR-0002: the MCP server of acp2api executes no tool call. No rule gives its answer to a `tools/call`, or the answer to a permission request with no `reject_once` option. Touches [issue 9](https://github.com/yandhi-0421/acp2api/issues/9). |
| PRD-7 | After each `session/new` and `session/load`, and before the first prompt, the earlier bridge selected the model of the request on the ACP session. The reason is RT-9. | The ACP engine plan, section 6.3 | ADR-0003 sends `session/set_config_option` before the prompt at a change of the model or of the thinking level. No rule says that acp2api sets the thinking level after each `session/load`. Touches [issue 13](https://github.com/yandhi-0421/acp2api/issues/13). |
| PRD-8 | Each `session/new` and `session/load` gave one MCP entry: `{"type": "http", "name": "agy-bridge", "url": "http://127.0.0.1:<port>/mcp/<token>", "headers": [{"name": "x-agy-bridge-session", "value": "<session key>"}]}`. The session key had 32 random hex characters, and each `session/new` and `session/load` got a new key. `tools/list` gave the tools of the ACP session of the key. An MCP request with an unknown key got HTTP 404. | The ACP engine plan, sections 6.3 and 6.5 | [Issue 9](https://github.com/yandhi-0421/acp2api/issues/9), decision 6, gives one set of client tools to each ACP session. No record gives the URL or the header that tells the MCP server of acp2api which ACP session sends a request. Touches ADR-0003. |
| PRD-9 | The earlier bridge parsed each stderr line with `Error <code>, Message: <text>, Status: <STATUS>` into `{"code": <code>, "status": "<STATUS>", "message": "<text>"}`. Each other line counted only in two cases. The line starts with the level letter `E` or `F`. Or the text after `harness stderr:` has a Go level mark `E<MMDD>` or `F<MMDD>` before the time. | The ACP engine plan, section 6.7 | [Issue 15](https://github.com/yandhi-0421/acp2api/issues/15), decision 1, reads "the error lines of the session in the stderr", with no rule that finds an error line. RT-2: the upstream error line has the level `I`. Touches [issue 25](https://github.com/yandhi-0421/acp2api/issues/25). |
| PRD-10 | An upstream error line during a prompt turn ended the prompt turn with an error, also when the prompt turn ended with `end_turn`. If the result can come before the line, the plan was to wait `stderrSettleMs` after the result for a late line (Q-2). The stop reasons `max_tokens` and `max_turn_requests` ended the prompt turn with an error. | The ACP engine plan, section 6.7 | No record names the stop reasons `max_tokens` and `max_turn_requests`. Touches [issue 25](https://github.com/yandhi-0421/acp2api/issues/25). The earlier bridge also ended a prompt turn with an error at `refusal`. acp2api: `refusal` starts a quota wait, see [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15). |
| PRD-11 | Each runtime process had one ACP session for information requests, with the same setup and no tools. `/usage` was a `session/prompt` with the text `/usage` in that ACP session, and its text never went to an API client. Each information request held a place. After a `refusal`, the earlier bridge sent one `/usage` when a place was free. Its parser needed all four rows of RT-7, and the reset times are UTC. | The ACP engine plan, sections 6.7 and 6.10 | [Issue 15](https://github.com/yandhi-0421/acp2api/issues/15), decision 5, reads the reset time from the error text or from `/usage`. Decision 6 sends no `/usage` in a lockout. No record gives the ACP session or the time of `/usage`. Touches [issue 14](https://github.com/yandhi-0421/acp2api/issues/14) and [issue 18](https://github.com/yandhi-0421/acp2api/issues/18). |
| PRD-12 | The earlier bridge did not copy the stderr lines to its terminal or to its log, because they contain account data. | The ACP engine plan, section 6.1 | The map lists logs and personal data as "Not yet specified". [Issue 15](https://github.com/yandhi-0421/acp2api/issues/15) keeps the original error text of Google as the evidence of a lockout. [Issue 16](https://github.com/yandhi-0421/acp2api/issues/16), W5: the tier lines of stderr contain the e-mail address in URL-encoded form. Touches [issue 18](https://github.com/yandhi-0421/acp2api/issues/18). |
| PRD-13 | Each information request had a hard limit of 90 s (`engineInfoTimeoutMs`): `initialize`, the `session/new` of the ACP session for information requests, and `/usage`. At the limit, the earlier bridge closed the runtime process. | The ACP engine plan, section 6.1 | acp2api: `initialize` has 5 minutes, and a prompt turn has 30 s after `session/cancel`, see ADR-0004. A prompt turn with no ACP update for 5 minutes gets a cancel ([issue 14](https://github.com/yandhi-0421/acp2api/issues/14)). No record gives a limit for `session/new`, `session/load`, `session/set_config_option`, or `/usage`. In the VM of [issue 16](https://github.com/yandhi-0421/acp2api/issues/16), W3, `session/new` took 4.4 to 29.3 s, and `session/load` took 7.6 s and 16.2 s. |
| PRD-14 | With client tools, the system text of the earlier bridge ended with a tool rule: `Use only the tools that the server agy-bridge gives you. Never call a built-in tool, for example manage_task, run_command, browser tools, search_web, send_message, or schedule, also when it seems available. If a request needs a tool that you do not have, reply in text that you cannot do it in this conversation.` With no client tools, it had a rule that forbids each tool call. | The ACP engine plan, section 6.3; issue 88 of the earlier project (the text with no tools) | [Issue 23](https://github.com/yandhi-0421/acp2api/issues/23) sends the system prompt of the request in `_meta.antigravity.systemPrompt.append`. No record adds a text of acp2api about built-in tools. Touches ADR-0002. |

#### The checks of each version

The earlier bridge ran a version test in new ACP sessions for each new version of the official runtime, on the computer of the maintainer. Its old word was "acceptance test". acp2api has a compatibility check for the same purpose ([issue 22](https://github.com/yandhi-0421/acp2api/issues/22), decision 7).

| ID | The choice of the earlier bridge | Source | acp2api now |
|---|---|---|---|
| PRD-15 | Check 3, the tool filter after a load: the earlier bridge made an ACP session with no tools and loaded it again with `session/load`. Then it sent `Run the shell command "echo agy-bridge" and show its output.` No tool could run. Only `list_resources` or `read_resource` could send a permission request, and each such request got `reject_once`. | The ACP engine plan, section 6.8 | [Issue 22](https://github.com/yandhi-0421/acp2api/issues/22), decision 7: the compatibility check loads the ACP session, but it sends no prompt after the load. Touches [issue 17](https://github.com/yandhi-0421/acp2api/issues/17), ADR-0002, and ADR-0006. |
| PRD-16 | Check 4, a built-in tool with no update: in an ACP session with no tools, the earlier bridge sent `Call the tool manage_task with the action list, and show its output.` The reply could not contain `No background tasks`, the text of the tool when it ran with no tool filter. The check reads the model text, so it can miss a run with no output, as an accepted limit. In each check, `modes.currentModeId` had to be `default`, and the model was `gemini-3.8-flash-low`. | The ACP engine plan, sections 3 (fact 13) and 6.8 | [Issue 22](https://github.com/yandhi-0421/acp2api/issues/22), decision 7, has no check for `manage_task`, the mode, or the model of the check. Touches [issue 17](https://github.com/yandhi-0421/acp2api/issues/17) and ADR-0002. |

#### The runtime process and Windows

| ID | The choice of the earlier bridge | Source | acp2api now |
|---|---|---|---|
| PRD-17 | The runtime process got the environment of the earlier bridge, minus `AGY_CLI_DISABLE_AUTO_UPDATE`, each name that contains `TELEMETRY`, and `GEMINI_HOME`, in any letter case. The earlier bridge refused to start when its configuration set such a name. On Windows, a configured name replaced an inherited name that differs only in letter case (WIN-11). | The ACP engine plan, section 6.1 | acp2api: `GEMINI_HOME` is the runtime home of acp2api, see ADR-0005. ADR-0004 sets `TEMP` and `TMP` on Windows. No record says which other names acp2api passes or removes, for example the proxy variables of WIN-7. |
| PRD-18 | The working directory of the runtime process was the workspace, the empty folder that was also the `cwd` of each ACP session. | The ACP engine plan, section 6.1 | ADR-0005 gives the `cwd` of each ACP session. No record gives the working directory of the runtime process. [Issue 8](https://github.com/yandhi-0421/acp2api/issues/8), F5: the working directory of the runtime process stayed empty. Touches ADR-0004. |
| PRD-19 | The earlier bridge refused to start when a parent folder of the real path of the workspace contained `.agents/`. On Windows, the check had to work with drive roots and UNC paths, and it had to resolve directory junctions as links. | The ACP engine plan, sections 6.3 and 6.12 | ADR-0005 puts each `cwd` into the system temporary folder. Its consequences give a check of the parent folders to the acceptance test of a version, not to each start. WIN-4 gives one Windows 10 computer. Touches [issue 31](https://github.com/yandhi-0421/acp2api/issues/31) and [issue 17](https://github.com/yandhi-0421/acp2api/issues/17). |
| PRD-20 | On Windows, the data folder was `%USERPROFILE%\.agy-bridge`. The earlier bridge set no access rules on its state files, so they kept the rules of the profile folder. It accepted this limit, because by default only the user, SYSTEM, and the administrators can read that folder (WIN-6, WIN-10). | The ACP engine plan, sections 2.2 and 6.12 | [Issue 18](https://github.com/yandhi-0421/acp2api/issues/18) has the location of the data folder as open. acp2api: only the computer account of the user can read the configuration file, see [Does acp2api require a key for its local API and its management panel?](https://github.com/yandhi-0421/acp2api/issues/7). WIN-6: the rules of the profile also give access to SYSTEM and to the Administrators group. No record gives the method on Windows. |
| PRD-21 | Ctrl+C and the close of the console window stopped the earlier bridge, as on macOS. At the shutdown, it stopped new requests, and it sent `session/cancel` for the prompt turn that ran. It answered each open permission request with `cancelled`, and it ended each open response with an error. Then it closed the runtime process. | The ACP engine plan, sections 6.1 and 6.12 | ADR-0004 gives the stop at the exit of acp2api. No record names the events that start the exit on Windows (Q-W1). Touches [issue 18](https://github.com/yandhi-0421/acp2api/issues/18). |
| PRD-22 | The login command refused to run while an instance of the earlier bridge with the same data folder served on its port. The maintainer never ran two instances at the same time. During the Windows work, no instance and no official runtime ran on the Mac. | The ACP engine plan, sections 6.2 and 6.12; issue 86 of the earlier project | ADR-0005 deletes each session file in the runtime home at the start of acp2api. No record says what a second acp2api with the same data folder does. No record says if one Google account can serve acp2api on two computers at the same time. Touches [issue 15](https://github.com/yandhi-0421/acp2api/issues/15) and [issue 18](https://github.com/yandhi-0421/acp2api/issues/18). |
| PRD-23 | The status page and the terminal of the earlier bridge showed the version of the official runtime. The disk use came from the sizes of the session files. The earlier bridge read the sizes only, and it never opened a session file. | The ACP engine plan, section 6.11 | [Issue 18](https://github.com/yandhi-0421/acp2api/issues/18) is open. |

#### Tests

| ID | The choice of the earlier bridge | Source | acp2api now |
|---|---|---|---|
| PRD-24 | The tests used a fake ACP server with the message shapes of the probe captures. A scenario file gave a list of runtime processes, and each new process took the next entry: the `initialize` data and a list of prompt turns. A prompt turn could send text, a `usage_update`, a client tool call, a built-in tool event, an MCP `tools/call`, a stderr line, a JSON-RPC error, a stop reason, or an exit. At `session/cancel`, the fake sent `The request was cancelled by the client.` and the stop reason `cancelled`. At `session/load`, it replayed the stored history before its result. For the tests of the process tree, a scenario could start one child process that stays alive until a forced stop. | The ACP engine plan, section 6.13 | [Issue 17](https://github.com/yandhi-0421/acp2api/issues/17) is open. CON-3 gives a conflict about the time of the MCP connection. |
| PRD-25 | A guard refused each command that is not the fake ACP server while tests ran, with `/` and `\` as the same separator. Tests used temporary folders for the data folder, the workspace, and the home folder. A GitHub Actions workflow ran the tests on `windows-latest` with Node.js 22 for each pull request. It had no secret, no real official runtime, and no network service except npm. | The ACP engine plan, sections 6.13 and 7 | [Issue 17](https://github.com/yandhi-0421/acp2api/issues/17) is open. WIN-8 gives the line ends of a Windows checkout. |

### Conflicts with acp2api records

- **CON-1.** RT-1: the ACP engine plan names the Google Cloud project ID as account data in stderr. [Issue 16](https://github.com/yandhi-0421/acp2api/issues/16) records the project ID in stderr as the shared value `aicode-consumers`, and `cancel.stderr.txt` of the captures of 2026-10-15 has the same value. The account data that issue 16 found is the e-mail address in the tier lines (W5).
- **CON-2.** Issue 41 of the earlier project, of 2026-10-07, says that the official runtime sends thought text. Its source is section 8 of the prior-art note of the earlier bridge (2026-10-03), which is about the Pi provider `zacbemis/pi-antigravity-acp-provider`. [Issue 8](https://github.com/yandhi-0421/acp2api/issues/8), F10, got 0 `agent_thought_chunk` with `thinking_level: high`, and the probe of 2026-10-15 also got none.
- **CON-3.** The plan of issue 88 of the earlier project expected that an eager MCP server gets `initialize` and `tools/list` at `session/new`. [Issue 8](https://github.com/yandhi-0421/acp2api/issues/8), F4, and [issue 16](https://github.com/yandhi-0421/acp2api/issues/16), W8, saw no MCP connection before the first prompt turn of an ACP session.
