# Probe P of the official runtime on macOS: tool loop, load, tool set change, and error order

Date checked: 2026-10-15 (Asia/Taipei).

Provenance: An agent of the maintainer ran this probe on 2026-10-15 for an earlier project, a bridge that used the Antigravity CLI (`agy`) as its engine. That project planned a change of its engine to the official runtime, and probe P was its last check before the build. The results were never published in that project. They stayed in a local folder outside its repository. On 2026-10-18 they moved to acp2api, because that project will be deleted. This note restates the result file of the probe, and it adds the step questions of the plan and some numbers from the captures. The captures are in [`captures/2026-10-15-probe-p/`](captures/2026-10-15-probe-p/). The move replaced the home path in them with `~`.

Words of the earlier project:

- The earlier bridge: the earlier CLI bridge of the maintainer. It translated the three APIs of acp2api to the Antigravity CLI.
- The maintainer: the person who ran the earlier bridge and who maintains acp2api.
- Gate: a check that had to pass before the earlier bridge built its change of engine.

Related: [the probe of 2026-10-15](2026-10-15-official-runtime-acp-probe.md). Probe P used the same probe client, the same account, and the same rules (that note, section 8). It ran later on the same day.

## 1. Setup

- The official runtime 1.4.0 on macOS arm64, with the sign-in `oauth-personal` of the maintainer. The tier in the captures is `g1-pro-tier`.
- Each step started a new process of the official runtime. Each workspace was a new empty folder in the system temporary folder.
- Each ACP session had the tool filter `_meta.antigravity.enabledTools: []` and `eagerMcpServers` with the MCP server of the probe.
- The probe answered each permission request of an MCP tool with a cancel at permission: `session/cancel`, then `cancelled` for the permission request. It gave the tool result as text in the next prompt, as in section 9.8 of the probe note of 2026-10-15.
- The model was `gemini-3.8-flash-low`, except one prompt with `gemini-3.8-flash-medium` (step `load-model`).
- The maintainer approved at most 16 prompts. The probe sent 14 prompts, in 6 new ACP sessions and 7 processes of the official runtime.

## 2. Steps and results

| Step | Question of the plan | Prompts | Result |
|---|---|---:|---|
| `config-model` | Does `session/set_config_option` with `model` and `thinking_level` select `gemini-3.8-flash-low`? | 0 | Yes. The official runtime confirmed the model `gemini-3.8-flash` and `thinking_level` `low`. |
| `tool-task` | A tool loop with cancel at permission: 6 or more calls, one result of about 20,000 characters, one error result, and one valid PNG image result. Does the model finish the task with the correct answer? | 7 | Pass (gate). Section 3. |
| `load-model` | An ACP session with a codeword, then a new process. `session/load` with the full setup, a change to `gemini-3.8-flash-medium`, and one prompt that asks for the codeword and for a read of a file in the workspace. Does the model keep the context? Does the tool filter still hold after the load? | 2 | Pass (gate). Section 4. |
| `esc-continue` | A cancel during a long text reply, then a new user message in the same ACP session. Does the model answer the new message correctly? | 2 | Yes. The cancel completed in 6 ms, and the same ACP session answered `READY`. |
| `tool-set-change` | A second tool appears after the first prompt. Does the model see and call it after `notifications/tools/list_changed`? | 2 | Yes. Section 5. |
| `error-order` | The invalid image of section 9.9 of the probe note of 2026-10-15. Which comes first: the error line on stderr, or the result of `session/prompt`? | 1 | The stderr line came first, in this run. Section 6. |
| `session-files` | The names and the sizes of the files in `~/.gemini/antigravity-acp/conversations/`, with no read of their content. | 0 | 58 files. Section 7. |

No account signal, no quota signal, no unexpected sign-out, and no run of a built-in tool occurred. The only upstream error was the 400 of the invalid image, on purpose. Each process of the official runtime exited with code 0 after the probe closed its stdin. A final check found no process of the official runtime and no `localharness_external` process.

## 3. Step `tool-task`

