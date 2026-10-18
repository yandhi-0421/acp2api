# Continue an ACP session only when a request continues its history

Each API request carries the full conversation, but an ACP session keeps its own history. acp2api keeps a record of the history of each ACP session. When a request continues that history, acp2api sends only the new items to that ACP session. Else, acp2api does a rebuild.

## Rules

- **Record.** The record of an ACP session holds a hash of each item that the ACP session holds, in the form that the API client sees.
  - The items are the system prompt, each text, each image, each tool call with its name and its arguments, and each tool result.
  - The system prompt is the first item. Thus a change of the system prompt stops the match.
  - A field that acp2api ignores, for example `cache_control`, is not part of a hash. Each run of white space counts as one space.
  - The record also holds a hash of the set of client tools: the names, the descriptions, and the schemas.
- **Match.** A request continues an ACP session only when all these conditions are true:
  - The record is the start of the conversation of the request.
  - After a prompt turn that ended with `end_turn`, the rest of the conversation is one or more user messages.
  - After a client tool call, the rest starts with the tool result of that call. Zero or more user messages follow it.
  - The request has the same set of client tools.
  - The ACP session has no prompt turn that runs now.
- **Choice.** When several ACP sessions match, acp2api uses the ACP session with the longest record. For records of the same length, it uses the ACP session that it used last.
- **Session hints.** acp2api does not use the session hints of the API clients, for example `x-claude-code-session-id` and `prompt_cache_key`.
- **End of a prompt turn.** An ACP session can continue only when the API client got the full text of the last reply.
  - These ACP sessions can continue: after `end_turn`, after cancel at permission, and after a prompt turn in which acp2api answered `reject_once`.
  - These ACP sessions never continue: after a cut at a stop sequence, after a disconnect of the API client, after an error, and after the stop reason `refusal`.
- **No wait.** When the only match has a prompt turn that runs now, the request does a rebuild. It does not wait for that ACP session.
- **Model.** A change of the model or of the `thinking_level` keeps the ACP session. acp2api sends `session/set_config_option` before the prompt.
- **Records in memory.** acp2api keeps the records only in memory. After a restart of acp2api, the next request of each conversation does a rebuild.
- **Stopped process.** When the process of an ACP session stops, acp2api keeps the record. The next match loads the ACP session with `session/load` in another process.
- **Rebuild.** The first prompt of a rebuild holds one fixed neutral line and the earlier part of the conversation as text with role labels.
  - Each image goes as an ACP `image` block at its place in the text.
  - [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23) decides the role labels.
  - [How does a client tool call pause and continue a prompt turn?](https://github.com/yandhi-0421/acp2api/issues/11) decides the text form of a tool result.
- **No notice.** acp2api tells the API client nothing about a rebuild. It writes a log line.

## Reasons

- With cancel at permission, Claude Code and Codex send one request for each tool call. A new ACP session for each request adds one `session/new` to each tool call. That call took 2.4 to 8.5 s on macOS, and 4.4 to 29.3 s in the Windows VM.
- In a continued ACP session, the model sees its earlier replies and tool calls as its own turns. After a rebuild, the model sees them only as user text.
- The official runtime keeps thought signatures in the history of an ACP session. A rebuild cannot give them to the new ACP session.
- ACP gives no other way to put an earlier conversation into a new ACP session. A prompt holds only user content blocks, and the official runtime 1.4.0 has no `fork`.
- Each ACP session has its own `localharness_external` process, and the official runtime keeps one SQLite file for each ACP session. Fewer ACP sessions give fewer processes and files.
- The match uses only the content of the conversation. Thus it works for Claude Code, Codex, and SillyTavern, and it finds each edit of an earlier message. SillyTavern sends no session hint.
- Two ACP sessions with the same record hold the same conversation. Thus either one gives the same result.
- After a cut at a stop sequence, the ACP session holds text that the API client never got. An example is a line in the voice of the user. The input tokens must show the conversation of the request ([Which token usage does acp2api report to each API?](https://github.com/yandhi-0421/acp2api/issues/20), decision 6).
- A wait for an ACP session gives no match. When its prompt turn ends, the ACP session holds one more reply.
- The official runtime reads the tools of an MCP server at the first prompt turn of an ACP session (ADR-0002). In the captures of Claude Code 2.1.299, each main request had 29 client tools. Each other request to `/v1/messages` had one message, and no tool or only the server tool `web_search`. Thus it was a separate conversation.
- `session/set_config_option` took 2 to 4 ms, and the official runtime changes the model at the next prompt turn.
- `session/load` in a new process continued an ACP session with the MCP server at a new URL.

## Considered options

- **A new ACP session for each request.** acp2api keeps no state. But each tool call then needs a `session/new` and a rebuild, and the model reads the full conversation again as user text.
- **Session hints as the key.** SillyTavern sends no hint, and a hint does not show an edit of an earlier message.
- **`session/load` with a new MCP server, or `notifications/tools/list_changed`, for a new set of client tools.** No probe gave a different set of client tools to an ACP session. A later probe can make this path cheaper than a rebuild.
- **No match change for a new system prompt, as in agy-bridge.** The model then sees an old system prompt. agy-bridge kept it only because its agent file could not change.
- **Records in a file.** A file format is stored data, and each change of it needs a migration. A restart of acp2api is rare, and the gain is one rebuild for each conversation.
- **A replay of each earlier user message as its own prompt.** The model answers each message again. The replies then differ from the conversation, and each one costs one model request.
- **The earlier conversation in `_meta.antigravity.systemPrompt.append`.** That option is for instructions, and nobody knows its size limit. [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23) decides its use.
- **Images as files with a path line, as in agy-bridge.** An ACP `image` block went through to the model. agy-bridge used files only because its CLI took no image input.

## Consequences

- Each rebuild and each new conversation adds one ACP session. Thus the number of `localharness_external` processes and conversation files of the official runtime grows. [How does acp2api start, keep, and stop the processes of the official runtime?](https://github.com/yandhi-0421/acp2api/issues/30) decides the limit of the processes. A new decision issue decides what acp2api does with the conversation files.
- In SillyTavern, each regenerated reply, each edit, each cut at a stop sequence, and each change of the system prompt causes a rebuild. A macro such as `{{time}}` or a world info entry can change the system prompt.
- The records hold no text. Thus acp2api cannot do a rebuild from `previous_response_id`, `item_reference`, or `conversation`, and it keeps their rejection.

Evidence: the grilling of [Which ACP session does acp2api use for each API request?](https://github.com/yandhi-0421/acp2api/issues/10); F3, F6, F8, F9, and F12 of [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8); W3 of [Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16); `docs/research/captures/2026-10-17-claude-code-check/runs.json`, runs `p7-tui` and `p10-tui-search`; and the agy-bridge note, section 3.3 ([link](https://github.com/yandhi-0421/agy-bridge/blob/cc55e4bcd8874357eaa1efbc6bd50c040d888fc2/docs/research/2026-10-15-antigravity-acp-server-engine.md)).
