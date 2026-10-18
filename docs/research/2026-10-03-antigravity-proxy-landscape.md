# Antigravity proxy landscape and account bans

Date checked: 2026-10-03

Provenance: The maintainer wrote this note on 2026-10-03 for an earlier project, a bridge that used the Antigravity CLI (`agy`) as its engine. The note moved to acp2api on 2026-10-18, because that project will be deleted. The move removed the links to the files of that project and changed no fact.

Words of the earlier project:

- the earlier bridge (the earlier CLI bridge): the project for which the maintainer wrote this note. The original text called it "this project".
- Gen 1, Gen 2, Gen 3: the three implementation patterns of section 1.
  - Gen 1: a third-party program uses the account credentials to send its own requests directly to the Google API.
  - Gen 2: the bridge runs the language server from the official installer on the local machine.
  - Gen 3: the bridge drives a real Antigravity client program while it runs.
- language server: the `language_server` binary in the official Antigravity installer. It is the part of the Antigravity client that talks to Google.
- desktop app: the agent-first Antigravity application, `Antigravity.app`.
- IDE: the Antigravity code editor, `Antigravity IDE.app`, a fork of VS Code.
- Cascade: an agent conversation in Antigravity, which the language server runs. It is not an ACP session.
- vm2api: a pattern of the Claude Code domain. The official client runs in an isolated VM, which copies the hardware identifiers and uses its own SOCKS5 egress.
- egress: the network path and the IP address that the traffic of the client uses to go to Google.
- ban: a 403 Terms of Service block that Google puts on an account. It is an action of Google, not a lockout of acp2api.
- quota stop: a rule of the earlier bridge that refused new requests for a group of models at a use threshold. It is not the quota wait of acp2api.

Method: we read GitHub repositories, source code, and issues directly. We compared these with community reports from linux.do, V2EX, and discuss.ai.google.dev.

## Summary

- The community uses three implementation patterns: Gen 1, Gen 2, and Gen 3.
- The ban wave hit the Gen 1 users: credential replay directly to the Google API.
- For one person with one account, four factors cause most of the risk: many accounts, quota exhaustion, data-center IP addresses, and no telemetry. For the earlier bridge, the note concluded that the architecture can prevent all four.

## 1. Three implementation patterns

### Gen 1: credential replay and direct API emulation

Gen 1 is the most common pattern. Most bans hit Gen 1 users.

