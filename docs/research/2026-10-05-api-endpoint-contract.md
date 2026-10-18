# API endpoint contract of the earlier CLI bridge

Date checked: 2026-10-05 (Asia/Taipei).

Provenance: The maintainer wrote this note on 2026-10-05 for an earlier project, a bridge that used the Antigravity CLI (`agy`) as its engine. The note moved to acp2api on 2026-10-18, because that project will be deleted. The move removed the links to the files of that project and changed no fact. The move also added:

- an "acp2api:" line where acp2api decided differently, or where an open acp2api issue decides the point;
- the acp2api answers to the open questions;
- Appendix A.

Words of the earlier project:

- **the earlier CLI bridge**, **the earlier bridge**: that earlier project. Its recommendations and decisions in this note are not acp2api decisions.
- **the maintainer**: the old word was "operator".
- **API client**: the old word was "harness". In acp2api, a "harness process" is a different thing.
- **engine**: the Antigravity CLI (`agy`) in its headless mode, with stream-json input and output. In this note, the engine is never the official runtime.
- **engine turn**: the work of the engine for one input, up to its `result` event.
- **turn** (alone): the work of the earlier bridge for one API request, from the request to the end of its response.
- **step**, **planner step**: a step is one unit of the engine stream (`step_update`). A planner step, or planner response, is an `agent_response` step. It carries the usage of one model request.
- **Cascade**: a conversation of the engine. The earlier bridge kept one record for each Cascade.
- **Cascade rule**: the rule of the earlier bridge for each request. It continues a Cascade whose record matches the conversation of the request. Else, it does a rebuild. The old text cited this rule as "#5".
- **rebuild**: in the earlier bridge, a new Cascade whose first user message holds the full conversation as text.
- **pending tool call**: a `tools/call` of the engine that the earlier bridge kept open until the API client sent the tool result.
- **model ID**: an ID in the model list of the CLI (`agy models`). The earlier bridge took it as the `model` of a request. acp2api uses model names instead.
- **model group**: a set of models that share one quota in the CLI: the Gemini models, and the Claude and GPT models.
- **fail-stop**: the state of the earlier bridge after an account-level signal, for example a 403. Only the maintainer ended it. It is near to the acp2api lockout, but its signals differ.
- **quota stop**: the refusal of the earlier bridge for a model group whose quota use reached a threshold, or that gave a `quota_cap` signal. It is near to the acp2api quota wait.
- **engine error**: an error that the engine reported. acp2api says "upstream error" for an error of the official runtime.
- **server tool**: the old word was "hosted tool".

The other domain terms have the meanings that [GLOSSARY.md](../../GLOSSARY.md) gives.

Scope: the state rules of `/v1/responses`, the stream events, usage, and stop fields of the three API endpoints, and the error shapes and retry rules. This note used no engine request and no model request.

Two architecture decisions of the earlier bridge set its design. In the first, the engine is a real Antigravity client, and the earlier bridge only translates the protocol. In the second, the earlier bridge turns off the tools of the engine itself. It gives the client tools to the engine through its own MCP server. The Cascade rule: the earlier bridge continues a matched Cascade, and it does a rebuild on a mismatch.

## Conclusion

- The engine stream gives text deltas, tool calls, and token counts. It gives no thought text, no stop reason, and no length limit signal.
- Codex sends the full conversation in each request, with `store: false` and no `previous_response_id`. Thus the Cascade rule applies to Codex directly.
- Other API clients and SDKs of the Responses API can send `previous_response_id` or `item_reference` items. The earlier bridge refuses them, because it records no response content (a decision of the maintainer).
- The earlier bridge cannot make a real Anthropic `signature`. This note recommends the safe policy: send no `thinking` block, no `reasoning` item, and no `reasoning_content`.
- The earlier bridge must send heartbeat events. Claude Code shows a network banner after 20 s without data, and Codex and Claude Code stop a silent stream after 300 s.
- HTTP 400 is the only status that no studied API client or SDK retries automatically and that Codex shows with its message text. This note recommends it for the fail-stop and the quota stop of the earlier bridge.

## Sources

The agent read all sources on 2026-10-05. Code links use a fixed commit.