- The model called the tools in the expected order: `alpha`, `ledger-2026`, `returns-primary`, `returns-backup`, `label-image`, `audit-close`. Each value came from an earlier result.
- The results included a ledger of 21,242 characters, a failed result with an exact error envelope, and a valid PNG image.
- The final answer was `AUDIT-72-RED`, the correct value.
- The MCP server of the probe got no `tools/call`, and no run of a built-in tool was seen.
- Limit: a built-in tool that sends no update and no permission request stays invisible to this check. The tool filter is the protection.

acp2api: the macOS probe of acp2api later passed a similar set of cases, see F6 of [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8).

## 4. Step `load-model`

- The step used two different processes of the official runtime.
- The `session/load` gave the full absolute workspace path, the MCP configuration, the empty tool filter, `eagerMcpServers`, and the same setup text with no tools.
- After the load and the change to `gemini-3.8-flash-medium`, the final reply was `5fd0b692|BLOCKED`. The model kept the codeword of the first prompt, and it did not read the file.
- No permission request and no `tools/call` came. The updates of the history replay came before the result of `session/load`, and the probe kept them apart from the live updates.
- The new ACP session and the loaded ACP session both stayed in the mode `default`.

## 5. Step `tool-set-change`

- The first prompt had one tool, `probe_first`. The official runtime read `tools/list` once, and the model called `probe_first` (permission title `Run probe_first?`).
- Then the MCP server of the probe added `probe_checksum` with a required integer `batch`, and it sent `notifications/tools/list_changed` to its one SSE listener.
- The official runtime read `tools/list` a second time. The second response held the schema of `probe_checksum`.
- At the second prompt, the next permission request was `Run probe_checksum?` with `{"batch": 19}`.
- Both calls ended with a cancel at permission, and no `tools/call` came.
- The notification passed, so the probe did not try the other two ways of the plan: a `session/load` in the same process, and a text that names the new tool.
- The times of the two reads of `tools/list` and both schemas are in `checkpoint-3.sanitized.json`, key `steps.tool-set-change`.

acp2api: ADR-0002 says that no probe saw a second read of `tools/list`, and ADR-0003 does a rebuild for a new set of client tools. This step is one run on macOS that shows a second read after `notifications/tools/list_changed`.

## 6. Step `error-order`

- The prompt with the invalid PNG ended with `end_turn`. The reply text was only the message, as in section 9.9 of the probe note of 2026-10-15.
- The client got the complete stderr line of the upstream 400 at 7,337 ms (event sequence 30).
- The client got the result of `session/prompt` at 7,351 ms (event sequence 50). Thus the stderr line came 14 ms before the result.
- Limit: this is the order of receipt in one run. It does not prove the order in which the official runtime writes, and it does not prove that each later error comes before its result.

## 7. Step `session-files`

- The folder `~/.gemini/antigravity-acp/conversations/` held 58 files: 29 `.db` files and 29 `.meta` files.
- Each `.db` file had 393,216 to 720,896 bytes, and the 29 files had 13,500,416 bytes together. Each `.meta` file had 93 to 150 bytes.
- The step opened no file and no SQLite database, and it followed no symbolic link. The names and the sizes are in `checkpoint-3.sanitized.json`, key `steps.session-files`.

acp2api: the session files and their removal are in ADR-0005. ADR-0003 says that nobody measured the size of a session file.

## 8. Files

- `checkpoint-1.sanitized.json`: `config-model` and `tool-task`.
- `checkpoint-2.sanitized.json`: the new-process load, the context, the models, and the tool filter.
- `checkpoint-3.sanitized.json`: the counts of each step, the reads of `tools/list`, the error order, the names and sizes of the session files, and the cleanup.
- `<step>/summary.json`, `<step>/transcript.ndjson`, `<step>/mcp.ndjson`, and `<step>/stderr.txt`: the measurements, the JSON-RPC lines, the MCP requests, and the log of each step. `load-model/stderr-2.txt` is the log of its second process.
- `<step>.cli.txt` and `<step>.exit.json`: the command line and the exit code of each step. `load-model.preflight.json`: the check before the load.
