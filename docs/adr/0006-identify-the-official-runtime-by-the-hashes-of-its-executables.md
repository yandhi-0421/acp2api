# Identify the official runtime by the hashes of its executables

acp2api identifies a build of the official runtime by the SHA-256 values of its two executables, before it starts a runtime process. It starts only an accepted version, from its own copy in the data folder of acp2api. A compatibility check on the computer of the user can make a new build an accepted version without a new release of acp2api.

## Rules

- **Build.** A build is the pair of SHA-256 values of `agy_acp_server` and `localharness_external`, with the file extension of the platform.
- **List.** Each release of acp2api holds a list of the builds that the acceptance test of the project passed, with their versions.
- **Platform.** Each executable of acp2api accepts only builds for its own operating system and CPU architecture.
- **Check before the start.** Before each start of a runtime process, acp2api calculates the two hashes of its copy. It starts the runtime process only when the build is an accepted version.
- **Check after `initialize`.** `agentInfo.version` must agree with the version that the list or the record of the compatibility check gives.
- **Compatibility check.** Only the user starts it, in the management panel. When it passes, acp2api writes the two hashes and the version into its state file.
- **Copy.** acp2api starts each runtime process from its own copy of the two executables. It keeps one copy.
- **Automatic copy.** When acp2api finds a build that is on the list, it can copy it with no step of the user. A build that is not on the list gets a copy only after the user starts its compatibility check.
- **No download.** acp2api never downloads the official runtime.

## Reasons

- The official runtime has no updater, and the ACP Registry gives no checksum for its archives (`docs/research/2026-10-15-official-runtime-acp-probe.md`, section 3.2).
- Each start of the official runtime refreshes the access token with Google ([Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8), F11). On Windows, a start took 47.5 to 156.8 s in the VM, and it unpacked 341 MB ([Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16), W3). A check of the hashes before the start avoids both for a build that acp2api does not accept.
- A hash also finds a damaged or changed file, and a new build with an old version number. The version in `agentInfo` does not.
- acp2api depends on the tool filter, `eagerMcpServers`, and the title `Run <tool>?` (ADR-0002), on `_meta.antigravity.systemPrompt.append`, and on the stderr lines of the lockout signals. A build that changes one of them breaks acp2api or hides a lockout signal.
- An editor updates its own copy of the official runtime. When acp2api runs that copy, an update can remove `localharness_external` while the runtime process runs. Windows does not replace an executable that runs.
- The user does not want each new build to wait for a release of acp2api.
- The Windows arm64 archive runs x64 Python under emulation, and only `localharness_external.exe` is native arm64 (issue 16, W1). An archive of the other CPU architecture gives no gain, and it doubles the acceptance work.

## Considered options

- **The version in `agentInfo` only.** It needs a start of the runtime process, with a token refresh. It does not find a damaged file or a new build with an old version number.
- **A start of the copy of an editor, in its place, and its updates.** acp2api needs no disk space for a copy. But an update of the editor can break a runtime process that runs, or bring a build that nobody checked.
- **Only the list of the release.** Each new build then waits for a release of acp2api.
- **A setting that allows each build with no check.** A build with a changed tool filter or changed stderr lines then runs with no warning.

## Consequences

- The compatibility check cannot check the stderr lines of the lockout signals, because a probe must not cause a 403. It also does not check three prompt turns at the same time or the memory that decides N (ADR-0004). The user accepted these risks for a build that only a compatibility check passed.
- The compatibility check sends 3 to 4 model requests through the Google account of the user.
- The copy takes about 400 MB on macOS and about 220 MB on Windows arm64.
- The state file of acp2api holds the records of the compatibility checks. They are stored data.
- Not tested: that a copy of the two executables starts on macOS and on Windows.

Evidence: the grilling of [How does the user install the official runtime and sign in, and how does acp2api find it?](https://github.com/yandhi-0421/acp2api/issues/22); F1 and F11 of issue 8; W1 and W3 of issue 16; the answer of [Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15); and sections 3.2 and 3.3 of the same note.
