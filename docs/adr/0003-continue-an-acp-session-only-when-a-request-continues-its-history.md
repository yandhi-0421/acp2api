# Continue an ACP session only when a request continues its history

Each API request carries the full conversation, but an ACP session keeps its own history. acp2api keeps a record of the history of each ACP session. When a request continues that history, acp2api sends only the new items to that ACP session. Else, acp2api does a rebuild.

## Rules

- **Record.** A record holds the exact hashes of its request items and the comparison data of its last reply.
  - The items are the system prompt, each text, each image, each tool call, and each tool result. A tool call has its call ID, its name, and its arguments.
  - The system prompt is the first item. Thus a change of the system prompt stops the match.
  - A field or a block that acp2api ignores is not part of a hash. Examples are `cache_control`, a `thinking` block, a `reasoning` item, and a `tool_addition` block.
  - Each hash reads the exact text, also its white space.
  - The record also holds a hash of the set of client tools: the names, the descriptions, and the schemas.
  - The last reply holds a hash of the clean form of its text, when present. For a tool call, it holds the call ID and tool name in plain text.
- **Last reply.** The last reply of a record is the reply of the last prompt turn of the ACP session. It has a text, a tool call, or both. Only for this reply, the comparison is not exact:
  - **Last tool call.** acp2api compares the call ID and the name. It does not compare the arguments.
  - **Last reply text.** acp2api compares the clean form of the two texts. The clean form keeps the indentation and the white space inside a line. To make the clean form, acp2api does these steps in this order:
    1. Remove each CR.
    2. Remove the white space at each line end.
    3. Trim the white space at the two ends of the text.
  - White space is each character with the Unicode property `White_Space`.
  - These two rules read only the content of the request. They test no API client.
