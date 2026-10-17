# Write acp2api in Rust

The project writes acp2api in Rust. CI builds one native executable for each target: macOS arm64, macOS x64, Windows x64, and Windows arm64. AI agents write the code, and models of other vendors review it. The maintainer does not read the code, so the compiler must reject as many defects as possible.

## Reasons

- Rust enums with data let the compiler reject a message shape that an API or ACP does not permit. acp2api translates many such shapes: the content blocks and the stream events of three APIs, and the ACP session updates.
- Rust has the official ACP Client library `agent-client-protocol` (3.4.0, 2026-10-16), which Zed uses. It also has the official MCP library `rmcp` (3.5.1), with a Streamable HTTP server.
- The maintainer wants acp2api to use little memory and little disk space. On macOS arm64, a hello-world HTTP server with only the Rust standard library is 0.36 MB, and it uses 1.9 MB of memory at idle.

## Considered options

- **Go.** Go has the simplest builds and one code style. But it has no official ACP library, and the most used community library had no release after 2026-06-02. Go also has no enums with data, so each message variant needs JSON decode code by hand. The hello-world server is 5.4 MB, and it uses 10.0 MB of memory at idle.
- **TypeScript on Bun.** Bun has the official ACP and MCP libraries, and one CI job builds the four targets. But the compiled hello-world server is 61.5 MB, and it uses 26.9 MB of memory at idle. Bun also has open bugs about child processes on Windows, and TypeScript types do not exist at run time.
- **TypeScript on Node.js.** A Node.js single executable application cannot target macOS x64.
- **A port of agy-bridge.** The maintainer keeps the earlier project agy-bridge (TypeScript on Node.js) only as a reference. Its translation logic, its tests, and its stream captures can guide acp2api, but its code does not move.

## Consequences

- On Windows, acp2api must put the official runtime into a Job Object. The ACP Rust library stops only the direct child process, and the official runtime can start more processes.
- The ACP Rust library had two major versions between 2026-07-23 and 2026-10-06. Pin its exact version, and upgrade it only in a planned change.
- `rmcp` needs the async runtime tokio. The written design selects the async runtime and each library.
- The four targets probably need two CI runners: one for macOS and one for Windows. [How does the project prove that v1 works on Windows and macOS?](https://github.com/yandhi-0421/acp2api/issues/17) decides the CI jobs.

Evidence: the answer of [Which languages have maintained ACP Client and MCP server libraries and build one binary for Windows and macOS?](https://github.com/yandhi-0421/acp2api/issues/6), and the grilling of [Which language and runtime does acp2api use?](https://github.com/yandhi-0421/acp2api/issues/12). The memory values come from one run on 2026-10-17 with Rust 1.96.0, Go 1.26.4, and Bun 1.3.14. They show hello-world servers, not acp2api.
