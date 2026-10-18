# Reach Google only through the official runtime

acp2api sends no request to Google itself. Each model request and each sign-in goes through the official runtime, and acp2api is only its ACP Client. acp2api never reads the sign-in, the tokens, or the keychain entries of the official runtime, and it never sends them in a request of its own. No fallback path calls a Google API directly.

## Reasons

- The account actions of Google in February 2026 mainly hit tools that called the Google backend with Antigravity or Gemini CLI OAuth tokens. Users reported HTTP 403 `PERMISSION_DENIED` with the reason `TOS_VIOLATION`.
- Such tools send no telemetry of a real client, copy a static `User-Agent`, and send requests of an abnormal shape. The research of the earlier CLI bridge of the maintainer found no report of mass bans of tools that drive a real Google client.
- This design decreases the risk, but it does not remove it. Two forum users later reported a 403 `TOS_VIOLATION` with only the official `agy`, one of them with scheduled `agy -p` calls. Google confirmed no cause.
- The official runtime makes each request itself. Thus the shape of the request and the sign-in traffic are those of a client that Google publishes. Its telemetry is that of the official runtime too, but the official runtime 1.4.0 drops some telemetry batches with `oauth-personal`.
- The user fixed these rules for the whole effort at the chart of the map on 2026-10-17: acp2api only translates, it reads no credentials, and it never connects to Google itself.

## Considered options

- **Direct API replay.** acp2api reads the OAuth token of the official runtime or of `agy`, and it calls the Google backend `cloudcode-pa.googleapis.com` itself. This design is the simplest and the fastest. It does not need the two executables of about 400 MB, a start of up to 157 s in the Windows VM, or a tool filter. It can also give exact token usage and thought text. But it is the pattern of the bans of February 2026, and it breaks the rule that acp2api reads no credentials.
- **A local language server of the Antigravity apps.** The earlier CLI bridge examined it and deferred it. The interface has no documentation, and it changes with each version of the app.

## Consequences

- acp2api has the limits of the official runtime: its start time, its memory, the undocumented tool filter, token usage with only the context size, and no thought text.
- The use of acp2api still breaks the Antigravity terms. The terms forbid access to the service through third-party software. This design decreases the chance that Google finds acp2api. It does not make the use agree with the terms.
- The official runtime puts `acp2api/<version>` into the `User-Agent` of its requests. Thus Google can find and block the requests of acp2api.
- The prompts hold the content of the API clients, not the content of an editor session. The design does not remove this difference.
- A live runtime process can still connect to Google, for example for a token or for telemetry. Only a stop of the runtime process makes sure that nothing goes to Google.
- A later version must not add a direct call to Google, also not as a fallback, and also not for quota data or for the model list.

Evidence: the scope and the rules of the map [Map: PRD of acp2api v1](https://github.com/yandhi-0421/acp2api/issues/1); the answer of [What do the Google terms and the February 2026 account actions say about third-party ACP Clients?](https://github.com/yandhi-0421/acp2api/issues/4); the answer of [Which `clientInfo` does acp2api send to the official runtime?](https://github.com/yandhi-0421/acp2api/issues/19); the answer of [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15), for the live process; the stderr lines of [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), for the telemetry batches; W3 of [Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16), for the start time; section 3.3 of `docs/research/2026-10-15-official-runtime-acp-probe.md`, for the sizes of the two executables; `docs/research/2026-10-03-antigravity-proxy-landscape.md`, for the patterns of the Antigravity proxies and the ban factors; and `docs/research/2026-10-18-earlier-cli-bridge-findings.md`, for the reasons of the earlier CLI bridge.
