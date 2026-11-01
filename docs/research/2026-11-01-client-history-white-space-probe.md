# Client history white space probe

Date checked: 2026-11-01 and 2026-11-02 (Asia/Taipei).

Provenance: This note gives the evidence for step 1 of [Preserve significant white space when matching an ACP session](https://github.com/yandhi-0421/acp2api/issues/173). It changes no product code. Step 2 is the decision of the owner. The method and the safety rules come from `docs/research/2026-10-18-probe-methods-and-safety-rules.md`.

Question: Does an API client send earlier assistant text, tool call arguments, and tool results back byte-identical in its next request?

Evidence levels in this note:

- **Observed**: a real client ran against a fake endpoint, and a script compared the decoded values.
- **Source**: a line in the source or the bundle of the client.
- **Inferred**: a conclusion from the observed values. acp2api did not run. A Python copy of its match rule gave these results.

## 1. Result

Each cell gives the observed result. "Stable" means that the first request form stayed unchanged in each later request.

| Client | Assistant text | Tool call arguments | Tool results |
| --- | --- | --- | --- |
| Claude Code 2.1.309 | Byte-identical, with one exception. A reply of only white space is removed. | Changed for built-in tools. Unchanged string values for the MCP tool, but keys and numbers can change. Section 4.2 gives each change. | Stable. The client changes the first form, and it can add text. |
| Codex CLI 0.163.0 | Byte-identical in each case. | Byte-identical in each case, also JSON white space, key order, and number forms. | Stable. The MCP text is byte-identical, after a header part of the client. |
| SillyTavern 1.18.0 | Changed in 20 of 24 text values. It trims the text, removes white space at each line end, and removes each CR. | Byte-identical in each case. | Stable. The text is byte-identical. An empty result becomes `[No content]`. |

Main conclusions:

- **Observed.** The collapse of white space gave no extra match for Claude Code or Codex. The current rule and an exact comparison gave the same result on each request pair.
- **Observed.** For SillyTavern, the current rule hides only changes inside the text. It does not hide a removed final newline, because that item ends in a space on one side only.
- **Inferred.** Under the current rule, each `Edit` call from Claude Code stops the match. An exact comparison does not change this. The cause is a rewrite by the client, not only white space.
- **Inferred.** The narrow option of the issue gives SillyTavern no gain over the current rule. Section 6 gives a different narrow rule.

## 2. Tested clients and setup

| Item | Value |
| --- | --- |
| Claude Code | 2.1.309. Modes: print with `stream-json` input in one process, print with `--resume` in a new process for each turn, and interactive through a pty. |
| Codex CLI | 0.163.0. Modes: `codex exec` with `exec resume` in a new process for each turn, and the interactive mode through a pty. |
| Codex model labels | `acp2api-probe-model` is an unknown name, so Codex uses its fallback metadata. acp2api model names take this path. `gpt-5.5` is a label only, and it adds the freeform `apply_patch` tool. |
| SillyTavern | 1.18.0, commit `8172dcd0e`, source "Custom (OpenAI-compatible)", in stream and non-stream modes. Each `power_user` value stayed at its default. The client ran as a real page in headless Chrome 156.0.8078.76, driven over CDP from Node.js 22.23.1. |
| Platform | macOS 27.2 on arm64. |
| APIs | Anthropic Messages, OpenAI Responses, and OpenAI Chat Completions, one for each client in this table order. |

The fake endpoint copies the event shapes of `crates/acp2api/src/dialect/messages.rs`, `responses.rs`, and `chat.rs`. A local MCP server gave each client one neutral tool, `probe_echo`. SillyTavern used one function tool that the driver registered.

Each client ran under `sandbox-exec` and `env -i`, with a new `HOME` and a new configuration folder. The sandbox profile also denies reads of `~/.codex`, `~/.claude`, `~/.claude.json`, and `~/Library/Keychains`. It also denies reads of the data folder and the `config.yaml` of the local SillyTavern. SillyTavern ran from a copy with no data, no secrets, and no third-party extensions.

## 3. Cases

`levers/cases.py` holds each value. Each value is made up.

| Case | Content |
| --- | --- |
| A01 | Python block with indentation, tab lines, spaces in a string, trailing spaces, and a final newline |
| A02, A03 | Runs of spaces; tabs |
| A04, A05 | Leading spaces; leading newlines |
| A06, A07, A08, A09 | Trailing spaces; one final newline; several final newlines; a trailing mix |
| A10, A11, A12 | CRLF inside; final CRLF; lone CR |
| A13 | No-break space, ideographic space, thin space, and zero-width space |
| A14 | Markdown hard break and a table |
| A15 | Text deltas that split inside white space, and deltas of only white space |
| A16 | Two text blocks in one reply. acp2api emits one block, so this case is extra. |
| A17 | A reply of only white space |
| A18, A19, A20 | Text after tool calls; white space at line ends only; a plain text turn |
| B01, B02, B04, B06 | Text before a tool call, with leading or trailing white space |
| T01 to T08 | `probe_echo` calls: white space inside strings, JSON white space, another key order, number forms, integer-like keys, escapes, Python code, and strings of only white space |
| R01 to R08 | Tool results: code, outer spaces, final newlines, CRLF, only white space, empty, two blocks, and an error |
| D01 to D05, E01 to E09 | Claude Code built-in tools: `Bash`, `Write`, `Edit`, `Read`, and `TaskCreate`. The D cases use a different key order, and the E cases use schema order. |
| D02, D07 (Codex) | `exec_command`; freeform `apply_patch` |

Each turn ends at a reply with no tool call. The driver then sends the next prompt. Each prompt marker `Pnn` can have leading and trailing spaces. Thus each emitted value has between 1 and 42 later requests.

The compare step reads each value after protocol parsing. For tool arguments, it reports these points separately:

- the raw bytes;
- the key order;
- each changed number;
- each changed string value.

## 4. Observed transformations

### 4.1 Assistant text

**Claude Code.** Each mode gave the same result. Each text in A01 to A16, A18, A19, and B01 to B06 was byte-identical in each later request. This includes CRLF, a lone CR, and each kind of final white space. Both blocks of A16 stayed separate.

**Claude Code, A17.** The client removes the reply of only white space. It then appends a newline to the prompt text and adds the user text `[Your previous response had no visible output. Please continue and produce a user-visible response.]`. It sends a new request in the same turn. Thus the earlier user item also changes. Print and interactive modes did the same.

**Codex.** Each text was byte-identical in each later request, also A17. In `gpt-5.5` and in the unknown model path, the result was the same.

**SillyTavern.** Stream and non-stream modes gave the same result:

| Case | Observed transformation | Exact match | Current rule |
| --- | --- | --- | --- |
| A02, A03, A13, A20 | None | Yes | Yes |
| A01 | Trailing spaces removed from one line, and the final newline removed | No | No |
| A04, A05, B06 | Leading white space removed | No | No |
| A06 to A09, A11, A16, A18, B01, B04 | Trailing white space removed | No | No |
| A10 | CRLF changed to LF | No | Yes |
| A12 | Each CR removed, also inside a line: `lone\rcarriage` became `lonecarriage` | No | No |
| A14 | Trailing spaces removed from a line, and the final newline removed | No | No |
| A15 | Trailing spaces removed from a line, and the final newlines removed | No | No |
| A19 | Trailing spaces and tabs removed from each line | No | Yes |
| B02 | Leading and trailing white space removed | No | No |
| A17 | The reply is absent from later requests | No | No |

Source: `public/script.js:6431` removes the white space at each line end. `public/script.js:6529` trims the text, because `power_user.trim_spaces` is true in `default/content/settings.json`. `public/scripts/openai.js:606` removes each `\r` when it builds the messages. Indentation and runs of spaces inside a line stayed unchanged.

### 4.2 Tool call arguments

**Codex.** The value of `arguments` and the value of `input` were byte-identical in each later request. This includes T02 with JSON white space, and T03 with `1.0`, `1e2`, and integer-like keys.

**SillyTavern.** The value of `arguments` was byte-identical in each later request, T01 to T08.

**Claude Code.** Each mode gave the same result. The later request holds a JSON object, so the raw bytes of the emitted arguments cannot survive. A difference of JSON white space alone is therefore a serialization difference with no effect on the match (T02).

| Case | Tool | Key order | Change inside values | Exact match | Current rule |
| --- | --- | --- | --- | --- | --- |
| T01, T02, T04 to T08 | `probe_echo` | Same | None | Yes | Yes |
| T03 | `probe_echo` | Integer-like keys `2` and `10` move first | `1.0` becomes `1`; `1e2` becomes `100`; `9007199254740993` becomes `9007199254740992` | No | No |
| D01, D03 | `Bash`, `Write` | Moved to schema order | None. `.md` content keeps its trailing white space. | No | No |
| D02, E01 | `Write` to `.py` | Moved in D02; same in E01 | White space removed at each line end of `content`. Line breaks stay, also CRLF and final newlines. | No | No |
| E02 | `Write` | Same | None, because the content has no such white space | Yes | Yes |
| D04, E03 | `Edit` | `replace_all` moves first | White space removed at each line end of `new_string` | No | No |
| E04 | `Edit` with clean strings | `replace_all` moves first | `replace_all: false` is added | No | No |
| E06 | `Bash` | Same | A leading `cd <cwd> && ` is removed from `command` | No | No |
| E05, E07, E08, E09, D05 | `Bash`, `Read`, `TaskCreate` | Same | None. `E05` keeps the spaces around its command. | Yes | Yes |

Source: `normalizeToolInput` in the bundle of Claude Code 2.1.309 holds these rules. The function that removes white space splits the text on `(\r\n|\n|\r)` and applies `replace(/\s+$/, "")` to each line. The test `/\.(md|mdx)$/i` exempts Markdown files. The `Edit` branch returns `{replace_all, file_path, old_string, new_string}`, which sets the key order. The same function also has a `Bash` branch and a `Read` branch.

### 4.3 Tool results

The client writes a tool result, so a record first holds the client form. Thus the first form does not stop a match. Only the stability between requests matters. In each run of each client, each tool result was stable.

| Value | Claude Code first form | Codex first form | SillyTavern first form |
| --- | --- | --- | --- |
| R01, R02, R03, R04 (text) | One `\n` is added at the end when a reminder block follows | A header part such as `Wall time: 0.0035 seconds\nOutput:` comes first. The text part is byte-identical. | Byte-identical |
| R05 (only white space) | Replaced by `(mcp__probe__probe_echo completed with no output)` | Header, then byte-identical text | Byte-identical |
| R06 (empty) | Replaced by the same text | Header only | `[No content]` |
| R07 (two blocks) | Two parts, with `\n` added to the second | Header, then two byte-identical parts | The driver joined the two blocks, so this case is not a test |
| R08 (error) | Trimmed, then a reminder follows. `is_error` is true. | Header, then byte-identical text. Responses has no error flag. | Byte-identical. Chat Completions has no error flag. |
| `Bash` or `exec_command` output | Trimmed on both sides, and a reminder follows | Unchanged inside a header with a chunk ID and a time | Not tested |

Claude Code 2.1.309 puts its `<system-reminder>` text inside the tool result or the user message. It sent no separate `role: "system"` message in these runs. ADR-0003 records that message for version 2.1.299.

### 4.4 Other client-written items

- **User text, system prompt, and tool schemas.** Exact hashes of these items were stable on each request pair, with two exceptions.
- **Exception 1.** Claude Code changes the prompt before A17, as section 4.1 shows.
- **Exception 2.** SillyTavern sent the example dialogue as one more system text in the first main request only. This changed the system prompt between requests 1 and 2. The cause was not found. `power_user.pin_examples` is false by default, but that setting alone does not explain the change.
- **Billing block.** The first system block of Claude Code held one value over the 43 requests of a run. The port removes it, as `crates/acp2api/src/dialect/content.rs:42-48` does.

## 5. Comparison with the current hash rule

The text hash is in `crates/acp2api/src/conversation.rs:301-316`. It changes each run of white space to one space, and it does not trim. Its callers are `item_hash`, `tool_call_hash`, and `tool_set_hash` in the same file. Each observed transformation has this effect:

| Observed transformation | Client | Would an exact comparison fail? | Does the current rule hide it? |
| --- | --- | --- | --- |
| Removed leading or trailing white space | SillyTavern | Yes | No. `A\n` becomes `A ` on one side, and `A` on the other side. |
| White space removed at line ends, not at the text ends | SillyTavern | Yes | Yes |
| CRLF changed to LF, not at the text ends | SillyTavern | Yes | Yes |
| CR removed inside a line | SillyTavern | Yes | No. A CR counts as a space. |
| A reply of only white space removed | Claude Code, SillyTavern | Yes | No |
| White space removed at line ends in `Write` or `Edit` arguments | Claude Code | Yes | No |
| Changed key order, added `replace_all`, changed numbers, removed `cd` prefix | Claude Code | Yes | No |
| JSON white space between tokens | Claude Code | No. The Messages API has an object, and acp2api emits compact JSON. | Does not apply |
| First form of a tool result | All three | No. The record holds that form. | Does not apply |

The rule cannot help tool arguments. The hash runs on the JSON text, and a newline in a string is the two characters `\n` in that text. Only literal spaces and unescaped non-ASCII white space can collapse. For example, `x = 1   \n` and `x = 1\n` stay different after the normalization. E01 shows this.

`records.rs:78-90` adds the last reply to the record. `conductor/request.rs:387-390` hashes the tool call in the form of the runtime. `runtime_driver.rs:1042-1045` builds that form with `input.to_string()`. The later Messages form is `block["input"].to_string()` in `dialect/content.rs:249`, and `Cargo.toml:14` enables `preserve_order`. Thus key order counts.

The table gives the simulation on the same requests. A pair is one main request and the next main request. A pair continues when the record is the start of the later request and the continuation rule of ADR-0003 holds. The cases are hard on purpose, so **these counts are not rates of real use**.

| Rule | Claude Code (each mode) | Codex `exec`, unknown model | Codex TUI, unknown model | Codex `exec`, `gpt-5.5` | SillyTavern (each stream mode) |
| --- | --- | --- | --- | --- | --- |
| Current rule | 32 of 42 | 28 of 28 | 28 of 28 | 21 of 21 | 10 of 28 |
| Exact for all | 32 of 42 | 28 of 28 | 28 of 28 | 21 of 21 | 8 of 28 |
| Issue option: exact, with the current normalization for the last reply text | 32 of 42 | 28 of 28 | 28 of 28 | 21 of 21 | 10 of 28 |
| Exact, with the clean form for the last reply text | 32 of 42 | 28 of 28 | 28 of 28 | 21 of 21 | 26 of 28 |
| Exact, with the clean form, and call ID and name for the last tool call | 41 of 42 | 28 of 28 | 28 of 28 | 21 of 21 | 26 of 28 |

The clean form removes each CR, the white space at each line end, and the white space at both text ends. `levers/port.py` holds it. It keeps indentation and runs of spaces inside a line.

The pairs that stay broken with the last rule:

- Claude Code: A17.
- SillyTavern: A17, and the pair after A01, where the system prompt changed.

## 6. Recommendation for step 2

These are recommendations, and the owner decides. Each one names its evidence.

1. **Remove the collapse of white space for each item.** Claude Code and Codex lost nothing in the simulation. SillyTavern lost 2 of 28 pairs, and rule 2 restores them. The collapse is the risk that the issue names, because it hides a change of Python indentation.
2. **Add one compatibility rule, only for the last reply text of a record.**
   - Compare the clean form of that text with the clean form of the later assistant text.
   - Three observed SillyTavern changes justify it: the CR removal, the removal at line ends, and the trim.
   - Each later comparison uses the exact form, because the record then holds the client form. All three clients kept that form stable.
   - This rule reads only text, so it has no test for one client. Claude Code and Codex stay unaffected, because their text is exact. The maintainer has a rule that Chat Completions gets no rule for SillyTavern. Thus the owner must confirm that this text rule is acceptable.
   - The cost is that an edit by the client of only this white space goes unseen in the last reply. Indentation and spaces inside a line stay significant.
3. **Do not take the issue option in its written form.** In the simulation, it gives SillyTavern the same 10 of 28 as the current rule. A final newline still stops the match, as the issue already noted. The part of the option that keeps user text, tool arguments, tool results, schemas, and the system prompt exact is safe. Each of these stayed stable between requests, apart from the two exceptions in section 4.4.
4. **Decide the tool call of Claude Code as a separate point.**
   - This is not a white space problem, and the current rule has it now.
   - For the last tool call of a record, compare the call ID and the name. acp2api makes the call ID with 16 random bytes (`dialect/mod.rs:287-290`).
   - In the simulation, this takes Claude Code from 32 to 41 of 42 pairs.
   - Sorted JSON keys are not enough. They repair only D01 and D03. They do not repair the added `replace_all`, the removed white space, the numbers, or the removed `cd` prefix.
   - The cost is that a client rewrite of the last call goes unseen. The ACP session then keeps the original arguments, and the model does not see the rewrite.
   - Each later comparison uses the exact client form. That form was stable in each run.
5. **Add no rule for A17 or for the first system prompt change of SillyTavern.** Each one costs one rebuild. A reply of only white space should be rare (not measured).

The cost of a miss is one rebuild. ADR-0003 records `session/new` at 2.4 to 8.5 s on macOS and 4.4 to 29.3 s in the Windows VM, and a rebuild loses the thought signatures.

If the owner accepts a change, these documents need an update: ADR-0003, "Record", and test idea MR-14 in `docs/research/2026-10-18-test-ideas-from-the-earlier-cli-bridge.md`. A search found no white space rule for the match in `docs/prd/acp2api.md` or in `features/acp2api/`.

## 7. Coverage gaps

- **acp2api did not run.** The rule results come from `levers/port.py`, a Python copy of the Rust rules. Nobody compared this copy with the Rust code in a run. Each effect on the product is inferred.
- **Real replies.** The fake sent synthetic replies. Nobody measured how often a real model reply starts or ends with white space. This rate decides the value of rule 2.
- **Runtime key order.** The earlier captures show `rawInput` only for tools of the runtime, and those keys look sorted, for example `CommandLine`, `Cwd`, `WaitMsBeforeAsync`. No capture shows a client tool with several keys. If the order is sorted, each `Write` call also changes order (D03). `Edit` changes in each order that this probe tried.
- **Claude Code.** No test covered `Agent`, `NotebookEdit`, `Bash` with `timeout`, compaction, large results that go to a file, `/rewind`, a resume in interactive mode, or a `thinking` block. The bundle shows more rewrites for `Read.offset` and for `\;` in `Bash`, and no run tested them. `ENABLE_TOOL_SEARCH=false` was set.
- **Codex.** No test covered compaction, a resume in interactive mode, or `reasoning` items. The `gpt-6` and later labels use `additional_tools` in the request and no `tools` field, and this probe did not use that form. With `gpt-5.5`, the MCP tool sat behind `tool_search`, so that run left out T01 to T08.
- **SillyTavern.** The run used one character, default settings, and the custom source. No test covered a group chat, regex scripts, swipes, continue, a message edit, `trim_sentences`, `collapse_newlines`, `auto_fix_generated_markdown`, or the Claude source. Its `Generate` path allows 5 tool rounds in one turn (`RECURSE_LIMIT`, `public/scripts/tool-calling.js:255`), so the script has a text turn after T04.
- **Multi-block tool result in SillyTavern.** The tool action returns one string, so R07 is not a test for this client.
- **Platform.** Only macOS. No client ran on Windows.

## 8. Limits and redaction

| Limit | Value |
| --- | --- |
| Requests for each fake endpoint | 120, then HTTP 400 |
| Time for each run | 300 s. The fake stops 20 s later. |
| Log size for each run | 40,000,000 bytes, then the log stops |
| Body size for each request | 4,000,000 bytes |
| Time for each turn | 45 s |
| MCP server | 200 messages, 300 s, no log file |
| Proxy | 2,000 log lines, host names only |

No run reached a limit. The largest raw log was 6,390,758 bytes, and the largest request count was 63. The second round gave the same values for each case.

No request reached a real model:

- The proxy refused each connection attempt. `refusedByProxy` in `results.json` gives the counts for each run.
- A check under the same `deny network-outbound` rule failed: `curl` to `1.1.1.1` ended with exit code 7.
- The clients used a made-up token and a fake URL.

`extract.py` keeps text only for synthetic case items. System prompts, tool schemas, context items, and the character text stay as hashes and lengths. It replaces the run folder, the home path, the name of the user, and each e-mail form, also `%40`.

A scan of the captures for the home path, the user name, `gmail`, `%40`, an e-mail address, a token prefix, `Authorization`, and `x-api-key` found one hit. That hit is the redaction pattern in `levers/extract.py`.

The captures are in `docs/research/captures/2026-11-01-history-white-space/`. The size of that folder is 568 KiB (`du -sh`):

- one JSON file for each of 8 runs;
- `results.json`;
- the levers in `levers/`.

The raw logs stayed outside the repository.

## 9. Reproduce

Requirements: macOS, Python 3 at `/opt/homebrew/bin/python3`, Node.js 22, Claude Code, Codex CLI, Google Chrome, and a local SillyTavern with `node_modules`. No package install is necessary. `PROBE_CLAUDE`, `PROBE_CODEX`, `PROBE_NODE`, `PROBE_ST`, `PROBE_ST_APP`, and `PROBE_ROOT` change the paths.

To run all clients, and to write new captures, use this command:

```sh
zsh docs/research/captures/2026-11-01-history-white-space/levers/run-all.sh "$PWD/capture-output"
```

To run the comparison again from the committed captures, use this command:

```sh
python3 -B docs/research/captures/2026-11-01-history-white-space/levers/compare.py docs/research/captures/2026-11-01-history-white-space
```

To run one client mode, use a command of this form:

```sh
python3 -B docs/research/captures/2026-11-01-history-white-space/levers/run.py --client claude --mode tui --name cc-tui --port 18805 --max-seconds 300 --max-requests 120 --max-log-bytes 40000000
```

`run.py` lists each mode. The fake model name and the tool steps of a run are in `levers/run-all.sh`.

## 10. Done-when lines of the issue

| Line | State |
| --- | --- |
| The research note names the tested client versions, cases, results, and coverage gaps. | Met: sections 2, 3, 4, and 7. |
| Each result compares the emitted content with the decoded content of a later request. | Met: section 3 and `results.json`. |
| The note distinguishes serialization differences from changes to semantic text. | Met: section 4.2. |
| Probe runs obey the request, duration, and log caps and the redaction rules. | Met: section 8. |
| The owner decides the match rule, with a reason tied to the evidence. | Open. This is step 2. |
| Each accepted compatibility rule names an observed transformation and its exact scope. | Open. Section 6 gives candidates. |
| Any accepted design change has the required document updates and an explicit implementation scope. | Open. This is step 2. |
