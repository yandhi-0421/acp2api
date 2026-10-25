# acp2api v1 product requirements document

**Status:** Blocked draft

The blockers are OD-1 and OD-2 in "Open decisions". The first build and one measurement close both.

## Summary

acp2api v1 is a local service for one person on Windows or macOS. It translates the Anthropic Messages API, the OpenAI Responses API, and the OpenAI Chat Completions API into ACP for the official runtime. Thus Claude Code, Codex, and Chat Completions clients can use the models of the Antigravity sign-in of the owner.

The product decisions come from the closed decision issues of the map [#1]. The ADRs in `docs/adr/` hold the technical decisions, and `GLOSSARY.md` defines the terms.

## Problem and outcome

Google publishes the official runtime for Antigravity. It speaks only ACP over stdio, and a personal sign-in gives only Gemini models ([#2]). Claude Code, Codex, and SillyTavern speak only their HTTP APIs ([#5]). Thus the owner cannot use the models of the official runtime from these API clients.

Other tools give such access with the OAuth tokens of Antigravity, and they call the Google backend directly. In February 2026, Google answered such tools with account actions, for example HTTP 403 `PERMISSION_DENIED` with the reason `TOS_VIOLATION` ([#4], ADR-0007). The owner wants access with a lower risk to the Google account.

The intended outcomes:

1. The owner points Claude Code, Codex, or a Chat Completions client at acp2api on the same computer. The API client then works with the models of the official runtime (SC-003, SC-004).
2. The client tools of the API client work, and no built-in tool of the official runtime runs. The target is a pure model, and acp2api makes a best effort (SC-004).
3. acp2api decreases the risk to the Google account. It reaches Google only through the official runtime, and it stops all requests when Google shows a risk (SC-003).
4. v1 works on Windows and macOS (SC-001 to SC-005).
5. acp2api uses little memory and little disk space (SC-006).

acp2api decreases the chance that Google finds its use. It does not make the use agree with the Antigravity terms, which forbid access through third-party software (ADR-0007).

## Users

- **The owner.** One person who runs acp2api on their own computer, with one Google account. The owner installs the official runtime, signs in, and uses the management panel. Nobody else uses the API of an instance.
- **API clients.** Programs on the same computer that send requests to acp2api:
  - Claude Code, through the Anthropic Messages API;
  - Codex, through the OpenAI Responses API;
  - a Chat Completions client, for example SillyTavern or the OpenAI SDK. acp2api follows the meaning of the OpenAI fields, with no rule for one API client.
- **The maintainer.** The person who releases acp2api. The maintainer runs the release gate, and puts each build that passed the acceptance test on the list of a release.

The access protection of acp2api stops web pages, other accounts of the computer, and mistakes. It does not try to stop the API clients on the same computer. These API clients can start acp2api and read its configuration file ([#7]).

## Non-goals

- Linux and Docker.
- Code signatures and notarization.
- More than one Google account, and account rotation.
- A check for a new version of acp2api.
- An emulation of a server tool, for example web search or the advisor of Claude Code.
- An admission of a built-in tool of the official runtime. A later version can admit a harmless built-in tool.
- A text extraction from a PDF.
- Model aliases, and a model catalog for Codex.
- Charts of the usage, a start at login, a tray icon, and an entry that stops or restarts acp2api.
- Desktop notifications.
- A promise for the versions of Claude Code and Codex that a release does not name.
- The open-source license. The maintainer decides it later, apart from the product.

## Product requirements

### Install and start

#### FR-001 — Release archives

Each release of acp2api has one binary archive for each of macOS arm64, macOS x64, Windows x64, and Windows arm64 on GitHub Releases. Each archive holds one executable of acp2api. The project has no installer.

**Pass:** the release page has the four archives. The executable of each archive starts on its platform.

#### FR-002 — Start command and terminal

The command `acp2api` starts acp2api. Its only options are `--port`, `--data-dir`, `--version`, and `--help`. v1 has no management command, for example `unlock`, `status`, or `stop`.

At each start, acp2api shows the address of the management panel in its terminal. While it runs, it shows one line for each request with the metadata of its log line (FR-056). It also shows one line for each change of state.

**Pass:** `acp2api --help` lists the four options and no command. The terminal shows the address at each start, one line for each request, and one line for each change of state.

#### FR-003 — Data folder

acp2api keeps its own files in the data folder:

- macOS: `~/Library/Application Support/acp2api/`;
- Windows: `%LOCALAPPDATA%\acp2api\`.

The option `--data-dir` sets another data folder. The data folder holds the configuration file `config.toml`, the state file, the logs, the runtime home, and the copy of the official runtime.

The temporary folders of ADR-0004 and ADR-0005 and the account coordination files stay outside the data folder.
Account coordination files do not move with `--data-dir` or a different system temporary folder (FR-006).

**Pass:** after a first start, the configuration file and the logs are in the data folder of the platform, or in the folder that `--data-dir` gives.

#### FR-004 — Settings and configuration file

The configuration file holds the five settings of v1, in TOML:

| Setting | Default | Where the owner changes it |
|---|---|---|
| Port | 8329 | The configuration file, or `--port` |
| Concurrency limit: 1, 2, or 3 | The smaller of 2 and the platform maximum (FR-049) | The management panel |
| Local API key | Random at the first start | The management panel: a new key |
| Management password | Random at the first start | The management panel |
| Path of `agy_acp_server` | None | The management panel |

- Each change in the management panel applies at once, and acp2api writes it into the configuration file.
- The owner can edit the configuration file while acp2api is stopped.
- Only the computer account of the owner can read the configuration file.
- When the port is in use, the start fails with message M-25. acp2api does not change to another port.
- When a setting has a value that acp2api cannot use, the start fails with message M-26.
- An unknown key or a TOML syntax error also stops the start with M-26. The message names the key, or the line when no key is identifiable.
- An absent port or concurrency limit takes its default. The option `--port` still overrides the port.
- An absent local API key gives a new random key. acp2api saves it in the configuration file and shows it in the terminal.
- When the owner removes the field of the management password, the next start makes a new random management password. It shows the password in the terminal.
- An absent path of `agy_acp_server` means that the owner specified no path.

v1 has no setting for the listen address, the log level, or an upstream proxy.

**Pass:**

- A change in the management panel shows in the configuration file and in the next request, with no restart.
- The file permissions give no other account of the computer read access to the configuration file.
- A start with the port in use fails with M-25. A start with a wrong value fails with M-26.
- An unknown key or a TOML syntax error gives M-26 with the key or the line.
- Without the port, acp2api uses 8329. Without the concurrency limit, it uses the default of FR-049.
- Without the local API key, the terminal shows a new key that acp2api accepts and saves. The earlier key no longer works.
- Without the path of `agy_acp_server`, acp2api uses the search of FR-042.
- A start with no field of the management password shows a new password, and that password opens the management panel.

#### FR-005 — First start

When acp2api makes the configuration file, it makes a random local API key and a random management password. It shows the address of the management panel, the local API key, and the management password in its terminal. Then it opens the management panel in the browser.

**Pass:** two first starts with two empty data folders give two different keys and two different passwords. The terminal shows the three values, and the browser opens the address.

#### FR-006 — One acp2api for each computer account

When acp2api runs for a computer account, another start for that account waits until it can read the first management panel address. It then fails with message M-24. This also applies with another data folder or another system temporary folder. Changes to `TMPDIR`, `TEMP`, or `TMP` do not permit a second instance. The first acp2api continues, and it keeps its folders.

Before the first address exists, the wait has no time limit. The owner can cancel the waiting start. Cancellation changes no file of the first instance. When that instance exits before it publishes an address, the waiting start can become the sole instance.

Unreadable or damaged account coordination data stops the new start with M-28. acp2api does not delete a lock or use an absent, stale, or unvalidated address in M-24.

**Pass:** a second start fails with M-24, also with another data folder or system temporary folder. After it, the first acp2api answers requests, and its temporary folders and session `cwd` folders are still there.

### Access protection

#### FR-007 — Local API key

Each request to an API of acp2api, also `GET /v1/models`, carries the local API key. The key is in the `Authorization: Bearer` header or in the `x-api-key` header. Else acp2api refuses the request with HTTP 401 (FR-033) and message M-01.

v1 has one local API key, and no setting turns the key off. The management panel shows the key, and it can replace the key with a new random key. The old key then stops at once.

**Pass:** a request with no key or a wrong key gets 401 on each endpoint. After a new key, a request with the old key gets 401, and a request with the new key gets an answer.

#### FR-008 — Requests from a browser

- acp2api listens only on 127.0.0.1.
- acp2api sends no CORS header, also not to an `OPTIONS` request.
- acp2api refuses each request with an `Origin` header, unless the origin is the management panel and the target is the management panel.
- acp2api refuses each request with a `Host` header other than `127.0.0.1:<port>` or `localhost:<port>`.

Each such refusal has an HTTP status from 400 to 499, and no prompt turn starts.

**Pass:** acp2api accepts no connection on another network address of the computer. A request to an API with `Origin: https://example.com` gets a 4xx status. A request with `Host: example.com` gets a 4xx status. No answer has a CORS header.

#### FR-009 — Management password and panel session

- The management password protects the full management panel and each management endpoint.
- After a correct password, acp2api gives a cookie for a panel session, with `HttpOnly` and `SameSite=Strict`.
- A panel session ends at a restart of acp2api, or after 12 hours.
- A change of the management password ends each panel session.
- acp2api keeps each management password only as a hash, including a random password that it makes at a start.
- To end a lockout, the owner enters the management password again, also in an open panel session.

**Pass:** each page and each endpoint of the management panel needs the password or a valid panel session. The configuration file holds no management password in plain text, only its hash. A panel session stops after 12 hours, after a restart, and after a change of the password.

### APIs

#### FR-010 — Endpoints and stream form

acp2api serves these endpoints on `http://127.0.0.1:<port>`:

- `POST /v1/messages`, also with the query `beta=true`: the Anthropic Messages API;
- `POST /v1/responses`: the OpenAI Responses API;
- `POST /v1/chat/completions`: the OpenAI Chat Completions API;
- `GET /v1/models`: the model names (FR-012).

Each `POST` endpoint gives a stream when `stream` is true, and one body when it is not. acp2api has no endpoint that counts tokens, for example `/v1/messages/count_tokens`.

A stream has the content type `text/event-stream`. acp2api sends each event when it has the event, not after the end of the response.

- Anthropic Messages API: `message_start`, then for each content block `content_block_start`, its deltas, and `content_block_stop`, then `message_delta`, then `message_stop`.
- OpenAI Responses API: the last event is `response.completed`, and it carries `response.id`.
- OpenAI Chat Completions API: `data:` lines with chunks, then `data: [DONE]`.

**Pass:** each endpoint answers a valid request in the success form of its API, with and without a stream. `/v1/messages/count_tokens` gives HTTP 404. In each stream, the events have the order above. With two or more reply text chunks, the first text delta leaves acp2api before the prompt turn ends. The last chunk follows FR-028, also when it is the only chunk.

#### FR-011 — Keep-alive events

After acceptance, acp2api sends keep-alive events while a streamed request waits in the queue or its prompt turn sends no update. Each interval below is less than 300 s:

- from acceptance to the first event;
- between two events;
- from the last event to the end of the response.

A request before acceptance has no stream and no keep-alive event (FR-026).

- Anthropic Messages API: the event `ping`.
- OpenAI Responses API: a real event, for example `response.in_progress`.
- OpenAI Chat Completions API: the written design decides the event.

**Pass:** an accepted request with a pause of 330 s in the queue keeps each interval below 300 s. A prompt turn with no update until acp2api cancels it (FR-051) gives the same result, up to the error event.

### Model names and thinking level

#### FR-012 — Model names

For each value of the config option `model` of the official runtime, acp2api offers these model names:

- the value, for example `gemini-3.8-flash`;
- the value with the suffix of each thinking level that the model has, for example `gemini-3.8-flash-low`.

At the start, acp2api reads the model names from the successful sign-in check of FR-038. That check creates one ACP session, with no prompt turn. A first `GET /v1/models` uses those names without another ACP session. acp2api reads them again at each later new ACP session.

acp2api rejects a request with a `model` that is not a model name, for example `claude-opus-5-5` or `gpt-5.5`. The error is HTTP 400 with message M-08, which lists the model names.

`GET /v1/models` returns one list that clients of the OpenAI APIs and of the Anthropic Messages API can read: `{"object": "list", "data": [{"id", "object": "model", "type": "model", "display_name", "created", "created_at", "owned_by"}]}`.

- The list holds each model name.
- The `display_name` is the name of the value in the config option of the official runtime.
- In a lockout, a quota wait, the state "sign-in required", and the state "runtime unavailable", it returns the error of that state (FR-033).

**Pass:**

- With a fake ACP agent that gives one model with the levels `low`, `medium`, and `high`, the list has four names.
- A start with a valid sign-in reads those names through one ACP session, before any API request. The first model-list request creates no further ACP session.
- When the fake ACP agent adds a model, the list holds it after the next new ACP session.
- A request with `claude-opus-5-5` gets 400 with the list of the model names.

#### FR-013 — Thinking level

- A model name with a suffix sets the thinking level. acp2api then ignores the effort fields of the request.
- For a model name without a suffix, the effort fields set the thinking level, as the table below gives.
- When no effort field sets a level, the thinking level is `high`.
- When the model does not have the level, acp2api uses the next higher level of the model. When the model has no higher level, acp2api uses its highest level.
- The `model` field of each response holds the model name of the request.

| Value of an effort field | Thinking level |
|---|---|
| `none`, `minimal`, `low`, and `thinking` with the type `disabled` | `low` |
| `medium` | `medium` |
| `high`, `xhigh`, `max`, `ultra` | `high` |

- `reasoning.effort`, `reasoning_effort`, and `output_config.effort` use the values of the table.
- `thinking` with the type `adaptive` or `enabled` sets no level. `budget_tokens` has no effect.
- When a request has `output_config.effort` and `thinking`, `output_config.effort` sets the level.
- acp2api ignores a value that the table does not give, and writes the value into the log. The default level then applies.

**Pass:** for each row of the table, the prompt turn runs at the level of the row. `gemini-3.8-flash-low` with `reasoning.effort: "high"` runs at `low`. A request with no effort field runs at `high`. A response to `gemini-3.8-flash` has `model: "gemini-3.8-flash"`.

### Request content

#### FR-014 — Use, ignore, reject, or emulate

acp2api gives each request field and each content block one action:

- **Use:** acp2api maps it to the official runtime, or does the work itself.
- **Ignore:** an API client sends it by default, or it is only a hint. A reply without its effect is still correct. acp2api sends no error, and the value does not go to the official runtime.
- **Reject:** the owner turned on a feature, and acp2api cannot give its result. acp2api answers with HTTP 400 (FR-033). The message is M-02 for a field and M-03 for a content block, unless a row names another message.
- **Emulate:** acp2api makes the exact result itself.

A field with a value that turns its feature off counts as an absent field, for example `logprobs: false`, `background: false`, `audio: null`, or `citations` with `enabled: false`.

Fields:

| Field | Action |
|---|---|
| `stream`, `stream_options.include_usage`, `n` with the value 1 | Use |
| The effort fields | Use (FR-013) |
| `tool_choice: "auto"` | Use |
| `temperature`, `top_p`, `top_k`, `presence_penalty`, `frequency_penalty`, `seed`, `logit_bias` | Ignore |
| `max_tokens`, `max_completion_tokens`, `max_output_tokens` | Ignore |
| `cache_control`, `prompt_cache_key`, `service_tier`, `store` with each value, `truncation`, `prediction` | Ignore |
| `metadata.user_id`, `user`, `safety_identifier`, `client_metadata` | Ignore. These values never go to the official runtime. |
| `context_management`, `output_config.task_budget`, and the tool fields `strict`, `defer_loading`, and `eager_input_streaming` | Ignore |
| `parallel_tool_calls`, `max_tool_calls`, `verbosity`, `text.verbosity` | Ignore |
| `response_format` with the type `json_object` | Ignore |
| `include: ["reasoning.encrypted_content"]`, `reasoning.summary`, `thinking.display` | Ignore (FR-018) |
| `stop`, `stop_sequences` | Emulate (FR-015) |
| `tool_choice: "none"` | Emulate (FR-022) |
| `n` greater than 1, `logprobs`, `top_logprobs`, an `include` value with logprobs | Reject |
| `response_format` or `text.format` with a JSON schema, `output_config.format` | Reject |
| A `tool_choice` that forces a tool: `required`, `any`, or the name of a tool | Reject |
| `web_search_options` | Reject |
| Audio output: `audio`, or `modalities` with `audio` | Reject |
| `background: true`, `previous_response_id`, `conversation` | Reject |
| `citations` with `enabled: true` | Reject |
| A request body that is not JSON, or that has more than 128 levels of objects and arrays | Reject, with message M-07 |
| Another top-level field | Ignore. acp2api writes the name of the field into the log, and the management panel counts it (FR-054). |

Content blocks and items:

| Block or item | Action |
|---|---|
| Text: `text`, `input_text` | Use |
| An input item of the Responses API with a `role` and no `type` | Use, as a message item |
| An image with base64 data | Use. The model gets the image at its place. |
| A text document, or a PDF | FR-017 |
| A document or a file with another media type | Reject, with message M-04 |
| Audio input | Reject |
| An image or a file with an `http` or `https` URL | Reject, with message M-05. acp2api sends no HTTP request for it. |
| A `file_id` | Reject, with message M-02 for the field `file_id` |
| `item_reference` | Reject |
| A last item from the assistant (a prefill) | Reject, with message M-27 |
| `thinking`, `redacted_thinking`, a `reasoning` item, and `reasoning_content` in an assistant message | Ignore (FR-018) |
| Tool calls, tool results, and tools | FR-022 to FR-024 |
| Another content block type | Reject, with message M-03 |

**Pass:** for each row, a request with that field or block gets the action of the row. A request with each Ignore field set gives the same prompt to the official runtime as a request without it. An unknown top-level field gives a log line with its name and no error.

#### FR-015 — Stop sequences

- acp2api finds the first match of a stop sequence in the reply text, and it cuts the reply before the match.
- It ends the response with the stop reason of the API: `finish_reason: "stop"` (Chat Completions API), or `stop_reason: "stop_sequence"` with the matched `stop_sequence` (Anthropic Messages API).
- Then it cancels the prompt turn.
- No text at or after the match reaches the API client, also in a stream.

**Pass:** a reply that holds the stop sequence ends before it, with the stop reason above. No stream event holds a character of the match or after it. The official runtime gets a cancel of the prompt turn.

#### FR-016 — No slash command from an API client

No text of an API client starts a slash command of the official runtime. The first text of a prompt can start with `/`, also after white space. Then acp2api puts the role label `[user]` on its own line before it. The text of each other prompt does not change. This rule does not apply to the `/usage` that acp2api itself sends (FR-052).

**Pass:** a request with the text `/logout` gives a prompt that starts with the line `[user]`, and the official runtime stays signed in. A request with one space and then the text `/usage` gives a prompt that starts with `[user]`.

#### FR-017 — Documents

- **A PDF in a message.** acp2api rejects the request with message M-06. This also applies to a PDF in an earlier message of the conversation.
- **A PDF in a tool result.** acp2api puts this note at the place of the PDF: `[A PDF document was here. acp2api cannot give PDF documents to the model.]`. The other content of the tool result does not change.
- **A text document.** The model gets the text at its place, in this form: the line `[document: <title>]`, then the text, then the line `[end of document]`. The title is the `title` of the document, else its `filename`. A document with neither starts with the line `[document]`. A text document is a `document` block with the source type `text` or `content`, or a document or a file with the media type `text/plain`.
- **Another media type.** acp2api rejects the request with message M-04.

**Pass:** each case gives the result above on each API. A tool result with a PDF and a text gives the note and the text to the model, and the API client gets no error.

#### FR-018 — Thought text

- acp2api sends no thought text on any API: no `thinking` block, no `redacted_thinking` block, no `reasoning` item, and no `reasoning_content`.
- When the official runtime sends thought text, acp2api drops it and writes a log line.
- acp2api ignores each `thinking` block, `redacted_thinking` block, and `reasoning` item of a request, and `reasoning_content` in an assistant message. The model does not get them, and they cause no error.

**Pass:** a prompt turn with thought text from the official runtime gives a response with no thought text, and one log line. A request with a `thinking` block gives no error, and the prompt holds no text of the block.

### What the model gets

#### FR-019 — System prompt

`GLOSSARY.md` defines the system prompt.

- The model gets the system prompt as an addition to the system text of the official runtime, not as user text.
- Each `system` field, `instructions` field, and initial `system` or `developer` message is one part of the system prompt. acp2api joins these parts with one blank line. Within each part, acp2api joins the text blocks in order with no added separator. A message with a `name` starts with its role label once, on its own line, for example `[system: example_user]`.
- When the first `system` block is the attribution block of Claude Code, acp2api removes it. acp2api finds the block by the start of its text. The client check of SC-003 finds that start, and the written design records it.
- A later `system` or `developer` message is a message of the conversation. It stays at its place, with its role label.

**Pass:** the ACP session of a request gets its system prompt as an addition to the system text. No prompt text holds the system prompt. A request of Claude Code gives no attribution block to the official runtime. A `system` message after a user message is at its place in the prompt, after the line `[system]`.

#### FR-020 — Role labels

- A role label is a line of its own before an item: `[user]`, `[assistant]`, `[system]`, or `[developer]`. With a `name`, the form is `[user: Alice]`.
- The content of an item starts on the line after its role label. Each role label and each sentence of FR-023 starts a new line. No blank line separates two items.
- In the text of a rebuild, one blank line follows the neutral line (FR-021).
- acp2api keeps the `name` of each message of each role.
- A tool call and a tool result get a sentence, not a role label (FR-023).
- When the only new item is an unnamed user message, acp2api sends its content unchanged, except for the role label of FR-016.
- A named user message always keeps its role label. Each other item gets its role label or its sentence.

**Pass:** a prompt with a tool result and a `system` message has the sentence of the tool result and the line `[system]` before the message. A prompt with one unnamed user message has no role label. A single Chat Completions user message with `name: "Alice"` gets the label `[user: Alice]`.

#### FR-021 — The conversation in each prompt

The model gets the conversation of each request one time, with no item that the API client did not send. ADR-0003 gives the method.

- When a request continues the conversation that an ACP session holds, only the new items go to the model.
- Else acp2api does a rebuild. The first prompt of a new ACP session holds the neutral line and the earlier part of the conversation as text with role labels. Each image is at its place, and the system prompt is not part of this text.
- The neutral line is `The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.`
- The next request of a conversation does a rebuild after each of these events:
  - a cut at a stop sequence;
  - a disconnect of the API client;
  - an error;
  - the stop reason `refusal`.
- After a restart of acp2api, the next request of each conversation does a rebuild.
- acp2api uses only the content of the conversation. It does not use session hints, for example `x-claude-code-session-id` or `prompt_cache_key`.
- acp2api tells the API client nothing about a rebuild. It writes a log line.

**Pass:**

- A request that adds one user message to the last conversation sends only that message, in the same ACP session.
- A request with an edited earlier message gives a new ACP session whose first prompt starts with the neutral line.
- A stop sequence cuts a reply. The next request then gives a new ACP session, and its text holds the reply only up to the cut.
- No response holds a notice of a rebuild, and the log has one rebuild line for each rebuild.

### Tools

#### FR-022 — Client tools

- The model gets the client tools of the request, with their names, descriptions, and schemas. The model gets no built-in tool.
- When the model calls a client tool, acp2api ends the response with that tool call, in the form of its API. Text before the call stays in the response.
  - Anthropic Messages API: a `tool_use` block and `stop_reason: "tool_use"`.
  - Chat Completions API: `tool_calls` and `finish_reason: "tool_calls"`.
  - Responses API: a `function_call` item, or a `custom_tool_call` item for a `custom` tool.
- Each response has no more than one tool call.
- No prompt turn waits for a tool result. Between two requests, a tool call holds no place.
- A `custom` tool of Codex, for example `apply_patch`, goes to the model as a tool with one string argument `input`. Its description holds the grammar.
- Each tool of a `namespace` tool of Codex goes to the model as one tool. acp2api returns the `namespace` in the `function_call`.
- `tool_choice: "none"`: the response has no tool call.
- acp2api rejects these tools:
  - a `tool_search` tool, with message M-09;
  - a client tool with the name `list_resources` or `read_resource`, with message M-10;
  - a tool with a `type` that acp2api does not know, with the `tool type` message of FR-024.

**Pass:**

- A model call of a client tool gives the API client a response with that one tool call, in the form of its API.
- A request with `tool_choice: "none"` gives a response with no tool call.
- The tools that the official runtime offers to the model are only the client tools of the request.
- A request with a tool from the reject list gets HTTP 400 with its message.

#### FR-023 — Tool calls and tool results as text

The model gets each tool result as text in the next prompt, in this form:

- The first line is `Result of your tool call <name> <arguments>:`. The result follows it.
- For an error result, ` (the tool reported an error)` follows the arguments.
- Each image of the result is at its place.
- When the arguments have more than 1,000 characters, the line holds only the first 1,000, and then the mark ` [arguments cut]`.
- `<arguments>` is the text of the arguments. A JSON string of the API client goes as it is. An object, for example the `input` of an Anthropic `tool_use` block, goes as compact JSON with its keys in the order of the request.
- When the conversation has no earlier tool call with the ID of the tool result, the first line is `Result of the tool call <id>:`.

In the text of a rebuild, an earlier tool call is the sentence `You called the tool <name> with the arguments <arguments>.`, with the full arguments. The same form of a tool result goes into a continued ACP session and into the text of a rebuild.

**Pass:** each case gives the form above in the prompt. A result with arguments of 5,000 characters gives a first line with 1,000 characters of the arguments and the mark ` [arguments cut]`.

#### FR-024 — Server tools

A server tool runs on the servers of the provider of an API, and acp2api has no such servers.

- **The advisor tool.** A tool with a `type` that starts with `advisor_` gets HTTP 400 in the error envelope of the Anthropic Messages API. acp2api also writes a log line. The message is `Input tag '<type>' found using 'type' does not match any of the expected tags`. Thus Claude Code sends the request again without the tool.
- **Other server tools of the Anthropic Messages API.** acp2api answers with HTTP 400 and the message `capability_rejected: server tool <type> (name <name>) is not supported by acp2api.`. For `web_search_20250305`, the message adds the sentence `To stop these calls, deny the WebSearch tool in the Claude Code settings.`
- **The `web_search` tool of Codex.** acp2api answers with HTTP 400 and message M-11.
- **A tool with a `type` that acp2api does not know**, on each API: HTTP 400 and the message `capability_rejected: tool type <type> (name <name>) is not supported by acp2api.`
- **A `tool_addition` block.** When its `tool` is a `tool_reference` to a tool in the `tools` of the request, acp2api ignores the block. acp2api rejects each other form of the block with message M-03.

**Pass:** each case gives the status and the exact message above. A request with an advisor tool and a `tool_addition` block gets the `Input tag` message, not the `tool_addition` error.

### Token usage

#### FR-025 — Token usage

- **Input tokens.** The value is the `used` field of the last usage update of the prompt turn. It goes into `input_tokens` (Anthropic Messages API and Responses API) or `prompt_tokens` (Chat Completions API).
- **Output tokens.** acp2api estimates the tokens of the text and of the tool call arguments that it returns. The count is the number of ASCII characters divided by 4 and rounded up, plus 1 token for each other character.
- **Total tokens.** The sum of the input tokens and the output tokens.
- **Cache tokens and reasoning tokens.** Where the API permits, acp2api leaves out each field for them. Else it sends 0.
- **A prompt turn with no usage update.** The input tokens are the local estimate of the input tokens (assumption A-1).

The local input estimate uses the output estimate formula above. It counts these fields of the full request after normalization, also for a continued ACP session:

- the system prompt text after removal of the attribution block;
- all message text and message names;
- each earlier tool call name and its full arguments;
- each tool result text;
- each declared client tool name, description, and schema, also its grammar for a `custom` tool.

The estimate joins these values with no separator. JSON argument strings stay unchanged. Argument objects and schemas use compact JSON, with keys in request order and non-ASCII characters unescaped. The count excludes images, PDFs, IDs, role labels, neutral lines, thought text, ignored fields, and the system text of the official runtime.

The place of the usage:

- Anthropic stream: `message_start` carries a local estimate of the input tokens. The last `message_delta` carries the values above.
- Responses stream: `response.completed` carries the usage.
- Chat Completions stream: only with `stream_options.include_usage`, the last chunk carries the usage.
- A response with no stream carries the usage in its body.

**Pass:** for a prompt turn whose last usage update has `used: 5183`, the input tokens are 5183 on each API. A reply of 9 ASCII characters and 2 other characters gives 5 output tokens. The same character counts in the input fields give a local estimate of 5. No response has a cache count other than 0.

### Errors and retries

#### FR-026 — Acceptance of a request

acp2api accepts a request after the checks of the local API key, of the request, of the state, and of the queue.

- A required sign-in check finishes before acceptance (FR-038), also when it must wait for a place.
- That wait counts toward the queue cap of FR-050. It sends no HTTP 200, response header, or stream event.
- For a streamed request, acp2api sends the status 200 and the headers at the acceptance.
- Each refusal before the acceptance is an HTTP status with the error envelope of its API. The written design decides the `type` and the `code` of the OpenAI error body.
- Each later error of a streamed request is a stream event (FR-027).
- A request with no stream gets each error as an HTTP status.

**Pass:** a streamed request that passed all admission checks has status 200 while it waits for a place. A request before its required sign-in check has no response yet. A failed check gives HTTP 400 with no stream event. A request with a wrong key gets 401 with no stream event.

#### FR-027 — Error events in a stream

- **Anthropic Messages API.** An `error` event with the `error.type` of the table of FR-033, column "In an Anthropic stream". No `message_stop` event follows it. After text, acp2api first sends `content_block_stop` for the open text block, and then the `error` event. acp2api sends no `overloaded_error` in a stream.
- **OpenAI Responses API.** A `response.failed` event.
  - For the 503 row of FR-033, the code is `server_error`, and `error.headers` holds `retry-after` with the seconds of FR-033.
  - For each other row, also a quota wait, the code is `invalid_prompt`.
  - acp2api never uses the code `insufficient_quota`.
- **OpenAI Chat Completions API.** One `data:` line with an `error` object. Then the stream ends with no `data: [DONE]`.

**Pass:** for each row of FR-033 that can occur after the acceptance, each API gives the event above. After text, the Anthropic stream has `content_block_stop` before `error`.

#### FR-028 — No text of an upstream error as reply text

The official runtime sends the text of an upstream error as normal reply text. No such text reaches the API client as reply text.

- acp2api holds the newest reply text chunk until the next reply text chunk. A usage update or thought text does not release it.
- At the end of a prompt turn, acp2api waits up to 200 ms for the error lines of the ACP session. The wait ends early when both lines are there: `Antigravity SDK error processing prompt for session <ID>` and the last line of its traceback.
- With no error line at the end of this wait, acp2api sends the held chunk.
- After an error line, acp2api drops the held chunk, and it ends the response with the error. The text that acp2api sent before the held chunk stays in the response.
- When an error line comes after the wait, the ACP session never continues, and acp2api writes a log line.

**Pass:** in a test, the last chunk of a prompt turn is the error text, and the error line comes 0 to 200 ms after it. The response then has the error and no error text. The text before that chunk is in the response.

#### FR-029 — Retry wait

An error of the 503 row or the 502 row of FR-033 starts a retry wait of 30 s for its conversation.

- In the retry wait, a request can have that conversation, or a conversation that continues it. When it selects the same model ID, it starts no prompt turn. acp2api answers it before the acceptance with the HTTP status of the row and message M-23.
- The model name and the thinking level of the request give the model ID. A request that selects another model ID runs.
- For the 503 row, `retry-after` gives the seconds to the end of the retry wait.
- A refused request does not make the retry wait longer.
- The match uses the system prompt, the items, and the set of client tools. `stream` and `max_tokens` do not change the match.
- acp2api keeps each retry wait only in memory.

**Pass:**

- After a 503-row error, the same request 5 s later gets 503 with `retry-after: 25`, and the official runtime gets no prompt.
- The same request 31 s later runs.
- A request that continues the failed conversation, in the retry wait, gets the same status.
- The same conversation with another thinking level runs at once.

#### FR-030 — Normal ends of a response

- **`max_tokens`.** `stop_reason: "max_tokens"` (Anthropic Messages API), `finish_reason: "length"` (Chat Completions API), or `status: "incomplete"` with the reason `max_output_tokens` (Responses API).
- **`refusal`, when `/usage` shows quota for the model of the request (FR-034).** `stop_reason: "refusal"` (Anthropic Messages API), `finish_reason: "content_filter"` (Chat Completions API), or `status: "incomplete"` with the reason `content_filter` (Responses API).

**Pass:** each stop reason gives the end above on each API, with no error.

#### FR-031 — Disconnect of the API client

When the API client disconnects during a prompt turn, acp2api cancels the prompt turn at once, and it sends no response. A request that waits in the queue leaves the queue at its disconnect.

At a disconnect during a prompt turn, acp2api clears the current ACP session. That ACP session holds part of a reply that the API client did not get.

**Pass:** after a disconnect during a prompt turn, the official runtime gets a cancel of the prompt turn. After a disconnect in the queue, the count of the queue decreases by one.

#### FR-032 — Messages

Each message that acp2api writes starts with `acp2api: `. The messages of FR-024 keep their own start, because Claude Code matches it. The section "Message texts" gives each text.

**Pass:** each error body and each error event of acp2api holds the text of its case from "Message texts".

#### FR-033 — Error statuses

| Case | HTTP status and headers | Anthropic `error.type` in the body | In an Anthropic stream | Intended retry of the API client |
|---|---|---|---|---|
| An absent or wrong local API key | 401 | `authentication_error` | Does not occur | No |
| A rejected field, content block, tool, model name, or body | 400 | `invalid_request_error` | Does not occur | No |
| A lockout, the state "sign-in required", the state "runtime unavailable", or the exit of acp2api | 400 with `x-should-retry: false` | `invalid_request_error` | `invalid_request_error` | No |
| A quota wait | 429 with `retry-after`: the seconds to the reset time | `rate_limit_error` | `rate_limit_error` | Claude Code retries only when the reset time is 60 s away or less |
| A full queue | 529 on the Anthropic Messages API and 503 on the two OpenAI APIs, with `retry-after: 10` | `overloaded_error` | Does not occur | Yes |
| The 503 row: a temporary upstream error, an update timeout, or a runtime process failure (see below) | 503 with `retry-after: 30` | `api_error` | `api_error` | Yes |
| The 502 row: another upstream error, for example a Google 400; the stop reason `max_turn_requests`, or a built-in tool that runs with no permission request | 502 with `x-should-retry: false` | `api_error` | `invalid_request_error` | No |

An upstream error that is a lockout signal, a quota signal, or a sign-in error (FR-034) gets the row of the state that it starts.

The 503 row includes a Google 5xx, an upstream timeout, a prompt turn without an update for 5 minutes, and a runtime process failure.

The message of an upstream error is M-18. The other causes of the 503 row and the 502 row have M-19 to M-22, and the exit of acp2api has M-15.
An ACP error without an HTTP error line uses the ACP form of M-18. It keeps its JSON-RPC code and message.

**Pass:** each case gives the status, the headers, and the `error.type` of its row. With the real Claude Code and Codex, the retries of SC-003 occur.

### States that stop requests

Several conditions can stop requests at the same time. Their priority for a response and the first panel tile is:

1. lockout;
2. runtime unavailable;
3. sign-in required;
4. quota wait;
5. normal state, when no condition remains.

The end of one condition does not end another condition. A successful sign-in check clears only sign-in required, then acp2api checks the other conditions before acceptance.

#### FR-034 — Lockout signals

acp2api reads lockout signals only from two error channels:

- the error of each ACP call;
- the error lines of the ACP session in the stderr of the official runtime.

The reply text counts only in a prompt turn that stderr marks as failed.

| Case | Result |
|---|---|
| 403 or `PERMISSION_DENIED`, with each reason, for example `TOS_VIOLATION`, `VALIDATION_REQUIRED` or "Verify your account", and `SUBSCRIPTION_REQUIRED` | Lockout |
| The text "Terms of Service" in an error channel, with no 403 | Lockout |
| A sign-in error that also has a 403 or a "Terms of Service" text | Lockout |
| Another sign-in error: 401, `UNAUTHENTICATED`, `-32000` "Authentication required", or `logged_out` | The state "sign-in required" (FR-038) |
| 429 `RESOURCE_EXHAUSTED`, or 402 | Quota wait (FR-036) |
| The stop reason `refusal`, with no quota text in an error channel | acp2api sends `/usage` one time. When it shows quota for the model of the request, the response ends as a refusal (FR-030). Else: quota wait. |
| 400, 5xx, a timeout, or an exit of the official runtime | No lockout. FR-033 gives the error. |
| A built-in tool runs with no permission request | No lockout (FR-047) |

A lockout signal starts a lockout in each state of acp2api.

**Pass:** an error with each row gives the result of the row. A normal reply that holds the sentence about the Terms of Service gives no lockout.

#### FR-035 — Lockout

During a lockout, acp2api sends no request to the official runtime.

- At the start, acp2api cancels each prompt turn that runs, refuses each request in the queue, and stops the runtime process.
- acp2api refuses each later request with the notice M-12 (FR-033).
- The response that started the lockout also holds the original error text of Google, because that text can hold the link to an appeal.
- acp2api shows the notice in one terminal line, and in the dark first tile of the management panel.
- During a lockout, acp2api starts no runtime process, sends no `/usage`, and the management panel has no sign-in entry.
- acp2api writes the lockout and its evidence into its state file: the signal, the time, and the original error text, with the redaction of FR-056.
- A restart of acp2api does not end a lockout. acp2api reads the state file at each start.
  - When the state file exists, and acp2api cannot read it, or the file is damaged, acp2api is in lockout.
  - When no state file exists, acp2api is not in lockout, and it makes a new state file.
- Only the owner ends a lockout. The management panel shows the evidence, the owner enters the management password, and the lockout ends at once. No timer and no automatic step end it.
- After the end, acp2api sends no test request. The next lockout signal starts a new lockout.
- When the owner deletes the state file, the lockout ends at the next start of acp2api. acp2api does not try to stop this act.

**Pass:**

- After a lockout signal, the official runtime gets no request, and no runtime process runs.
- Each request gets M-12, and the first response also holds the original text.
- After a restart, acp2api is still in lockout, and a damaged state file gives a lockout.
- After a deletion of the state file and a restart, acp2api is not in lockout.
- After the password in the management panel, lockout ends. The next request runs when no other condition stops it.

#### FR-036 — Quota wait

After a quota signal, acp2api starts no prompt turn until the reset time. With no condition of higher priority, it refuses each request with the 429 of FR-033 and message M-16. The wait ends without the owner.

- acp2api reads the reset time from the error text. Only when the error text has none, acp2api sends `/usage` for it.
- When neither gives a reset time, the quota wait lasts 60 minutes.
- After the wait, the next request runs when no other condition stops it. A new quota signal starts a new quota wait.

**Pass:** with no other condition, a quota signal whose reset time is 2 minutes away gives HTTP 429 with `retry-after` up to 120. The first request at the reset time runs. With no reset time anywhere, the wait lasts 60 minutes.

#### FR-037 — Requests in the queue at the start of a state

A quota wait, the state "sign-in required", and the state "runtime unavailable" end the requests in the queue at their start. Each such request gets the error of that state. The prompt turns that run continue to their end. Only a lockout cancels them (FR-035).

This includes a request before acceptance: it gets an HTTP error, not a stream event. An accepted streamed request gets a stream error. An error for a condition that is already active does not flush the queue again. When conditions overlap, the priority above selects the error.

**Pass:** two requests wait in the queue, and one prompt turn runs. A quota signal then ends the two requests with 429, and the prompt turn ends normally.

#### FR-038 — Sign-in required

The official runtime has no valid sign-in: the owner never signed in, or the official runtime signed out.

- acp2api starts no prompt turn. It refuses each request with message M-13, which gives the address of the sign-in entry.
- For each API request that arrives in this state, acp2api checks the sign-in one time before acceptance. This also applies during a quota wait.
- A check waits for a place in the queue when all places are occupied. Each such request keeps its own required check.
- A failed check gives HTTP 400 and M-13. A successful check clears sign-in required, then acp2api checks each remaining admission condition.
- A successful sign-in also clears only sign-in required. An unexpired quota wait remains.
- acp2api keeps no record of sign-in required, and it checks the sign-in again at each start.
- At the start, the check uses one `session/new` after the runtime process becomes available. During a lockout, no check occurs (FR-035).
- A successful check gives the model names of FR-012. A sign-in error gives the state "sign-in required" before any API request.

**Pass:** a start with no sign-in gives the state "sign-in required" before any API request. Each failed check then gives M-13, and the official runtime gets no prompt. A successful check gives the model names. After a sign-in, the next request runs when no other condition stops it.

#### FR-039 — Sign-in entry

- The management panel has one sign-in entry. It uses only the sign-in method `oauth-personal`.
- The official runtime opens the browser and keeps the token. acp2api never sees the token.
- While acp2api waits, the management panel shows the time left, the sign-in address from the official runtime, and a cancel entry.
- acp2api waits up to 10 minutes. After that time or a cancel, acp2api replaces the runtime process, so that the official runtime stops its wait. This replacement is not a failure of FR-041.
- The owner signs in through the management panel one time, also when an editor signed in before.
- The management panel has no sign-in entry during a lockout.

**Pass:** the sign-in entry gives a sign-in with `oauth-personal`, and the management panel shows the address and the time left. A cancel ends the wait, and the state stays "sign-in required". No file of acp2api holds the token.

#### FR-040 — Runtime unavailable

acp2api has no runtime process that it can use, so it starts no prompt turn. It refuses each request with message M-14, which names the reason and lists the accepted versions. The reasons are:

- acp2api found no executable of the official runtime;
- acp2api found only builds that are not accepted versions;
- the runtime process had 3 failures in 10 minutes (FR-041).

The condition ends when a start of a runtime process succeeds. Other conditions remain until their own end. In this state, acp2api starts no runtime process by itself. These events start a new try:

- the retry entry of the management panel;
- a restart of acp2api;
- a new path, or a new search;
- a passed compatibility check.

**Pass:** each reason gives the state and M-14 with that reason. After the retry entry and a successful start, the next request runs when no other condition stops it.

### Official runtime

#### FR-041 — Runtime process

- acp2api starts one runtime process at its own start. During a lockout, it starts none.
- All ACP sessions and all prompt turns use this one runtime process. acp2api does not stop it when no request comes.
- When the runtime process holds N ACP sessions, and a request needs one more, acp2api keeps each new request in the queue. When no prompt turn runs, it replaces the runtime process. A measurement decides N (OD-1). N is a constant, not a setting.
- These events are failures:
  - a start with no answer in 5 minutes;
  - an exit that acp2api did not cause;
  - a prompt turn that does not end 30 s after its cancel.
- After a failure, each prompt turn of the runtime process ends with the 503 row of FR-033 and message M-20. acp2api starts a new runtime process at once. After 3 failures in 10 minutes, acp2api goes into the state "runtime unavailable" (FR-040).

**Pass:**

- After a start, one runtime process runs, and two prompt turns at the same time use it.
- With N ACP sessions, the next request that needs a new ACP session waits, then gets its response from a new runtime process.
- Three exits of the fake ACP agent in 10 minutes give the state "runtime unavailable".

#### FR-042 — Search for the official runtime

The owner unpacks the official runtime from the ACP Registry, or an editor installs it. acp2api never downloads it.

- acp2api searches at each start, after the start of the runtime process. When it has no copy of an accepted version, it searches before that start.
- It searches these places:
  - the folder of the acp2api executable;
  - `PATH`;
  - the install folders of the editors that install the official runtime from the ACP Registry, for example Zed and JetBrains IDEs.
- The management panel has an entry that starts a new search, and an entry for the path of `agy_acp_server`.
- A build needs `localharness_external` in the folder of `agy_acp_server`. The search ignores a folder without it. For such a path, the management panel shows the row "Path without localharness_external" of "Texts of the management panel", and acp2api keeps its earlier path.

**Pass:** with a build in an install folder of Zed, a start finds it. A path that the owner gives finds the build. With no build anywhere, acp2api goes into the state "runtime unavailable" with the first reason of FR-040.

#### FR-043 — Accepted versions and the copy

- Each release of acp2api holds a list of the builds that the acceptance test passed, with their versions. A build is the pair of SHA-256 values of its two executables (ADR-0006).
- Each executable of acp2api accepts only builds for its own operating system and CPU architecture.
- acp2api starts each runtime process only from its own copy in the data folder, and only when the copy is an accepted version. It keeps one copy.
- When acp2api has no copy that is an accepted version, it searches. Then it copies the found accepted version with the highest version, with no step of the owner. A changed copy is not an accepted version, and it counts as a found build.
- When acp2api has a copy, it never changes the build by itself. The management panel shows "new build found" only for a found build with a higher version than the build in use. An older build shows only in the result of the search.
- The owner can change to another accepted build in the management panel. acp2api then replaces the runtime process.
- A build that is not on the list gets a copy only after the owner starts its compatibility check.

**Pass:**

- A changed byte in the copy stops the start. With an accepted build in the Zed folder, acp2api copies it and starts. With no other build, acp2api goes into the state "runtime unavailable" with the second reason of FR-040.
- A new build in the Zed folder shows "new build found", and the build in use does not change.
- A build for another CPU architecture is not accepted.

#### FR-044 — Compatibility check

For a build that is not an accepted version, the owner can start a compatibility check in the management panel.

- Before the start, the management panel shows the cost: 3 to 4 model requests through the Google account of the owner.
- It also shows the risks that the check does not check:
  - the stderr lines of the lockout signals;
  - three prompt turns at the same time;
  - the memory that decides N.
- The check runs as a replacement of the runtime process. New requests wait in the queue.
- The check passes when the build gives each fact that acp2api needs ([#22], decision 7):
  - ACP version 1, the system prompt addition, a load of an ACP session, MCP over HTTP, and the method `oauth-personal`;
  - the tool filter, the title `Run <tool>?`, and cancel at permission, with no built-in tool that runs (ADR-0002);
  - a reply that holds a codeword of the system prompt and a tool result that went as text;
  - the files of the official runtime only in the runtime home of acp2api (ADR-0005).
- After a pass, acp2api writes the two hashes and the version into its state file. The build is then an accepted version on that computer. The management panel shows it as "checked on this computer", apart from a build on the list.
- After a failure, acp2api starts the earlier accepted version again.

**Pass:** with a fake ACP agent that has all facts, the check passes, and the build gets the label "checked on this computer". With a fake ACP agent that runs a built-in tool, the check fails, and the earlier build runs again.

#### FR-045 — Exit of acp2api

At its exit, acp2api cancels each prompt turn that runs. It ends the response of each such prompt turn, and each request in the queue, with the error of the exit and message M-15. Then it stops the runtime process with all its child processes. After a crash of acp2api, the runtime process stops too (ADR-0004).

**Pass:** a request with a prompt turn that runs, and a request in the queue, both get M-15 at the exit. After the exit, the runtime process and all its child processes have ended. After a kill of acp2api, they end without a step of the owner.

### Safety toward Google and the official runtime

#### FR-046 — Only through the official runtime

- acp2api sends no request to Google. Each model request and each sign-in goes through the official runtime (ADR-0007).
- acp2api never reads, changes, or deletes the sign-in files, the tokens, or the keychain entries of the official runtime.
- acp2api runs the official runtime in its own runtime home in the data folder. Thus no global configuration of the owner reaches an ACP session, for example an MCP server, a hook, or a skill in `~/.gemini`.

**Pass:** in the tests with a fake ACP agent, acp2api opens network connections only on 127.0.0.1. The acceptance test finds no read of `~/.gemini` by the official runtime.

#### FR-047 — Pure model

- acp2api declares no file system capability and no terminal capability to the official runtime.
- acp2api allows no permission request.
- At the permission request of a client tool, acp2api cancels the prompt turn and returns the tool call (FR-022).
- At each other permission request, acp2api refuses that one request. The prompt turn continues, and acp2api writes a log line.
- When a tool that is not a client tool runs with no permission request, acp2api cancels the prompt turn. It ends the response with the 502 row of FR-033 and message M-22.
  - The management panel shows a notice until the owner dismisses it, and the log has a line.
  - acp2api reports each such event one time, and it goes into no new state.
- The `cwd` of each ACP session is a new empty folder in the system temporary folder, not a project of the owner.

**Pass:** the official runtime gets no file system or terminal capability and no allow. A built-in tool with a permission request gets a refusal, and the reply continues. A built-in tool with no permission request gives M-22, one notice, and one log line.

#### FR-048 — Identity toward the official runtime

acp2api gives the official runtime the client name `acp2api`, the title `acp2api`, and the version of acp2api. The name of an editor or of an API client never goes there. No setting changes these values.

**Pass:** the official runtime gets `"clientInfo": {"name": "acp2api", "title": "acp2api", "version": "<version>"}` at each start.

### Concurrency and queue

#### FR-049 — Places

- The default concurrency limit is the smaller of 2 and the platform maximum. The management panel offers 1 through that maximum.
- A change applies at once. A lower limit does not cancel work that already holds a place.
- New work obtains a place only when the occupied count is below the new limit.
- A request holds a place from its first call to the official runtime to the end of its prompt turn.
- In the state "sign-in required", the sign-in check of a request holds a place.
- When no prompt starts, the place remains occupied until that request's outstanding ACP call ends or the runtime process ends.
- Each `/usage` holds a place.
- The start of a runtime process holds no place.
- A platform can fail the acceptance test with 3 prompt turns at the same time. Then the highest value on that platform is the highest value that passed. The release notes name it, and the management panel of that platform offers no higher value. A higher value in the configuration file stops the start with message M-26.
- The platform maximum must be at least 1. An arm64 release passes SC-002 at its declared maximum, even when that maximum is below 3.

**Pass:** with the limit 2 and 3 requests, the official runtime gets no call for the third request until one of the first two ends. A change to 3 lets the third request start at once.

#### FR-050 — Queue

- The other requests wait in the queue, in the order of arrival.
- Requests before a required sign-in check and accepted requests share one queue and its cap. Before acceptance, a request sends no keep-alive event.
- A request waits with no time limit while its API client stays connected.
- The queue holds no more than 32 requests. acp2api refuses each request after that at once, with the full-queue row of FR-033 and message M-17.

**Pass:** requests start in the order of arrival. With 32 requests in the queue, the next request gets 529 or 503 with `retry-after: 10`.

#### FR-051 — A prompt turn with no update

When a prompt turn gets no update for 5 minutes, acp2api cancels it. It ends the response with the 503 row of FR-033 and message M-19.

**Pass:** a fake prompt turn with no update for 5 minutes ends with that error, and the official runtime gets a cancel.

### Quota

#### FR-052 — Quota queries

acp2api sends `/usage` to the official runtime only in three cases:

- for the reset time of a quota wait, when the error text has none (FR-036);
- one time after the stop reason `refusal` (FR-034);
- when the owner refreshes the quota in the management panel.

acp2api refuses a second refresh in 60 s. The management panel shows the last result and its time. An open management panel and an idle acp2api send no `/usage`, and acp2api sends none during a lockout.

**Pass:** an hour of an open management panel with no refresh gives no `/usage`. Two refreshes in 60 s give one `/usage`.

### Management panel

#### FR-053 — Actions

The management panel is the only place for each action of the owner:

- the sign-in, and the cancel of a sign-in;
- a new search, the path of `agy_acp_server`, the compatibility check, and a change of the build;
- the retry entry in the state "runtime unavailable";
- the end of a lockout;
- a new local API key, a change of the management password, and a change of the concurrency limit;
- a refresh of the quota, a read of the model names, and the dismissal of a notice.

**Pass:** each action above works in the management panel, and no CLI command does any of them.

#### FR-054 — Content

The management panel is one long page of tiles, the layout of variant A of the prototype ([#18]):

1. A black global bar with the language switch and the exit of the panel. Under it, a frosted bar with the anchors of the sections and the main action of the current state.
2. The first tile: the state in large type, its sentence, its action, and three figures.
   - The figures are the places in use, the requests in the queue, and the build in use.
   - The first tile is dark only for "sign-in required", "runtime unavailable", and lockout. Quota wait uses a light tile.
   - In a lockout, it also shows the evidence and the entry that ends the lockout.
3. A band of notices, only when a notice exists.
4. The tile "official runtime":
   - the build in use with its version, its label, and its source;
   - the runtime process, the result of the search, and a new build;
   - the entries of FR-042 to FR-044.
5. The tile "quota", with the places and the queue: the last result of `/usage` and its time, the refresh entry, and the concurrency limit.
6. The tile "setup": the address, the local API key with a copy entry and a new-key entry, the model names, and the setup text.
7. The tile "logs":
   - the last 50 requests with their metadata, from memory;
   - a table of the unknown field names since the start, with the API, the count, and the last time;
   - the log folder.
8. The tile "settings": the concurrency limit, the change of the management password, and the read-only port and data folder.
   - The owner changes the port through the configuration file or `--port`.
   - The owner changes the data folder through `--data-dir`.
9. A footer with the version of acp2api.

Model names: after their first read, the tile "setup" shows them. Before that read, an entry reads them now. That read holds one place, and it makes one ACP session.

The setup text has a placeholder for the local API key, and the owner copies the key separately:

- Claude Code: `ANTHROPIC_BASE_URL` with the address, `ANTHROPIC_AUTH_TOKEN`, and a model name in `ANTHROPIC_MODEL`, in each `ANTHROPIC_DEFAULT_*_MODEL`, and in `CLAUDE_CODE_SUBAGENT_MODEL`. `CLAUDE_CODE_MAX_CONTEXT_TOKENS` holds the window.
- Codex: a model name in `model`, the window in `model_context_window`, and a provider with the base URL `<address>/v1` and `wire_api = "responses"`.
- A Chat Completions client: the base URL `<address>/v1` and the local API key.
- The window is the context size that the official runtime reported last for the model, for example 1,048,576 for `gemini-3.8-flash` (assumption A-3).

The management panel shows no email address of the Google account. The README, not the management panel, lists the request fields that acp2api ignores. A view of the management panel sends no request to the official runtime.

**Pass:** the page has the items above in each of these six cases:

- the normal state, and the normal state with a new build;
- sign-in required, and quota wait;
- runtime unavailable, and lockout.

Also:

- A view of the page gives no call to the official runtime.
- No page holds the email address of the account.
- The README lists each Ignore row of FR-014.

#### FR-055 — Language and look

- The management panel has Chinese and English. A browser language that starts with `zh` selects Chinese, and each other browser language selects English. The owner can change the language.
- The section "Texts of the management panel" gives the texts of the states, the actions, and the labels. For each other text, the management panel uses the terms of `GLOSSARY.md`: the column "Term" in English and the column "Chinese term" in Chinese.
- The management panel follows the section "Visual rules of the management panel".
- It loads no font and no other file from an external address.
- v1 has only the light mode.

**Pass:**

- A browser with the language `zh-CN` shows the Chinese texts. A browser with `en-US` or `fr-FR` shows the English texts. The switch changes the language.
- A load of the page gives requests only to `127.0.0.1:<port>`.
- A check of the styles of the page finds the values of "Visual rules of the management panel":
  - each control in `#0066cc`;
  - no shadow, and no web font;
  - each status color only on a dot of 9 px.

### Logs

#### FR-056 — Log content and personal data

A log holds only metadata:

- one line for each request, with these fields:
  - the time, the request ID, and the API;
  - the model name, and a short ID of the ACP session;
  - `new`, `continue`, or `rebuild`;
  - the duration, the usage, and the result;
  - the error code, and the unknown field names;
- one line for each change of state;
- the lines that other FRs name, for example for a rebuild.

For a refused request, the log line has the model name when the body parses and `model` is a string. Otherwise, it has `model: null`.

No log line, no terminal line, and no state file holds these items:

- a message text, the system prompt, a tool argument, or a tool result;
- the local API key, or the management password.

The only exception is the terminal at a start that makes a new key or password (FR-004, FR-005). It shows only the new values.

For the error texts of the official runtime:

- acp2api does not copy the stderr of the official runtime into its log. It writes only the parsed error line: the code, the status, and the message.
- Before acp2api writes an error text into the log or into the state file, it replaces each email address with `<REDACTED>`. This applies to the `@` form and the `%40` form. It also replaces each value with the form of a token:
  - a value after `Bearer `;
  - a run of 32 or more characters from the set `A-Z a-z 0-9 - _ . + / =`.
- v1 has no setting for a log of the content of the requests.

**Pass:** a test request with a marker string in each content field leaves no marker in the logs, the terminal, or the state file. A stderr line with `name%40example.com` gives `<REDACTED>` in the log.

#### FR-057 — Log size

The logs have a total size cap of 50 MB, and acp2api deletes the oldest log file first. Each log file has a size cap of 10 MB. At each start, acp2api deletes each log file that is older than 30 days.

The age of a log file is the time since its last modification.

**Pass:** a run that writes 60 MB of log lines leaves no more than 50 MB of logs, and no log file over 10 MB. A log file whose last modification was 31 days ago is gone after a start.

### Message texts

`<…>` marks a value that acp2api fills in. FR-024 gives the texts for the server tools and the tool types.

| ID | Case | Text |
|---|---|---|
| M-01 | An absent or wrong local API key | `acp2api: the local API key is absent or wrong.` |
| M-02 | A rejected field | `acp2api: the request field <field> is not supported.` `<field>` is the path of the field. When a value turned the feature on, ` = <value>` follows in JSON, for example `n = 2`, `tool_choice = "required"`, or `response_format.type = "json_schema"`. |
| M-03 | A rejected content block | `acp2api: the content type <type> is not supported.` |
| M-04 | A rejected media type | `acp2api: the media type <media type> is not supported.` |
| M-05 | An image or a file with a URL | `acp2api: the content type <type> with a URL is not supported. Send the data as base64.` |
| M-06 | A PDF in a message | `acp2api: a PDF document in a message (content type <type>) is not supported. Remove that message from the conversation.` |
| M-07 | A body that is not JSON, or that is too deep | `acp2api: the request body is not valid JSON, or it has more than 128 levels.` |
| M-08 | A model name that acp2api does not offer | `acp2api: the model name <model> is not available. The model names are: <names>.` |
| M-09 | A `tool_search` tool | `acp2api: the tool tool_search is not supported.` |
| M-10 | A client tool with the name of a built-in tool | `acp2api: the client tool name <name> is not supported, because a built-in tool of the official runtime has this name.` |
| M-11 | The `web_search` tool of Codex | `acp2api: the server tool web_search is not supported. To stop these calls, set web_search = "disabled" in the Codex configuration.` |
| M-12 | A lockout | `acp2api: lockout since <start time> after <signal>; acp2api sends no request to the official runtime until the owner ends the lockout at <address of the management panel>.` The response that started the lockout adds ` Original error: <original error text>`. |
| M-13 | The state "sign-in required" | `acp2api: the official runtime is not signed in. Sign in at <address of the sign-in entry>.` |
| M-14 | The state "runtime unavailable" | `acp2api: the official runtime is unavailable: <reason>. Accepted versions: <versions>. See <address of the management panel>.` |
| M-15 | The exit of acp2api | `acp2api: acp2api stopped before the end of this request.` |
| M-16 | A quota wait | `acp2api: the Google quota is used up until <reset time>. acp2api starts no prompt turn before that time.` |
| M-17 | A full queue | `acp2api: the queue is full (32 requests). Send the request again later.` |
| M-18 | An upstream error | With an HTTP error line: `acp2api: upstream error <HTTP code> <status>: <message>`. Without one: `acp2api: upstream ACP error <code>: <message>`, with the JSON-RPC values. |
| M-19 | A prompt turn with no update for 5 minutes | `acp2api: the official runtime sent no update for 5 minutes, so acp2api cancelled the prompt turn.` |
| M-20 | A failure of the runtime process | `acp2api: the runtime process failed, so acp2api ended the prompt turn.` |
| M-21 | The stop reason `max_turn_requests` | `acp2api: the prompt turn reached the limit of model requests of the official runtime.` |
| M-22 | A built-in tool with no permission request | `acp2api: the built-in tool <name> of the official runtime ran with no permission request, so acp2api cancelled the prompt turn.` |
| M-23 | A request in a retry wait | `acp2api: a request of this conversation failed less than 30 s ago. Send the request again in <seconds> s. The error was: <error>` `<error>` is the earlier message without its start `acp2api: `. |
| M-24 | A second start for the same computer account | `acp2api: another acp2api runs for this computer account. Its management panel is at <address>.` |
| M-25 | A port in use | `acp2api: the port <port> is in use. Set another port in the configuration file or with --port.` |
| M-26 | A setting with a wrong value, an unknown key, or a TOML syntax error | `acp2api: the setting <setting> in <path of the configuration file> has a value that acp2api cannot use.` `<setting>` is the key. For a syntax error with no identifiable key, it is `at line <n>`, with a one-based line number. |
| M-27 | A last item from the assistant (a prefill) | `acp2api: a last message from the assistant (a prefill) is not supported.` |
| M-28 | Unreadable or damaged account coordination data | `acp2api: cannot use the account coordination data at <path>.` |

The reasons of M-14 are `acp2api found no executable of the official runtime`, `no build that acp2api found is an accepted version`, and `the runtime process failed 3 times in 10 minutes`.

### Texts of the management panel

`<…>` marks a value that the management panel fills in.

| Item | Chinese | English |
|---|---|---|
| Normal state | 运行中 | Running |
| Sentence of the normal state | 官方运行时已就绪。请求正常转发。 | The official runtime is ready. acp2api forwards the requests. |
| State "sign-in required" | 需要登录 | Sign-in required |
| Sentence of "sign-in required" | 官方运行时没有有效的登录。登录之前，acp2api 拒绝每个请求。 | The official runtime has no valid sign-in. Until a sign-in, acp2api refuses each request. |
| Sign-in entry | 用 Google 账户登录 | Sign in with Google |
| Sign-in wait | 正在等待浏览器完成登录，还剩 <time>。如果浏览器没有打开，复制这个地址：<address> | acp2api waits for the sign-in in the browser: <time> left. If no browser opened, copy this address: <address> |
| Cancel of the sign-in | 取消登录 | Cancel the sign-in |
| State "quota wait" | 配额等待 | Quota wait |
| Sentence of "quota wait" | Google 配额已用完。<reset time> 之前，acp2api 不启动 prompt 轮次，在本地拒绝请求。到时间后自动结束配额等待。 | The Google quota is used up. Until <reset time>, acp2api starts no prompt turn and refuses each request. The quota wait then ends automatically. |
| State "runtime unavailable" | 运行时不可用 | Runtime unavailable |
| Sentence: no executable | 没有找到官方运行时，所以 acp2api 没有启动运行时进程。 | acp2api found no official runtime, so it started no runtime process. |
| Sentence: no accepted version | 找到的构建不是已接受版本，所以 acp2api 没有启动运行时进程。 | No build that acp2api found is an accepted version, so it started no runtime process. |
| Sentence: 3 failures | 运行时进程在 10 分钟内失败了 3 次，所以 acp2api 不再自动启动它。 | The runtime process failed 3 times in 10 minutes, so acp2api does not start it again by itself. |
| Retry entry | 重试 | Retry |
| Compatibility check entry | 对 <version> 做兼容性检查 | Run the compatibility check for <version> |
| State "lockout" | 已锁死 | Lockout |
| Sentence of "lockout" | <time>，官方运行时返回了一个可能危及 Google 账户的响应。从那时起，acp2api 不向官方运行时发送任何请求。 | At <time>, the official runtime gave a response that shows a risk to the Google account. Since then, acp2api sends no request to the official runtime. |
| Entry that ends a lockout | 输入管理密码，结束锁死 | End the lockout with the management password |
| Notice of a built-in tool | 内置工具 <name> 在没有权限请求的情况下运行。acp2api 取消了这个 prompt 轮次，并给 API 客户端返回了错误。 | The built-in tool <name> ran with no permission request. acp2api cancelled the prompt turn and sent an error to the API client. |
| New build | 发现新构建 <version> | New build found: <version> |
| Label of a build on the list | 发布列表 | On the release list |
| Label of a build that a compatibility check passed | 本机已检查 | Checked on this computer |
| Refresh of the quota | 刷新配额 | Refresh the quota |
| New search | 重新搜索 | Search again |
| Path entry | 填写 agy_acp_server 的路径 | Set the path of agy_acp_server |
| Path without localharness_external | 这个文件夹里没有 localharness_external，acp2api 没有使用这个路径。 | This folder has no localharness_external, so acp2api did not use this path. |
| Read of the model names | 现在读取模型名 | Read the model names now |
| New local API key | 换一个新密钥 | Make a new key |
| Change of the management password | 修改管理密码 | Change the management password |
| Dismissal of a notice | 知道了 | Dismiss |
| Sections | 官方运行时、配额、名额与队列、接入、日志、设置、通知 | Official runtime, Quota, Places and queue, Setup, Logs, Settings, Notices |

## Success criteria

SC-001 to SC-005 are the release gate. A release needs a pass of all five ([#17]).

- **SC-001 — Tests with a fake ACP agent.** The tests start acp2api with a fake ACP agent in place of the official runtime. They send real HTTP requests as a fake API client. They cover each FR that needs no Google account. While they run, a guard refuses each executable except the fake ACP agent. **Pass:** at the commit of the release, CI passed the tests on `macos-15`, `macos-15-intel`, `windows-latest`, and `windows-11-arm`. CI starts when the repository is public, so no release comes before that time. Until then, the maintainer runs the tests on the Mac.
- **SC-002 — Acceptance test of each build.** A test program runs each build with a Google account of the maintainer. It runs on the Mac and in the Windows VM. It first runs the real compatibility check, and it writes the SHA-256 values for the list. It obeys the safety rules of `docs/research/2026-10-18-probe-methods-and-safety-rules.md`. The facts that it shows:
  - the tool filter, the full tool schemas with no file read, and the title `Run <tool>?`;
  - prompt turns at the platform maximum of FR-049, and the memory of the runtime process and each harness process (arm64 only);
  - on macOS, the stop of the harness processes after a crash of acp2api;
  - the system prompt addition with a system prompt of the size of the one of Claude Code;
  - no read of `~/.gemini` and of the parent folders of a `cwd`, and the files of the official runtime only in the runtime home;
  - the size of a session file, and a start of the copy of the two executables;
  - an ACP session after a sign-in in the same runtime process;
  - the options `model` and `thinking_level`, and the model ID of each thinking level;
  - the error text as the last chunk, with the stderr error line in the wait of 200 ms, on macOS and on Windows;
  - a second error of an ACP session after the error of a bad image.

  **Pass:** each build on the list of the release passed its set. An arm64 build passed the full set, with no more than 25 model requests on each platform. An x64 build passed, under emulation, the compatibility check and the checks with no model request, with no more than 5 model requests. Before the first prompt, the test program showed the planned count, and the maintainer approved it.
- **SC-003 — Client checks with a fake ACP agent.** On the Mac, real Claude Code and real Codex run against acp2api with the fake ACP agent. They run in the sandbox of the earlier checks ([#27], [#33]). The official OpenAI SDK sends one request with a stream and one request with no stream to the Chat Completions API. The checks show:
  - the first text of the attribution block of Claude Code;
  - the `Input tag` message, and the advisor tool with a model name of acp2api;
  - the keep-alive events in a long pause;
  - `output_config.effort` in the requests of Claude Code, and the change of the thinking level after `/effort`;
  - with a lockout in a synthetic state file, and in the state "sign-in required", both API clients stop with no retry and show the message;
  - a temporary upstream error gives a retry after 30 s, not before;
  - the next requests of Claude Code after an `error` event in print mode, in interactive mode, and in a subagent, with the retry wait;
  - a request of Claude Code with no stream that takes more than 300 s.

  **Pass:** each check passes with the versions of Claude Code and Codex that the release names. These are the newest versions at the time of the release. No request goes to Google, Anthropic, or OpenAI.
- **SC-004 — A short task with the real official runtime.** A fixed task in a small test repository needs at least 5 client tool calls: a read of a file, an edit, and a command. In the middle of the task, the maintainer restarts acp2api, so that the task does a rebuild. Codex edits a file with an `apply_patch` command through its `exec_command` tool. **Pass:** in each run, the result files are the same as the expected files, and no reply text holds a tool call as text. A fixed list of patterns finds such text. Each release has 1 run for each of Claude Code and Codex on the Mac, and 1 run for each in the Windows VM. The first release has 3 runs for each API client on the Mac.
- **SC-005 — Release checklist.** The maintainer does the manual steps with the archives of the release candidate, on the Mac and in the Windows VM:
  - the install from the archive, the first start, the management panel, and the sign-in;
  - the search for the official runtime that Zed installed;
  - the stop and a restart.

  Only when a JetBrains IDE can install the official runtime from the ACP Registry with no paid plan, the checklist gets a step for it. Else the release notes say that the search of the JetBrains folders depends on the reports of owners. **Pass:** each step passed on both platforms.
- **SC-006 — Little memory and little disk space.** acp2api uses little memory and little disk space ([#12]). The known values: the copy of the official runtime takes about 400 MB on macOS and about 220 MB on Windows arm64 (ADR-0006). That copy is outside the control of acp2api. **Pass:** measurement gap. The population, the targets, and the initial values of the memory of acp2api and of the size of its executable are open (OD-2). The evidence source is a measurement of the first build.

## Assumptions

- **A-1 (FR-025).** When a prompt turn ends with no usage update, the input tokens are the local estimate of the input tokens, the value of `message_start`. Such a prompt turn occurred on Windows after cancel at permission ([#16], W6). The local estimate is a value of [#20], decision 4.
- **A-2 — Retired:** FR-004 defines the result of an invalid configuration file.
- **A-3 (FR-054).** Before the official runtime reports a context size, the setup text has a placeholder for the window. [#13] takes the window from the context size that the official runtime reports.

## Open decisions

- **OD-1 — The memory budget for N.** It affects FR-041 and SC-002. N is the number of ACP sessions after which acp2api replaces the runtime process (ADR-0004). The acceptance test on arm64 measures the memory of the runtime process and of each harness process. Then the maintainer decides the budget, and the budget gives N. N is a constant of acp2api, so a later release can change it.
- **OD-2 — The targets of SC-006.** The maintainer decides the limits of the memory and of the executable size after the first build and its measurement ([#12]).

## User journey

1. The owner unpacks the official runtime from the ACP Registry, or an editor installs it.
2. The owner unpacks acp2api and starts it. The terminal shows the address, the local API key, and the management password, and the management panel opens.
3. acp2api searches for the official runtime, and it copies a build that is on its list. With no such build, the state is "runtime unavailable". The owner then gives a path, or starts a compatibility check.
4. The state is "sign-in required". The owner signs in through the management panel, also when an editor signed in before.
5. The owner copies the setup text and the local API key into each API client. Then the API clients send requests.

## Product constraints

### Platforms

v1 supports Windows and macOS, with the four targets of FR-001. The only Windows computer of the project is a VM on an arm64 Mac. Thus the x64 builds run only under emulation before a release ([#17]).

### Visual rules of the management panel

The management panel uses these parts of the Apple design analysis `DESIGN.md` that the maintainer gave ([#18], decision 11):

| Rule | Value |
|---|---|
| Color of each control | Action Blue `#0066cc`. No other color for a control. |
| Surfaces | `#ffffff`, `#f5f5f7`, and `#272729` |
| Ink | `#1d1d1f` |
| Shapes | Pill buttons and pill inputs. The radii are 8 px, 18 px, and pill. |
| Depth | No shadow. The bar under the global bar is frosted, with a backdrop blur. |
| Type scale | 56 px for the state, then 40, 34, 28, 21, 17, 14, and 12 px for fine print |

The changes to `DESIGN.md`:

- A status color is only on a dot of 9 px, and text is always next to it. No control and no background has a status color.
- The status colors are good `#0ca30c`, warning `#fab219`, serious `#ec835a`, and critical `#d03b3b`.
- The page uses only the system fonts. Chinese text has no negative letter spacing.
- The first tile is dark only in sign-in required, runtime unavailable, and lockout. Quota wait uses a light tile.
- The forms, the error lines, and the tables use the hairlines, the pill inputs, and the type sizes above.

[#1]: https://github.com/yandhi-0421/acp2api/issues/1
[#2]: https://github.com/yandhi-0421/acp2api/issues/2
[#4]: https://github.com/yandhi-0421/acp2api/issues/4
[#5]: https://github.com/yandhi-0421/acp2api/issues/5
[#7]: https://github.com/yandhi-0421/acp2api/issues/7
[#12]: https://github.com/yandhi-0421/acp2api/issues/12
[#13]: https://github.com/yandhi-0421/acp2api/issues/13
[#16]: https://github.com/yandhi-0421/acp2api/issues/16
[#17]: https://github.com/yandhi-0421/acp2api/issues/17
[#18]: https://github.com/yandhi-0421/acp2api/issues/18
[#20]: https://github.com/yandhi-0421/acp2api/issues/20
[#22]: https://github.com/yandhi-0421/acp2api/issues/22
[#27]: https://github.com/yandhi-0421/acp2api/issues/27
[#33]: https://github.com/yandhi-0421/acp2api/issues/33