| Source | Version |
| --- | --- |
| [openai/codex](https://github.com/openai/codex/tree/823ea830c0fd418b09ff02d36cad9a1fff66465b) | commit `823ea830` |
| [openai/openai-python](https://github.com/openai/openai-python/tree/becc1d20eed83c1b8d85e15dc131a372d9dc7813) | commit `becc1d20`. The types come from the OpenAPI specification of the API. |
| [openai/openai-node](https://github.com/openai/openai-node/tree/fb6955621e1cf6653659adb75094ebace83ebfe9) | commit `fb695562` |
| [anthropics/anthropic-sdk-typescript](https://github.com/anthropics/anthropic-sdk-typescript/tree/d49bdab458000bcdffe77bd84b03293f31824fb3) | commit `d49bdab4` |
| [vercel/ai](https://github.com/vercel/ai/tree/0ca1ab97408d52e7973893cd7f368294b04d9a91) | commit `0ca1ab97`. OpenCode uses its packages. |
| [anomalyco/opencode](https://github.com/anomalyco/opencode/tree/907b3bc518fa48e90e8ec24dd327d13eee71c36c) | commit `907b3bc5` |
| OpenAI guides: [conversation state][oai-state], [error codes][oai-errors] | web pages |
| Claude API docs: [streaming][ant-stream], [errors][ant-errors] | web pages |
| Claude Code docs: [gateway protocol][cc-gw], [errors][cc-errors], [network configuration][cc-net], [model configuration][cc-model] | web pages |
| Antigravity CLI docs: [headless mode][agy-headless] | web page |
| Probes of the earlier bridge: a live probe of the CLI (2026-10-04), an MCP passthrough prototype (2026-10-05), and its run logs. acp2api has no copy of them. | probe sets A to E |

The platform.openai.com reference pages returned HTTP 403 to the fetch tool. Thus the OpenAI parameter facts come from the official SDK types. The SDK files say that they come from the OpenAPI specification.

## 1. What the engine stream gives

Sources: the [headless page][agy-headless], the raw output of set A (scratch file `multi-turn.stdout.ndjson`), and the set E run logs.

| Engine output | Fact |
| --- | --- |
| Text | `step_update.text_delta` on `agent_response` steps with `state: ACTIVE`. The last delta can arrive on the `DONE` step. |
| Thought text | None. The headless page lists no thought field. No probe output contains thought text. |
| Thought size | `usage.thinking_tokens` on each `agent_response` step with `state: DONE`. |
| Tool call | A `tools/call` request on the MCP server of the earlier bridge, and a `tool` step with `tool_name: call_mcp_tool`. |
| Usage of one model request | `usage` on the `agent_response` step with `state: DONE`: `input_tokens`, `output_tokens`, `thinking_tokens`, `cache_read_tokens`, `total_tokens`. |
| Usage in `result` | Cumulative for the full conversation. The headless page says that `usage`, `num_turns`, and `duration_seconds` "accumulate across the session". |
| End of an engine turn | One `result` event with `status` (`SUCCESS`, `ERROR`, `CANCELED`, `INTERRUPTED`, `INVALID`, `WAITING`, `RUNNING`) and `response`. |
| Stop reason, length limit, refusal | No field. The headless page documents none. |

The run logs confirm these relations:

- `total_tokens` is `input_tokens + output_tokens`. `output_tokens` contains `thinking_tokens`. Example: the reply `OK` had `output_tokens: 104` and `thinking_tokens: 103` (set A).
- `result.usage` is the sum of all step usage since the Cascade started, also over more than one process. Cascade `50974b23` had 3207 input tokens after E3. M1 added one step with 2248. The M1 result showed 5455 (in the run logs of the prototype: the E3 result, and the M1 step and result).
- `cache_read_tokens` was 0 in each probe. It is Not confirmed whether `input_tokens` contains `cache_read_tokens`.
- In E3, the `tools/call` arrived 12 ms before the `agent_response` step with its usage (lines 19 to 22 of the E3 log). Thus the usage of a planner response can arrive after its tool call.
- An engine turn that the earlier bridge stops with SIGINT ends with `status: ERROR` and `error: interrupted` (the live probe note of the earlier bridge, section "Cancellation").

Thus the earlier bridge can fill these fields truthfully: text, tool calls, the token counts of each model request, and an end of an engine turn. It cannot fill thought text, a signature, a length stop, a stop sequence, a refusal, or a content filter result.

## 2. `/v1/responses` state

### 2.1 Official behavior

| Parameter | Official behavior | Source |
| --- | --- | --- |
| `previous_response_id` | "The unique ID of the previous response to the model." It chains the responses of a conversation. "Cannot be used in conjunction with `conversation`." | [create params L148-L154][py-prev] |
| `store` | "Defaults to true when omitted." A stored response stays for at least 30 days. | [create params L248-L254][py-store] |
| `conversation` | "Items from this conversation are prepended to `input_items`." The API adds the input and output items to the conversation after the response. | [create params L58-L64][py-conv] |
| `instructions` | A system or developer message. With `previous_response_id`, "the instructions from a previous response will not be carried over". | [create params L100-L106][py-instr] |
| `include` | Adds output data. `reasoning.encrypted_content` adds an encrypted copy of the thought tokens to `reasoning` items, for stateless use with `store: false`. | [create params L66-L86][py-include] |
| `truncation` | `auto` drops items from the start of the conversation. `disabled` is the default, and an input that is too large fails with a 400 error. | [create params L323-L331][py-trunc] |
| `background` | Runs the response in the background. | [create params L49-L53][py-bg] |
| Cost of a chain | "Even when using `previous_response_id`, all previous input tokens for responses in the chain are billed as input tokens." | [conversation state guide][oai-state] |
| `reasoning` item | `encrypted_content` "is populated by default". The next request must use the value from `response.output_item.done`, because the value in `response.output_item.added` "may be incomplete". | [reasoning item L51-L61][py-rs-item] |

The official error for an unknown `previous_response_id` is Not confirmed.

### 2.2 What Codex sends

Codex supports only the Responses API for a `model_providers` entry. The value `wire_api = "chat"` gives an error ([`model-provider-info` L100-L131][cx-wire]).

| Topic | Codex behavior | Source |
| --- | --- | --- |
| Transport | HTTP and SSE. A `model_providers` entry has `supports_websockets: false` by default. Only the WebSocket request type has a `previous_response_id` field. | [`model-provider-info` L197-L199][cx-ws-flag], [request types L279-L365][cx-req-types], [client L11-L18][cx-client-doc] |
| State | `store: false` and `stream: true` in each request. The HTTP request has no `previous_response_id`. | [client L1002-L1018][cx-req] |
| Input | The full conversation in each request. | [client L904][cx-input] |
| `include` | Always `["reasoning.encrypted_content"]`. | [client L967][cx-include] |
| `instructions` | The base instructions of the model, in each request. | [client L942-L947][cx-instr] |
| `prompt_cache_key` | The Codex session ID. | [client L584-L596][cx-pck], [client L984][cx-pck2] |
| Headers | `session-id`, `thread-id`, `x-client-request-id`, and `x-openai-subagent` for a subagent. | [endpoint L86-L93][cx-headers], [headers L5-L31][cx-headers2] |
| Item IDs | Codex removes each item `id` that does not have the form `prefix_suffix`. | [client L1023-L1032][cx-ids], [item ID L36-L39][cx-ids2] |
| Compaction | A `model_providers` entry has no remote compaction by default. Codex compacts the conversation itself. | [capabilities L30-L52][cx-caps] |

Item types that can occur in the Codex `input`:

| Item `type` | When |
| --- | --- |
| `message` (`user`, `developer`, `assistant`) | In each conversation turn. Content parts are `input_text`, `input_image`, and `output_text`. |
| `reasoning` | When an earlier response returned one. It has `summary`, optional `content`, and `encrypted_content`. |
| `function_call`, `function_call_output` | Calls of `function` tools and their results. A call can have a `namespace`. |
| `custom_tool_call`, `custom_tool_call_output` | Calls of `custom` (free-form) tools. |
| `local_shell_call` | Items of old Codex versions. |
| `web_search_call`, `image_generation_call`, `tool_search_call`, `tool_search_output` | Items of server tools. |
| `compaction`, `context_compaction`, `compaction_trigger` | Remote compaction. Codex does not use them with a `model_providers` entry by default. |
| `additional_tools`, `agent_message`, `configuration_update` | Internal Codex modes. |

Source: [`ResponseItem` L1010-L1245][cx-items], [`ContentItem` L878-L896][cx-content].

acp2api: a later Codex version differs. Codex `rust-v0.163.0` sends no top-level `instructions`. Its base instructions are the first `input` item, with the role `developer`, see [What do Claude Code, Codex, and SillyTavern need from a custom API endpoint?][i5]. In Codex `8c07694`, `tool_search` has `execution: "client"`, see [How do client tools reach the model?][i9].

Tool types that can occur in the Codex `tools` list are `function`, `namespace`, `custom` (with a Lark grammar), `web_search`, and `tool_search` ([tool spec L21-L56][cx-toolspec]).
A `model_providers` entry has the `web_search` capability by default ([capabilities L19-L28][cx-caps-default]). The default web search mode is `cached` ([config types L376-L382][cx-ws-mode], [config L3785][cx-ws-default]). Thus Codex sends the server tool `web_search` to the earlier bridge unless the maintainer sets `web_search = "disabled"`. No live Codex request confirmed this.

The Codex stream reader:

- It reads `response.output_text.delta` for text and `response.output_item.done` for each item. It runs a tool call from `response.output_item.done` ([SSE L344-L504][cx-sse]).
- It reads `response.completed` for `id` and `usage`. In `usage`, the fields `input_tokens`, `output_tokens`, and `total_tokens` are mandatory ([SSE L106-L126][cx-completed]).
- A stream that ends before `response.completed` is an error: "stream closed before response.completed" ([SSE L557-L562][cx-closed]).
- It stops a stream after 300 s with no SSE event ([`model-provider-info` L66-L68][cx-defaults], [SSE L564-L569][cx-idle]).
- It reads `response.reasoning_summary_text.delta` and `response.reasoning_text.delta`, but it does not need them.

### 2.3 Other API clients and SDKs

- The `stream()` helper of openai-python needs `response.created` before each other event. It needs `response.output_item.added` before each content event of that item ([accumulator L330-L394][py-acc]).
- The OpenAI package of the Vercel AI SDK (`@ai-sdk/openai`) assumes `store: true` when the API client sets no value ([language model L436][ai-store]). It then sends earlier output items as `{"type": "item_reference", "id": ...}` ([input conversion L670-L673][ai-itemref]). Thus an API client with this package needs the earlier bridge to resolve item IDs that the earlier bridge made.
- OpenCode configures a custom OpenAI-compatible service with `@ai-sdk/openai-compatible`, which uses `/v1/chat/completions` ([OpenCode docs L2596-L2603][oc-doc], [package L71][oc-pkg]).

### 2.4 Map to Cascades

The earlier bridge first makes one full conversation from the request. Then it applies the Cascade rule to that conversation. The table gives the actions that the earlier bridge chose.

| Request content | Action of the earlier bridge |
| --- | --- |
| `store: false`, full `input` (Codex) | Use the `input` as the conversation. |
| `store: true` or no `store` | Accept. Record no response content. Offer no retrieval endpoint (a decision of the maintainer, see below). |
| `previous_response_id` | Refuse with HTTP 400. The message tells the maintainer to set `store: false` in the API client. Do not do a rebuild, because the earlier bridge does not know the conversation. |
| `item_reference` | Refuse with HTTP 400, with the same message. |
| `conversation` | Refuse with HTTP 400. The earlier bridge has no Conversations API. |
| `background: true` | Refuse with HTTP 400. |
| `reasoning` items in `input` | Use them for the match only as opaque items. Do not put them in the text of a rebuild. The earlier bridge never makes them, so they come from a different service. |
| `include` with `reasoning.encrypted_content` | Accept. It has no effect, because the earlier bridge returns no `reasoning` item. |
| `include` with `message.output_text.logprobs` | Refuse with HTTP 400. |
| `truncation` | Accept. The engine compacts a long Cascade by itself. |
| Session hint | Use `prompt_cache_key`, then the `session-id` header, as the session identifier of the Cascade rule. |

- Row "`reasoning` items in `input`". acp2api: it ignores each `reasoning` item, also in the match, see [What does acp2api return for `thinking` blocks and `reasoning` items?][i24].
- Row "Session hint". acp2api: it uses no session hint of the API clients, see [ADR-0003][adr3].

The earlier bridge matches its own output items in the next `input` by `call_id` and by content. Item IDs are only a hint, because an API client can remove them.
The first draft of this note recommended a response record for `previous_response_id` and `item_reference`. The maintainer rejected it on 2026-10-05.

## 3. Stream detail for each endpoint

### 3.1 Anthropic `/v1/messages`

Event order ([streaming docs][ant-stream]):

1. `message_start` with a `Message` that has empty `content`.
2. For each content block: `content_block_start`, one or more `content_block_delta`, then `content_block_stop`. The `index` is the position in the final `content` array.
3. One or more `message_delta` events with `delta.stop_reason`, `delta.stop_sequence`, and `usage`.
4. `message_stop`.

More rules:

- A stream can contain "any number of `ping` events". The TypeScript SDK skips `ping` also before `message_start` ([SDK streaming L135-L143][ant-sse-ping]).
- The `usage` counts in `message_delta` "are *cumulative*". The SDK replaces `input_tokens`, the prompt cache counts, and `output_tokens_details` when `message_delta` contains them ([MessageStream L584-L615][ant-ms-delta]).
- Delta types are `text_delta`, `input_json_delta`, `citations_delta`, `thinking_delta`, and `signature_delta` ([SDK types L2806-L2811][ant-deltas]). `input_json_delta` carries partial JSON. The final `tool_use.input` is an object.
- A `thinking` block gets one `signature_delta` "just before the `content_block_stop` event" ([SDK types L3064-L3073][ant-sig]).
- A mid-stream error is `event: error` with `{"type": "error", "error": {"type": ..., "message": ...}}`.
- `stop_reason` values: `end_turn`, `max_tokens`, `stop_sequence`, `tool_use`, `pause_turn`, `refusal`, `model_context_window_exceeded` ([SDK types L3095-L3102][ant-stop]).
- `usage` fields: `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `cache_creation`, `server_tool_use`, `service_tier`, `inference_geo`, and `output_tokens_details.thinking_tokens` ([SDK types L3905-L3955][ant-usage], [L2776-L2788][ant-otd]).
- The non-stream response is one `Message`: `id`, `type: "message"`, `role: "assistant"`, `content`, `model`, `stop_reason`, `stop_sequence`, `usage`.

Claude Code behavior with an `ANTHROPIC_BASE_URL` endpoint ([gateway protocol][cc-gw], [errors][cc-errors], [network configuration][cc-net]):

- It needs the full event sequence "through the final `message_delta` and `message_stop` events". A body that ends before `message_delta` is a dropped connection.
- After 20 s with no data, it shows `Waiting for API response · will retry in … · check your network`.
- Its byte-level watchdog and its event-level watchdog stop a silent stream after 300 s on a gateway connection. Bytes such as `ping` events reset the watchdogs.
- It can send the same request again without `stream` when a stream breaks before any complete content. Thus the earlier bridge must also serve `stream: false`.
- It sends `x-claude-code-session-id` on each request, and `x-claude-code-agent-id` for a subagent.
- It returns earlier `thinking` blocks in later requests. When the service refuses a signature, Claude Code "removes earlier thinking blocks from the request, retries, and keeps them out of every later request".
- It collapses thought output by default. `Ctrl+O` shows it as gray text ([model configuration][cc-model]).
- `/v1/messages/count_tokens` is optional. Without it, Claude Code estimates the context size from the characters.

### 3.2 OpenAI `/v1/chat/completions`

- Each SSE `data:` line is one `chat.completion.chunk` with `id`, `object`, `created`, `model`, and `choices[].delta` and `choices[].finish_reason`. The stream ends with `data: [DONE]`.
- `delta` fields are `role`, `content`, `refusal`, `tool_calls`, and the deprecated `function_call` ([chunk type L77-L93][py-delta]). The official schema has no field for thought text.
- A tool call delta has `index`, `id`, `type: "function"`, and `function.name` and `function.arguments`. Later deltas add argument fragments.
- `finish_reason` values: `stop`, `length`, `tool_calls`, `content_filter`, `function_call` ([chunk type L110-L118][py-finish]).
- With `stream_options: {"include_usage": true}`, one more chunk comes before `[DONE]`. It has `choices: []` and the `usage` of the full request. All other chunks have `usage: null` ([stream options L24-L32][py-incl-usage]).
- `usage` fields: `prompt_tokens`, `completion_tokens`, `total_tokens`, `prompt_tokens_details.cached_tokens`, and `completion_tokens_details.reasoning_tokens` ([usage type L59-L71][py-usage]).
- A `data:` object with an `error` key makes the official SDKs raise an error ([openai-python L92-L106][py-sse-err], [openai-node L149-L154][node-sse-err]).
- The SDKs ignore SSE comment lines that start with `:` ([openai-python L404-L405][py-sse-comment]).
- The non-stream response is one `chat.completion` with `choices[].message` and `usage`.

De facto thought fields: the `@ai-sdk/openai-compatible` package reads `delta.reasoning_content`, then `delta.reasoning` ([chat model L678][ai-rc]). OpenCode uses this package for custom services. OpenCode pins version 2.0.41. That this version reads the same fields is Not confirmed. Codex and Claude Code do not use this endpoint.

### 3.3 OpenAI `/v1/responses`

Event order for a text answer (event types from the [SDK event union][py-events]):

1. `response.created` and `response.in_progress`, each with the full `Response` object and `status: "in_progress"`.
2. `response.output_item.added` with a `message` item.
3. `response.content_part.added` with an `output_text` part.
4. `response.output_text.delta` with `item_id`, `output_index`, `content_index`, and `delta`.
5. `response.output_text.done`, `response.content_part.done`, and `response.output_item.done`.
6. `response.completed` with the full `Response`, `status: "completed"`, and `usage`.

Each event has a `sequence_number`. A `function_call` item uses `response.function_call_arguments.delta` and `response.function_call_arguments.done` in place of steps 3 to 5.
A `reasoning` item uses `response.reasoning_summary_part.added`, `response.reasoning_summary_text.delta`, and the `.done` events.

- `status` values: `completed`, `failed`, `in_progress`, `cancelled`, `queued`, `incomplete` ([response L507-L512][py-status]).
- `incomplete_details.reason` values: `max_output_tokens`, `max_messages`, `content_filter`, `steered` ([response L57-L67][py-incomplete]).
- End events: `response.completed`, `response.incomplete`, `response.failed`, and the `error` event with `code`, `message`, and `param`.
- `usage` fields: `input_tokens`, `input_tokens_details.cached_tokens`, `input_tokens_details.cache_write_tokens`, `output_tokens`, `output_tokens_details.reasoning_tokens`, `total_tokens` ([usage L28-L47][py-rusage]).
- The non-stream response is one `Response` object with `output`, `status`, and `usage`.

Codex treats `response.incomplete` with a reason other than `interrupted` as an error. It treats `response.failed` by its `error.code` ([SSE L413-L463][cx-sse-end], [failed parser L47-L102][cx-failed]).

### 3.4 Field map against the engine

| API field | Anthropic | Chat completions | Responses | Engine source | Quality |
| --- | --- | --- | --- | --- | --- |
| Text | `text` block | `delta.content` | `output_text` | `text_delta` | Truthful |
| Tool call | `tool_use` block | `delta.tool_calls` | `function_call` or `custom_tool_call` item | `tools/call` arguments | Truthful |
| Thought text | `thinking` block | none (de facto `reasoning_content`) | `reasoning` item summary | none | Leave empty |
| Signature or encrypted thought | `signature` | none | `encrypted_content` | none | Leave empty |
| Input tokens | `input_tokens` | `prompt_tokens` | `input_tokens` | step `input_tokens` | Truthful for each model request. One response can contain more than one model request. |
| Output tokens | `output_tokens` | `completion_tokens` | `output_tokens` | step `output_tokens` | Truthful |
| Thought tokens | `output_tokens_details.thinking_tokens` | `completion_tokens_details.reasoning_tokens` | `output_tokens_details.reasoning_tokens` | step `thinking_tokens` | Truthful |
| Prompt cache read | `cache_read_input_tokens` | `prompt_tokens_details.cached_tokens` | `input_tokens_details.cached_tokens` | step `cache_read_tokens` | Truthful value. The relation to `input_tokens` is Not confirmed. |
| Prompt cache write | `cache_creation_input_tokens` | none | `cache_write_tokens` | none | Send 0 |
| Normal end | `end_turn` | `stop` | `completed` | `result.status: SUCCESS` | Truthful |
| Tool call end | `tool_use` | `tool_calls` | `completed` with a call item | `tools/call` | Truthful |
| Length stop | `max_tokens` | `length` | `incomplete` | none | Never send |
| Stop sequence, refusal, content filter | `stop_sequence`, `refusal` | `content_filter` | `incomplete` | none | Never send |
| Model | `model` | `model` | `model` | `init.model` or the request | Truthful |
| Message or response ID | `id` | `id` | `id` | the earlier bridge | Value of the earlier bridge |

acp2api: [Which token usage does acp2api report to each API?][i20] decides the token fields. An emulated stop sequence sends `stop_sequence` (Anthropic) or `stop` (Chat Completions), see [Which request fields and content blocks does acp2api use, ignore, or reject?][i21].

### 3.5 How a pending tool call ends a response

The earlier bridge ends the response at once after a `tools/call` (its second architecture decision). In E3, the usage of the planner step came 12 ms after the `tools/call` (section 1). Thus the earlier bridge waits for the `tool` step of that call, for a short time, before it closes the response.

acp2api: it returns the tool call at the permission request of the official runtime, and it cancels the prompt turn (cancel at permission), see [ADR-0002][adr2].

Anthropic:

```text
content_block_start  {"type":"tool_use","id":"toolu_<id>","name":"<tool>","input":{}}
content_block_delta  {"type":"input_json_delta","partial_json":"<full JSON arguments>"}
content_block_stop
message_delta        {"stop_reason":"tool_use","stop_sequence":null}, usage
message_stop
```

Chat completions:

```text
delta {"tool_calls":[{"index":0,"id":"call_<id>","type":"function","function":{"name":"<tool>","arguments":""}}]}
delta {"tool_calls":[{"index":0,"function":{"arguments":"<full JSON arguments>"}}]}
choices[0].finish_reason "tool_calls"
usage chunk (only with include_usage), then [DONE]
```

Responses:

```text
response.output_item.added            {"type":"function_call","id":"fc_<id>","call_id":"call_<id>","name":"<tool>","arguments":"","status":"in_progress"}
response.function_call_arguments.delta {"delta":"<full JSON arguments>"}
response.function_call_arguments.done  {"arguments":"<full JSON arguments>"}
response.output_item.done             {..., "status":"completed"}
response.completed                    {"status":"completed", "output":[...], "usage":{...}}
```

For a `custom` tool, the Responses item is `custom_tool_call` with `call_id`, `name`, and `input` ([`ResponseItem` L1134-L1154][cx-custom]).
Text that the model wrote before the tool call comes first in the same response.

### 3.6 The `thinking` signature problem

The engine gives no thought text and no signature. The Claude API says that the next request must return `thinking` blocks "exactly as received". It refuses a changed or rebuilt block with a 400 error ([errors page][ant-errors]).

| Option | What Claude Code shows | Risk |
| --- | --- | --- |
| A. No thought block | No thought section. The token count is in `usage` only. | The earlier bridge itself has no risk. If a user sends this conversation later to the real Claude API with `thinking` on, the result is Not confirmed. |
| B. `redacted_thinking` block with a value of the earlier bridge in `data` | A redacted thought marker. The exact view is Not confirmed. | The value is not real encrypted data. The real API refuses it. Claude Code recovery for this case is Not confirmed. |
| C. A text block such as `[635 thought tokens]` | Extra text in the answer | The text enters the conversation of the API client. The model reads it in later turns. |
| D. `thinking` block with empty text and a made-up signature | An empty thought section | A fake signature. It breaks the rule of the earlier bridge not to fake output (its first architecture decision). |

Recommendation of this note, which the earlier bridge took: option A. The same policy applies to the other endpoints: no `reasoning` item and no `reasoning_content`.
Codex and the openai-python helper do not need a `reasoning` item ([SSE L344-L504][cx-sse], [accumulator L330-L394][py-acc]).

## 4. Error shapes and retry rules

### 4.1 Error bodies

| API | HTTP error body | Mid-stream error |
| --- | --- | --- |
| OpenAI | `{"error": {"message", "type", "param", "code"}}`. The SDK reads `body.error` ([openai-python L851-L882][py-status-err], [APIError L65-L79][py-apierror]). | Chat: a `data:` object with `error`. Responses: `response.failed` with `response.error.code` and `message`, or the `error` event. |
| Anthropic | `{"type": "error", "error": {"type", "message"}, "request_id"}`. Types: `invalid_request_error` (400), `authentication_error` (401), `billing_error` (402), `permission_error` (403), `not_found_error` (404), `request_too_large` (413), `rate_limit_error` (429), `api_error` (500), `timeout_error` (504), `overloaded_error` (529) ([errors page][ant-errors]). | `event: error` with the same body. |

### 4.2 Automatic retry by API client and SDK

| Status | openai-python and openai-node | anthropic-sdk-typescript | Claude Code | Codex |
| --- | --- | --- | --- | --- |
| 400 | No | No | No. Exception: a 400 whose text matches a refused capability, for example `thinking` or `output_config.effort`. | No (`InvalidRequest`). It shows the body text. |
| 401 | No | Once, only with its token cache | Only with `apiKeyHelper` | Not confirmed (a token refresh path exists for ChatGPT sign-in) |
| 403 | No | No | Only with `apiKeyHelper` | Yes (`UnexpectedStatus`), up to 5 stream retries |
| 408, 409 | Yes | Yes | Not confirmed | 408: yes (`UnexpectedStatus`). 409: yes. |
| 429 | Yes | Yes | Yes for a temporary throttle. No with `x-should-retry: false`. | No HTTP retry. `insufficient_quota` and `usage_limit_reached` stop at once. Other 429: retry only with `Retry-After`. |
| 5xx, 529 | Yes | Yes | Yes, up to 10 times | Yes: up to 4 HTTP retries, then stream retries |
| Header `x-should-retry: false` | No retry | No retry | No retry | Ignored |
| Default count | 2 | 2 | 10 | 4 HTTP, 5 stream |

Sources: [openai-python L885-L927][py-retry], [constants L8-L13][py-const], [openai-node L2065-L2086][node-retry], [openai-node L1016-L1018][node-default], [anthropic L1424-L1462][ant-retry], [anthropic L676][ant-default], [Claude Code errors][cc-errors], [Codex transport retry L22-L41][cx-retry], [Codex retry config L454-L460][cx-retry-cfg], [Codex status map L101-L255][cx-status], [Codex retry classes L389-L438][cx-retry-class].

Codex also retries a `response.failed` whose `error.code` is not in its list of known codes ([failed parser L98-L101][cx-failed]).
Codex shows a fixed text for `insufficient_quota`: "Quota exceeded. Check your plan and billing details." ([error L162-L163][cx-quota-text]). Thus that code hides the reason that the earlier bridge gives.

### 4.3 Status for a fail-stop and a quota stop

HTTP 400 is the only status that no API client or SDK in section 4.2 retries automatically. Codex and Claude Code also show its message text.
HTTP 403 fits a fail-stop better, but Codex retries it 5 times. HTTP 429 fits a quota stop better, but Codex then shows a fixed text, and the SDKs and Claude Code retry it without the header.

Recommendation for the earlier bridge: send HTTP 400 with the error body of the endpoint, the header `x-should-retry: false`, and no `Retry-After`. The message starts with a stable code and gives the reason.

acp2api: open on 2026-10-18. [How does acp2api report an upstream error and a cancelled prompt turn to each API?][i25] decides the status and the body of each error, also for a lockout and a quota wait.

## Recommended bridge contract

This section gives the choices of the earlier bridge, not the decisions of acp2api. An "acp2api:" line follows each rule where acp2api decided differently. Some rules also get a line where an open acp2api issue decides the point. A rule with no such line has no different acp2api decision on record on 2026-10-18. The error forms of acp2api are open in [How does acp2api report an upstream error and a cancelled prompt turn to each API?][i25].

On 2026-10-05, the maintainer accepted this contract with four decisions:

- **Thought content.** The earlier bridge sends no thought text (section 3.6, option A).
- **Server tools.** The earlier bridge refuses a server tool with a clear error. For the Codex `web_search` tool, the error gives the fix (rule 12).
- **Engine errors.** The earlier bridge sends `x-should-retry: false`, so that API clients do not retry automatically (rule 31). The user retries manually.
  - acp2api: open on 2026-10-18, see [How does acp2api report an upstream error and a cancelled prompt turn to each API?][i25].
- **Response content.** The earlier bridge records no response content. It refuses `previous_response_id` and `item_reference` (rules 8 and 9). Claude Code, Codex, and OpenCode do not use them.

### General

1. Serve `/v1/chat/completions`, `/v1/responses`, and `/v1/messages`, each with `stream: true` and `stream: false`.
2. Make one conversation from each request. Then apply the match of the Cascade rule.
   - acp2api: it matches the record of each ACP session with its own rules. For example, a new system prompt or a new set of client tools causes a rebuild, see [ADR-0003][adr3].
3. Use these session hints in this order: `x-claude-code-session-id` with `x-claude-code-agent-id`, `prompt_cache_key`, the `session-id` header.
   - acp2api: it uses no session hint of the API clients, see [ADR-0003][adr3].
4. Use IDs with a prefix: `msg_`, `toolu_`, `call_`, `fc_`, `resp_`, `chatcmpl-`. Codex keeps only IDs of the form `prefix_suffix`.
5. When a request repeats the tail that the Cascade processes now, connect the new HTTP response to that turn. Do not send the user message or the tool result to the engine again. API clients send a request again after a broken stream.
   - acp2api: an ACP session with a prompt turn that runs now does not match, so such a request does a rebuild, see [ADR-0003][adr3].
6. Accept unknown top-level fields and record them in the log. Codex sends fields such as `client_metadata`.

### `/v1/responses` state

7. Accept `store: false` with the full `input` as the normal case.
8. Record no response content. Treat `store: true` and an absent `store` as `store: false`.
9. Refuse each `previous_response_id` and each `item_reference` with HTTP 400 and the code `bridge_unsupported`. The message tells the maintainer to set `store: false` in the API client. Do not do a rebuild.
10. Refuse `conversation`, `background: true`, and `include` with `message.output_text.logprobs`.
11. Accept `include` with `reasoning.encrypted_content`, `truncation`, `reasoning.summary`, and `context_management` with no effect.
12. Refuse the server tool types `web_search`, `file_search`, `code_interpreter`, `image_generation`, `computer_use_preview`, `mcp`, `local_shell`, and `tool_search`. The error names the tool. For `web_search`, it gives the Codex fix `web_search = "disabled"`.
    - acp2api: it also rejects these tools, but not all as server tools: Codex executes `tool_search` itself, and `local_shell` is a tool type that acp2api does not know. Its error texts differ, see [How do client tools reach the model?][i9] and [What does acp2api do with each server tool that Claude Code sends?][i28].
13. Pass `function` tools, `namespace` tools, and `custom` tools through the MCP server of the earlier bridge. A `custom` tool becomes an MCP tool with one string argument `input`. Its grammar goes in the description.

### Streams

14. Send the event sequence of section 3 for each endpoint. Always end with `message_stop`, `[DONE]`, or `response.completed`.
15. Send a heartbeat after 10 s without an event. Use `ping` for Anthropic and `response.in_progress` for Responses. Use a `: ping` comment line for chat completions.
    - acp2api: keep-alive events come at an interval shorter than 300 s. The written design decides the interval, and the event of the Chat Completions API, see [How many prompt turns run at the same time, and what does a queued request get?][i14].
16. Send the HTTP status and the first events at once, also while the request waits in the queue of the earlier bridge.
17. End a response with one tool call (section 3.5). Wait a short time for the `tool` step of the call. Then the usage of the planner step is available.
    - acp2api: it returns the tool call at the permission request, and it cancels the prompt turn (cancel at permission), see [ADR-0002][adr2].

### Usage

18. Build the usage of a response from the step usage of its model requests. Do not use `result.usage`, because it is cumulative.
    - acp2api: the input tokens are the `used` value of the last `usage_update` of the prompt turn, see [Which token usage does acp2api report to each API?][i20].
19. Report the `input_tokens` of the last model request. Report the sum of `output_tokens` and, separately, the sum of `thinking_tokens`. API clients use the input count as the context size.
    - acp2api: the output tokens are a local estimate. It leaves out the reasoning token fields, or it sends 0 where the API needs them, see [Which token usage does acp2api report to each API?][i20].
20. Report zero when a response contains no model request. An example is the second of two tool calls (run P1).
    - acp2api: open on 2026-10-18. [How does acp2api report an upstream error and a cancelled prompt turn to each API?][i25] decides the usage of a prompt turn with no `usage_update`.
21. In Anthropic streams, send `input_tokens: 0` in `message_start`. Send the real values in the last `message_delta`.
    - acp2api: `message_start` carries a local estimate of the input tokens, see [Which token usage does acp2api report to each API?][i20].
22. Send chat completions `usage` only with `stream_options.include_usage`.

### Stop reasons

23. Use only these values: `end_turn` and `tool_use` (Anthropic), `stop` and `tool_calls` (chat), `completed` and `failed` (Responses).
    - acp2api: it also sends `stop_sequence` (Anthropic) for an emulated stop sequence, see [Which request fields and content blocks does acp2api use, ignore, or reject?][i21].
24. Never send `max_tokens`, `length`, `incomplete`, `stop_sequence`, `refusal`, or `content_filter`. The engine gives no such signal.
    - acp2api: it sends `stop_sequence` for an emulated stop sequence, see [Which request fields and content blocks does acp2api use, ignore, or reject?][i21].

### Thought content

25. Send no `thinking`, `redacted_thinking`, `reasoning`, or `reasoning_content` output.
26. Accept these blocks and items in the input. Leave them out of the text of a rebuild.

### Parameters

| Parameter | Action of the earlier bridge |
| --- | --- |
| `max_tokens`, `max_completion_tokens`, `max_output_tokens` | Accept. No effect. The engine has no output limit control. |
| `temperature`, `top_p`, `top_k`, `seed`, `presence_penalty`, `frequency_penalty` | Accept. No effect. Record in the log. |
| `stop`, `stop_sequences` (not empty) | Refuse |
| `n` greater than 1, `logprobs`, `top_logprobs` | Refuse |
| `response_format` or `text.format` with a JSON schema, `output_config.format` | Refuse. Refer to the open questions. |
| `tool_choice: "auto"` | Honor |
| `tool_choice: "none"` | Accept. Answer each `tools/call` of that turn with an MCP error result. |
| `tool_choice` that forces a tool (`required`, `any`, a name) | Refuse |
| `parallel_tool_calls` | Accept. One tool call in each response. |
| `thinking`, `output_config.effort`, `reasoning.effort`, `reasoning_effort` | Accept. No thought output. If the model ID ends with an effort suffix (`-low`, `-medium`, `-high`), the suffix sets the effort and the earlier bridge ignores the request value. Else the earlier bridge maps the request value to `--effort`: `minimal` and `low` to `low`, `medium` to `medium`, `high`, `xhigh`, and `max` to `high`. `thinking.budget_tokens` has no effect. A live probe must confirm `--effort` for each model. |
| `cache_control`, `metadata`, `user`, `service_tier`, `prompt_cache_key` | Accept |

The specification of the earlier bridge later changed the effort row to "Accept. No effect." (Appendix A).

- Row "`stop`, `stop_sequences`". acp2api: it emulates stop sequences. It cuts the reply before the first match, and it cancels the prompt turn, see [Which request fields and content blocks does acp2api use, ignore, or reject?][i21].
- Row "`tool_choice: "none"`". acp2api: it answers `reject_once` to each permission request of a client tool, so the response has no tool call, see [How do client tools reach the model?][i9].
- Row of the effort fields. acp2api: the effort fields select the thinking level with another map and the default `high`. An unknown value gives no error, see [Which model names does the API of acp2api offer?][i13].

### Errors

27. Check the fail-stop and the quota stop before the HTTP status. Send HTTP 400 with `x-should-retry: false` and no `Retry-After`.
    - acp2api: open on 2026-10-18. [How does acp2api report an upstream error and a cancelled prompt turn to each API?][i25] decides the points of rules 27 to 31.
28. Put the code `bridge_fail_stop` or `bridge_quota_stop` at the start of the message. OpenAI bodies also carry it in `error.code`.
29. In a fail-stop message, give the signal and its time. In a quota stop message, give the model group, the window, the percentage, and the reset time.
    - acp2api: a lockout notice gives the start time, the signal, and the address of the management panel. It also says that acp2api sends no request to the official runtime. The response that started the lockout also has the original error text of Google, see [Which signals start a lockout, and how does the user end it?][i15].
30. When a stop starts after the HTTP status, send the mid-stream error form of the endpoint. A retry of the API client then gets the HTTP 400.
31. For other engine errors, send HTTP 502 (Anthropic `api_error`) with `x-should-retry: false`. Codex ignores the header. Thus, for a short time after an error, rule 5 also answers a repeated request from the record. It starts no new engine turn.
32. Refuse unsupported features with HTTP 400 and the code `bridge_unsupported`. Name the field.
    - acp2api: a rejected tool gets an error text of [What does acp2api do with each server tool that Claude Code sends?][i28], for example `capability_rejected: tool type <type> (name <name>) is not supported by acp2api.` [How does acp2api report an upstream error and a cancelled prompt turn to each API?][i25] decides the other error forms.

### Effects that a user sees

| Decision | What the user sees | Recommendation |
| --- | --- | --- |
| Thought content | Claude Code shows no thought section, also with `Ctrl+O`. Codex and OpenCode show no thought summary. | No thought output (option A) |
| Server tools | Codex fails at the first request until the maintainer sets `web_search = "disabled"`. The error text gives this fix. | Refuse with a clear error |
| `previous_response_id` and `item_reference` | Stateless API clients such as Codex work. An API client that sends these fields gets an error that tells the maintainer to set `store: false`. | Refuse (a decision of the maintainer) |
| `conversation`, `background` | API clients that use these features get an error. | Refuse |
| Stop errors | Claude Code shows `API Error: 400` with the reason. Codex shows the error body. No API client retries. | HTTP 400 |
| Engine errors | The API client shows the error at once and does not retry. The user retries by hand. | No automatic retry by the API client |
| Long answers | `max_tokens` has no effect. An answer is never cut at a token limit. | Accept with no effect |
| Restart of the earlier bridge | A Codex or Claude Code conversation continues from the Cascade record on disk. A pending tool call is lost, so its result causes a rebuild. | Keep the records on disk |
| Wait for the engine | Claude Code shows no network banner during a long model step, because the earlier bridge sends heartbeats. | Heartbeat every 10 s |

- Row "Restart of the earlier bridge". acp2api: it keeps the records only in memory, so after a restart the next request of each conversation does a rebuild, see [ADR-0003][adr3].

## Open questions

- Does `input_tokens` of the engine contain `cache_read_tokens`? A probe with a long stable prefix can show a non-zero value.
  - acp2api: no acp2api issue answers it, because it concerns only the engine. acp2api leaves out the cache token fields, or it sends 0 where the API needs them, see [Which token usage does acp2api report to each API?][i20].
- How does the engine report a context overflow, a safety refusal, or a length limit? No probe produced one.
  - acp2api: no acp2api issue answers it. For the official runtime, [What does the official runtime support over ACP on Windows and macOS?][i2] records that a model error comes as reply text with the stop reason `end_turn`. It also records, from a static reading only, that an exhausted quota ends a prompt turn with the stop reason `refusal`.
- Does Claude Code retry an `error` event of a non-server type before any content? The docs describe only server errors.
  - acp2api: not answered. [How does acp2api report an upstream error and a cancelled prompt turn to each API?][i25] is open on 2026-10-18.
- Does Claude Code take its context size from `input_tokens` of the last response? This decides rule 19.
  - acp2api: answered for Claude Code 2.1.299 by [Check how Claude Code reads the token usage and handles a rejected server tool][i27]. Claude Code shows the `input_tokens` of the last `message_delta`, never the value of `message_start`.
- Does Codex count SSE comment lines for its idle timer? Rule 15 uses `response.in_progress` to avoid this question.
  - acp2api: not tested. [What do Claude Code, Codex, and SillyTavern need from a custom API endpoint?][i5] inferred from the Codex source that comment lines probably do not reset the idle timer. Thus [How many prompt turns run at the same time, and what does a queued request get?][i14] uses a real event for the Responses API.
- Can `--json-schema` of the CLI serve `response_format` for one turn? The init field `json_schema` exists, but the scope of the option is Not confirmed.
  - acp2api: no acp2api issue answers it, because it concerns only the CLI. acp2api rejects an output format with a JSON schema, see [Which request fields and content blocks does acp2api use, ignore, or reject?][i21].
- Does `--effort` change the behavior of each model, also of a model ID with an effort suffix? The parameter table gives the planned map. A live probe must confirm it.
  - acp2api: no acp2api issue answers it for the CLI. Appendix A.3 gives the later result of the earlier bridge. For the official runtime, [Which model names does the API of acp2api offer?][i13] maps the effort fields to the config option `thinking_level`.
- An API client can send a tool result and a new user message in one request. An example is a tool that the user stops in Codex. The engine then continues its engine turn after the tool result. The order of these two inputs needs a rule.
  - acp2api: answered by [How does a client tool call pause and continue a prompt turn?][i11] and [ADR-0003][adr3]. The prompt starts with the tool result, and each item after it gets its role label.
- Codex compacts the conversation with a normal Responses request. The earlier bridge can serve it in the current Cascade or in a separate Cascade. The next request is a rebuild in both cases (Cascade rule).
  - acp2api: answered by [Which ACP session does acp2api use for each API request?][i10]. The compaction request usually continues the ACP session, and the first request after the compaction does a rebuild.
- Claude Code model discovery keeps only IDs that contain `claude` or `anthropic` ([gateway protocol][cc-gw], section "Model discovery"). The names of Gemini models in the `/model` picker need a decision.
  - acp2api: decided by [Which model names does the API of acp2api offer?][i13]. acp2api offers each model of the official runtime under its own name. Its setup text tells the user to set a model name in `ANTHROPIC_MODEL`, in each `ANTHROPIC_DEFAULT_*_MODEL` variable, and in `CLAUDE_CODE_SUBAGENT_MODEL`.

## Appendix A: the parameter table of the earlier bridge

This appendix gives the choices of the earlier bridge, not the decisions of acp2api. It gives the state of the earlier bridge at commit `5103584` (2026-10-15). [Which request fields and content blocks does acp2api use, ignore, or reject?][i21] names these two sources of its delegated rows:

- the section "Parameters" of the specification of the earlier bridge;
- the module of the parameter checks of the earlier bridge, `src/api/params.ts`.

The specification is later than this note. It replaced the effort row of the section "Parameters" above with "Accept. No effect." (A.3).

### A.1 The table of the specification

| Parameter | Action |
| --- | --- |
| `max_tokens`, `max_completion_tokens`, `max_output_tokens`, `temperature`, `top_p`, `top_k`, `seed`, `presence_penalty`, `frequency_penalty`, `truncation`, `cache_control`, `metadata`, `user`, `service_tier`, `prompt_cache_key`, `context_management`, `include` with `reasoning.encrypted_content`, `parallel_tool_calls`, `reasoning.effort`, `reasoning_effort`, `output_config.effort`, `thinking` | Accept. No effect. |
| `stop`, `stop_sequences` (not empty), `n` > 1, `logprobs`, `top_logprobs` | Refuse with `bridge_unsupported` |
| `response_format` or `text.format` with a JSON schema, `output_config.format` | Refuse with `bridge_unsupported` |
| `tool_choice` `auto` | Normal |
| `tool_choice` `none` | Answer each `tools/call` of the turn with an MCP error result |
| `tool_choice` that forces a tool | Refuse with `bridge_unsupported` |

The effort fields have no effect, because the earlier bridge never sends `--effort` to the engine. The section "Engine" of the specification gives the reason. The CLI refuses `--effort` for each model ID with no effort suffix. Each listed ID already sets its effort or has no effort.

### A.2 Behavior that the table does not state

**Error form.** The shared error module of the earlier bridge builds each refusal of the table:

- The code is `bridge_unsupported`, and the HTTP status is 400. The message is `bridge_unsupported: <detail>`.
- The response has the header `x-should-retry: false` and no `Retry-After`.
- `/v1/messages` sends `{"type": "error", "error": {"type": "invalid_request_error", "message": "<message>"}}`.
- The two OpenAI endpoints send `{"error": {"message": "<message>", "type": "invalid_request_error", "param": null, "code": "bridge_unsupported"}}`.

**Refusals.** The checks run in the order of this table. The first refusal ends the check.

| Field | Endpoints | Refused when | Detail text |
| --- | --- | --- | --- |
| `stop` | `/v1/chat/completions`, `/v1/responses` | The value is present and is not `null`, `""`, or `[]`. | `stop is not supported: the engine sends no stop sequences.` |
| `stop_sequences` | `/v1/messages` | The value is present and is not `null`, `""`, or `[]`. | `stop_sequences is not supported: the engine sends no stop sequences.` |
| `n` | `/v1/chat/completions` | A number greater than 1 | `n greater than 1 is not supported: the bridge answers with one choice.` |
| `logprobs` | `/v1/chat/completions` | A value that JavaScript counts as true, for example `true`. `false` and `null` pass. | `logprobs is not supported: the engine sends no logprobs.` |
| `top_logprobs` | `/v1/chat/completions` | The value is present and is not `null`. This includes `0`. | `top_logprobs is not supported: the engine sends no logprobs.` |
| `response_format` | `/v1/chat/completions` | The `type` is `json_schema`. Another type, for example `json_object`, passes. | `response_format with a JSON schema is not supported: the engine takes no output schema.` |
| `text.format` | `/v1/responses` | The `type` is `json_schema`. | `text.format with a JSON schema is not supported: the engine takes no output schema.` |
| `output_config.format` | `/v1/messages`, `/v1/responses` | The value is present and is not `null`. | `output_config.format is not supported: the engine takes no output schema.` |
| `tool_choice` | all three | Each value other than the accepted values below | `tool_choice <the value as JSON> is not supported: the bridge cannot force a tool.` |

- **Accepted `tool_choice` values.** No value, `null`, `"auto"`, and `"none"`. On `/v1/messages`, also an object whose `type` is `"auto"` or `"none"`. The other two endpoints refuse each object.
- **`tool_choice: "none"`.** The MCP server of the earlier bridge answers each `tools/call` of the turn at once, with `isError: true` and the text `Tool calls are off for this request.` The section "Tools" of the specification gives this rule.
- **Prefill.** On all three endpoints, a separate check refuses a request whose last item is an assistant item. The detail text is `the request ends with an assistant item: the bridge does not continue a prefill.`
- **Effort fields.** `reasoning.effort`, `reasoning_effort`, and `output_config.effort` take each value with no check. No value reaches the arguments of the engine.

**Unknown top-level fields.** The earlier bridge accepts each top-level field that is not in the list of its endpoint. It writes the name of the field, not its value, into the request log. The lists:

- `/v1/chat/completions`: `model`, `messages`, `stream`, `stream_options`, `temperature`, `max_tokens`, `max_completion_tokens`, `top_p`, `seed`, `presence_penalty`, `frequency_penalty`, `metadata`, `user`, `service_tier`, `prompt_cache_key`, `stop`, `n`, `logprobs`, `top_logprobs`, `response_format`, `tool_choice`, `tools`, `parallel_tool_calls`, `reasoning_effort`.
- `/v1/messages`: `model`, `messages`, `system`, `stream`, `temperature`, `max_tokens`, `top_p`, `top_k`, `metadata`, `user`, `service_tier`, `stop_sequences`, `tools`, `tool_choice`, `thinking`, `context_management`. The list has no `output_config`, so its name goes into the log. The check of `output_config.format` above still applies.
- `/v1/responses`: `model`, `input`, `instructions`, `stream`, `store`, `previous_response_id`, `conversation`, `background`, `include`, `tools`, `tool_choice`, `parallel_tool_calls`, `reasoning`, `text`, `output_config`, `temperature`, `top_p`, `seed`, `truncation`, `metadata`, `user`, `service_tier`, `prompt_cache_key`, `max_output_tokens`.

**Session hints.** The module reads two headers. An empty header value counts as no value.

- `x-claude-code-session-id`. When `x-claude-code-agent-id` is also present, the hint is `<session ID>/<agent ID>`.
- `session-id`, the last choice.

The endpoint parsers read the body hint `prompt_cache_key`. The order of the hints is the order of rule 3.

acp2api: it uses no session hint, see [ADR-0003][adr3].

### A.3 The effort map before 2026-10-06

The first version of the module had the map of the effort row of the section "Parameters" above:

- A model ID that ends with `-low`, `-medium`, or `-high` sets the effort itself. The earlier bridge then sent no `--effort`, and it ignored the request value.
- Else, the first value of `reasoning.effort`, `reasoning_effort`, and `output_config.effort`, in this order, gave the `--effort` value. `minimal` and `low` gave `low`, `medium` gave `medium`, and `high`, `xhigh`, and `max` gave `high`.
- The map did not read `thinking`. A request with no effort value gave no `--effort`.
- Each other value gave this error: `bridge_unsupported: the effort value <the value as JSON> is not supported. Valid values: minimal, low, medium, high, xhigh, max.`

On 2026-10-06, after a live check, the earlier bridge removed the map. The module gives the reason. CLI 1.2.17 refuses `--effort` for each model ID without an effort suffix. It also refuses each value that conflicts with the suffix of a Gemini ID. Each listed ID already sets its effort.

acp2api: the effort fields select the thinking level with its own map, see [Which model names does the API of acp2api offer?][i13].

[agy-headless]: https://antigravity.google/docs/cli/headless/
[oai-state]: https://developers.openai.com/api/docs/guides/conversation-state
[oai-errors]: https://developers.openai.com/api/docs/guides/error-codes
[ant-stream]: https://platform.claude.com/docs/en/build-with-claude/streaming
[ant-errors]: https://platform.claude.com/docs/en/api/errors
[cc-gw]: https://code.claude.com/docs/en/llm-gateway-protocol
[cc-errors]: https://code.claude.com/docs/en/errors
[cc-net]: https://code.claude.com/docs/en/network-config
[cc-model]: https://code.claude.com/docs/en/model-config
[cx-wire]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider-info/src/lib.rs#L100-L131
[cx-ws-flag]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider-info/src/lib.rs#L197-L199
[cx-defaults]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider-info/src/lib.rs#L66-L68
[cx-retry-cfg]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider-info/src/lib.rs#L454-L460
[cx-req-types]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/common.rs#L279-L365
[cx-client-doc]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/client.rs#L11-L18
[cx-req]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/client.rs#L1002-L1018
[cx-input]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/client.rs#L904
[cx-include]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/client.rs#L967
[cx-instr]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/client.rs#L942-L947
[cx-pck]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/client.rs#L584-L596
[cx-pck2]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/client.rs#L984
[cx-ids]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/client.rs#L1023-L1032
[cx-ids2]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/protocol/src/response_item_id.rs#L36-L39
[cx-headers]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/endpoint/responses.rs#L86-L93
[cx-headers2]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/requests/headers.rs#L5-L31
[cx-caps]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider/src/capabilities.rs#L30-L52
[cx-caps-default]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/model-provider/src/capabilities.rs#L19-L28
[cx-items]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/protocol/src/models.rs#L1010-L1245
[cx-content]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/protocol/src/models.rs#L878-L896
[cx-toolspec]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/tools/src/tool_spec.rs#L21-L56
[cx-ws-mode]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/protocol/src/config_types.rs#L376-L382
[cx-ws-default]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/core/src/config/mod.rs#L3785
[cx-sse]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/sse/responses.rs#L344-L504
[cx-sse-end]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/sse/responses.rs#L413-L463
[cx-completed]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/sse/responses.rs#L106-L126
[cx-closed]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/sse/responses.rs#L557-L562
[cx-idle]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/sse/responses.rs#L564-L569
[cx-failed]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/sse/responses_error.rs#L47-L102
[cx-retry]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-client/src/retry.rs#L22-L41
[cx-status]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/codex-api/src/api_bridge.rs#L101-L255
[cx-retry-class]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/protocol/src/error.rs#L389-L438
[cx-quota-text]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/protocol/src/error.rs#L162-L163
[cx-custom]: https://github.com/openai/codex/blob/823ea830c0fd418b09ff02d36cad9a1fff66465b/codex-rs/protocol/src/models.rs#L1134-L1154
[py-prev]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_create_params.py#L148-L154
[py-store]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_create_params.py#L248-L254
[py-conv]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_create_params.py#L58-L64
[py-instr]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_create_params.py#L100-L106
[py-include]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_create_params.py#L66-L86
[py-trunc]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_create_params.py#L323-L331
[py-bg]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_create_params.py#L49-L53
[py-rs-item]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_reasoning_item.py#L51-L61
[py-status]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response.py#L507-L512
[py-incomplete]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response.py#L57-L67
[py-rusage]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_usage.py#L28-L47
[py-events]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/responses/response_stream_event.py
[py-acc]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/lib/streaming/responses/_responses.py#L330-L394
[py-delta]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/chat/chat_completion_chunk.py#L77-L93
[py-finish]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/chat/chat_completion_chunk.py#L110-L118
[py-incl-usage]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/chat/chat_completion_stream_options_param.py#L24-L32
[py-usage]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/types/completion_usage.py#L59-L71
[py-sse-err]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/_streaming.py#L92-L106
[py-sse-comment]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/_streaming.py#L404-L405
[py-status-err]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/_client.py#L851-L882
[py-apierror]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/_exceptions.py#L65-L79
[py-retry]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/_base_client.py#L885-L927
[py-const]: https://github.com/openai/openai-python/blob/becc1d20eed83c1b8d85e15dc131a372d9dc7813/src/openai/_constants.py#L8-L13
[node-retry]: https://github.com/openai/openai-node/blob/fb6955621e1cf6653659adb75094ebace83ebfe9/src/client.ts#L2065-L2086
[node-default]: https://github.com/openai/openai-node/blob/fb6955621e1cf6653659adb75094ebace83ebfe9/src/client.ts#L1016-L1018
[node-sse-err]: https://github.com/openai/openai-node/blob/fb6955621e1cf6653659adb75094ebace83ebfe9/src/core/streaming.ts#L149-L154
[ant-retry]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/client.ts#L1424-L1462
[ant-default]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/client.ts#L676
[ant-sse-ping]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/core/streaming.ts#L135-L143
[ant-ms-delta]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/lib/MessageStream.ts#L584-L615
[ant-deltas]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/resources/messages/messages.ts#L2806-L2811
[ant-sig]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/resources/messages/messages.ts#L3064-L3073
[ant-stop]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/resources/messages/messages.ts#L3095-L3102
[ant-usage]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/resources/messages/messages.ts#L3905-L3955
[ant-otd]: https://github.com/anthropics/anthropic-sdk-typescript/blob/d49bdab458000bcdffe77bd84b03293f31824fb3/src/resources/messages/messages.ts#L2776-L2788
[ai-rc]: https://github.com/vercel/ai/blob/0ca1ab97408d52e7973893cd7f368294b04d9a91/packages/openai-compatible/src/chat/openai-compatible-chat-language-model.ts#L678
[ai-store]: https://github.com/vercel/ai/blob/0ca1ab97408d52e7973893cd7f368294b04d9a91/packages/openai/src/responses/openai-responses-language-model.ts#L436
[ai-itemref]: https://github.com/vercel/ai/blob/0ca1ab97408d52e7973893cd7f368294b04d9a91/packages/openai/src/responses/convert-to-openai-responses-input.ts#L670-L673
[oc-doc]: https://github.com/anomalyco/opencode/blob/907b3bc518fa48e90e8ec24dd327d13eee71c36c/packages/web/src/content/docs/providers.mdx#L2596-L2603
[oc-pkg]: https://github.com/anomalyco/opencode/blob/907b3bc518fa48e90e8ec24dd327d13eee71c36c/packages/opencode/package.json#L71
[adr2]: ../adr/0002-cancel-each-client-tool-call-at-its-permission-request.md
[adr3]: ../adr/0003-continue-an-acp-session-only-when-a-request-continues-its-history.md
[i2]: https://github.com/yandhi-0421/acp2api/issues/2
[i5]: https://github.com/yandhi-0421/acp2api/issues/5
[i9]: https://github.com/yandhi-0421/acp2api/issues/9
[i10]: https://github.com/yandhi-0421/acp2api/issues/10
[i11]: https://github.com/yandhi-0421/acp2api/issues/11
[i13]: https://github.com/yandhi-0421/acp2api/issues/13
[i14]: https://github.com/yandhi-0421/acp2api/issues/14
[i15]: https://github.com/yandhi-0421/acp2api/issues/15
[i20]: https://github.com/yandhi-0421/acp2api/issues/20
[i21]: https://github.com/yandhi-0421/acp2api/issues/21
[i24]: https://github.com/yandhi-0421/acp2api/issues/24
[i25]: https://github.com/yandhi-0421/acp2api/issues/25
[i27]: https://github.com/yandhi-0421/acp2api/issues/27
[i28]: https://github.com/yandhi-0421/acp2api/issues/28
