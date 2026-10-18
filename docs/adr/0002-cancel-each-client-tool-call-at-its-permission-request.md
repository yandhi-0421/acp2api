# Cancel each client tool call at its permission request

acp2api gives the client tools of a request to the model as the tools of an MCP server of acp2api on 127.0.0.1. For each call of such a tool, acp2api uses cancel at permission: it cancels the prompt turn at the permission request, and it returns the tool call to the API client. The tool result goes to the model as text in a later prompt. The MCP server of acp2api lists the client tools, and it executes no tool call.

## Reasons

- The official runtime ends a held `tools/call` after 180 s. A tool of an API client can take longer, for example when Claude Code waits for the approval of the user.
- acp2api never allows a permission request of the official runtime. A held call needs an allow.
- No prompt turn stays open between two API requests. Thus a tool call keeps no runtime process alive, and it holds no place of the prompt turns that run at the same time.
- The next API request does not have to reach an open prompt turn. The tool result can go to any ACP session that holds the history of the conversation.
- On macOS, cancel at permission passed a result of about 50,000 characters, an image result, an error result, and a task with five calls. The task also continued after `session/load` in a new runtime process. On Windows, it passed the task with five calls.

## Considered options

- **Held call.** acp2api allows the permission request of its own tool, and it holds the `tools/call` until the API client sends the result. The model then gets a function result, and several calls can wait at the same time. The design breaks the rule against an allow, and a call fails when the API client needs more than 180 s. It also keeps the prompt turn, its runtime process, and its place open between two API requests.
- **Text protocol.** acp2api puts the tool descriptions into the prompt, and it parses the tool calls from the reply text. No open-source bridge of the official runtime tried this design. The parser is fragile, and acp2api must buffer the stream until it knows if the text is a tool call.

## Consequences

- acp2api uses two undocumented session options of the official runtime 1.4.0: the tool filter `_meta.antigravity.enabledTools: []` and `_meta.antigravity.eagerMcpServers`.
  - Without the tool filter, `view_file` and `manage_task` run with no permission request. `manage_task` also sends no `tool_call` update.
  - Without `eagerMcpServers`, the model reads the MCP tool schemas with `view_file`, and the tool filter removes `view_file`.
  - Thus acp2api accepts only a version of the official runtime for which an acceptance test checked both options. ADR-0006 gives the version check. A compatibility check on the computer of the user counts as such a test.
- With `eagerMcpServers`, the permission request has no `_meta.mcp`. Only its title `Run <tool>?` names the tool. Thus acp2api cannot tell a client tool from a built-in tool with the same name. The acceptance test must also check this title.
- The official runtime reads the tools of an MCP server at the first prompt turn of the session. In one run on macOS, it read them again after `notifications/tools/list_changed`, and the model called the added tool. acp2api sends no such notification. Thus acp2api gives one set of client tools to each ACP session. [Which ACP session does acp2api use for each API request?](https://github.com/yandhi-0421/acp2api/issues/10) decides what acp2api does when a request has another set, and ADR-0003 gives the reason.
- Each response of acp2api has no more than one tool call. Thus, when the model wants several calls at the same time, each call after the first probably needs one more model request. No probe tested parallel calls.
- The model gets each tool result as user text, not as a function result. Each cancel also adds a failed tool call and the text `The request was cancelled by the client.` to the history of the ACP session.
- A prompt turn that acp2api cancels at a permission request can end with no `usage_update`.
- Each case of the probes ran one time. A test with real API clients and longer tasks must check the quality of the tool use.

Evidence: the grilling of [How do client tools reach the model?](https://github.com/yandhi-0421/acp2api/issues/9), the answer of [How do the open-source Antigravity ACP bridges pass client tools and stop built-in tools?](https://github.com/yandhi-0421/acp2api/issues/3) with its addendum, F6 and F9 of [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), W6 of [Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16), and section 5 of `docs/research/2026-10-15-official-runtime-probe-p.md`.
