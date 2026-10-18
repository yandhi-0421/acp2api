# Run the official runtime in a runtime home of acp2api

acp2api starts each runtime process with `GEMINI_HOME` set to a runtime home that acp2api owns, not the default `~/.gemini`. Thus no global configuration of the user reaches an ACP session, and each session file in that runtime home belongs to acp2api. acp2api deletes each session file that it does not need again.

## Rules

- **Runtime home.** acp2api sets `GEMINI_HOME` of each runtime process to one folder in the data folder of acp2api.
- **Sign-in.** The user signs in through the sign-in entry of the management panel. A sign-in of an editor or of another ACP Client in `~/.gemini` does not sign in acp2api.
- **Login files.** acp2api never reads, changes, or deletes the files of the sign-in in the runtime home, for example `settings.json` and `acp_token.json`.
- **Keychain.** acp2api does not set `AGY_ACP_FORCE_FILE_STORAGE`. Thus, on macOS, the official runtime keeps the personal token in the login keychain.
- **Session `cwd`.** The `cwd` of each ACP session is a new empty folder in the folder `acp2api` of the system temporary folder. It is not in the folder of a runtime process of ADR-0004. When the folder is absent before a `session/load`, acp2api makes it again.
- **Deletion at the start of acp2api.** Before acp2api starts its first runtime process, it deletes each session file in its runtime home and each `cwd` of an ACP session.
- **Deletion before each later start.** Before acp2api starts another runtime process, it deletes the session file and the `cwd` of each ACP session that has no record. ADR-0003 gives the drop of a record.

## Reasons

- The official runtime reads `config/mcp_config.json`, `config/hooks.json`, and the skills in `config/skills/` and `antigravity-cli/skills/` of its runtime home. Source: the agy-bridge note, section 3.3, row "Global files".
- The official runtime 1.4.0 tried to parse `~/.gemini/config/mcp_config.json` at the start of each agent session ([Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8)).
- The tool filter does not govern MCP tools. The permission request of an MCP tool names the tool only in its title `Run <tool>?` (ADR-0002). Thus acp2api cannot tell a tool of a global MCP server from a client tool with the same name.
- A hook or a skill of the user changes what the model does. The target of acp2api is a pure model, and acp2api only translates.
- In `~/.gemini`, the session files of acp2api and of the editors are in one folder. Thus a deletion there can remove a file of another ACP Client.
- The records of ADR-0003 are only in memory. Thus, after a restart of acp2api, no earlier session file is used again.
- A runtime process keeps the session files of its ACP sessions open, and no ACP method closes an ACP session (ADR-0004). Windows does not delete an open file. Thus acp2api deletes session files only when no runtime process runs.
- On macOS, the system temporary folder is not in the folder of the user. The macOS probe found no `.agents`, `.gemini`, `.git`, `AGENTS.md`, or `GEMINI.md` in the parent folders of such a `cwd` (issue 8). A data folder in the folder of the user can have `~/.gemini` or a `.git` in a parent folder.
- A later runtime process can load the ACP session. Thus its `cwd` is not in the folder of the runtime process, which acp2api deletes after the stop (ADR-0004).
- The login keychain protects the token better than a file. acp2api serves one Google account, so an editor and acp2api can share the token of that account.
- The official runtime 1.4.0 obeys `GEMINI_HOME`. Its stderr says `Gemini home resolved to ~/.gemini (default; $GEMINI_HOME is unset)` (issue 8). A `session/resume` of an unknown ACP session gives `-32002` `Session not found in the current GEMINI_HOME` (agy-bridge note, section 3.2).

## Considered options

- **The default `~/.gemini`.** A sign-in of an editor then also signs in acp2api. But each global MCP server, hook, and skill of the user reaches the ACP sessions of acp2api. The session files of acp2api also mix with other session files.
- **No deletion of session files.** Each rebuild and each new conversation adds one session file (ADR-0003). Nobody measured the size of a session file.
- **A list of the session files in the management panel, and a deletion by the user.** The user cannot see which session files acp2api still needs.
- **Deletion only at the start of acp2api.** It is simpler, but the session files then grow until the next restart of acp2api.
- **Each `cwd` in the data folder of acp2api.** All files of an ACP session are then in one place. But on macOS, a parent folder can then hold `~/.gemini`, a `.git`, or an `AGENTS.md` of the user.
- **`AGY_ACP_FORCE_FILE_STORAGE=1`.** The token then goes into the runtime home also on macOS, so no editor shares it. But a file protects the token less than the login keychain.

## Consequences

- The user signs in through the management panel one time, also when an editor signed in before. This replaces a reason of decision 3 of [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15): a sign-in outside acp2api no longer ends the state "sign-in required".
- On macOS, the official runtime keeps the personal token in the login keychain (service `gemini`, account `antigravity-acp`), not in the runtime home. Not tested: what a sign-in in the runtime home of acp2api does with that keychain entry.
- The acceptance test of a version checks that the official runtime reads no file of `~/.gemini`. It also checks that the settings, the token file on Windows, and the session files go into the runtime home.
- Only the static strings and one parse failure show that the official runtime loads global MCP servers, hooks, and skills. No run checked it.
- Between two starts of a runtime process, the session files grow with each new ACP session. The acceptance test of a version measures the size of a session file.
- On Windows, the system temporary folder is in the folder of the user. The acceptance test of a version checks that the official runtime reads nothing from the parent folders of a `cwd`.

Evidence: the grilling of [How does the user install the official runtime and sign in, and how does acp2api find it?](https://github.com/yandhi-0421/acp2api/issues/22) and [What does acp2api do with the conversation files of the official runtime?](https://github.com/yandhi-0421/acp2api/issues/31); F11 of issue 8; W5 of [Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16); C3 of [What does the official runtime support over ACP on Windows and macOS?](https://github.com/yandhi-0421/acp2api/issues/2); and the agy-bridge note, sections 3.2 and 3.3 ([link](https://github.com/yandhi-0421/agy-bridge/blob/cc55e4bcd8874357eaa1efbc6bd50c040d888fc2/docs/research/2026-10-15-antigravity-acp-server-engine.md)).