- **After a match.** The next record holds each item of the request in the form that the API client sent. Thus each later comparison of these items is exact.
- **Match.** A request continues an ACP session only when all these conditions are true:
  - The record is the start of the conversation of the request. Only the last reply uses the comparison of the rule "Last reply".
  - After a prompt turn that ended with `end_turn`, the rest of the conversation is one or more user messages.
  - After a client tool call, the rest starts with the tool result of that call. Zero or more user messages follow it.
  - In the two conditions above, a `system` or `developer` message after the system prompt counts as a user message.
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
- **Record drop.** acp2api drops the record of an ACP session that no request created or continued in the last 24 hours. ADR-0005 then deletes its session file.
- **Stopped process.** When the runtime process of an ACP session stops, acp2api keeps the record. The next match loads the ACP session with `session/load` in another runtime process.
- **Rebuild.** The first prompt of a rebuild holds one fixed neutral line and the earlier part of the conversation as text with role labels.
  - Each image goes as an ACP `image` block at its place in the text.
  - The system prompt is not part of this text. It goes in `_meta.antigravity.systemPrompt.append`. [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23) decides this and the role labels.
  - [How does a client tool call pause and continue a prompt turn?](https://github.com/yandhi-0421/acp2api/issues/11) decides the text form of a tool result.
- **No notice.** acp2api tells the API client nothing about a rebuild. It writes a log line.

## Reasons

- With cancel at permission, Claude Code and Codex send one request for each tool call. A new ACP session for each request adds one `session/new` to each tool call. That call took 2.4 to 8.5 s on macOS, and 4.4 to 29.3 s in the Windows VM.
- In a continued ACP session, the model sees its earlier replies and tool calls as its own turns. After a rebuild, the model sees them only as user text.
- The official runtime keeps thought signatures in the history of an ACP session. A rebuild cannot give them to the new ACP session.
- ACP gives no other way to put an earlier conversation into a new ACP session. A prompt holds only user content blocks, and the official runtime 1.4.0 has no `fork`.
- Each ACP session has its own harness process, and the official runtime keeps one session file for each ACP session. Fewer ACP sessions give fewer harness processes and session files.
- The match uses only the content of the conversation. Thus it works for Claude Code, Codex, and SillyTavern, and it finds each edit of an earlier message. SillyTavern sends no session hint.
- acp2api gives no `thinking` block, no `reasoning` item, and no `tool_addition` block to the model. An API client can remove an old `thinking` block from a later request, for example Claude Code after a rejection of its signature. A hash of such a block then stops the match, and the model gains nothing.
- Claude Code 2.1.309 changes the arguments of its built-in tools before it sends them back:
  - `Edit` gets `replace_all:false`.
  - `Write` and `Edit` lose the white space at each line end.
  - `Bash` loses a `cd <cwd> && ` at the start of the command.
  - The keys move to the order of the schema.
- Each of these changes stopped the match. In a product run at commit `3b7d52f`, 10 such returns gave 10 rebuilds and 1 replacement at N = 8. With unchanged arguments, 10 returns gave 0 rebuilds.
- A comparison of the call ID and the name finds the same call. acp2api makes the call ID from 16 random bytes, and that value comes back with the tool call.
- An exact hash shows each change of Python indentation and each change of spaces in a string. The collapse of white space hid such a change, and the ACP session kept the earlier text.
- In the probe of `docs/research/2026-11-01-client-history-white-space-probe.md`, the collapse gave no extra match on tool arguments. The hash reads the JSON text, and there a newline in a string is the two characters `\n`.
- An API client can send reply text back in a clean form. A reply of only white space can go away. Apart from that case, the probe found three changes of assistant text: a removed CR, removed white space at a line end, and a trim. Indentation and spaces inside a line did not change. Thus the clean form holds these three changes and no other change.
- After the first return, the API client keeps its own form. In the probe, that form was stable in each later request. Thus the rule for the last reply is not necessary for an earlier item.
- Two ACP sessions with matching records can differ in the last reply within the permitted comparison rules. Thus either eligible ACP session can continue the request.
- After a cut at a stop sequence, the ACP session holds text that the API client never got. An example is a line in the voice of the user. The input tokens must show the conversation of the request ([Which token usage does acp2api report to each API?](https://github.com/yandhi-0421/acp2api/issues/20), decision 6).
- A wait for an ACP session gives no match. When its prompt turn ends, the ACP session holds one more reply.
- The official runtime reads the tools of an MCP server at the first prompt turn of an ACP session (ADR-0002). In the captures of Claude Code 2.1.299, each main request had 29 client tools. Each other request to `/v1/messages` had one message, and no tool or only the server tool `web_search`. Thus it was a separate conversation.
- Claude Code 2.1.299 puts a `role: "system"` message after the first user message and after each tool result. Each of its 15 main requests in the captures ended with such a message. Without the rule for `system` and `developer` messages, no request of Claude Code continues an ACP session.
- `session/set_config_option` took 2 to 4 ms, and the official runtime changes the model at the next prompt turn.
- `session/load` in a new runtime process continued an ACP session with the MCP server at a new URL.
- A `session/load` needs the session file of the ACP session, so the session file stays while acp2api keeps the record. Without a drop of records, no session file goes away before a restart of acp2api (ADR-0005).

## Considered options

- **A new ACP session for each request.** acp2api keeps no state. But each tool call then needs a `session/new` and a rebuild, and the model reads the full conversation again as user text.
- **Session hints as the key.** SillyTavern sends no hint, and a hint does not show an edit of an earlier message.
- **`session/load` with a new MCP server, or `notifications/tools/list_changed`, for a new set of client tools.** In one run on macOS, the official runtime read `tools/list` again after the notification, and the model called the added tool. No probe removed a tool, changed a schema, gave a new set with `session/load`, or sent the notification on Windows. Thus these cases still need a rebuild, and the notification can save a rebuild only when a tool is added. In the captures of Claude Code 2.1.299, the main requests of one conversation had the same number of client tools. A later use of the notification needs no migration, because acp2api keeps the records only in memory.
- **No match change for a new system prompt, as in the earlier CLI bridge of the maintainer.** The model then sees an old system prompt. The earlier CLI bridge kept it only because its agent file could not change.
- **Each `system` and `developer` message in the system prompt, as in the earlier CLI bridge.** Then the system prompt of Claude Code changes with each request, and each request does a rebuild.
- **A rejection of each `system` message after the system prompt.** The Claude Code gateway guide says that Claude Code then stops such messages, but it does not give the error text that Claude Code recognizes. No source shows such a fallback for Codex or SillyTavern.
- **The collapse of white space for each item.** This was the earlier rule. It hides a change of indentation or of spaces in a string. It does not hide a removed final newline, because `A\n` becomes `A ` on one side only.
- **The exact form for each item, with the collapse only for the last reply text.** This is the narrow option of [Preserve significant white space when matching an ACP session](https://github.com/yandhi-0421/acp2api/issues/173). In the simulation of the probe, this option gave 10 of 28 request pairs for one API client. The collapse for each item gave the same count, and the clean form gave 26 of 28. These counts come from hard test cases, so they are not rates of real use.
- **Sorted JSON keys for the arguments of a tool call.** They repair only another key order. They do not repair an added `replace_all`, removed white space, a removed `cd` prefix, or a changed number.
- **Records in a file.** A file format is stored data, and each change of it needs a migration. A restart of acp2api is rare, and the gain is one rebuild for each conversation.
- **No drop of records.** A conversation that continues after a long time needs no rebuild. But the session files then stay until a restart of acp2api. In probe P, the `.db` file of each of 29 session files in the default runtime home had 384 to 704 KiB. Nobody measured the session file of a long conversation.
- **A replay of each earlier user message as its own prompt.** The model answers each message again. The replies then differ from the conversation, and each one costs one model request.
- **The earlier conversation in `_meta.antigravity.systemPrompt.append`.** That option is for instructions, and nobody knows its size limit. [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23) decides its use.
- **Images as files with a path line, as in the earlier CLI bridge.** An ACP `image` block went through to the model. The earlier CLI bridge used files only because its CLI took no image input.

## Consequences

- Each rebuild and each new conversation adds one ACP session. Thus the number of harness processes and session files grows. [How does acp2api start, keep, and stop the processes of the official runtime?](https://github.com/yandhi-0421/acp2api/issues/30) decides the limit of the harness processes. ADR-0005 decides what acp2api does with the session files.
- In SillyTavern, each regenerated reply, each edit, each cut at a stop sequence, and each change of the system prompt causes a rebuild. A macro such as `{{time}}` or a world info entry can change the system prompt.
- A change of the arguments of the last tool call does not reach the history of the ACP session. That history keeps the arguments of the model. FR-023 puts the arguments of the request in the first line of the tool result. Thus that line can show the form of the API client.
- An API client can change the arguments of the last tool call on purpose. It can also change only the white space that the clean form removes. acp2api does not see these changes.
- A change of white space in each other item costs one rebuild. This also applies to the system prompt and to the set of client tools.
- A reply of only white space has an empty clean form. When an API client removes such a reply, the item is absent, and the match stops.
- The records hold no message text or tool arguments. A last tool call keeps its ID and name in plain text. Thus acp2api cannot do a rebuild from `previous_response_id`, `item_reference`, or `conversation`, and it keeps their rejection.

Evidence: the grilling of [Which ACP session does acp2api use for each API request?](https://github.com/yandhi-0421/acp2api/issues/10); the grilling of [How does the system prompt of a request reach the model?](https://github.com/yandhi-0421/acp2api/issues/23), for the `system` and `developer` messages; the grilling of [What does acp2api return for `thinking` blocks and `reasoning` items?](https://github.com/yandhi-0421/acp2api/issues/24) and [What does acp2api do with each server tool that Claude Code sends?](https://github.com/yandhi-0421/acp2api/issues/28), for the blocks that acp2api ignores; the Claude Code gateway guide, section "Automatic retry and error forwarding" ([link](https://code.claude.com/docs/en/llm-gateway-protocol.md)); F3, F6, F8, F9, and F12 of [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8); W3 of [Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16); `docs/research/captures/2026-10-17-claude-code-check/runs.json`, runs `p7-tui` and `p10-tui-search`, the last role of each main request, and the `toolCount` of the main requests of runs `p2-pdf`, `p3-search`, and `p10-tui-search`; `blocks.json` in the same directory, `messageRolesAfterPdfRead`; section 3.3 of `docs/research/2026-10-15-official-runtime-acp-probe.md`; sections 5 and 7 of `docs/research/2026-10-15-official-runtime-probe-p.md`; sections 4, 5, and 6 of `docs/research/2026-11-01-client-history-white-space-probe.md`, with its captures in `docs/research/captures/2026-11-01-history-white-space/`; and the decisions of the owner and the product run in [Claude Code rewrites tool arguments, which can stop each continuation](https://github.com/yandhi-0421/acp2api/issues/218).
