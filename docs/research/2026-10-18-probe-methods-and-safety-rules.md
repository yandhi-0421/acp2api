# Methods and safety rules of the probes of acp2api

Date checked: 2026-10-18 (Asia/Taipei).

Provenance: This note collects the methods and the lessons of the probes and checks of acp2api on 2026-10-17 and 2026-10-18. The issues are [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), [Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16), [Probe the token count, the PDF block, and the slash commands of the official runtime on macOS](https://github.com/yandhi-0421/acp2api/issues/26), and [Check how Claude Code reads the token usage and handles a rejected server tool](https://github.com/yandhi-0421/acp2api/issues/27). Their resolution comments give the results and most settings. This note gives the knowledge that only the sessions of those probes had. It is for the agent that writes a fake ACP agent, a fake API client, an acceptance test, or a new probe.

## 1. Limits for each probe, fake endpoint, and fake agent

On 2026-10-17, a fake Anthropic Messages endpoint for the check of issue 27 went into a loop:

- The fake answered each request with a new tool call.
- Claude Code adds a `role: "system"` message after each tool result. The fake read only the last message. Thus it never saw the tool result, and it sent a new tool call each time.
- In about 6 minutes, the fake logged 5,372 request bodies, 15 GB. The disk of the Mac of the maintainer then had 103 MB free.
- The shell of the maintainer could not write its history. The tool calls of the agent session failed with `ENOSPC`, so the agent stopped the processes through a terminal.

Rules for each probe, each fake endpoint, each fake ACP agent, and each lever that logs runs:

- Give it a request cap. After N requests, it answers an error and stops its log.
- Give each run a timeout that stops the client.
- Give each log file a size cap.
- To find the tool result of the last tool call, read all messages after the last assistant message, not only the last message.
- The Mac of the maintainer has only about 15 GiB free. A runaway log stops the other work of the maintainer.

## 2. Redaction of captures

Captures go into the repository and into the text of issues. The redaction of the probes used these rules:

- Write `~` in place of the home path, and `<user>` in place of the name of the user.
- After a sign-in, the official runtime 1.4.0 writes the tier of the account to stderr. That line has an `upgradeUri` with the e-mail address of the account in URL-encoded form: `https://accounts.google.com/AccountChooser?Email=<name>%40<domain>&continue=…`. A rule that matches only `@` misses it. The Windows probe captured it 6 times, and a second pass replaced it before the commit.
- The sign-in address holds an OAuth `client_id`. Replace it. The probe of 2026-10-15 kept only the summary of its sign-in step, because the log of that step holds the sign-in address.
- Before a commit of captures, scan them for `%40`, `@`, `gmail`, the name of the user, and the home path.

The same stderr reaches acp2api at run time. The decision about logs and personal data must cover the URL-encoded address. The map lists that decision under "Not yet specified".

## 3. Network rules

- The probes of the official runtime ran with the network settings of the shell of the maintainer. That shell sets an HTTP proxy in `HTTPS_PROXY`, and the official runtime reaches Google through it.
- A check of an API client against a fake endpoint sends nothing to Anthropic, OpenAI, or Google. The check of issue 27 ran Claude Code 2.1.299 with these settings:
  - `env -i`, so that Claude Code gets none of the variables of the shell, also not the proxy of the maintainer;
  - a `sandbox-exec` profile that denies each outbound connection except to the fake and to one local proxy: `(version 1)(allow default)(deny network-outbound)(allow network-outbound (remote ip "localhost:<fake>"))(allow network-outbound (remote ip "localhost:<proxy>"))`;
  - `HTTPS_PROXY` and `HTTP_PROXY` to that local proxy, which logs and refuses each `CONNECT`;
  - a new `CLAUDE_CONFIG_DIR` with `hasCompletedOnboarding` and the trust flag of the work directory, `ANTHROPIC_BASE_URL` to the fake, and a made-up `ANTHROPIC_AUTH_TOKEN`;
  - for the advisor tool only, a copy of `cachedGrowthBookFeatures` from the configuration of the local Claude Code, because the advisor needs a feature flag that Claude Code fetches;
  - for the interactive mode, a Python pty driver, and a `statusLine` command that records its stdin JSON.
- The `claude` command is not on the `PATH` of the Mac of the maintainer. The desktop app of Claude ships it at `~/Library/Application Support/Claude/claude-code/<version>/<hash>/claude.app/Contents/MacOS/claude`.
- Claude Code tried to connect to `api.anthropic.com` and to a Datadog intake. In interactive mode, it also tried `github.com`, `raw.githubusercontent.com`, and `downloads.claude.ai`. Issue 27 lists the counts.

## 4. Model requests and the Google account

- Before the first prompt of a probe, the maintainer approves a number of prompts. For example, the maintainer approved 17 prompts for phase B of the Windows probe.
- A probe never causes a 403, a Terms of Service block, or a sign-out on purpose.
- A probe uses one account and changes no account. The macOS probes used the paid account of the maintainer. The Windows probe used a second account with no paid plan, tier `free-tier`.
- A lockout guard reads each stdout message and each stderr line of the official runtime. It looks for a 403, `PERMISSION_DENIED`, a text about the terms of service, a sign-out, and `Authentication required`. A hit stops the probe. In the Windows probe, `Authentication required` stopped two phases before the sign-in, as planned.
- A probe reads no credential file and no keychain entry of the official runtime.

## 5. Windows checks in a VM

- The maintainer has no Windows computer. A check with the real official runtime on Windows runs in a Windows 11 ARM VM (UTM) on the Mac of the maintainer. The disk of the VM is on an external drive.
- Before such a check, tell the maintainer. The maintainer must connect the external drive first.
- The first start of the official runtime after a boot of the VM can take more than 120 s. Use a timeout of 300 s for `initialize`. Issue 16, W3, gives the start times.
- Each start leaves a `_MEI*` folder in `%TEMP%`. Delete these folders only when no `agy_acp_server` process runs.
- The probe ran over SSH from the Mac. The SSH server of the VM needs a firewall rule for TCP port 22 from the Mac, or the Windows Firewall off.
- The proxy app of the maintainer routes the traffic of the Mac in TUN mode. Then a TCP connection to port 22 of the VM succeeds also when the VM is down. Wait for the `SSH-2.0-` banner, not for an open port.
- `utmctl` takes the name of the VM, not the name of its bundle. `utmctl stop <name> --request` shuts Windows down in about 30 s.
- CI cannot sign in to Google. Thus a CI runner on Windows can run only the tests with a fake ACP agent.
