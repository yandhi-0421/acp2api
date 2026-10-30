# Run all ACP sessions in one runtime process

acp2api runs all ACP sessions in one runtime process. It starts the runtime process at its own start, and it keeps it while acp2api runs. After a fixed number of ACP sessions, acp2api replaces the runtime process, because the official runtime cannot close an ACP session.

## Rules

- **One runtime process.** All ACP sessions, and all prompt turns that run at the same time, use one runtime process.
- **Start.** acp2api starts the runtime process at its own start. During a lockout, it starts no runtime process ([Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15)).
- **Start limit.** When `initialize` gives no result in 5 minutes, the start failed.
- **No idle stop.** acp2api does not stop the runtime process when no request comes.
- **Replacement after N ACP sessions.** When the runtime process holds N ACP sessions, and a request needs one more ACP session, acp2api does these steps:
  1. It keeps each new request in the queue.
  2. When no prompt turn runs, it stops the runtime process.
  3. It starts a new runtime process.
- **The value of N.** A measurement of the memory of the harness processes decides N. N is a constant, not a setting of the user.
- **Failure.** Each of these events is one failure:
  - a failed start;
  - an exit that acp2api did not cause;
  - a prompt turn that does not end in 30 s after `session/cancel`.
  - an ACP write to a live runtime process that does not end in 30 s;
  - more than 128 MiB of ACP writes that wait for the runtime process.
- **After a failure.**
  - Each prompt turn of the runtime process ends with an error. acp2api starts a new runtime process at once.
  - After 3 failures in 10 minutes, acp2api goes into the state "runtime unavailable". It then starts no runtime process by itself.
  - The retry entry of the management panel and a restart of acp2api start a new runtime process. When the start succeeds, the state ends.
- **Stop.** At its exit, acp2api cancels each prompt turn that runs. It ends each request in the queue with an error. Then it stops the runtime process:
  - On macOS, acp2api closes stdin. After 5 s, it kills the process group.
  - On Windows, acp2api stops the process tree with its Job Object at once.
- **Temporary folder on Windows.** Each runtime process gets its own `TEMP` and `TMP` folder in the folder `acp2api` of the system temporary folder.
  - After the process tree stops, acp2api deletes that folder.
  - At its start, acp2api deletes each folder that an earlier run left there.
- **Crash of acp2api.** On Windows, the Job Object closes with the process of acp2api, and Windows stops the process tree. On macOS, the end of stdin stops the runtime process.

## Reasons

- On Windows, a start took 47.5 to 156.8 s in the VM, and one user report gives 12 to 16 s on a real computer. Each start also unpacks 341 MB into a `_MEI*` folder. On macOS, a start took 1.3 to 1.5 s.
- One runtime process ran the prompt turns of two ACP sessions at the same time on macOS.
- With one runtime process for each place, an ACP session in a busy runtime process must load in another runtime process. Nobody tested one ACP session in two runtime processes at the same time.
- The error line of the Python server names the ID of the ACP session and holds the full error text. Thus acp2api can connect each error to its prompt turn also in one runtime process.
- Each ACP session keeps one harness process alive. The `sessionCapabilities` of the official runtime 1.4.0 are only `list` and `resume`, so no method closes an ACP session. Only a stop of the runtime process stops its harness processes.
- Each auxiliary request of Claude Code, for example for a title, is a separate conversation with its own ACP session (ADR-0003). Thus the number of harness processes grows fast.
- acp2api checks the sign-in at each start ([Which signals start a lockout, and how does the user end it?](https://github.com/yandhi-0421/acp2api/issues/15)). That check needs a runtime process.
- On Windows, the end of stdin never stopped the runtime process. A kill of the root process left the Python process and the harness processes alive. The `_MEI*` folder stayed after each stop, and a `TEMP` folder of the ACP Client made its removal safe.
- One user report says that version 1.1.1 of the official runtime did nothing at `session/cancel`. Thus a prompt turn can stay open after a cancel. Only a stop of the runtime process then frees its place.

## Considered options

- **One runtime process for each place.** Each place has its own runtime process. An error line then belongs to one prompt turn with no ID of an ACP session. But each runtime process adds a start and a `_MEI*` folder on Windows, and an ACP session must load into the runtime process that is free.
- **A replacement that starts before the stop.** acp2api starts the new runtime process in the background before it reaches N, and new ACP sessions go to it. The old runtime process stops after its last prompt turn. This hides the start time, but two runtime processes run for a short time. A later version can add it if the stop on Windows is too long.
- **A stop after an idle time, as in the earlier CLI bridge of the maintainer.** On Windows, each start costs a start time and a write of 341 MB. N already limits the number of harness processes.
- **A start at the first request.** The first request then waits for the start, and the sign-in check needs a runtime process anyway.

## Consequences

- A crash of the runtime process ends each prompt turn that runs.
- After each replacement, each conversation that continues loads its ACP session with `session/load` (ADR-0003). That took 8.3 to 8.5 s on macOS, and 7.6 s and 16.2 s in the Windows VM.
- The acceptance test of a version must check these facts:
  - concurrent prompt turns in one runtime process at the platform maximum of FR-049, on macOS and on Windows;
  - the memory of the runtime process and of each harness process, which decides N, and if a harness process exits by itself;
  - on macOS, if the harness processes stop after a crash of acp2api.

Evidence: the grilling of [How many prompt turns run at the same time, and what does a queued request get?](https://github.com/yandhi-0421/acp2api/issues/14) and [How does acp2api start, keep, and stop the processes of the official runtime?](https://github.com/yandhi-0421/acp2api/issues/30); F8, F11, and F12 of [Probe the official runtime on macOS with a test MCP server and an empty `cwd`](https://github.com/yandhi-0421/acp2api/issues/8); W2, W3, and W4 of [Probe the official runtime on Windows 11 ARM in a VM](https://github.com/yandhi-0421/acp2api/issues/16); the answer of [What does the official runtime support over ACP on Windows and macOS?](https://github.com/yandhi-0421/acp2api/issues/2); and sections 3.3 and 9.9 of `docs/research/2026-10-15-official-runtime-acp-probe.md`.
