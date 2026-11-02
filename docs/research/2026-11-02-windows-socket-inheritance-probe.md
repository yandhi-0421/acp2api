# Windows can pass a duplicate socket to a child process

Date: 2026-11-02.

The maintainer supplied the Windows probe results and trace results in [issue #223](https://github.com/yandhi-0421/acp2api/issues/223).
This macOS session did not repeat the Windows runs.
The archived [probe source](captures/2026-11-02-windows-socket-inheritance/inherit.rs) retains the original code.

## Results on `windows-latest`

| Case | Result |
|---|---|
| Client handle | `inherit=0` |
| `TcpStream` duplicate | `inherit=1` |
| No child, duplicate | EOF after 0 ms |
| Child, no duplicate | EOF after 0 ms |
| Child starts while the duplicate is open | EOF after 8116 ms, when the child ended |
| Child starts after the duplicate closed | EOF after 0 ms |
| Listener | `inherit=0` |
| `TcpListener` duplicate | `inherit=1` |
| Socket accepted from either listener | `inherit=0` |

## Method and limits

The probe binds sockets on loopback. It reads the handle flags with `GetHandleInformation`.
The stream cases vary the duplicate, the child, and the order of the duplicate close.
The server measures the time from the parent closes to EOF.
The listener case also checks sockets accepted from each listener.

The source makes seven connections and starts three child commands.
Each socket read has a 12 s timeout. Each child command makes nine loopback pings.
For a repeat, use a 90 s run timeout and a 64 KiB log cap.
At the timeout, stop the process tree.
The probe sends no API request and uses no account or credential.

## Trace and cause

At 11 s after the client close, both TCP ends were `ESTABLISHED`.
The test process owned the client end. Thus no FIN had left that socket.
A new product request completed in 3 ms, with no gap in the product heartbeat.
These observations rule out a stalled product for that failure.

The fake API client duplicates its socket in `crates/acp2api-acceptance/src/client.rs`.
A child that starts while that duplicate is open can retain the socket and delay EOF.
Product startup also duplicates its listener in `crates/acp2api/src/startup.rs`.
A live runtime process can then retain the port after an abnormal product exit.
The accepted sockets in the probe did not have the inherit flag.

## Remaining interval

The duplicate and `SetHandleInformation` are separate operations.
A concurrent child start in that short interval can still inherit the socket.
The fix adds no lock.
