# First release build: archive sizes and resident memory

Date: 2026-11-02 (Asia/Taipei). Issue: [#56](https://github.com/yandhi-0421/acp2api/issues/56).
Source: `366a7905455324cacb7cf7de83b269470c2be8aa`, plus the release workflow of this issue.
The product source, dependencies, PRD, and acceptance contract have no changes.

This note supplies evidence for OD-2 of `docs/prd/acp2api.md`.
It sets no memory limit or executable size limit.
The owner still decides those limits.

## Build conditions

- Host: macOS 27.2, build 26B118, arm64.
- Compiler: `rustc 1.96.0 (ac68faa20 2026-05-25)`.
- Release profile: the Cargo default, with no `CARGO_PROFILE_RELEASE_*` override.
- Build commands: `cargo build --release --locked -p acp2api --bin acp2api --target <target>`.
- Build limits: four jobs, no incremental compilation, 900 seconds per build, 4 MiB per command log.
- The local default toolchain is 1.96.0. The workflow selects that same version explicitly.

The target triples are `aarch64-apple-darwin` and `x86_64-apple-darwin`.
The x64 build used a copy of the installed toolchain, with its x64 standard library.
The Windows MSVC targets passed clippy on this Mac, including the workflow's static CRT flag.
This session did not link a Windows executable. No Windows executable or archive size was measured here.

## Archive sizes

The workspace version is `0.1.0`.
Each archive contains one regular file named `acp2api`, with executable permission.
An extraction and byte comparison passed for both archives.
These are local builds, not files from a GitHub dry run.

| Target | Executable, bytes | Archive, bytes |
|---|---:|---:|
| macOS arm64 | 5,929,728 | 2,334,666 |
| macOS x64 | 6,142,556 | 2,423,683 |
| Windows x64 | Not measured | Not measured |
| Windows arm64 | Not measured | Not measured |

The archive names are `acp2api-0.1.0-<target>.tar.gz`.
`sizes.json` in the evidence folder records the SHA-256 values of both executables and both archives.
The Windows jobs will record their sizes when the owner runs the workflow dry run.

## Memory population and limits

**Boundary:** these memory values belong to an isolated measurement host, not the original archive executable.
The archive executable could not write its account coordination data outside this session's writable roots.
Its local start check failed with M-28 before the management panel address appeared.
That check remains unproven locally.

The measurement host links the same optimized acp2api library as the arm64 archive executable.
It calls `acp2api::run` with `Ports::production()` and real time.
Its only change to `Environment` selects a new account coordination folder inside the writable test root.
It adds no test setting to the product executable.

The load probe uses the acceptance crate's `fake-acp-agent` and `Hub`.
It seeds a configuration file before the product starts. Thus its table excludes first-start password hash generation.
A new state file records the hashes of that fake as a checked build.
This fixture does not run a compatibility check or admit a real official runtime.
The fake ACP agent, its child processes, the controller, and the load generator are outside the measured PID.
No official runtime, keychain entry, or Google account is used.

Each trial has these limits:

- Three HTTP requests, all to the local product host.
- Two concurrent streams, then one stream after a replacement of the runtime process.
- A 75-second probe watchdog and an 80-second measurement deadline.
- A 15-second wait for each condition and each response read.
- A 1 MiB response cap and a 64 KiB captured output cap.
- A 64 KiB cap for each product log, checked during each sample.
- Each fake ACP process has a 500-line cap, a 1 MiB line cap, and a 60-second deadline.
- The surrounding command has a 240-second deadline and a 4 MiB log cap for all three trials.

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
The surrounding command had a 45-second deadline and a 4 MiB log cap.

| Empty data folder, one trial | Steady RSS, bytes | Observed peak RSS, bytes |
|---|---:|---:|
| Before the management panel address | 23,658,496 | 25,640,960 |
| First second after the address | 25,821,184 | 25,821,184 |
| Next five idle seconds | 25,821,184 | 25,821,184 |

The first-start observed peak was 24.625 MiB.
`cold-memory.json` retains this trial. `cold_start.py` repeats it with zero HTTP requests.
This trial shows why an initialized configuration must not stand for an empty data folder.
The following three-trial table measures the initialized configuration and fake ACP workload.

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
| During replacement | 7,716,864–7,766,016 | 7,782,400 |
| After recovery and the third stream | 7,733,248–7,798,784 | 7,798,784 |

The largest observed value in the initialized trials is 7.4375 MiB, where 1 MiB is 1,048,576 bytes.
The three recovered-phase peaks were 7,749,632, 7,798,784, and 7,733,248 bytes.
`memory-1.json` through `memory-3.json` retain the samples and request evidence.

These are sampled RSS values. A shorter transient between samples can be missed.
RSS differs from private memory, physical footprint, and the total memory of a process tree.
The phase pauses and three-request workload bound this observation; it is not a capacity or throughput test.
The probe does not establish a production maximum for long prompts, large responses, or long uptime.
The isolated host also leaves the original archive's memory behavior unproven.

## Repeat the checks on this Mac

The evidence folder is `~/acp2api-build/56-round1/`.
It retains the probe sources, the command wrapper, raw samples, archive files, and command results.
The command wrapper enforces timeouts and log caps. It also applies the task's build environment.
Run these commands from this worktree, after all earlier test processes have exited:

```sh
evidence="$HOME/acp2api-build/56-round1"
python3 "$evidence/run.py" repeat-memory 1800 python3 "$evidence/reproduce_memory.py"
python3 "$evidence/run.py" repeat-cold-memory 45 python3 "$evidence/cold_start.py"
python3 "$evidence/run.py" repeat-native-build 900 cargo build --release --locked -p acp2api --bin acp2api --target aarch64-apple-darwin
python3 "$evidence/run.py" repeat-native-package 30 python3 .github/release.py package aarch64-apple-darwin
python3 "$evidence/run.py" repeat-native-start 90 python3 .github/release.py start aarch64-apple-darwin
```

`reproduce_memory.py` builds the workspace and the arm64 release before it compiles the two probe programs.
It then runs three trials in sequence. It requires the retained sources in the evidence folder.
The last command needs permission to write the native account coordination folder.
The current restricted session does not have that permission.

For the other release build, install the x64 standard library for Rust 1.96.0 first.
Then use the same release build and package commands with `x86_64-apple-darwin`.
For the four native platform checks, run `.github/workflows/release.yml` through `workflow_dispatch`.
That event creates artifacts and `SHA256SUMS`, but no GitHub release.

## Evidence for the release scenarios

Both scenarios in `features/acp2api/release-archives.feature` retain `@wip`.
Their Given steps have no definitions in the Cucumber runner.
That runner does not fetch a GitHub release or run executables on four operating system and CPU combinations.
No acceptance contract file changed.

| Scenario | Executable check | Current proof |
|---|---|---|
| A release has one archive for each target platform | `checksums` validates four archive names and their contents. `draft` verifies five draft assets. | Local boundary checks passed. GitHub draft contents remain unproven. |
| The executable of each archive starts on its platform | Each native matrix job unpacks its archive and waits for the versioned management panel address. | The original local arm64 executable stopped at M-28. GitHub starts remain unproven. |

The runner choices are `macos-latest`, `macos-15-intel`, `windows-latest`, and `windows-11-arm`.
GitHub lists each label for public repositories in its [runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
The Intel macOS job uses a native runner. It does not need Rosetta.
The Windows jobs select static CRT linkage to keep the archive to one executable.
Rust documents that flag in its [linkage reference](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes).
