# Test acp2api through a test host executable with virtual time

The release executable and the test host call the same entry point of acp2api. Only the test host selects the adapters of the test harness. The release executable has no test setting that changes the accepted builds of ADR-0006.

The test harness controls virtual time through a clock port. It waits for work already in progress before it advances time. This permits exact checks of long waits and the 200 ms error window without real waits (SC-001 and FR-028).

A setting in the release executable would expose a way to bypass the accepted-build rule. Tests inside the test process would need another start path for the scenarios of process control. The test host keeps one start path, at the cost of a second executable and a protocol for virtual time.

The first freeze of the acceptance contract includes the shared harness. A later change to those frozen lines needs approval. The written design in `docs/design/acp2api.md` gives the interfaces, the synchronization points, and the checks of this protocol.
