# First release executables: archive sizes and resident memory

Date: 2026-11-02 (Asia/Taipei). Issue: [#56](https://github.com/yandhi-0421/acp2api/issues/56).
Local source: `366a7905455324cacb7cf7de83b269470c2be8aa`, plus the release workflow of this issue.
The product source, dependencies, PRD, and acceptance contract have no changes.

This note supplies evidence for OD-2 of `docs/prd/acp2api.md`.
It sets no memory limit or executable size limit.
The owner still decides those limits.

## Local compilation conditions

- Host: macOS 27.2, build 26B118, arm64.
- Compiler: `rustc 1.96.0 (ac68faa20 2026-05-25)`.
- Release profile: the Cargo default, with no `CARGO_PROFILE_RELEASE_*` override.
- Compilation command: `cargo build --release --locked -p acp2api --bin acp2api --target <target>`.
- Compilation limits: four jobs, no incremental compilation, 900 seconds per command, 4 MiB per command log.
- The local default toolchain is 1.96.0. The workflow selects that same version explicitly.

The target triples are `aarch64-apple-darwin` and `x86_64-apple-darwin`.
The x64 compilation used a copy of the installed toolchain, with its x64 standard library.
Both Windows MSVC targets passed local clippy checks with the workflow's static CRT flag.

## Archive sizes

The workspace version is `0.1.0`.
The [GitHub dry run 41473254093](https://github.com/yandhi-0421/acp2api/actions/runs/41473254093) passed all four compilation, archive, and start checks.
The checksum job also passed. The draft job was skipped.
The probe branch permitted branch pushes and applied the version check to tag refs.

The table below contains values from that GitHub run.
Each macOS archive contains `acp2api`, with executable permission. Each Windows archive contains `acp2api.exe`.
Each archive contains one regular file.

| Target | Executable, bytes | Archive, bytes |
|---|---:|---:|
| `aarch64-apple-darwin` | 5,929,728 | 2,334,575 |
| `x86_64-apple-darwin` | 6,136,772 | 2,421,133 |
| `x86_64-pc-windows-msvc` | 6,424,064 | 2,348,498 |
| `aarch64-pc-windows-msvc` | 5,250,560 | 2,148,319 |

The macOS archive names are `acp2api-0.1.0-<target>.tar.gz`. The Windows names end in `.zip`.
[The run capture](captures/2026-11-02-first-release-build/github-run-41473254093.json) contains the four sizes, start lines, runner images, and job links.

The next table contains the earlier local measurements. An extraction and byte comparison passed for both local archives.
[The local size capture](captures/2026-11-02-first-release-build/sizes.json) also contains their SHA-256 values.

| Local target | Executable, bytes | Archive, bytes |
|---|---:|---:|
| macOS arm64 | 5,929,728 | 2,334,666 |
| macOS x64 | 6,142,556 | 2,423,683 |

The local x64 executable is 5,784 bytes larger than the GitHub executable. Its archive is 2,550 bytes larger.
No controlled comparison isolated the cause of this difference.

The [earlier run 41473019529](https://github.com/yandhi-0421/acp2api/actions/runs/41473019529) failed the version check in all four jobs.
The prior version check treated a branch push as a tag push.
[Its metadata capture](captures/2026-11-02-first-release-build/github-run-41473019529.json) preserves those failures.

## Memory population and limits

**Scope:** these memory values belong to an isolated measurement host, not the original archive executable.
The local archive executable could not write its account coordination data outside the permitted write roots.
Its [local start check](captures/2026-11-02-first-release-build/local-start-failure.json) failed with M-28 before the management panel address appeared.

The measurement host links the same optimized acp2api library as the arm64 archive executable.
It calls `acp2api::run` with `Ports::production()` and real time.
Its only change to `Environment` selects a new account coordination folder inside the writable test root.
The product executable has no new test option.

The load probe uses the acceptance crate's `fake-acp-agent` and `Hub`.
It seeds a configuration file before the product starts. Thus its table excludes first-start password hash generation.

The fixture seeds the state file with accepted hashes for the fake ACP agent and its descriptor.
It does not run a compatibility check or admit a real official runtime.

The fake ACP agent, its child processes, the controller, and the load generator are outside the measured PID.
No official runtime, keychain entry, or Google account is used.

Each trial has these limits:

- Three HTTP requests, all to the local product host.
- Two concurrent streams, then one stream after a replacement of the runtime process.
- A 75-second probe watchdog and an 80-second measurement deadline.
- A 15-second wait for each condition and each response read.
- A 1 MiB response cap and a 64 KiB captured output cap.
- A 64 KiB cap for each product log, checked at each sample.
- Each fake ACP process has a 500-line cap, a 1 MiB line cap, and a 60-second deadline.
- The outer command has a 240-second deadline and a 4 MiB log cap for all three trials.

The workload uses short synthetic prompts and the reply `probe-reply`.
The first two streams use Messages and Responses. Each stays open for about three seconds.

The controller then makes the fake runtime process exit.
acp2api starts a replacement. A Chat Completions stream verifies recovery.
This exercises failure recovery, not the eight-session replacement threshold of FR-041.

All three responses had HTTP 200, the expected reply, and the correct terminal stream event in each trial.
The fake reported no failure. Each trial recorded exactly two runtime process starts.

## Resident memory results

A separate first-start trial used a new empty data folder, with no fake ACP agent or HTTP request.
It used the same isolated host, production ports, and native account coordination override.
The observer sampled RSS about every 5 ms, through one second after the address and five seconds of idle time.
The run had a 30-second deadline, a 64 KiB terminal cap, and a 64 KiB cap for each product log.
The outer command had a 45-second deadline and a 4 MiB log cap.

| Empty data folder, one trial | Steady RSS, bytes | Observed peak RSS, bytes |
|---|---:|---:|
| Before the management panel address | 23,658,496 | 25,640,960 |
| First second after the address | 25,821,184 | 25,821,184 |
| Next five idle seconds | 25,821,184 | 25,821,184 |

The first-start observed peak was 24.625 MiB.
[The cold-start capture](captures/2026-11-02-first-release-build/cold-memory.json) retains this trial.
[The cold-start probe](captures/2026-11-02-first-release-build/cold_start.py) repeats it with zero HTTP requests.
This trial shows why an initialized configuration must not stand for an empty data folder.
The three-trial table below measures the initialized configuration and fake ACP workload.

The observer calls macOS `proc_pidinfo` with `PROC_PIDTASKINFO = 4`.
It reads `pti_resident_size` for the product host PID about every 50 ms.
The steady value is the median of samples from the last second of each phase.
The table gives the range of those medians across three trials.
The peak is the largest observed sample across the same three trials.

| Phase | Steady RSS range, bytes | Observed peak RSS, bytes |
|---|---:|---:|
| After the management panel address | 6,291,456–6,340,608 | 6,340,608 |
| Idle for five seconds | 6,291,456–6,340,608 | 6,340,608 |
| Two open streams | 7,487,488–7,569,408 | 7,569,408 |
| After those streams | 7,553,024–7,569,408 | 7,634,944 |
| Runtime process replacement | 7,716,864–7,766,016 | 7,782,400 |
| After recovery and the third stream | 7,733,248–7,798,784 | 7,798,784 |

The largest observed value in the initialized trials is 7.4375 MiB, where 1 MiB is 1,048,576 bytes.
The three recovered-phase peaks were 7,749,632, 7,798,784, and 7,733,248 bytes.
The [first](captures/2026-11-02-first-release-build/memory-1.json), [second](captures/2026-11-02-first-release-build/memory-2.json), and [third](captures/2026-11-02-first-release-build/memory-3.json) captures retain the samples and request evidence.

These are sampled RSS values. A shorter transient between samples can be missed.
RSS differs from private memory, physical footprint, and the total memory of a process tree.
The phase pauses and three-request workload bound this observation; it is not a capacity or throughput test.
The probe does not establish a production maximum for long prompts, large responses, or long uptime.
The isolated host also leaves the original archive's memory behavior unproven.

## Evidence and repeat commands

The [capture directory](captures/2026-11-02-first-release-build/) retains redacted JSON results and probe sources. It contains no executable or archive.
[The index](captures/2026-11-02-first-release-build/capture-index.json) lists file hashes and the two wrapper adaptations.
[The redaction result](captures/2026-11-02-first-release-build/redaction-scan.json) reports zero unredacted hits before the files entered the repository.

The measurement host and Rust probe sources retain their original hashes.
The two wrappers use `ACP56_PROBE_OUTPUT` for new outputs. They keep historical captures intact.
The command wrapper preserves the original timeout, log cap, and compilation environment.

Use Rust 1.96.0 for a repeat on macOS arm64.
After all earlier test processes exit, run these commands from the repository root:

```sh
captures=docs/research/captures/2026-11-02-first-release-build
export ACP56_PROBE_OUTPUT="$HOME/acp2api-build/56-repeat"
python3 "$captures/run.py" repeat-memory 1800 python3 "$captures/reproduce_memory.py"
python3 "$captures/run.py" repeat-cold-memory 45 python3 "$ACP56_PROBE_OUTPUT/cold_start.py"
python3 "$captures/run.py" repeat-native-compile 900 cargo build --release --locked -p acp2api --bin acp2api --target aarch64-apple-darwin
python3 "$captures/run.py" repeat-native-package 30 python3 .github/release.py package aarch64-apple-darwin
python3 "$captures/run.py" repeat-native-start 90 python3 .github/release.py start aarch64-apple-darwin
```

[The reproduction wrapper](captures/2026-11-02-first-release-build/reproduce_memory.py) compiles the workspace, arm64 release executable, and two probe programs.
It then runs three trials in sequence. New results and probe executables go to `ACP56_PROBE_OUTPUT`.
The final command needs write access to the native account coordination folder.

For the x64 release executable, install the x64 standard library for Rust 1.96.0 first.
Then use the same compilation and package commands with `x86_64-apple-darwin`.
For the four native platform checks, run `.github/workflows/release.yml` through `workflow_dispatch`.
That event creates artifacts and `SHA256SUMS`, but no GitHub release.

## Evidence for the release scenarios

Both scenarios in `features/acp2api/release-archives.feature` retain `@wip`.
Their Given steps have no definitions in the Cucumber runner.
That runner does not fetch a GitHub release or execute all four target platforms.
No acceptance contract file changed.

| Scenario | Executable check | Current proof |
|---|---|---|
| A release has one archive for each target platform | `checksums` validates four archive names and their contents. `draft` verifies five draft assets. | Local boundary checks passed. GitHub draft contents remain unproven. |
| The executable of each archive starts on its platform | Each native matrix job unpacks its archive and waits for the versioned management panel address. | All four starts passed in GitHub run 41473254093. The earlier local arm64 start failed with M-28. |

The runner choices are `macos-latest`, `macos-15-intel`, `windows-latest`, and `windows-11-arm`.
GitHub lists each label for public repositories in its [runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
The Intel macOS job uses a native runner. It does not need Rosetta.
The successful run used these images: `macos-26-arm64`, `macos-15` (Intel), `windows-2025-vs2026`, and `windows-11-vs2026-arm64`.

The Windows jobs select static CRT linkage to keep the archive to one executable.
Rust documents that flag in its [linkage reference](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes).

At a first start, [the product opens the default browser](../../crates/acp2api/src/startup.rs#L153).
The archive check stops acp2api but does not close that browser. It can remain on the runner until job cleanup.
This known effect did not block the successful dry run. The product behavior has no change.

After an upload, the draft check reads the [GitHub asset digests](https://docs.github.com/en/rest/releases/assets#get-a-release-asset).
It compares each archive digest with `SHA256SUMS`. It also compares the manifest asset digest with the local manifest file.
An absent or different digest fails the check.

An interrupted upload can leave old and new assets together in the draft.
The final digest check rejects a mixture of old and new assets.
The owner publishes after all checks pass. A real draft upload with this digest check remains unproven.
