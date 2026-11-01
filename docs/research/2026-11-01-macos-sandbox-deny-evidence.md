# macOS sandbox denial evidence

Date: 2026-11-01. Platform: Darwin 27.2.
These measurements came from native runs outside a nested sandbox, including a review of HEAD `8c93665e9961db7ae4209cf13918e0b916b85948`.
They were supplied with the review and implementation brief; the implementation sandbox did not repeat these native measurements.
No account, token, or external service was used. The destination `192.0.2.1:9` belongs to TEST-NET.

## Historical and live logs

With the same window and predicate, `log show` returned sandbox denials on its first read.
The next three reads returned no denials.
Thus repeated historical queries cannot supply a cumulative observation window.

Initial live-stream startup probes received all denials in five of five runs with one predicate clause.
They also received all denials in three of three runs with the full predicate.
These short runs did not establish complete delivery over longer windows.

A 100-second run issued one denied control per second and lost 13 of 100 denial lines between visible lines.
There was no `lossEvent`, dropped-message notice, or suppression notice.
Other windows lost every control for tens of seconds. A warm window with 150 triples lost none.
The cause of these silent windows is unknown.

A dial planted in the real host produced 17 destination failures, two missing-control failures, and one false pass in 20 scenario runs.
That false pass disproves the claim that visible controls at both ends prove a complete record.
`log stream` was rejected as the evidence channel because its silent loss can hide the host's forbidden attempt.

A redacted example shows the message shape, not a complete capture:

```text
kernel[0:3f9eba] (Sandbox) Sandbox: Python(72753) deny(1) network-outbound remote:*:9
```

The message does not expose the remote address. Its PID belongs to the denied process, not the kernel metadata.
Neither correct parsing nor a repeated delivery poll repairs a line that never arrives.

## Kernel signal rule

The measured profile was:

```text
(version 1)(allow default)
(deny network-outbound (with send-signal SIGKILL) (remote ip "*:*"))
(allow network-outbound (remote ip "localhost:*"))
```

| Probe | Result |
|---|---|
| Dial to `192.0.2.1` | 30 of 30 processes died by SIGKILL. |
| Loopback dial to `127.0.0.1` | 20 of 20 processes lived. |
| Dial to `127.0.0.2` | Process died by SIGKILL; no repetition count was supplied. |
| Forbidden dial in a child | Child died by SIGKILL; parent lived and exited 0. |

A separate review probe killed all 80 forbidden controls: 40 short and 40 failed attempts.
All 40 loopback controls in that probe lived.
These are separate probe cohorts; their counts are not combined with the table above.

The allow rule also permits `::1`. That accepted gap remains loopback-only traffic.
The Windows audit check compares literal destination addresses and covers `::1` for the same product code.

The signal rule makes the host exit status the evidence for its first forbidden attempt.
A child-only death does not establish a forbidden attempt by its parent.
Native controls and a 20-run planted-host check remain required for the revised harness.
