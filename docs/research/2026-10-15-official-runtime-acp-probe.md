# The official runtime as the engine of a bridge: static inspection and live probe

Date checked: 2026-10-15 (Asia/Taipei).

Provenance: The maintainer wrote this note on 2026-10-15 for an earlier project, a bridge that used the Antigravity CLI (`agy`) as its engine. The note examined a change of that engine to the official runtime 1.4.0. The note moved to acp2api on 2026-10-18, because that project will be deleted. The move removed the links to the files of that project and changed no fact. The raw lines of the probe are in [`captures/2026-10-15-acp-probe/`](captures/2026-10-15-acp-probe/). The move replaced the home path in them with `~`.

Scope: a proposal to replace the engine of the earlier bridge (the Antigravity CLI in headless stream-json mode) with the official runtime.

Method: public documentation, the ACP schema, GitHub issues, the code of the earlier bridge, the strings of the programs of the official runtime (section 3.3), and a live probe (sections 8 and 9).

Words of the earlier project:

- The earlier bridge: the earlier CLI bridge of the maintainer. It translated the three APIs of acp2api to the Antigravity CLI.
- The maintainer: the person who ran the earlier bridge and who maintains acp2api. The original note said "operator".
- The CLI: the Antigravity CLI `agy`, the engine of the earlier bridge. It has no ACP mode.
- Cascade: a conversation of the CLI engine. The earlier bridge kept a record for each Cascade. It marked the record `broken` after a cancel by SIGINT, and the next request then did a rebuild.
- The harness: the Go program `localharness_external` of the official runtime. acp2api calls its process a harness process. The original note also used "harness" for an API client; this copy writes "API client" for that meaning.
- The four layers: the protection of the earlier bridge against built-in tools. Layer 1 was a locked agent profile with no tools. Layer 2 was deny rules in the project file of the CLI. Layer 3 was an empty workspace with no `.agents/` above it. Layer 4 was the step check.
- The step check: the check of the earlier bridge that stopped a CLI turn at an unexpected tool event of the CLI.
- The fail-stop: the stop of the earlier bridge after an account-level signal. acp2api calls the matching state "lockout".
- The gate: the rule of the earlier bridge that only one conversation generates at a time.

Related: [the findings of the earlier CLI bridge](2026-10-18-earlier-cli-bridge-findings.md), for the MCP passthrough prototype of the CLI and the research on other bridges.

## 1. Summary

Result of the probe of 2026-10-15 (section 9):

| Gate | Result |
|---|---|
| G0, a second signed-in client | Accepted by the maintainer. The maintainer signed in. |
| G1, tool lock | Pass, with conditions. `_meta.antigravity.enabledTools: []` removes each built-in tool, also `manage_task`. Each remaining tool call asks for permission before it runs. With no filter, the official runtime is weaker than the CLI: a file read and `manage_task` ran with no permission request. |
| G2, hold time | Fail for a held MCP call: the official runtime ends a `tools/call` after 3 minutes, and then the model continues without the result. No ACP field changes the limit. A different tool design passed one test (section 9.8): the client cancels the prompt turn at the permission request of the tool, and it gives the tool result as text in the next prompt of the same ACP session. No call stays open, so no time limit applies. |
| G3, models | No difference. The ACP session gives only `gemini-3.8-flash` (`-high`, `-medium`, `-low`). The maintainer said that the CLI now also gives only this model, and that the Claude and GPT models will retire soon. |
| G4, fail-stop signals | Possible from stderr. For an HTTP 400, the reply text had only the message. The log of the official runtime had a line in a fixed format with the HTTP code and the status: `Error 400, Message: ..., Status: INVALID_ARGUMENT` (section 9.9). The probe did not cause an account-level error, so the 403 path is not seen. |

Other results:

- Quota: the slash command `/usage` of the official runtime gives both model groups and both windows. It sends no model request. The CLI is not necessary for quota data.
- Cancel: `session/cancel` ended the prompt turn in 10 ms. The same ACP session then answered a new prompt. Thus a cancel does not need a rebuild of the conversation, if the client changes its rule.
- Resume: `session/load` in a new process of the official runtime continued the conversation.
- Thought text: no `agent_thought_chunk` came, also with `gemini-3.8-flash-high`. The wish of the earlier bridge for thought text gets no help.
- Token usage: `usage_update` gives only the context size. A client cannot give the API client the input, output, thinking, and cache tokens of each response, as the earlier bridge did with the CLI.
- SIGINT was not a reason of the earlier bridge to reject the official runtime. Its engine decision gave two reasons: a separate binary with its own sign-in, and documentation only in a community registry. The earlier bridge kept the cancel on Windows out of its scope.
- The tool filter, the eager MCP option, and the system text option are in the source of the official runtime, not in a public document.

acp2api: the later probes of acp2api confirmed and extended these results on macOS and Windows. See [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8) and [Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16).

## 2. Sources