| Project | GitHub stars | Type |
|---|---|---|
| [router-for-me/CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI) | 53.9k | Go service. Converts many CLIs to one API. |
| [NoeFabris/opencode-antigravity-auth](https://github.com/NoeFabris/opencode-antigravity-auth) | 11k | OAuth plugin for OpenCode. |
| [justlovemaki/AIClient2API](https://github.com/justlovemaki/AIClient2API) | 8.8k | Node service. Converts many clients to one API. |
| [badrisnarayanan/antigravity-claude-proxy](https://github.com/badrisnarayanan/antigravity-claude-proxy) | 4k | Anthropic-compatible proxy. Web panel for many accounts. |

The technical data below comes from the CLIProxyAPI and AIClient2API source code, as of 2026-10.

**Authentication.** These projects use the public OAuth client of the Antigravity desktop client. This client comes from Codeium. The data is:

- `ClientID`: the OAuth client ID of that client. This copy leaves out the value.
- Callback port: 51121.
- Scopes: `cloud-platform`, `userinfo.email`, `userinfo.profile`, `cclog`, and `experimentsandconfigs`.

As an alternative, the projects read the sign-in session of the user from the local Antigravity database, `state.vscdb`.

**Endpoints.** The projects send requests to `https://cloudcode-pa.googleapis.com`. They also use `https://daily-cloudcode-pa.googleapis.com`, which is the preview channel. The API version is `v1internal`. The methods are:

- `:streamGenerateContent`
- `:generateContent`
- `:countTokens`
- `:onboardUser`. This is a long-running operation (LRO). Its metadata contains `{ide_type: ANTIGRAVITY, ide_version, ide_name}`.
- `:loadCodeAssist`
- `:fetchAvailableModels`

**Request construction.** The projects put the Gemini-format body in an envelope: `{"project":"<id>","request":{...},"model":"..."}`. They also do these steps:

- They clean the thought signatures.
- They normalize the function declarations (`parameters` → `parametersJsonSchema`).
- They add the default `safetySettings`.

The projects get the project ID from the `onboardUser` LRO poll. If the poll fails, they make a random local ID as a fallback.

**Header policy.** The projects use an allowlist. They remove all headers that come from the caller. Then they set only three headers:

- `Content-Type`
- `Authorization: Bearer`
- A copied `User-Agent` with the form `antigravity/<ver> darwin/arm64`, for example `antigravity/2.8.1 darwin/arm64`.

Some auxiliary endpoints use `google-api-nodejs-client/10.3.0` or `gl-node/22.21.1`.

**Version gate.** The server makes models available by client version. It refuses new models to clients below version 2.9.0. For this reason, CLIProxyAPI reads the official Google update manifest every 3 hours. The manifest is at `antigravity-hub-auto-updater-…run.app/manifest/latest-arm64-mac.yml`. CLIProxyAPI copies the version number from the manifest. Its fallback version is 2.9.1.

**Primary defect: no telemetry.** The OAuth request asks for the `cclog` scope (Cloud Code log). But the CLIProxyAPI source code has no `onUserEvent` call and no other telemetry call. The real client sends telemetry and lifecycle events all the time. Direct API replay does not. This difference is the most constant signal of a client that is not real. A proxy can copy a `User-Agent`. It cannot copy a behavior stream.

### Gen 2: native language server bridge

In the Gen 2 pattern, the real language server sends the requests. Thus the request features can be the same as the features of the real client.

The example is [lbjlaq/Antigravity-Tools-LS](https://github.com/lbjlaq/Antigravity-Tools-LS) (428 stars, Rust). It applies the vm2api philosophy to Antigravity:

- An asset supplier extracts the real language server binary and its certificates. It gets them from the official installer packages (`.dmg`, `.deb`, `.exe`, `.tar.gz`). It can synchronize the version automatically. Antigravity-Tools-LS calls this binary `ls_core`.
- The tool emulates the extension server. It gives the OAuth token to the language server.
- The README says that the language server process opens its own HTTPS and gRPC connections to the Google back end. The code does not agree. Refer to the correction below.
- The application layer parses the stream output of the language server. It translates the output to the OpenAI API, the Anthropic API, and the Gemini API. It supports the Cascade agent, tool calls, and image generation.
- An LRU pool controls the language server process instances.

Related risk: the tool also has an account switch at the database level. It writes to `state.vscdb` in one atomic operation. But an account switch is a confirmed ban trigger. Refer to the V2EX report in section 2.

**Correction (2026-10-03).** The first version of this note copied the README claim above. A later source code check found these facts:

- The bridge sets the Cloud Code endpoint of the language server to its own local proxy. The proxy sends each Cloud Code request again with its own HTTP client.
- The bridge itself sends OAuth, quota, and update requests to Google.
- The tool gives the language server a false `device_fingerprint` and writes new telemetry IDs on an account switch.

Thus Antigravity-Tools-LS is not a true Gen 2 implementation. The evidence is in section 2 of a note of the earlier bridge on the local engine interface: a static read of the Antigravity-Tools-LS source at commit `d312237` (2026-03-27).

### Gen 3: drive the real app while it runs

Gen 3 has the highest fidelity.

The example is [jackwener/open-antigravity](https://github.com/jackwener/open-antigravity) (207 stars). It has one condition: the Antigravity desktop app must run on the local machine, with a minimum of one open workspace.

Note (2026-10-03): open-antigravity targets the Antigravity editor from before the product split. Antigravity now has two products: the desktop app and the IDE. open-antigravity also reads an `apiKey` from `state.vscdb`. The details are in section 1 of a note of the earlier bridge on the local engine interface: a static read of open-antigravity at commit `fab7583` (2026-03-21).

- The bridge service uses a local interface to create or reuse a Cascade. It uses the headers `x-workspace` and `x-conversation-id`.
- The bridge gives API clients an OpenAI-compatible and Anthropic-compatible `base_url` and key.
- All traffic to Google comes from the real client.
- linux.do users report "据说不会封号" ("it is said that this does not cause bans").

Costs:

- The desktop app and the workspace must stay alive.
- The bridge gets a Cascade, not a raw model API.
- One Cascade limits the throughput.

### Related tools

These tools are not account proxies. But they are related to the earlier bridge.

- [yuaotian/antigravity-proxy](https://github.com/yuaotian/antigravity-proxy) (4.3k stars). It injects a DLL into Antigravity. The DLL sends all Antigravity traffic through SOCKS5, with no TUN device. It prevents timeouts at login and at `daily-cloudcode-pa`. It is a tool to control the network egress.
- Tools of the same type:
  - ProxyLane. You drag a process onto it, and it sends that process through the proxy.
  - antigravity-2.0-no-tun-login-proxy. It uses a `version.dll` injection.
  - [OpsinTech/AntigravityProxyLauncher](https://github.com/OpsinTech/AntigravityProxyLauncher). It uses macOS DYLD injection, FakeIP, and MITM translation.
- [lbjlaq/Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager). It is an account and quota manager. Its issue tracker is the best place to monitor ban signals.

## 2. Account bans

### Timeline

Each item has more than one source.

- **2026-02, the ban wave.** Google sent a large number of 403 ToS bans. The error text is `"Gemini has been disabled in this account for violation of Terms of Service"`. The bans hit paid subscribers. Some of them were normal users with no proxy (false positives). Related posts on discuss.ai.google.dev:
  - *"Code Assist Disabled (403 ToS), No Email Response for 8 Days"* (2026-02-26).
  - *"Prevent don't scold as general guidance — and how about not banning paying users"* (2026-03-08).
  - *"Eligibility check failed: PERMISSION_DENIED (code 403) appeared while using Antigravity CLI"* (2026-06-24).
- **2026-02-13, V2EX.** Post title: *"谷歌封禁大量 Antigravity OAuth 登录账号"* ("Google bans many Antigravity OAuth login accounts").
  - Some users only switched accounts with Antigravity Tools. They also got a ban.
  - Google considers an account switch as a bypass of the quota defense.
  - An unban depends on luck and on an email appeal.
- **2026-02-26/27, the "amnesty".** In [Antigravity-Manager #2143](https://github.com/lbjlaq/Antigravity-Manager/issues/2143), the title is *"之前被 403 封号的账号集体解封了，谷歌大赦天下"* ("all accounts with a 403 ban are free again, Google gives an amnesty").
  - Google removed the ban from these accounts at the same time.
  - But many accounts lost the subscription: *"被封的号都解了，但是会员全掉了"* ("all banned accounts are free, but all memberships are gone").
  - Old accounts usually kept the membership. New accounts usually lost it.
- **After the amnesty, a second ban wave.** Some users started the proxy again. Google banned their accounts again in a few days.
  - *"今天早上起来，2个PRO给封了，我就是刚把周限用完了"* ("this morning, two PRO accounts had a ban; I had just used all of the weekly limit").
  - *"下午账号被封了"* ("the account got a ban in the afternoon").
  - The same issue has reports of free accounts. These accounts used the proxy all the time and got no ban.
  - Free-tier accounts seem to have a lower ban risk. PRO accounts are the primary target.
- **Current status.** The READMEs of the main projects show warnings:
  - antigravity-claude-proxy: *"Google has been issuing ToS violation bans on accounts connected to this proxy"*.
  - opencode-antigravity-auth: *"A number of users have reported their Google accounts being banned or shadow-banned"*.
  - Both projects tell users not to use their primary account.

### Ban factors

This list comes from community evidence. It is not an official statement from Google.

1. **No telemetry.** Gen 1 asks for the `cclog` scope but never sends logs. Thus its behavior stream is different from the behavior stream of the real client. The difference is constant, and a proxy cannot fake it.
2. **Many accounts and account switches.** An account switch at the `state.vscdb` level causes a ban (V2EX report, 2026-02-13).
3. **Quota exhaustion with a proxy.** Google banned an account just after the user used all of its weekly limit (the second ban wave in #2143).
4. **Egress IP.** Data-center IP addresses and IP addresses that rotate have a higher risk than residential IP addresses.
5. **Request shape and content.** A linux.do user wrote: *"antigravity 要是真想封号，直接看你的提示词进行封号"* ("if Antigravity really wants to ban you, it can ban you from your prompts"). Google can use the content to see the difference between a raw API session and a Cascade task.
6. **Concurrency and rhythm.** A bot can send requests 24 hours a day, 7 days a week, and in parallel. A human user does not.
7. **Rough enforcement.** The ban waves also hit normal paid users (false positives). The responses to appeals are slow. In one case, there was no reply after 8 days.

### Conclusions for the earlier bridge

The user scenario of the earlier bridge is: one person, one account, a paid subscription, home broadband, and personal use only.

- Factors 2, 3, and 4 do not apply to this scenario.
- The architecture choice controls factors 1, 5, and 6.
- Gen 2 and Gen 3 remove factors 1 and 5. The real client binary sends its own telemetry and makes requests with the real shape.
- Gen 1 shows all of these factors to Google.

The earlier bridge recorded its architecture decision in its ADR-0001. An amendment of 2026-10-07 to that ADR says that Gen 2 and Gen 3 remove factor 5 for the request shape, but not fully for the content. [2026-10-18-earlier-cli-bridge-findings.md](2026-10-18-earlier-cli-bridge-findings.md), section "Reasons of the earlier bridge for its architecture", gives the reasons of that ADR.

## 3. Map the vm2api philosophy to Antigravity

The right column gives the plan of the earlier bridge.

| vm2api (Claude Code domain) | The earlier bridge |
|---|---|
| One VM for each slot. The real Claude Code binary runs in the VM. | The real language server runs on the local Mac (Gen 2). Or the real desktop app stays alive (Gen 3). |
| The Console Setup Token is the identity. Accounts never mix. | One Google account. Never switch it. Do not read or write `state.vscdb`. |
| Each slot has its own sticky SOCKS5 egress. | Home broadband is the egress. If a proxy is necessary, use one fixed node. Never rotate it. |
| All telemetry goes through. | The real client binary sends its own telemetry. The bridge does not filter it. |
| No prompt injection. | The bridge sends the prompts through with the semantics of a coding agent conversation. It does not replace the system prompt. |
| Native subagent concurrency is 20 or less. | The concurrency limit is 1, in series, as a real IDE user works. The quota stop is at 70–80%. |

acp2api: acp2api drives the official runtime `agy_acp_server`, not the language server or the desktop app, see [What does the official runtime support over ACP on Windows and macOS?](https://github.com/yandhi-0421/acp2api/issues/2).

acp2api: by default 2 requests run on the official runtime at the same time, and the user can set 1 to 3, see [How many prompt turns run at the same time, and what does a queued request get?](https://github.com/yandhi-0421/acp2api/issues/14).

acp2api: a quota signal starts a quota wait until the reset time, see [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15).

The Gen 1 projects show that one vm2api lesson is correct. vm2api says: do not remove telemetry, and do not fake fingerprints. The Gen 1 projects fake only a part of the client. They use a header allowlist and a copied `User-Agent`. They can copy the static fingerprint. They cannot copy the dynamic behavior stream.