| Alias | Source |
|---|---|
| `acp` | https://github.com/agentclientprotocol/agent-client-protocol at commit `1cebe1c01837379abf34dd62b71e9c75bf140669` (2026-10-14): `docs/protocol/v1/*.mdx`, `schema/v1/schema.json`, `schema/v1/schema.unstable.json`, `docs/rfds/tool-call-name.mdx`, `docs/announcements/session-usage-stabilized.mdx`. The pages at https://agentclientprotocol.com/protocol/v1/ have the same text. |
| `reg` | https://github.com/agentclientprotocol/registry: `antigravity-acp/agent.json` at commit `594f25bfb5ef48082c11f3c43ad0c11f60488e9d` (2026-10-14); `.protocol-matrix/latest.json` at commit `ae79bdd` (generated 2026-10-14). |
| `zed` | https://antigravity.google/docs/ide/extensions/zed/ and https://antigravity.google/docs/ide/extensions/ (fetched 2026-10-15). |
| `faq` | https://antigravity.google/docs/faq/ (fetched 2026-10-15). |
| `cli` | https://github.com/google-antigravity/antigravity-cli at commit `89081f7` (2026-10-14): `CHANGELOG.md`, issues. |
| `node` | https://github.com/nodejs/node branch `v22.x` (`9b16463`), `doc/api/child_process.md`, `subprocess.kill([signal])`. |
| `piacp` | The Pi provider for the official runtime, `zacbemis/pi-antigravity-acp-provider`, commit `07e369b`. |

Issues in `cli` come from users, unless the row says otherwise. This note marks each fact from them as "user report".

## 3. Facts

### 3.1 ACP protocol

| Topic | Fact | Source |
|---|---|---|
| Cancel | The client sends the notification `session/cancel`. The agent "SHOULD stop all language model requests and all tool call invocations as soon as possible". It "MUST respond to the original `session/prompt` request with the `cancelled` stop reason". The client "MUST respond to all pending `session/request_permission` requests with the `cancelled` outcome". | `acp:docs/protocol/v1/prompt-turn.mdx`, section "Cancellation" |
| Late updates | The agent "MAY send `session/update`" after the cancel, before it answers `session/prompt`. The client "SHOULD still accept tool call updates". | Same |
| MCP servers | `session/new` takes `cwd`, `additionalDirectories`, `mcpServers`, and `_meta`. The `http` and `sse` entries have `name`, `url`, and `headers`. The `stdio` entry has `name`, `command`, `args`, and `env`. No entry has a timeout field. The capability `mcpCapabilities.http` permits `http`. | `acp:schema/v1/schema.json`, `NewSessionRequest`; `session-setup.mdx` |
| Tool calls | `tool_call` has `toolCallId`, `title`, and optional `name`, `kind`, `status`, `content`, `locations`, `rawInput`, `rawOutput`. No field names an MCP server. "ACP does not define a naming or namespacing scheme." | `acp:schema/v1/schema.json`; `acp:docs/rfds/tool-call-name.mdx` |
| Permissions | "The Agent MAY request permission from the Client." A permission request is not mandatory before a tool runs. | `acp:docs/protocol/v1/tool-calls.mdx` |
| Tool set and system text | The stable protocol has no field that removes agent tools or sets the system text. Session modes can change the tools, as each agent defines. The unstable schema has `NewSessionRequest.systemPrompt` (`append` or `override`) behind `sessionCapabilities.systemPrompt`. The override "does not change tool availability, MCP servers, permissions". | `acp:session-modes.mdx`; `acp:schema/v1/schema.unstable.json` |
| Errors | The codes are the JSON-RPC codes, `-32800` (request cancelled), `-32000` (authentication required), and `-32002` (resource not found). The error page says "Documentation coming soon". No rule tells how an agent reports a model error. | `acp:schema/v1/schema.json`, `ErrorCode`; `acp:docs/protocol/v1/error.mdx` |
| Stop reasons | `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`. | `acp:schema/v1/schema.json` |
| Client identity | `clientInfo` in `initialize` is optional. Clients "SHOULD provide" it. | `acp:docs/protocol/v1/initialization.mdx` |
| Usage | `usage_update` (`used`, `size`, optional `cost`) is stable since 2026-06-05. An agent "MAY" send it. It describes the context window, not a subscription quota. `PromptResponse.usage` is unstable. | `acp:docs/announcements/session-usage-stabilized.mdx`; `acp:schema/v1/schema.unstable.json` |
| Slash commands | The agent sends `available_commands_update`. The client runs a slash command as prompt text, for example `/web <query>`. | `acp:docs/protocol/v1/slash-commands.mdx` |

### 3.2 The official runtime

| Topic | Fact | Source |
|---|---|---|
| Release | Version `1.4.0`, author `Google LLC`, license `proprietary`. Archives on `dl.google.com` for macOS (arm64, x86_64), Linux, and Windows. The command is `./agy_acp_server.par` (`.exe` on Windows). The entry has no checksum. Earlier versions: 1.3.0 (2026-10-02), 1.2.1 (2026-09-23), 1.1.1 (2026-09-03). | `reg:antigravity-acp/agent.json` |
| Capabilities | `authMethods: ["agent"]`; `loadSession`, `sessionList`, `sessionResume` are true; `sessionFork`, `sessionStop`, `setModel` are false. `session/new` with no sign-in gives `-32000` `Authentication required`. A resume of an unknown ACP session gives `-32002` `Session not found in the current GEMINI_HOME`. | `reg:.protocol-matrix/latest.json` |
| More capabilities | Version 1.3.0: MCP `http` and `sse` true; prompt `image`, `audio`, and `embedded_context` true; auth IDs `oauth-personal`, `oauth-business`, `gemini-api-key`, `agent-platform`; modes `default`, `auto_edit`, `yolo`. User report. | `cli` issue #31, comment 5998582492 |
| Official documentation | The Zed page installs the official runtime through "External Agents > Add > Install from Registry". It gives the auth types `oauth-personal` ("Free, Pro, and Ultra"), `oauth-business`, `gemini-api-key`, and `agent-platform`. It says nothing about tools, permissions, or MCP. The 93 pages of the docs site do not contain "ACP" or "Agent Client Protocol". The changelog has no ACP item. | `zed`; docs sitemap |
| Support | Google staff answer issues about the official runtime with "this repository is exclusively for the Antigravity CLI". | `cli` issues |
| Sign-in | The official runtime keeps its own sign-in in `~/.gemini/antigravity-acp/`. The CLI sign-in does not authenticate it. User reports. The official page says that the account system is "unified" across the products. | `piacp:README.md:68`; `cli` issues #31, #1045; `zed` |
| Models | With `oauth-personal`, version 1.3.0 gave only Gemini models. Another model gave `-32602` "Model ... is not available for the current authentication method". User report. | `cli` issue #1168 |
| Cancel | Version 1.1.1: no answer to `session/prompt`, and no action on `session/cancel`. User report. The issue is closed (2026-09-29). | `cli` issue #1041 |
| Quota and usage | Version 1.1.1 gives only `stopReason`, with no usage data. Quota is available only from the CLI command `/usage`. User report. | `cli` issue #1045 |
| Permissions | Version 1.3.0 sends `session/request_permission` for `run_command`, with `allow_always`, `allow_once`, and `reject_once`. User report. | `cli` issue #1053 |
| Sessions | An ACP session cannot continue after a restart of the official runtime. User report. Section 9.5 contradicts it for version 1.4.0. | `cli` issue #997 |
| Other | The official runtime reads `~/.gemini/config/hooks.json` (#1152). MCP server names must match `^[a-zA-Z0-9_-]+$` (#956). The official runtime has fewer slash commands than the CLI (#888). User reports. | `cli` issues |
| Custom agents, project permissions, `.agents/mcp_config.json`, `timeoutSeconds` | No source tells whether the official runtime reads them. | — |
| Auto-update | No source tells that the official runtime updates itself. The registry gives one archive for each version. | `reg` |

### 3.3 Static inspection of the official runtime 1.4.0

On 2026-10-15, with the approval of the maintainer, the agent downloaded `agy-acp-server-1.4.0-darwin-arm64.zip` (112,734,313 bytes, SHA-256 `21a32eeab85b394a515510c84539bda4b2504bf2917e0a201f5c4130b13581a5`) from the registry URL. The archive has two programs:

- `agy_acp_server.par` (281 MB). It contains the Python source of the ACP server and of the `google.antigravity` SDK.
- `localharness_external` (120 MB). It is a Go program, the agent harness.

Both programs have the signature `Developer ID Application: Google LLC (EQHXZ8M8AV)`. The agent read only the output of `strings`. It did not change or decompile the programs. This list gives facts, not code.

| Topic | Fact |
|---|---|
| Built-in tool filter | `session/new`, `session/load`, and `session/resume` read `_meta.antigravity.enabledTools` and `_meta.antigravity.disabledTools`. The source text says: "`enabledTools` is an allowlist: every built-in outside it is disabled, so `[]` disables all of them." The filter governs the SDK built-ins: `list_directory`, `search_directory`, `find_file`, `view_file`, `create_file`, `edit_file`, `run_command`, `ask_question`, `start_subagent`, `run_workflow`, `generate_image`, `search_web`, `read_url_content`, `schedule`, `finish`. "An MCP/custom tool name ... this filter does not govern". |
| Harness tools | The Go harness has more tools, for example `manage_task`, `send_message`, `define_subagent`, `manage_subagents`, and `notebook`. No string shows that the filter governs them. |
| Custom agents | The official runtime gives the harness no agent file. The config options are only `model`, `mode`, and `thinking_level`. |
| Modes | The modes change only the permission handler. `default` asks. `auto_edit` approves the edit tools. `yolo` approves all tools. |
| Permissions | With no filter, these tools run with no question: `view_file`, `list_directory`, `search_directory`, `find_file`, `schedule`, `finish`, `ask_question`. Each other tool asks, also each MCP tool. The official runtime loads no deny rules. Its `settings.json` holds only the auth type and the Google Cloud project. |
| MCP | `http` and `sse` entries become Streamable HTTP servers. No ACP field sets a tool call timeout, and the official runtime ignores `timeoutSeconds` in `mcp_config.json`. The harness has a tool call timeout. Its default value is not in the strings. The model reads MCP tool schemas from files with `view_file`, unless `_meta.antigravity.eagerMcpServers` names the server. |
| Global files | The official runtime reads `<home>/config/mcp_config.json`, `<home>/config/hooks.json`, and skills from `<home>/config/skills`, `<home>/antigravity-cli/skills`, `<cwd>/.gemini/skills`, and `<cwd>/.agents/skills/`. `<home>` is `$GEMINI_HOME`, else `~/.gemini`. A `hooks.json` in the workspace runs only when the workspace is trusted. |
| System text | `_meta.antigravity.systemPrompt.append` adds text to the system text. No field replaces the system text. |
| Models | The official runtime removes each model whose ID does not start with `gemini`, unless `clientInfo.name` is `zed` or `xcode`, or starts with `jetbrains.`. The filter uses the client name, not the auth method. |
| Client name | `clientInfo.name` and `clientInfo.version` go into the `User-Agent` of the requests of the official runtime, as `host_path=<name>/<version>`. |
| Errors | The official runtime does not give a model error as a JSON-RPC error. It sends the error text as an `agent_message_chunk` and ends the prompt turn with a stop reason (`end_turn`, or `refusal` for an exhausted quota). The text can contain an HTTP code, a status such as `PERMISSION_DENIED`, an appeal link, or a quota reset time. The official runtime gives no error ID and no retry flag. `-32000` with `data.reason` (`onboarding_failed`, `logged_out`) occurs only on the sign-in paths. |
| Quota | The slash command `/usage` reads the quota summary from the service and returns Markdown text. It sends no model request. |
| Cancel | `session/cancel` sends a halt request to the harness. The prompt turn ends with `cancelled` and one text chunk. |
| Sessions | Each conversation of the official runtime is a SQLite file in `<home>/antigravity-acp/conversations/`. `session/load` replays the history. A client must send `mcpServers`, the tool filter, and the system text again on each load. |
| Updates | Neither program has an updater. |

acp2api: the runtime home and the session files are in ADR-0005. The build hashes are in ADR-0006. The client name is in [Which `clientInfo` does acp2api send to the official runtime?](https://github.com/yandhi-0421/acp2api/issues/19). The lockout signals are in [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15).

### 3.4 Other facts

- Node 22 on Windows: "the process will always be killed forcefully and abruptly (similar to `'SIGKILL'`)" for `'SIGINT'` (`node`). Thus the SIGINT cancel of the earlier bridge does not port to Windows.
- The official FAQ says: "Using third-party software, tools, or services to access Antigravity is a violation of our Terms of Service". It also says: "Wrapping `agy` inside third-party coding agents or applications, or automating account rotation, is not supported under consumer plans" (`faq`). These statements apply to each engine. The engine decision of the earlier bridge already accepted this risk.
- The official runtime writes its sign-in in its own directory. A rule of the earlier bridge forbade a read of it. Another rule forbade a download of the official runtime from `dl.google.com`: the maintainer installs it.

## 4. The engine contract of the earlier bridge and the ACP equivalents

"Equivalent" means that the official runtime gives the same function. "None" means that it gives no equivalent. The last column gives the result after the static inspection (section 3.3) and the probe (section 9).

| Contract of the CLI engine of the earlier bridge | ACP equivalent | Result |
|---|---|---|
| Locked agent profile (layer 1): `excludeDefaultComponents`, `inheritCustomizations: false`, `commandExecutionPolicy: "off"`, `tools: []` | `_meta.antigravity.enabledTools: []`. The official runtime ignores agent files. | Equivalent, stronger: no built-in tool stays, also not `manage_task` |
| System text and tool catalog in the body of the agent file | `_meta.antigravity.systemPrompt.append`. | Equivalent (append only) |
| Deny rules in the CLI project file (layer 2) | The client answers each `session/request_permission`. With the tool filter, each tool call asks. | Equivalent, stronger: the answer comes before the tool runs |
| Empty workspace with no `.agents/` above it (layer 3) | `cwd` of `session/new`. | Equivalent |
| Step check on `step_update` events (layer 4) | `tool_call` updates and permission requests. With eager MCP, only the title `Run <tool>?` names the tool. | Equivalent, with a weaker tool identity |
| Stop at `denied_actions` (step check) | The client sees each permission request that it refuses. | Equivalent |
| MCP server of the earlier bridge with `timeoutSeconds: 1800`; a pending tool call waits up to 29 minutes | The official runtime ends a call after 3 minutes. No field changes it. With the design of section 9.8, no call stays open. | None for a held call; a different design passed one test |
| Each `tools/call` carries the conversation ID in `_meta` | One MCP URL or header for each ACP session. | Equivalent, stronger |
| Quota rows from `agy -p /usage` (the quota stop of each model group) | The slash command `/usage`, as Markdown text, with no model request. | Equivalent, new parser |
| Fail-stop signals from stderr, `AGY_ERROR`, and `result.error` | `-32000` on the sign-in paths. An upstream error gives a log line `Error <code>, Message: <text>, Status: <status>`, and the reply text has only the message. | Partial: a fixed text format, not JSON; the 403 path is not seen |
| Cancel by SIGINT; the record becomes `broken` | `session/cancel`: 10 ms, and the ACP session stays usable. | Equivalent, stronger |
| Continue with `--conversation`; one process for each Cascade; close after 5 idle minutes | `session/load` in a new process continued the conversation. One process of the official runtime can hold many ACP sessions. | Equivalent |
| Model change in the same model group | `session/set_model` worked. | Equivalent |
| Model list and model groups by ID prefix | The model list of the ACP session: `gemini-3.8-flash` only. | Equivalent (the maintainer: the CLI gives the same model now) |
| Token usage from `step_update` | `usage_update` gives the context size only. No input, output, thinking, or cache split. | Partial |
| Acceptance test for each CLI version | `agentInfo` in `initialize`. The maintainer updates the official runtime. | Equivalent, with work for the maintainer |
| No thought text | No `agent_thought_chunk` came. | No gain |

## 5. Gates

The earlier bridge set these gates. Each gate had to pass before the official runtime could replace the CLI. A failed gate stopped the change, or it needed a new decision of the maintainer about the engine or the tool policy.

- **G0, decision of the maintainer.** The official runtime needs its own sign-in. This adds a signed-in client on the account. If the CLI stays for `/usage`, the earlier bridge uses two signed-in clients. The maintainer decides this before a live probe.
- **G1, tool lock.** The official runtime must give a tool lock that is not weaker than layer 1 with the catalog instruction of the earlier bridge. The earlier bridge made all four layers mandatory. If no lock exists, the model can see each built-in tool, not only `manage_task`. Then the maintainer must decide the tool policy again.
- **G2, hold time.** The MCP client of the official runtime must keep a `tools/call` open for approximately 30 minutes. With the CLI, a call that reaches its limit ends with an error, and the model continues without the result (the MCP passthrough prototype of the CLI, set C; see [the findings of the earlier CLI bridge](2026-10-18-earlier-cli-bridge-findings.md)). Thus, with a short limit, a client tool that runs longer than the limit makes one of two results:
  - the earlier bridge cancels before the limit, the Cascade becomes `broken`, and the next request rebuilds the conversation with one more model request; or
  - the model continues outside the gate, which a rule of the earlier bridge forbade.
- **G3, models.** With the sign-in of the maintainer, the ACP session must list the Claude and GPT models. User report #1168 says that it does not.
- **G4, fail-stop signals.** The errors of the official runtime must carry enough upstream detail for the signal table of the earlier bridge: an HTTP 403, `PERMISSION_DENIED`, a Terms of Service block, a sign-out, and a quota cap. A probe must not cause a 403 or a block on purpose. Thus only static evidence and normal errors can answer G4. If the errors are generic, the maintainer chooses between two options:
  - a fail-stop on each unknown error, which stops the earlier bridge more often; or
  - a gap in the fail-stop rule.

Quota is not a gate: the `/usage` command of the official runtime gives the quota rows.

Section 1 gives the result of each gate. For G2, the probe tested one more tool design (section 9.8). The client ends the prompt turn with `session/cancel` at the permission request of the client tool, and it sends the tool result as text in the next prompt of the same ACP session. Then no `tools/call` stays open. The tool result becomes a user message, not an MCP result.

acp2api: this design is cancel at permission, see ADR-0002.

## 6. Possible gains

| Gain | Condition |
|---|---|
| A cancel with no signal, on each platform | Confirmed: 10 ms. |
| A conversation continues after a cancel, with no rebuild | Confirmed for the official runtime. The rule of the earlier bridge "a cancelled Cascade becomes `broken`" must change. |
| A tool lock that acts before a tool runs | Confirmed with `enabledTools: []` and permission answers. |
| A smaller fixed context | 4,802 tokens with the tool filter, 14,838 tokens with no filter. The probe did not measure the CLI. |
| Thought text | Not confirmed: no `agent_thought_chunk` came. |
| No time limit for a client tool, and no open HTTP request while the API client runs the tool | The design of section 9.8. One test passed. |
| One MCP configuration for each ACP session, with no `.agents/mcp_config.json` and possibly no CLI project file in `~/.gemini/config/projects/` | G1 passes without the project file. |
| Image input as a prompt block | The official runtime has `promptCapabilities.image`. |
| A model change in a running ACP session | The official runtime has a model setting for the ACP session. |
| A standard protocol with structured tool events | — |

Costs:

- One more signed-in client (G0).
- No official documentation of the contract of the official runtime, and no support channel.
- The maintainer installs and updates the official runtime. The registry gives no checksum.
- A new engine layer, a new acceptance test, and a new fake engine for the tests. The earlier bridge had no runtime dependencies, so it needed its own JSON-RPC code.
- A new quota parser for the Markdown output of `/usage`.
- The undocumented `_meta.antigravity` options can change in a new version of the official runtime with no notice.
- The official runtime reads global files of the maintainer: `~/.gemini/config/mcp_config.json`, `~/.gemini/config/hooks.json`, and the skills of the CLI.
- With the design of section 9.8, the tool result is a user message, not a structured tool result. Only the permission title gives the tool name.
- The token usage for the API client is less exact: the official runtime gives only the context size.
- The fail-stop reads a text line of the log of the official runtime, not a JSON line. The 403 path is not seen.

acp2api: the global files are out of reach in a runtime home of acp2api (ADR-0005). The token usage is in [Which token usage does acp2api report to each API?](https://github.com/yandhi-0421/acp2api/issues/20).

## 7. A cheaper path for Windows only

This section is about the CLI engine of the earlier bridge. If a Windows port was the only reason for the change, one CLI probe on Windows could answer the question first. The earlier bridge marked a cancelled Cascade `broken`. Thus the only question was this: after a forced stop of the CLI process, does the service stop the generation? If it does, a forced stop is a sufficient cancel on Windows, and the engine does not change.

## 8. Probe plan

The maintainer approved the probe and did the sign-in. The agent ran the probe with a client of its own, outside the code of the earlier bridge. Each step started a new process of the official runtime and wrote its raw lines to `captures/2026-10-15-acp-probe/`.

Rules:

- One account. No account change. No `GEMINI_HOME`, no `--gemini_dir`, no telemetry variable.
- The client refuses each permission request. It never selects the modes `yolo` or `auto_edit`.
- The client sends no `clientInfo`.
- The workspace is a new empty directory. No parent directory contains `.agents/`. A custom agent file goes only into the workspace, never into `~/.gemini/config/`.
- No read of a credential file or the keychain. No 403 on purpose.
- The model is the lowest-cost Gemini model that the ACP session lists.

| Step | Question | Model requests |
|---|---|---|
| `caps` | Capabilities, auth methods, modes, config options, and the error with no sign-in | 0 |
| `login` | The maintainer signs in with `oauth-personal`; the ACP session lists its models (G3) | 0 |
| `tools-ask` | Which tools the model sees with the default setup (G1) | 1 |
| `locked` | Whether a workspace agent file with the locked profile has an effect (G1) | 1 |
| `sysprompt` | Whether the system text field works, if the official runtime has it | 0 or 1 |
| `command` | Whether a shell command asks for permission, and what the client sees (G1) | 1 |
| `read` | Whether a file read asks for permission or runs with no question (G1) | 1 |
| `manage-task` | Whether `manage_task` runs with no question (G1) | 1 |
| `echo` | The MCP round trip, the `tools/call` metadata, and the `tool_call` shape (step check) | 1 to 2 |
| `hold` | How long the official runtime keeps a `tools/call` open, and what it does at its limit (G2) | 1 to 2 |
| `cancel` | Time from `session/cancel` to the stop reason, and updates after the cancel | 1 to 2 |
| `usage-cmd` | Whether a quota command exists | 0 or 1 |
| `resume` | Whether an ACP session continues in a new process | 2 |
| `bad-model` | The error shape for an unknown model (G4, in part) | 0 |

Changes during the probe:

- For `echo` and `hold`, the client allowed the permission requests of the tools of its own MCP server, and refused each other request. This was the allow rule of layer 2 of the earlier bridge.
- After the static inspection, the agent ran six more steps with the tool filter of section 3.3 (section 9.2).
- After the first report, the maintainer approved two more steps with at most 4 prompts: `cancel-tool` (section 9.8) and `bad-image` (section 9.9).

Plan: approximately 11 to 16 small model requests. Actual: 19 prompts, then 4 more prompts (section 9). The `hold` step can take up to 35 minutes of wall time. With the CLI, a waiting tool call sends no model request (the MCP passthrough prototype of the CLI, section "30-minute hold"). The probe records whether the official runtime does the same.

## 9. Probe results

Date: 2026-10-15, from 10:47 to 11:07, and from 11:25 to 11:27 for sections 9.8 and 9.9 (Asia/Taipei). The official runtime `1.4.0`, protocol 1. The maintainer signed in with `oauth-personal`. The official runtime showed the tier `Google AI Pro`. Model `gemini-3.8-flash-low`, except one step with `gemini-3.8-flash-high`. The probe sent 23 prompts. The `usage_update` events show approximately 39 model requests. The `/usage` output changed from 99% to 97% remaining (Gemini Models, weekly) and from 100% to 98% (five-hour).

The raw lines are in [`captures/2026-10-15-acp-probe/`](captures/2026-10-15-acp-probe/). The probe client was a one-off script outside the repository. Each workspace was a new directory in the system temporary directory, because the home directory of the maintainer contains `~/.agents/`. The files of each step are:

- `<step>.transcript.ndjson`: each JSON-RPC line between the client and the official runtime, with the direction and the time in ms from the start of the step.
- `<step>.mcp.ndjson`: each HTTP request to the MCP server of the probe.
- `<step>.stderr.txt`: the log of the official runtime.
- `<step>.summary.json`: the measurements of the step.
- The prefix `locked-meta-` marks the steps with the tool filter `{"enabledTools": [], "eagerMcpServers": ["agy-bridge"]}` (section 9.2). The MCP server of the probe had the name `agy-bridge`.
- `echo-rejected` and `locked-meta-echo-denied` are runs in which the client refused the MCP tool call.
- The `login` step keeps only its summary, because its log contains the sign-in URL.
- `locked-meta-cancel-tool` and `locked-meta-bad-image` are the two steps of sections 9.8 and 9.9.

### 9.1 ACP session with no tool filter

- `session/new` gave the modes `default` (current), `auto_edit`, and `yolo`; the config options `model`, `thinking_level` (`low`, `medium`, `high`; current `high`), and `mode`; the models `gemini-3.8-flash-high`, `-medium`, and `-low`; and the slash commands `plan`, `logout`, and `usage`.
- A one-line prompt used 14,838 tokens of context (`usage_update.used`; `size` 1,048,576).
- The model named 18 tools: `ask_question`, `call_mcp_tool`, `define_subagent`, `generate_image`, `invoke_subagent`, `list_resources`, `manage_subagents`, `manage_task`, `read_resource`, `read_url_content`, `replace_file_content`, `run_command`, `run_workflow`, `schedule`, `search_web`, `send_message`, `view_file`, `write_to_file`.
- An agent file with the locked profile in `<workspace>/.agents/agents/` had no effect. The model did not know the codeword of the file and named the same 18 tools.
- `run_command`: the official runtime sent a permission request. The probe refused, and the command did not run. The `rawInput` gave the working directory `~/.gemini/antigravity-cli/skills`.
- `view_file`: no permission request. The model read the file. It first tried a path in `~/.gemini/antigravity-cli/skills/`. The first `tool_call` update of each read had the status `in_progress`.
- `manage_task`: the tool ran and gave `No background tasks are currently running.` The client got no permission request and no `tool_call` update.
- MCP tool: the permission request had `_meta.mcp.server` `agy-bridge` and `is_mcp_tool_call: true`. After an allow answer, the `tools/call` reached the MCP server of the probe with `_meta["antigravity.google/conversation_id"]` and the session header of `session/new`. The model got the result.

### 9.2 ACP session with the tool filter

`session/new` had `_meta.antigravity` = `{"enabledTools": [], "eagerMcpServers": ["agy-bridge"]}`.

- A one-line prompt used 4,802 tokens of context.
- The model named 3 tools: `list_resources`, `read_resource`, and `mcp_agy-bridge_probe_echo`.
- A request for `manage_task`: the model replied that the tool is not available.
- A request to read a file: the model tried `list_resources`, `probe_echo`, and `read_resource`. Each call sent a permission request. The probe refused each one, and the model did not read the file.
- A request for a shell command: the model said that it has no shell tool. It tried `probe_echo`, which sent a permission request.
- MCP round trip: with eager MCP, the permission request had only the title `Run probe_echo?` and the arguments. It had no `_meta.mcp`. After an allow answer, the `tools/call` reached the MCP server with the same `_meta` and header as in 9.1. The tool result that the model saw started with two lines, `Created At` and `Completed At`.
- `_meta.antigravity.systemPrompt.append`: the model gave the codeword of the appended text.

### 9.3 Hold time

The MCP server of the probe did not answer `tools/call`.

- The `tools/call` arrived 14.6 s after the prompt.
- 180.0 s after the call, the official runtime closed the MCP request and sent `notifications/cancelled` with the reason `context deadline exceeded`. The log said `MCP tool call to server "agy-bridge" timed out after 3m0s`.
- The `tool_call` became `failed`. Then the model sent one more model request and replied with the error text. The stop reason was `end_turn`.

### 9.4 Cancel

- The probe sent `session/cancel` after the first text chunk. The prompt turn ended with `cancelled` 10 ms later.
- Before the result, one more text chunk (`The request was cancelled by the client.`) and one `usage_update` arrived.
- The same ACP session then answered `READY` to a new prompt.

### 9.5 Resume

The probe stopped the official runtime after the first prompt and started a new process. `session/load` replayed the history. The model gave the codeword of the first prompt. User report #997 describes an earlier version.

### 9.6 Errors and quota

- An unknown model ID: `-32602` with the text `Model '<id>' is not available for the current authentication method.` and `data.availableModels`.
- No sign-in: `-32000` `Authentication required`, with `data.message` that names the settings file.
- `/usage`: Markdown text. Each group has a weekly row and a five-hour row with the percentage remaining and the reset time, to the minute. No `usage_update` came, thus no model request.
- The probe did not cause an account-level error. Section 3.3 gives the error path from the source.

### 9.7 Other observations

- The official runtime tried to read the global file `~/.gemini/config/mcp_config.json`. The file of the maintainer was empty, so the parse failed.
- The official runtime wrote `~/.gemini/antigravity-acp/settings.json` with the auth type.
- The official runtime starts a loopback proxy that adds the credentials to the requests of the harness.
- The maintainer said on 2026-10-15 that the CLI now also gives only Gemini 3.8 Flash. This probe did not run the CLI.

### 9.8 Tool call by cancel and text result

The step `cancel-tool` used the tool filter of section 9.2 and one MCP tool, `probe_lookup(key)`. The task needed two calls: the value of the key `alpha` names a second key, and the value of the second key is the answer. The probe never answered a `tools/call`. At each permission request, it sent `session/cancel` and answered the permission request with `cancelled`. Then it sent the tool result in the next prompt, in this fixed format:

```text
Result of your tool call probe_lookup {"key":"alpha"}:
next key: k-8fc970e6
```

| Prompt | Model action | Stop reason | `usage_update` |
|---|---|---|---|
| 1, the task | Asked for `probe_lookup` with `alpha` | `cancelled` after 3.6 s | 1 |
| 2, the first result | Asked for `probe_lookup` with `k-8fc970e6`, the key from the result | `cancelled` after 3.6 s | 1 |
| 3, the second result | Replied `VALUE-81e5081d`, the correct value, with no tool call | `end_turn` | 1 |

- The MCP server got no `tools/call`. Each cancel added the text `The request was cancelled by the client.` to the ACP session. The model was not confused by it in this test.
- The permission request gave the arguments in `rawInput` (`{"key": "alpha"}`). Only the title `Run probe_lookup?` gave the tool name.
- This is one run of one task with two calls. It does not show the quality for long tool results, images, or many calls.

acp2api: this is cancel at permission (ADR-0002). The macOS probe of acp2api passed the long, image, error, and five-call cases, see F6 of [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8).

### 9.9 Upstream error

The step `bad-image` used the tool filter and sent a prompt with an image block that had a PNG signature but no valid image data.

- The prompt turn ended with `end_turn`. No JSON-RPC error came.
- The reply text was only the message: `Unable to process input image. Please retry or report in https://developers.generativeai.google/guide/troubleshooting`.
- The log of the official runtime (stderr) had these lines, among others:
  - The harness: `received model response error: Error 400, Message: Unable to process input image. ..., Status: INVALID_ARGUMENT, Details: []`.
  - The Python server: `Antigravity SDK error processing prompt for session <session ID>`, then the exception `AntigravityConnectionError: Agent execution terminated due to error. ("agent executor error: Error 400, Message: ..., Status: INVALID_ARGUMENT, Details: []...")`.
- Thus the log gives the HTTP code and the canonical status in a fixed text format. The reply text does not give them.

## 10. Open questions

1. Does the design of section 9.8 keep the quality of the tool use with real client tools: long results, images, errors, and many calls in one task? The tool result becomes a user message.
2. Does a 403, a Terms of Service block, a sign-out, or a quota cap give the same log line format as section 9.9? A probe must not cause them on purpose.
3. The probe sent no `clientInfo`. The official runtime puts the client name into the `User-Agent` of its requests. Does the service treat an ACP session with no client name, or with the name of the earlier bridge, in a different way from an ACP session of an editor? No public source tells.
4. Does the official runtime send telemetry like the CLI? The earlier bridge never removed telemetry. A probe cannot see it.
5. Does a new version of the official runtime keep the `_meta.antigravity` options and the permission title `Run <tool>?`? The acceptance test must check them for each version.

acp2api, later answers:

- Question 1: the macOS probe of acp2api passed the long, image, error, and five-call cases, see F6 of [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8).
- Question 2: still open. acp2api lists the same line format for a 403, a 401, and a 429 as a critical assumption, see [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15).
- Question 3: acp2api sends the name `acp2api` with its version, see [Which `clientInfo` does acp2api send to the official runtime?](https://github.com/yandhi-0421/acp2api/issues/19).
- Question 4: still open.
- Question 5: acp2api starts only an accepted version, see [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9) and ADR-0006.
