# acp2api v1 design

**Status:** approved design, with the measurement gaps of OD-1 and OD-2.

The owner accepted the ten initial choices and all recommendations of the final review on 2026-10-18. This design incorporates both rounds.

- PRD: `docs/prd/acp2api.md`, status `Blocked draft`, blockers OD-1 and OD-2.
- Feature files: `features/acp2api/`.
- Parent issue: [yandhi-0421/acp2api#35](https://github.com/yandhi-0421/acp2api/issues/35).

## One-way decisions and new dependencies

### One-way decisions

Each item gives the decision, its reason, its effect, and its way back. The approval of this design confirms these decisions, and no one-way action.

1. **The test seam is a second executable, the test host.**
   - Decision: the crate `acp2api` exports `run(Ports, args, Environment)`. The release `main` gives the production adapters. A test executable `acp2api-test-host` in the test crate gives the test adapters.
   - The release executable reads no test variable and has no test feature.
   - Reason: the tests must accept a fake build, control time, and refuse each other executable. A seam in the release executable would be a hidden setting that accepts any build. ADR-0006 rejects such a setting.
   - Effect: the tests run the same `run` function as the release, but not the same `main`. A change to frozen harness lines needs approval.
   - ADR-0008 records this choice and the choice of virtual time below.
   - Way back: before the freeze of #36, a change of one file. After the freeze, a change of frozen harness files, which is stop reason 1 of `bdd`.
2. **Tests use virtual time, and production code takes holds at four input points.**
   - Decision: all waits of acp2api go through the `Clock` port. In tests, time moves only when a step moves it. Production code takes a `Hold` at four input points (section "Virtual time").
   - Reason: about 40 scenarios need 5 minutes to 30 days. Real waits are not possible. A real 200 ms window is not stable on a slow CI runner.
   - Effect: four kinds of messages in the product carry a `Hold`. The production `Hold` does nothing.
   - Way back: the same as for decision 1.
3. **Stored data.** Section "Stored data" gives each item:
   - the layout of the data folder;
   - the names and the form of the settings in `config.toml`;
   - the form of `state.json`;
   - the names of the log files;
   - the form of the password hash;
   - the fixed account paths and the versioned address and ownership records.

   Reason: FR-003, FR-004, FR-035, FR-044, and FR-057 need them. Effect: the owner edits `config.toml`, and later versions read both files. Way back: a migration at the start of a later version. `state.json` has a `format` field for that migration.
4. **Published contracts toward the API clients and the owner.** Section "Decided values" gives each item:
   - the OpenAI error `type` and `code` of each case;
   - the keep-alive events and their interval;
   - the address of the management panel and of the sign-in entry;
   - the rule that removes the attribution block of Claude Code.

   Reason: FR-011, FR-019, FR-026, M-12 to M-14, and M-24 leave these values to the written design. Effect: API clients and the owner see them. Way back: before the first release, an edit of one module each. After a release, a change for external users.
5. **Security rules.**
   - acp2api keeps the management password only as an Argon2id hash. This also applies to the random password of a first start.
   - The configuration file and the state file allow only the computer account of the owner (FR-004).
   - A fixed account folder holds two permanent OS lock files and a separate panel address record.
   - The account folder is independent of `--data-dir`, `TMPDIR`, `TEMP`, and `TMP`. Section "Account ownership" defines it.
   - Only the successful account owner can clean its files. Another account has a separate temporary subtree.
   - Later versions must retain the lock identities. A path change needs a migration that preserves mutual exclusion with earlier versions.
   - Way back: a new release. No revert undoes the exposure of a password in plain text, so the safe direction comes first.
6. **The base of the code.**
   - tokio and axum (on hyper) carry all asynchronous code and all HTTP.
   - ACP uses the types of `agent-client-protocol-schema` with an own JSON-RPC line loop. It does not use the connection of `agent-client-protocol`.
   - The MCP server of the client tools is an own small module, not `rmcp`.
   - Reasons for the own line loop:
     - FR-028 needs the order of the stdout lines and the stderr lines in which acp2api reads them.
     - The settle of virtual time needs a count of the lines that acp2api handled. The library hides both in its tasks.
     - The library had three major versions from 2026-06-24 to 2026-10-06 (crates.io).
   - Reasons for the own MCP server:
     - The official runtime needs only `initialize`, `notifications/initialized`, and `tools/list`.
     - A minimal server passed with the real official runtime 1.4.0 in the probe of 2026-10-15 (`docs/research/captures/2026-10-15-acp-probe/locked-meta-echo.mcp.ndjson`).
     - `rmcp` had three major versions in 2026.
   - ADR-0001 names `agent-client-protocol` and `rmcp` as reasons for Rust, and it gives the choice of each library to the written design. This design keeps the ACP types of the same project, pinned to an exact version.
   - Way back: the ACP line loop is one module (`runtime_driver::acp`), and the MCP server is one module (`runtime_driver::mcp`). A change to the libraries touches only these modules. tokio and axum are in most modules, so their replacement is a migration.
7. **cucumber is the Gherkin runner.** All step definitions use its macros. Way back: an edit of each step file.

### New dependencies

No code exists, so each dependency is new. The facts come from crates.io and from the package files of each crate on 2026-10-18 (choose-boring-technology). Here, "build script" is the install hook of a Rust crate: a `build.rs` in the package.

| Crate | Version | Release date | License | Build script | Ships in the release | Reason |
|---|---|---|---|---|---|---|
| [tokio](https://crates.io/crates/tokio/1.53.3) | 1.53.3 | 2026-10-13 | MIT | no | yes | asynchronous processes, pipes, sockets, and tasks |
| [axum](https://crates.io/crates/axum/0.8.9) | 0.8.9 | 2026-04-14 | MIT | no | yes | the HTTP server of the APIs, the panel, and the MCP server |
| [hyper](https://crates.io/crates/hyper/1.12.0) | 1.12.0 | 2026-10-06 | MIT | no | yes, through axum; direct only in the test crate | the fake API client needs control of each header |
| [serde](https://crates.io/crates/serde/1.0.229) | 1.0.229 | 2026-07-18 | MIT OR Apache-2.0 | yes | yes | the three APIs, ACP, and the stored data |
| [serde_json](https://crates.io/crates/serde_json/1.0.154) | 1.0.154 | 2026-10-11 | MIT OR Apache-2.0 | yes | yes | the feature `preserve_order` keeps the key order of the request (FR-023) |
| [toml](https://crates.io/crates/toml/1.1.8+spec-1.1.0) | 1.1.8+spec-1.1.0 | 2026-10-09 | MIT OR Apache-2.0 | no | yes | `config.toml`, `builds.toml`, and the run file of the tests |
| [agent-client-protocol-schema](https://crates.io/crates/agent-client-protocol-schema/1.12.0) | =1.12.0 | 2026-10-16 | Apache-2.0 | no | yes | the ACP v1 types (ADR-0001); major version 1 since 2026-06-24 |
| [sha2](https://crates.io/crates/sha2/0.11.1) | 0.11.1 | 2026-10-12 (0.11.0: 2026-03-25) | MIT OR Apache-2.0 | no | yes | the build hashes (ADR-0006) |
| [argon2](https://crates.io/crates/argon2/0.6.0) | 0.6.0 | 2026-08-27 | MIT OR Apache-2.0 | no | yes | the hash of the management password (FR-009) |
| [getrandom](https://crates.io/crates/getrandom/0.4.3) | 0.4.3 | 2026-06-17 | MIT OR Apache-2.0 | yes | yes | the local API key, the random password, and the secret MCP paths |
| [windows-sys](https://crates.io/crates/windows-sys/0.61.2) | 0.61.2 | 2025-10-06 | MIT OR Apache-2.0 | no | Windows only | the Job Object, Ctrl+C and Ctrl+Break, and the file ACL |
| [libc](https://crates.io/crates/libc/0.2.191) | 0.2.191 | 2026-10-16 | MIT OR Apache-2.0 | yes | macOS only | the process group and `killpg` |
| [cucumber](https://crates.io/crates/cucumber/0.23.0) | 0.23.0 | 2026-04-23 | MIT OR Apache-2.0 | no | no (test crate only) | the Gherkin runner |

- Each crate comes from the crates.io registry. None has a git source.
- The crates need Rust 1.88 or later. The account locks need Rust 1.89 or later, which added `std::fs::File::try_lock`.
- The maintainer has Rust 1.96.0. The review checked each listed version through the crates.io API and its package manifest.
- Each crate name links to its registry version. The package manifest supplies its build-script and Rust-version facts.
- A crate that the dependency tree already holds can become a direct dependency only with the version of the lock file. An example is `futures-util` through axum.
- No date library. All times in messages and logs are RFC 3339 in UTC. `/usage` gives its reset times in UTC, for example `resets Oct 24, 2026, 12:34 UTC` (`docs/research/captures/2026-10-17-macos-token-pdf-slash-probe/`).
- No log library, no `rmcp`, no connection of `agent-client-protocol`, and no `dirs` (subtract-before-you-add).

## Problem

acp2api v1 translates three HTTP APIs into ACP for one runtime process of the official runtime, for one owner (PRD, section "Summary"). The shape is not obvious, for three reasons:

- One request crosses five concerns with different owners:
  - the HTTP forms of three APIs;
  - the state of acp2api;
  - the choice of an ACP session;
  - one shared runtime process;
  - the stream back.
- The tests must run the real product against a fake ACP agent. They need control of time, and no test may start the real official runtime.
- A change to frozen harness lines needs approval. The harness permits later additions without an edit of the earlier contract.

FRs in scope and their modules (section "Shape" gives each module):

| FRs | Module |
|---|---|
| FR-001 | the release workflow |
| FR-002 to FR-006, FR-045 | `startup`, `store`, `journal` |
| FR-007, FR-008, FR-010 (routes), FR-012 (list) | `front` |
| FR-009, FR-053 to FR-055 | `panel` |
| FR-010, FR-011, FR-013 (effort fields), FR-014, FR-015 (fields), FR-017, FR-018, FR-022, FR-024, FR-025 (render), FR-026, FR-027, FR-030 | `dialect` with `messages` |
| FR-016, FR-019 to FR-021, FR-023, FR-025 (estimate) | `conversation` |
| FR-012, FR-013 (resolution), FR-015 (cut), FR-021 (choice), FR-028, FR-029, FR-031, FR-034 to FR-041, FR-044 (script), FR-047 (policy), FR-049 to FR-052 | `conductor` |
| FR-039 (sign-in address), FR-041 (process), FR-046 to FR-048 | `runtime_driver` with the `ProcessLauncher` port |
| FR-042 to FR-044 | `builds` |
| FR-056, FR-057 | `journal` |
| FR-032, FR-033 | `messages` |

FRs that this design does not fully design:

- FR-041, the value of N: open decision OD-1. The design has the constant and the replacement rule. The three scenarios about N stay without a slice tag.
- SC-006: open decision OD-2. The design keeps memory low, because records hold only hashes and acp2api keeps no history text. It sets no target.

Limits from the ground step:

- **Preserve:**
  - ADR-0001 to ADR-0007, FR-001 to FR-057, the message texts, and SC-001;
  - the caps of `CLAUDE.md` for each fake;
  - the freeze rule of `bdd` (`.claude/skills/bdd/references/automate.md`, section "The step definitions").
- **Change:** everything. No code exists, so the code trace of the ground step was skipped.
- **Avoid:**
  - a fake that loops and fills the disk (`docs/research/2026-10-18-probe-methods-and-safety-rules.md`, section 1);
  - a test that can start a real build;
  - `tokio::time::pause` with a real child process. With paused time and no work, the tokio clock advances to the next timer (tokio 1.53.3, `src/time/clock.rs`, lines 122–123);
  - SIGINT as a cancel on Windows.
- **Risk:**
  - the long waits of about 40 scenarios;
  - the account lock against parallel tests;
  - the real folders of the maintainer;
  - the freeze of the harness;
  - the free disk of the Mac of the maintainer: about 9.5 GiB at this review on 2026-10-18.

## Usage

### Caller 1: the HTTP handler of `POST /v1/messages`

File `crates/acp2api/src/front.rs`. The guard layer of `front` has already checked `Host`, `Origin`, and the local API key.

```rust
async fn post_messages(State(app): State<App>, head: RequestHead, body: Bytes) -> Response {
    app.serve(&dialect::MESSAGES, ApiRequest { head, body }).await
}

impl App {
    async fn serve(&self, dialect: &dyn Dialect, req: ApiRequest) -> Response {
        let hold = self.clock.hold(HoldKind::HttpRequest); // covers submission, never a queued or external wait
        let Parsed { turn, form } = match dialect.parse(&req.head, &req.body) {
            Ok(parsed) => parsed,
            Err(refusal) => return dialect.refusal(&refusal),
        };
        match self.conductor.admit(turn, &form, hold).await {
            Admission::Refused(refusal) => dialect.refusal(&refusal),
            Admission::Accepted(ticket) => dialect.respond(form, ticket), // 200 and a stream now, or one body at the end
        }
    }
}
```

`POST /v1/responses` and `POST /v1/chat/completions` use the same function with `dialect::RESPONSES` and `dialect::CHAT`.

### Caller 2: the two `main` functions

```rust
// crates/acp2api/src/main.rs: the release executable
fn main() -> ExitCode {
    acp2api::run(Ports::production(), std::env::args_os(), Environment::from_process())
}

// crates/acp2api-acceptance/src/bin/acp2api-test-host.rs: tests only
fn main() -> ExitCode {
    let run = RunFile::read_from_env();          // the path is in ACP2API_TEST_RUN; the harness writes the file
    let hub = HubLink::connect(&run.hub);        // 127.0.0.1
    let ports = Ports {
        clock: Arc::new(HarnessClock::new(hub.clone())),
        launcher: Arc::new(GuardedLauncher::new(OsLauncher, run.fake_program, hub.clone())),
        release: Arc::new(run.release),
        browser: Arc::new(RecordingBrowser::new(hub.clone())),
        stop: Box::new(HubStop::new(hub)),
    };
    acp2api::run(ports, std::env::args_os(), run.environment)
}
```

Only these two functions choose adapters. Only the test host reads `ACP2API_TEST_RUN`.

### Caller 3: the step definitions

Folder `crates/acp2api-acceptance/tests/steps/`. The step "acp2api is in the normal state" only records the setup. The first step that needs acp2api starts it and settles.

```rust
// common.rs (frozen by #36)
#[given("acp2api is in the normal state")]
async fn normal_state(s: &mut Scenario) {
    s.plan(Setup::normal());
}

// system_prompt_and_role_labels.rs (#36)
#[given(expr = "the official runtime replies {string} to the next prompt")]
async fn replies(s: &mut Scenario, text: String) {
    s.fake().next_prompt(TurnPlan::new().chunk(&text).usage(1000, 1_048_576).end(StopReason::EndTurn));
}

#[when(expr = "an API client sends a request to {string} with one unnamed user message {string}")]
async fn sends_one_message(s: &mut Scenario, endpoint: Endpoint, text: String) {
    let answer = s.client().await.send(ApiCall::one_user_message(endpoint, &text)).finish(&mut s.time).await;
    s.keep(answer);
}

#[then(expr = "the prompt that the official runtime gets is {string}")]
async fn prompt_is(s: &mut Scenario, text: String) {
    s.time.settle().await;
    assert_eq!(s.fake().record().prompts().last().expect("a prompt").text(), text);
}

// settings.rs (#60): a later slice adds a method; it does not edit Setup::normal
#[given("the build of acp2api for this platform has the highest concurrency limit 2, as its release notes name")]
async fn highest_limit_two(s: &mut Scenario) {
    s.plan(Setup::normal().highest_concurrency_limit(2));
}
```

## Shape

### Crates and files

```text
Cargo.toml                         workspace: crates/acp2api, crates/acp2api-acceptance
clippy.toml                        disallowed methods (section "Rules that the build enforces")
crates/acp2api/
  release/builds.toml              the release facts: listed builds, highest limit of each platform
  src/main.rs                      the release main
  src/lib.rs                       run, Ports, Environment, the five port traits
  src/os/                          the production adapters of the five ports
  src/startup.rs                   the start order and the exit order
  src/front.rs                     routes, Host/Origin/CORS/key guard, GET /v1/models, 404
  src/dialect/{mod,messages,responses,chat}.rs
  src/conversation.rs              pure: domain types, hashes, prompt text, token estimate
  src/conductor/{mod,core/..}.rs   one actor; the pure core owns all state of acp2api
  src/runtime_driver/{mod,process,acp,stderr,mcp}.rs
  src/builds.rs                    search, hashes, accepted versions, copy
  src/store.rs                     config.toml and state.json
  src/journal.rs                   terminal lines, logs, redaction, notices, last 50 requests
  src/panel/{mod,page,texts}.rs    the page, the management endpoints, panel sessions
  src/messages.rs                  M-01 to M-28, the texts of FR-024, the error rows
crates/acp2api-acceptance/         publish = false
  src/bin/acp2api-test-host.rs
  src/bin/fake-acp-agent.rs        std and serde_json only
  src/                             the harness: Scenario, Hub, TimeLine, FakeRuntime, ApiClient, Owner, TestRoot, caps
  tests/acceptance.rs              harness = false: the cucumber runner
  tests/steps/<feature stem>.rs    one file for each file of features/acp2api/, and common.rs
```

The crate `acp2api` is a library and an executable. These items are public: `run`, `Ports`, `Environment`, the five port traits with their types, and `os`. Each other module is `pub(crate)`, so an import from the test crate fails the build (deep-modules).

### Ports

Five ports, each with two adapters. Section "Seams" gives the adapters.

```rust
pub fn run(ports: Ports, args: impl IntoIterator<Item = OsString>, env: Environment) -> ExitCode;

pub struct Ports {
    pub clock: Arc<dyn Clock>,
    pub launcher: Arc<dyn ProcessLauncher>,
    pub release: Arc<dyn ReleaseFacts>,
    pub browser: Arc<dyn Browser>,
    pub stop: Box<dyn StopSignal>,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Moment;                                   // monotonic milliseconds
    fn wall(&self) -> SystemTime;                              // log dates, lockout start, reset times
    fn sleep_until(&self, at: Moment) -> BoxFuture<'static, Hold>;
    fn hold(&self, kind: HoldKind) -> Hold;                    // virtual time does not pass while a Hold lives
}
pub enum HoldKind { HttpRequest, RuntimeLine, McpRequest, TurnEvent }

pub trait ProcessLauncher: Send + Sync {
    fn launch(&self, spec: LaunchSpec) -> Result<Launched, LaunchError>;
}
pub struct LaunchSpec { pub program: PathBuf, pub args: Vec<OsString>, pub env_set: Vec<(OsString, OsString)>,
                        pub cwd: PathBuf, pub own_temp: Option<PathBuf> }
pub struct Launched { pub stdin: ChildStdin, pub stdout: ChildStdout, pub stderr: ChildStderr,
                      pub tree: Box<dyn ProcessTree> }
pub trait ProcessTree: Send {
    fn pid(&self) -> u32;
    fn exited(&mut self) -> BoxFuture<'_, ExitStatus>;
    fn stop(self: Box<Self>, clock: Arc<dyn Clock>) -> BoxFuture<'static, ()>;
}
pub enum LaunchError { Refused { program: PathBuf }, Io(io::Error) }

pub trait ReleaseFacts: Send + Sync {
    fn version(&self) -> &Version;
    fn platform(&self) -> Platform;
    fn listed_builds(&self) -> &[ListedBuild];
    fn highest_concurrency_limit(&self) -> ConcurrencyLimit;
}
pub trait Browser: Send + Sync { fn open(&self, panel: &Url); }
pub trait StopSignal: Send { fn stopped(&mut self) -> BoxFuture<'_, ()>; }

pub struct Environment { pub account_root: PathBuf, pub home: Option<PathBuf>, pub local_app_data: Option<PathBuf>,
                         pub temp: PathBuf, pub path: Vec<PathBuf>, pub exe_dir: PathBuf }
```

Invariants and error modes:

- `run` returns only after the process tree of the runtime process has stopped. A start error (M-24, M-25, M-26) writes one terminal line and returns a non-zero exit code.
- `Environment::from_process` is the only code that reads the environment variables of the platform:
  - `HOME` on macOS, and `LOCALAPPDATA` on Windows, because FR-003 names the data folders with them;
  - `TMPDIR` on macOS, or `TEMP` and `TMP` on Windows;
  - `PATH`.

  A start with an absent required variable fails with one terminal line.
- The same factory resolves `account_root` through native account APIs. It never derives that value from an environment override.
- The test host supplies an isolated `account_root` as a value. This adds no port and no release option.
- `ProcessTree::stop` on macOS closes stdin, waits 5 s on the clock, and kills the process group. On Windows, it closes the Job Object at once (ADR-0004).
- `LaunchError::Refused` and `LaunchError::Io` are both a failed start of the runtime process (FR-041).
- A `ConcurrencyLimit` is 1 to 3. The parse of the configuration file also compares it with `highest_concurrency_limit` (FR-049).
- In production, `hold` returns an empty `Hold`. `sleep_until` waits to the deadline and returns an empty `Hold`.
- A successful timer returns a `Hold` for the work that its wake starts. The owner keeps it until that work finishes.
- A loop registers its next wait before it releases the earlier `Hold`. Cancellation releases a pending sleep.

### Domain types

`conversation` owns the domain model of one request. Each dialect parses into it. Nothing outside `dialect` sees a wire type (boundary-discipline).

```rust
pub enum Role { User, Assistant, System, Developer }
pub enum Part { Text(String), Image { media_type: String, base64: String },
                Document { title: Option<String>, text: String }, PdfNote }
pub enum Arguments { ClientText(String), Object(String) }   // Object: compact JSON in the key order of the request
pub enum Item {
    Message { role: Role, name: Option<String>, parts: Vec<Part> },
    ToolCall { id: CallId, name: String, arguments: Arguments },
    ToolResult { id: CallId, parts: Vec<Part>, error: bool },
}
/// The dialect rejects a final assistant item with M-27 before it constructs Last.
pub enum Last { Messages(NonEmpty<Item>), ToolResult { result: Item, then: Vec<Item> } }
pub struct SystemPart { pub role: Role, pub name: Option<String>, pub text: String }
pub struct Conversation { pub system: Vec<SystemPart>, pub earlier: Vec<Item>, pub last: Last }
pub struct ClientTool { pub name: String, pub description: String, pub schema: String,
                        pub kind: ToolKind, pub namespace: Option<String> }
pub enum ToolKind { Function, Custom }

pub struct TurnRequest { pub model: ModelAsk, pub conversation: Conversation, pub tools: ClientTools,
                         pub tool_choice: ToolChoice, pub stop: Vec<String>, pub unknown_fields: Vec<String> }
pub struct ModelAsk { pub name: String, pub effort: EffortAsk }
pub enum EffortAsk { Absent, Level(ThinkingLevel), Unknown(String) }
pub enum ToolChoice { Auto, Off }
pub struct ReplyForm { pub stream: bool, pub include_usage: bool, pub model_name: String }

pub fn prompt(kind: SessionUse, items: &[Item]) -> Vec<PromptBlock>;   // FR-016, FR-017, FR-020, FR-021, FR-023
                                                                       // items: the new items for Continue, else all items
pub fn system_prompt_addition(system: &[SystemPart]) -> Option<String>; // FR-019
pub fn record_hashes(conversation: &Conversation) -> Vec<ItemHash>;    // ADR-0003
pub fn tool_set_hash(tools: &ClientTools) -> ToolSetHash;              // ADR-0003
pub fn output_tokens(text: &str) -> u64;                               // FR-025
pub fn input_tokens(turn: &TurnRequest) -> u64;                        // FR-025: full normalized input, not only new items
```

- `dialect::messages` removes the attribution block of Claude Code before it builds `system` (section "Decided values"). The block changes with each request, so the hash of the system prompt must never hold it.
- Each system part keeps its original `System` or `Developer` role. A named part uses that role in its label.
- An ignored field or block never enters a domain type. Thus it is never part of a hash (ADR-0003).

Each answer that is not a success is one value of one type. `messages` owns the map from a case to a row of FR-033 (make-illegal-states-unrepresentable):

```rust
pub enum Row { Key, Rejected, Stopped, Quota, QueueFull, Temporary503, Other502 }  // the rows of FR-033
pub struct Refusal { pub row: Row, pub message: Message, pub retry_after: Option<Seconds>, pub param: Option<String> }
pub enum Stop { EndTurn, MaxTokens, Refusal, StopSequence(String), ToolUse(ToolCall) }
pub enum TurnEnd { Done { stop: Stop, usage: Usage }, Failed(Refusal) }
pub enum TurnEvent { KeepAlive, Text(String), End(TurnEnd) }
pub enum Admission { Refused(Refusal), Accepted(Ticket) }   // Ticket: the receiver of (TurnEvent, Hold); a drop is a disconnect
```

`Message` is one enum of M-01 to M-28 and of the texts of FR-024. Its `Display` implements their PRD definitions. Each dialect maps a `Row` and a `Message` to its own status, headers, and error form with an exhaustive `match`.

M-18 has separate HTTP and ACP payload variants. The ACP variant needs no fabricated HTTP code or status.

### The dialect port

```rust
pub(crate) trait Dialect: Sync {
    fn parse(&self, head: &RequestHead, body: &[u8]) -> Result<Parsed, Refusal>;
    fn refusal(&self, refusal: &Refusal) -> Response;
    fn respond(&self, form: ReplyForm, ticket: Ticket) -> Response;
}
pub(crate) static MESSAGES: Messages; pub(crate) static RESPONSES: Responses; pub(crate) static CHAT: Chat;
```

`parse` is pure. It applies these rules:

- the tables of FR-014, with the depth limit of 128, and the rule that a value which turns a feature off counts as absent;
- the effort fields of FR-013, into `EffortAsk`;
- the documents of FR-017 and the thought text of FR-018;
- the tools of FR-022 and FR-024, where an advisor tool comes before each other check.

`respond` writes these items:

- the stream events of FR-010 in their order;
- the keep-alive event of its API for each `TurnEvent::KeepAlive`;
- the usage of FR-025 at its place;
- the stop reasons of FR-015 and FR-030, and the one tool call of FR-022;
- the error events of FR-027.

`refusal` writes the error envelope of its API, with the values of FR-033 and of section "Decided values".

### The conductor

The conductor is one actor. Its pure core owns all mutable state of acp2api while it runs. Each change of that state is one call of `Core::handle`.

```rust
#[derive(Clone)]
pub(crate) struct Conductor { /* the sender of the mailbox */ }
impl Conductor {
    pub(crate) fn admit(&self, turn: TurnRequest, form: &ReplyForm, hold: Hold) -> BoxFuture<'static, Admission>;
    pub(crate) fn model_names(&self, hold: Hold) -> BoxFuture<'static, Result<Vec<ModelEntry>, Refusal>>;
    pub(crate) fn owner(&self, action: OwnerAction) -> BoxFuture<'static, OwnerResult>;
    pub(crate) fn view(&self) -> Arc<View>;          // what the panel and the terminal read
}

pub(crate) struct Core { /* mode, conditions, work, concurrency limit, records, retry waits, catalog, deadlines */ }
impl Core {
    pub(crate) fn handle(&mut self, event: Event, now: Moment, wall: SystemTime) -> Vec<Command>;
    pub(crate) fn next_deadline(&self) -> Option<Moment>;
}

pub(crate) enum Mode {
    Lockout(LockoutEvidence),                        // no usable runtime; the driver can only finish its shutdown
    RuntimeUnavailable(UnavailableReason),
    Running(RuntimePhase),
}
pub(crate) struct Conditions { sign_in: SignInCondition, quota: Option<QuotaWait> }
pub(crate) enum SignInCondition { Clear, Required }
pub(crate) struct QuotaWait { until: SystemTime, deadline: Moment, timer: TimerId }
pub(crate) enum RuntimePhase { Starting { since: Moment }, Ready { generation: u32, sessions: u16 },
                               Replacing(ReplaceCause), Checking(Build) }
pub(crate) enum OwnerAction { StartSignIn, CancelSignIn, Search, SetRuntimePath(PathBuf),
    RunCompatibilityCheck(Build), UseBuild(Build), Retry, EndLockout, NewLocalApiKey,
    ChangePassword(String), SetConcurrencyLimit(ConcurrencyLimit), RefreshQuota, ReadModelNames,
    DismissNotice(NoticeId) }
```

`GET /v1/models` calls `model_names` to check the mode and read the catalog from the startup sign-in check (FR-012, FR-038). The shell of the actor does only these steps:

1. It takes the next message from the mailbox, or it wakes at `clock.sleep_until(core.next_deadline())`.
2. It calls `core.handle(event, clock.now(), clock.wall())`.
3. It carries out the commands:
   - ACP calls through `runtime_driver`;
   - `TurnEvent`s to the tickets;
   - writes of `state.json` and `config.toml` through `store`;
   - lines through `journal`;
   - starts and stops of the runtime process, and calls of `builds`.
4. It drops the `Hold` of the message. Then it goes back to step 1.

Each timer of acp2api is a deadline in the core, also the keep-alive of each stream. Thus the shell is the only sleeper of the product, except `ProcessTree::stop`.

The core has these parts. Each part is a private module with a pure interface.

- **`mode`.** The states of FR-034 to FR-040. Their effects on the queue (FR-037) and on the prompt turns (FR-035).
- **`places`.** The concurrency limit and one private table of work (FR-049, FR-050). Queued and occupied counts derive from its variants.
  - A request holds a place from its first ACP call to the end of its prompt turn.
  - The sign-in check, each `/usage`, and the read of the model names each hold a place.
- **`records`.** The records and the match, the choice, and the drop of ADR-0003. The N rule of ADR-0004, with the constant `SESSIONS_PER_RUNTIME_PROCESS` (value open: OD-1).
- **`retry`.** The retry waits of FR-029. The key is the hash of the system prompt, the items, the tool set, and the model ID.
- **`catalog`.** The model names (FR-012) and the resolution of the thinking level (FR-013).
  - After `initialize`, the startup sign-in check sends one `session/new`, with one place and no prompt turn.
  - On success, its result fills the catalog. A first model-list request creates no further ACP session.
  - A request that arrives before this check completes waits for its result. It does not start another check.
  - Each later new ACP session refreshes the catalog from its result.
  - The catalog has the suffix of a thinking level only for a model ID `<value>-<level>` in `models.availableModels` of the result.
  - The check uses the signal rules of FR-034. During a lockout, it sends nothing.
- **`turns`.** One machine for each prompt turn:
  - the held reply text chunk and the 200 ms window (FR-028); usage and thought updates do not release the chunk;
  - the stop scan across chunks (FR-015);
  - cancel at permission, and the refusal of each other permission request (FR-047);
  - the deadline of 5 minutes with no update (FR-051), and the deadline of 30 s after a cancel (FR-041);
  - the token usage (FR-025) and the drop of thought text (FR-018).
- **`signals`.** The table of FR-034 over the error of each ACP call and the stderr error lines.
- **`quota`.** `/usage` (FR-052), the parse of the reset time, and the limit of one refresh in 60 s.
- **`owner`.** The owner actions of FR-053, the sign-in wait of FR-039, the script of the compatibility check of FR-044, and the notices of FR-047.

Rules of the core:

- **R1.** The core records each change of state before the `TurnEvent::End` that reports it leaves acp2api. This applies to the mode, a retry wait, a record, and the state file. Thus each response is a point at which a test can read the new state.
- **R2.** A lockout signal ends each prompt turn, refuses the queue, and stops the runtime process in one call of `handle` (FR-035).
- **R3.** A `Ticket` gets exactly one `TurnEvent::End`, as its last event. A drop of the `Ticket` is a disconnect: the request leaves the queue, or the core sends `session/cancel` (FR-031).

#### Admission and occupied places

The HTTP caller keeps `admit(turn, form, hold)`. A pending admission cannot write a stream. The conductor hides its check, queue position, and place.

```rust
struct PendingRequest { turn: TurnRequest, form: ReplyForm, reply: AdmissionReply }
struct AcceptedRequest { turn: TurnRequest, form: ReplyForm, output: TicketOutput }
struct Operation { generation: u32, id: OperationId }
enum Work {
    CheckQueued(PendingRequest),
    Checking { request: PendingRequest, call: Operation },
    AcceptedQueued(AcceptedRequest),
    AcceptedWorking { request: AcceptedRequest, execution: RequestExecution },
    Retiring(OutstandingWork),
    OtherPlaceWork(OtherWork),
}
type WorkTable = BTreeMap<WorkId, Work>; // WorkId preserves arrival order; only the core mutates this table
```

`PendingRequest` and `AcceptedRequest` are not cloneable. Acceptance consumes the reply route and creates the ticket output (make-illegal-states-unrepresentable).
Queued variants hold no place. Each other variant holds one place until its ACP work ends or its runtime process ends.
The table has at most 32 queued requests. Its variant counts define queue length and occupied places; no second counter or queue needs synchronization.
The dispatcher takes the oldest queued request only when occupied places are below the current limit.
A lower limit preserves occupied entries. A completed sign-in check can continue in the same entry without another acquisition.

| Input | Transition |
|---|---|
| Request with no required check | Complete admission checks, then refuse or create an accepted entry. |
| Request that requires a sign-in check | Create a pending entry; acquire a place or wait in the shared queue. |
| Successful check | Clear only sign-in required, update the catalog, and repeat remaining admission checks. |
| Failed sign-in check | Return HTTP 400 with M-13; create no ticket. |
| New stop condition | End queued requests under FR-037; use the effective priority in the PRD. |
| End of a condition | Clear only that condition and evaluate pending work again. |
| Disconnect while queued | Remove that request. |
| Disconnect during an ACP call | Discard its reply route; retain a retiring entry until the call or process ends. |

A request retains its own required check after another request clears sign-in required. A passed check can still yield a quota refusal.
Current-generation check results take effect in conductor event order. An operation ID prevents duplicate completion; the runtime generation rejects retired results.
Each lockout signal is classified before those lifecycle checks, also for a disconnected request.
An error for an already active condition does not flush the queue again.
Before each prompt dispatch, the core checks every applicable condition again.
An expired quota deadline has no effect on admission, even before its timer event is handled. A timer clears only its matching wait.

The admission future owns one cancellation lease. Acceptance transfers it to the ticket, and a lost handoff acts as a disconnect.
The shell sends acceptance before its dependent prompt command. A failed acceptance handoff suppresses that command.
A disconnect during a prompt uses the existing cancel path. A disconnect during a non-prompt ACP call cannot release capacity early.

No work entry, unresolved admission, or external wait owns a `Hold`. The shell registers continuations and deadlines before it releases the incoming hold.
A completion or disconnect carries a new short hold through the next core transition. An occupied place can therefore wait while virtual time advances.
Only an accepted stream has a keep-alive timer. Its first deadline starts at acceptance (FR-011).

Transitional state (#47): the conductor keeps a list of active requests and the records of the idle ACP sessions (ADR-0003). The count of records has no limit, because the drop of ADR-0003 does not exist yet. When a turn writes no record, acp2api forgets its ACP session. #60 replaces this state with the work table above.

### The runtime driver

`runtime_driver` carries out the ACP commands of the conductor on one runtime process. It gives the conductor each line as an event.

```rust
pub(crate) struct RuntimeDriver { /* the launcher, the clock, the MCP tool sets */ }
impl RuntimeDriver {
    pub(crate) fn start(&self, copy: &Copy, generation: u32, events: Mailbox) -> Result<RuntimeProcess, LaunchError>;
}
impl RuntimeProcess {
    pub(crate) fn send(&self, call: AcpCall);         // initialize, session/new, session/load, set_config_option,
                                                      // session/prompt, session/cancel, permission answers, authenticate
    pub(crate) fn stop(self) -> BoxFuture<'static, ()>;
}
```

- `process` builds the `LaunchSpec`:
  - the program is the copy in the data folder;
  - `GEMINI_HOME` is the runtime home (ADR-0005), and acp2api does not set `AGY_ACP_FORCE_FILE_STORAGE`;
  - on Windows, `TEMP` and `TMP` use the owned `processes/<generation>/` folder in section "Account ownership" (ADR-0004).
- `acp` is the JSON-RPC line loop. For each line of stdout, it does these steps in order:
  1. It takes a `Hold` of the kind `RuntimeLine`.
  2. It parses the line with the types of `agent-client-protocol-schema`.
  3. It puts the event with its hold into the mailbox of the conductor.

  It answers a request with an unknown method with `-32601`.
- `acp` sends `initialize` with `clientInfo` `{"name": "acp2api", "title": "acp2api", "version": <ReleaseFacts::version>}` (FR-048). It declares no file system and no terminal capability (FR-047).
- With each `session/new` and `session/load`, `acp` sends `_meta.antigravity.enabledTools: []`, `eagerMcpServers`, and `systemPrompt.append` (ADR-0002, ADR-0003).
- `stderr` reads stderr one line at a time, with a `RuntimeLine` hold. It gives the conductor only two kinds of lines:
  - each error line, parsed into the ID of the ACP session, the HTTP code, the status, and the message, after the redaction of FR-056;
  - the sign-in address of a sign-in wait (FR-039). Only the panel shows it. No log line holds it, because it holds an OAuth `client_id` (`docs/research/2026-10-18-probe-methods-and-safety-rules.md`, section 2).

  acp2api writes no other stderr text anywhere.
- `mcp` serves `POST /mcp/<secret>` for each ACP session, with one set of client tools:
  - `initialize`, `notifications/initialized` (202), and `tools/list`;
  - `-32601` for `server/discover` and each other method, and a tool error for `tools/call`;
  - 405 for `GET`.

  Each request takes a `McpRequest` hold.

### Data flow of one request

1. `front` checks `Host` and `Origin` (FR-008) and the local API key (FR-007).
2. `dialect.parse` gives a `TurnRequest` and a `ReplyForm`, or a `Refusal`.
3. `conductor.admit` sends the request to the core. The core does these checks in this order (FR-026):
   1. the mode;
   2. the model name, from the catalog of the successful sign-in check;
   3. the retry wait;
   4. the queue.

   Then it answers `Refused` or `Accepted`.
4. With a place, the core chooses the ACP session: continue, load, or new, with the N rule. When the model ID changed, it sends `session/set_config_option`. Then it sends `session/prompt` with the result of `conversation::prompt` (ADR-0003, FR-021).
5. `runtime_driver` gives each update, permission request, result, and error line as an event. The machine of the prompt turn in the core gives `TurnEvent`s to the ticket.
6. `dialect.respond` writes each `TurnEvent` to the API client. Each write takes and drops a `TurnEvent` hold.
7. At the end of the prompt turn, the core changes the record, the retry wait, and the mode (R1). It frees the place and starts the next request of the queue. `journal` writes the request line (FR-056).

### Owner of each piece of state

| State | One writer | Readers |
|---|---|---|
| The mode, the places, the queue, the records, the retry waits, the prompt turns, and the count of ACP sessions | the conductor core | the panel and the terminal, through `view()` |
| The failures of the last 10 minutes, the model names, the last `/usage` result, and the notices | the conductor core | the panel and `GET /v1/models` |
| The settings and `config.toml` | `store::Settings`, called only by the conductor shell after an owner action | `front` (the key), `panel` (the password hash), and the core (the limit), through a watch |
| `state.json` | `store::StateFile`, called only by the conductor shell | `builds` and the core, at the start |
| The copy, the search result, and the check copy | `builds`, called only by the conductor shell | `view()` |
| The tool sets of the MCP paths | `runtime_driver::mcp`, fed only by the conductor shell | the MCP handlers |
| The panel sessions | `panel` | none |
| The log files, the last 50 requests, and the counts of unknown fields | `journal` | `panel` |

Some rules cross the state, the places, the records, and the runtime process (FR-035, FR-037, FR-041, ADR-0003). One owner holds all of them, so they need no lock (separate-before-serializing-shared-state).

### Start and exit

`startup` does these steps in this order:

1. Parse the four options (FR-002). `--help` and `--version` end here.
2. Claim the account as section "Account ownership" defines. A duplicate gets M-24 with the first instance's panel address.
3. Open the data folder. Read and validate `config.toml` (FR-003 to FR-005). A wrong value, unknown key, or syntax error ends with M-26. Generate, deliver, and save each absent credential as section "Stored data" defines.
4. Bind `127.0.0.1:<port>`. A port in use ends with M-25. Install the panel handlers, publish its actual address, and release the startup gate.
5. Open the logs. Delete each log file older than 30 days (FR-057).
6. Read `state.json` (FR-035, FR-044).
7. After their process trees end, delete the owned session files and temporary folders (ADR-0004, ADR-0005). Section "Account ownership" limits cleanup.
8. Start the conductor. It starts the runtime process from an accepted copy. With no accepted copy, it searches first (FR-042, FR-043). During a lockout, it starts none. After `initialize`, the sign-in check supplies the initial model names (FR-012, FR-038).
9. Write the panel address to the terminal (section "Decided values"). At a first start, open the panel through the `Browser` port.
10. Serve until `StopSignal::stopped`. Then do the exit of FR-045:
    - the core ends each prompt turn and each request in the queue with M-15;
    - the runtime process stops with its tree.
    - cleanup finishes before the account ownership ends.

### Account ownership

`startup` owns one private lifecycle for account admission, address publication, and cleanup. Callers still use `run(Ports, args, Environment)`.

The production factory supplies these fixed locations as `Environment.account_root`:

| Platform | Account folder |
|---|---|
| macOS | `<native account home>/Library/Application Support/acp2api-account/` |
| Windows | `<native LocalAppData>/acp2api-account/` |

The macOS factory uses the process account ID and its `pw_dir`. Windows uses `SHGetKnownFolderPath` for the process account.
Neither factory trusts an override of `HOME` or `LOCALAPPDATA` for account ownership. The data folder still follows FR-003.
The account folder is a sibling of the default data folder. Thus `--data-dir` leaves the default data folder empty.
The paths have no version component. Two starts for the same native account must resolve the same account folder.

The folder contains `startup.lock`, `instance.lock`, `panel`, and `owned-temporary-files.json`.
The two lock files stay empty and permanent. acp2api never deletes, renames, truncates, or replaces them.
Only `startup` opens their handles. The handles cannot be copied to a runtime process.
On Windows, their share mode omits `FILE_SHARE_DELETE`.

The protocol is:

1. Acquire `startup.lock` with cancellable attempts through `File::try_lock` and the existing clock.
2. Try `instance.lock` without a wait.
3. If it is occupied, read and validate `panel` under the startup gate. Return M-24 with that address, or M-28 for unusable data.
4. If it is free, keep both locks and ignore the earlier address record.
5. Prepare the settings and bind the listener. Install the panel handlers before publication.
6. Write the actual panel address. Release `startup.lock`, but keep `instance.lock` through shutdown and cleanup.

Address publication does not wait for the official runtime. The initial panel can therefore open during runtime initialization.
On normal exit, keep the listener and the lifetime lock through runtime shutdown and cleanup.
Then acquire the startup gate, close the listener, clear `panel`, and release the lifetime lock before the gate.
A contender never waits for the lifetime lock while it owns the gate, so the lock order has no cycle.

The `panel` record has a format version, the actual loopback address, and the publisher's OS process identity.
The identity includes the process creation identity, so PID reuse cannot validate an old record.
This check rejects stale publication during delayed OS lock release. It never grants ownership or permits lock removal.
An address record contains no password or key. A malformed live record is a startup error, not evidence of free ownership.

Each account has its own subtree at `<temp>/acp2api/<account-id>/`.
The stable `<account-id>` is the SHA-256 of the canonical account-root path. This reuses the hash dependency of ADR-0006.
Each run has a separate child folder. Its `sessions/` and `processes/` children remain separate (ADR-0004, ADR-0005).
Cleanup never sweeps the shared `<temp>/acp2api/` parent.

The ownership file records each run's absolute temporary subtree before its creation. It uses format version 1 and an array of owned roots.
Each root also records the process identity needed to confirm that its runtime tree ended.
After deletion, or proof that a root is absent, remove its entry. The record lets a later start find earlier temporary roots.
Only the account owner can read or change these records. Validate each path, owner, permission, and file type before cleanup.
Reject symlinks and reparse points that escape an owned root. An untrusted record cannot authorize deletion.
The account lock alone does not prove that an earlier runtime tree ended. Native process checks must confirm that fact before cleanup.
The startup-gate wait has no total deadline and remains cancellable (FR-006). Cancellation changes no file of the owner.
An unreadable or damaged coordination path gives M-28 with that path. No diagnostic removes or replaces a lock.

Sources: [Rust file locks](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock),
[Windows share mode](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.share_mode),
[Windows lock release](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex),
[Apple account lookup](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/getpwent.3.html),
and [Windows account folders](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath).

The review ran a bounded Rust probe on macOS with seven child processes and no network requests.
Delayed publication, simultaneous claims, stale metadata, and a crash before publication passed. The lock-file identities stayed unchanged.
This proves those lock operations only. Product behavior, native Windows behavior, permissions, and recovery of runtime children still need their acceptance checks.

### The test harness

#### Scenario world and test root

```rust
pub struct Scenario { root: TestRoot, hub: Hub, pub time: TimeLine, host: HostSlot,
                      client: Option<ApiClient>, owner: Option<Owner>, plan: Option<Setup> }
impl Scenario {
    pub fn plan(&mut self, setup: Setup);                 // a Given step records the setup
    pub async fn client(&mut self) -> &mut ApiClient;     // starts the test host at the first use, then settles
    pub async fn owner(&mut self) -> &mut Owner;          // the fake browser of the owner
    pub fn fake(&self) -> FakeRuntime;                    // the behavior and the record of the fake ACP agent
    pub async fn restart(&mut self); pub async fn stop(&mut self); pub async fn kill(&mut self);
}
```

- `TestRoot` is a new folder for each scenario, in `target/scenario-roots/<run>/<scenario>/`. It contains the account, platform, data, and build folders.
- Its children are `account/`, `home/`, `local-app-data/`, `temp/`, `path/`, `exe/`, `data/`, and `builds/`.
- The run file gives these folders to the test host as its `Environment`. Thus each scenario has its own platform folders, folders of Zed and JetBrains, account lock, and `~/.gemini`. Parallel scenarios do not block each other (FR-006).
- A port number in a step text, for example 8329, is an alias of a free port of the scenario. A step that needs one real fixed port takes one lease for the whole test run. Thus the scenarios with that port run one at a time.
- The harness deletes each test root after a passed scenario. At startup, it removes owned output of earlier runs that have ended. It preserves active runs and unknown paths.

#### `Setup::normal()`

Issue #36 writes it, and no later slice edits it. It gives these items:

- one fake build `1.4.0` for the platform of the host, put directly as the copy in `data/official-runtime/`;
- a run file whose release facts list only that build, with the highest concurrency limit 3 and the version of the crate;
- a `config.toml` with these values:
  - the port alias and `concurrency_limit = 2`;
  - a random local API key;
  - the hash of a random management password, which the scenario keeps;
- no `state.json`;
- the defaults of `FakeRuntime`:
  - signed in;
  - the model `gemini-3.8-flash` with the name "Gemini 3.8 Flash" and the levels `low`, `medium`, and `high`;
  - the context size 1,048,576;
  - for a prompt with no plan: the reply `OK`, one usage update, and `end_turn`;
  - for a cancel: the answer of the real official runtime, which is a `failed` update, the text `The request was cancelled by the client.`, and `cancelled`.

From #36 on, acp2api starts the runtime process from the copy. #58 adds the hash check before the start and the search after the start. The copy is on the list, and the search finds nothing in the empty folders of the test root. Thus the step does not change.

Later slices add `Setup` methods. Examples are `.highest_concurrency_limit(2)`, `.build(FakeBuild::listed("1.5.0").at(Place::Zed))`, `.no_sign_in()`, and `.state_file(StateSeed::lockout())`.

#### The fake build and the guard (SC-001)

- A fake build is two files in `builds/<label>/`:
  - a copy of `fake-acp-agent` with the name of `agy_acp_server` on the platform: `agy_acp_server.par` on macOS, and `agy_acp_server.exe` on Windows;
  - a descriptor with the name of `localharness_external` on the platform.
- The descriptor is a small TOML text. It holds a fake marker, the version, the platform, the label, the hub address, and the token of the scenario.
- acp2api only hashes and copies the descriptor. The fake reads the descriptor next to its own executable.
- Thus each version, each platform, and each changed byte is a different descriptor, with a different pair of hashes. The executables keep their signature, because the harness never changes their bytes.
- The fake starts each harness process as a copy of itself, with the argument `--harness`. Each harness process connects to the hub. Thus the hub sees its end, also after a kill of acp2api.
- The guard is `GuardedLauncher`, the test adapter of `ProcessLauncher`:
  - before each start, it calculates the SHA-256 of the program;
  - it refuses each program whose hash is not the hash of `fake-acp-agent`, and it reports each refusal to the hub;
  - it wraps the real `OsLauncher`, so the process group, the Job Object, and the Windows `TEMP` folder run in each scenario.
- `RecordingBrowser` starts no program. The clippy rule of section "Rules that the build enforces" stops each other way to start a program.

#### The fake ACP agent

The fake is a puppet. All its behavior is Rust code of the harness, in the test process. Thus a later slice adds harness code and does not change the fake executable.

- At its start, the fake reads its descriptor and connects to the hub. It sends `Hello` with these values:
  - its token, its label, and its role (server or harness);
  - its PID and the PID of its parent;
  - its argv, its `cwd`, and the names of its environment variables;
  - the values of `GEMINI_HOME`, `TEMP`, and `TMP`.
- It relays each line of its stdin to the hub (`Got`). It carries out the commands of the hub: `Out(line)`, `Err(line)`, `StartHarness(count)`, and `Exit(code)`.
- At the end of its stdin, it does what the real official runtime does on its platform (ADR-0004). On macOS, it stops its harness processes and exits. On Windows, it continues.
- Caps (`CLAUDE.md`):
  - 5,000 lines in each direction, then the exit code 70;
  - 300 s of real time, then the exit code 71;
  - an exit at the loss of the hub;
  - no file.
- It uses only the standard library and `serde_json`. It writes the JSON-RPC shapes of the captures in `docs/research/captures/`, not through an ACP library. Thus it is an independent oracle of the wire shapes.

`FakeRuntime` in the harness answers each ACP request from the defaults and from the plans of the scenario. At the first prompt of each ACP session, it calls `tools/list` on the MCP path of that ACP session. The official runtime does the same with `eagerMcpServers` (ADR-0002). Its plans of prompt turns only grow:

```rust
pub struct TurnPlan { steps: Vec<TurnStep>, end: TurnEndPlan }
pub enum TurnStep { Chunk(String), Thought(String), Usage { used: u64, size: u64 }, After(Duration),
    ClientToolCall { name: String, arguments: String }, BuiltInPermission { title: String },
    BuiltInRuns { name: String }, ErrorLine { code: u16, status: String, message: String },
    Stderr(String), HoldUntil(Gate) /* later slices add variants only */ }
pub enum TurnEndPlan { Stop(StopReason), RpcError { code: i64, message: String }, IgnoreCancel, Exit(i32), Never }
```

The record holds each line in both directions, each `Hello`, each MCP request and its answer, and each end of a process. Each entry has its virtual time. Steps read it through typed queries, for example `prompts()`, `cancels()`, `client_info()`, `session_meta()`, `offered_tools()`, `usage_queries()`, and `processes_alive()`.

The fake tracks active prompt turns and cancellation state by ACP session. A cancellation affects only its named session and cannot complete another session's turn. A completed turn leaves no pending cancellation state.

#### The fake API client and the fake browser

- `ApiClient` sends raw HTTP/1.1 through hyper, so a step can set each header, also a wrong `Host` or an `Origin`.
- `ApiClient` reads each SSE event and stamps it with the virtual time of its arrival. Its SSE reader has its own unit tests. A drop of a call is a disconnect.
- `Owner` is a fake browser. It has a cookie jar and an `Accept-Language` header. It reads the HTML text of the page, and it posts the forms of the management endpoints.
- Caps: 300 requests for each scenario, 8 MiB for each answer, and 30 s of real time for each request.

#### Virtual time

The hub owns the virtual time `now`. The wall time is a fixed epoch of the scenario plus `now`.

- `HarnessClock`, the test adapter of `Clock`, reports each deadline of acp2api to the hub. It counts live holds and wakes sleepers only in the deadline phase.
- `GuardedLauncher` counts the lines on stdin, stdout, and stderr of each fake process. The fake reports the lines that it got, and the hub knows the lines that it ordered.
- `FakeRuntime` and `ApiClient` register their own deadlines on the same `TimeLine`, for example for `TurnStep::After`.

```text
settle():                                   # fails after 10 s of real time and names the open channel
  repeat:
    wait until the test host reports: no live Hold, and each awakened owner has finished or waits again
    s = snapshot of the counts
    if s is balanced and s == previous: return
    previous = s; send a drain tick with the same now   # drain mailboxes; do not wake product timers
  balanced: each line that the hub ordered was read by the test host, each line that the
            test host wrote was relayed by the fake, each HTTP request of ApiClient and Owner
            took its HttpRequest hold or got its answer, each MCP request got its answer,
            and ApiClient read each byte that is available

advance_to(target):
  loop:
    settle()
    t = earliest deadline of the test host, of FakeRuntime, and of ApiClient
    if there is none or t > target: now = target; tick; settle(); return
    now = t; announce now without waking product timers
    dispatch external inputs scheduled at t; settle()
    wake product timers due at t; settle()
```

- "When 60 minutes pass" is `advance_to(now + 60 min)`. "At the time 5 s" is `advance_to(mark + 5 s)`.
- `ApiCall::finish` alternates `settle` with an advance to the next deadline until the answer ends. Its virtual limit is 3 hours.
- The design uses no `tokio::time::pause`, no time scale, and no real sleep.
- A self-test of the harness in #36 checks the settle against bursts of lines.
- At one virtual instant, scheduled external inputs arrive before product deadline callbacks. Thus the 200 ms boundary of FR-028 is deterministic.
- Delivery transfers ownership from the sleep future to its returned `Hold`. The hub keeps that work pending until the `Hold` drops, also while another owner registers a timer. Section "Ports" gives the rule for the owner.
- `ProcessTree::stop` keeps the timer's `Hold` until the forced stop and process wait finish.
- A one-shot timer and a process-stop timer can finish without another sleep registration. Self-tests cover these terminal owners and work still executing after a wake.

#### Runner and step files

- `crates/acp2api-acceptance/tests/acceptance.rs` has `harness = false`. It runs cucumber over `features/acp2api/` with these settings:
  - the default filter `not @wip`, which is the exclusion rule of `bdd`;
  - 4 scenarios at the same time;
  - a real-time limit of 20 s for each step.
- There is one step file for each feature file, with the stem of the feature file. Steps that several feature files use are in `common.rs`.
- The default test command of the project is `cargo test --workspace --locked`. It runs the unit tests and the runner.
- Other caps, each of which stops the scenario and names itself:
  - a terminal capture of 4 MiB for each test host;
  - a hub record of 20,000 events or 16 MiB for each scenario;
  - a size check of 128 MiB for each test root at its end.

#### How a test sets the highest concurrency limit (FR-049)

`Setup::highest_concurrency_limit(2)` writes the value into the release facts of the run file. The test host gives these release facts to `run`. In the release, `release/builds.toml` gives the value for each platform.

### Rules that the build enforces

- `clippy.toml` disallows these methods (encode-lessons-in-structure):
  - `std::process::Command::new` and `tokio::process::Command::new` outside `os`;
  - `TcpStream::connect` in the crate `acp2api`;
  - `tokio::time::sleep`, `tokio::time::Instant::now`, `std::time::Instant::now`, `std::time::SystemTime::now`, and `std::env::temp_dir` outside `os`.
- CI runs clippy with `-D warnings`.
- A match on `Row`, `Message`, `Mode`, `Stop`, or `TurnEvent` has no default arm. Thus a new case shows each place that must handle it.

### Stored data

Data folder (FR-003):

```text
<data folder>/
  config.toml                         the settings
  state.json                          the lockout and the records of the compatibility checks
  logs/acp2api-YYYY-MM-DD-<n>.log     <n> starts at 1; FR-057 defines log age
  runtime-home/                       GEMINI_HOME of each runtime process (ADR-0005)
  official-runtime/                   the copy: agy_acp_server.par and localharness_external (macOS),
                                      agy_acp_server.exe and localharness_external.exe (Windows)
  official-runtime-check/             the check copy of a compatibility check; it replaces the copy only after a pass
```

The fixed account folder is outside the data folder. Section "Account ownership" defines its paths, formats, permissions, and migration constraint.

```text
<account root>/
  startup.lock                       the permanent startup gate
  instance.lock                      the permanent lifetime lock
  panel                              the versioned address and publisher identity
  owned-temporary-files.json         the versioned ownership records

<temp>/acp2api/<account-id>/<run>/
  sessions/<id>/                     the empty cwd of each ACP session (ADR-0005)
  processes/<generation>/            TEMP and TMP of each runtime process, Windows only (ADR-0004)
```

`config.toml` (FR-004):

```toml
port = 8329
concurrency_limit = 2
local_api_key = "acp2api-…"                     # "acp2api-" and 43 characters of base64url (32 random bytes)
management_password_hash = "$argon2id$v=19$…"   # PHC string; m=19456, t=2, p=1
agy_acp_server_path = "/path/to/agy_acp_server.par"   # absent: none
```

- Each setting is a top-level key.
- acp2api writes the file with owner-only permissions: the mode 0600 on macOS, and an ACL with only the account of the owner on Windows.
- A value that acp2api cannot use stops the start with M-26, where `<setting>` is the key. These values are not usable:
  - a port outside 1 to 65535;
  - a concurrency limit outside 1 to the highest value of the platform;
  - an empty local API key, or a key with a character other than visible ASCII;
  - a hash that is not an Argon2id PHC string;
  - a path that is not absolute.
- An unknown key stops the start with M-26. `<setting>` is the unknown key.
- A syntax error also gives M-26. `<setting>` is the identifiable key of the failed line, or `at line <n>` with a one-based line number.
- Defaults apply only to absent keys, never to a value that fails validation (boundary-discipline):
  - `port` uses 8329, unless `--port` overrides it;
  - `concurrency_limit` uses the smaller of 2 and the platform maximum (FR-049);
  - `local_api_key` gets a new random key, which acp2api saves before it accepts requests;
  - `management_password_hash` gets the hash of a new random password;
  - `agy_acp_server_path` stays absent.
- The terminal shows only the key or password made at this start (FR-004, FR-005, FR-056).
- Before atomic persistence, write and flush each new credential to the terminal. If delivery fails, return the error without persistence.
- Preserve valid saved credentials after a failed start. A later start generates and delivers only credentials whose fields remain absent.
- The random management password has 20 characters from `a-z` and `2-9`, in four groups of five with `-` between them.
- A change in the panel writes the whole file again, through a temporary file and a rename. A comment in the file does not survive such a write.

`state.json` (FR-035, FR-044):

```json
{
  "format": 1,
  "lockout": null,
  "checked_builds": [
    { "version": "1.6.0", "platform": "macos-arm64",
      "agy_acp_server_sha256": "<64 hex>", "localharness_external_sha256": "<64 hex>",
      "checked_at": "2026-10-18T12:00:00Z" }
  ]
}
```

- In a lockout, `lockout` is `{"since": "<RFC 3339 UTC>", "signal": "<signal>", "original_error_text": "<text>"}`. The text has the redaction of FR-056.
- acp2api parses the file strictly. A lockout starts when the file exists and one of these is true (FR-035):
  - acp2api cannot read it or parse it;
  - it has another `format`.
- An absent file gives no lockout and a new file.
- Each write is a whole file, through a temporary file and a rename.

`crates/acp2api/release/builds.toml` (ADR-0006, FR-049):

```toml
[highest_concurrency_limit]
macos-arm64 = 3
macos-x64 = 3
windows-x64 = 3
windows-arm64 = 3

[[build]]
version = "1.4.1"
platform = "macos-arm64"
agy_acp_server_sha256 = "<64 hex>"
localharness_external_sha256 = "<64 hex>"
```

The release workflow writes this file from the result of the acceptance test (SC-002). `include_str!` puts it into the executable. A unit test parses it.

### Decided values

**Setting names (FR-004).** `port`, `concurrency_limit`, `local_api_key`, `management_password_hash`, and `agy_acp_server_path`.

**OpenAI error bodies (FR-026).** The body is `{"error": {"message", "type", "param", "code"}}`. A Chat Completions stream sends the same `error` object (FR-027).

- The `type` follows the HTTP status: `invalid_request_error` for 400 and 401, `rate_limit_error` for 429, and `server_error` for 502 and 503.
- The `code` names the case, as the table gives.
- `param` is the field path for M-02, and `model` for M-08. Else it is `null`.

| Message | Status | `code` |
|---|---|---|
| M-01 | 401 | `invalid_api_key` |
| M-02 | 400 | `unsupported_parameter` |
| M-03 | 400 | `unsupported_content_type` |
| M-04 | 400 | `unsupported_media_type` |
| M-05 | 400 | `unsupported_url` |
| M-06 | 400 | `unsupported_pdf` |
| M-07 | 400 | `invalid_json` |
| M-08 | 400 | `model_not_found` |
| M-09 | 400 | `unsupported_tool` |
| M-10 | 400 | `unsupported_tool_name` |
| M-11 | 400 | `unsupported_server_tool` |
| FR-024, a tool type that acp2api does not know | 400 | `unsupported_tool_type` |
| FR-024, the advisor tool | 400 | `unsupported_advisor_tool` |
| FR-024, another server tool of the Anthropic Messages API | 400 | `unsupported_server_tool` |
| M-12 | 400 | `lockout` |
| M-13 | 400 | `sign_in_required` |
| M-14 | 400 | `runtime_unavailable` |
| M-15 | 400 | `acp2api_stopped` |
| M-16 | 429 | `quota_wait` |
| M-17 | 503 | `queue_full` |
| M-18 | 503 or 502, as its row | `upstream_error` |
| M-19 | 503 | `no_update` |
| M-20 | 503 | `runtime_process_failed` |
| M-21 | 502 | `max_turn_requests` |
| M-22 | 502 | `built_in_tool_ran` |
| M-23 | the status of the earlier row | `retry_wait` |
| M-27 | 400 | `unsupported_prefill` |

- The OpenAI API uses `invalid_api_key`, `unsupported_parameter`, and `model_not_found` for the same cases. The other codes are the names of the cases of acp2api.
- The `response.failed` event of the Responses API keeps the codes of FR-027: `server_error` for the 503 row, and `invalid_prompt` for each other case. No body and no event uses `insufficient_quota`.
- Evidence:
  - the OpenAI SDK reads `error.message` and keeps the other fields (openai-python 3.35.1, `openai/_streaming.py`, lines 94–101);
  - Codex chooses its HTTP retry by the status only (`docs/research/2026-10-05-api-endpoint-contract.md`, section 4.2).

**Keep-alive events (FR-011).** The core sends `TurnEvent::KeepAlive` when a stream had no event for 15 s.

- Anthropic Messages API: `event: ping` with `data: {"type": "ping"}`.
- OpenAI Responses API: `response.in_progress` with the response object and the next `sequence_number`.
- OpenAI Chat Completions API: the SSE comment line `: keep-alive` and a blank line.
  - A client that obeys the SSE specification ignores a comment line. The OpenAI SDK does too (openai-python 3.35.1, `openai/_streaming.py`, line 410).
  - A chunk with an empty delta would reach the code of each API client, so the design does not use it.
- At the acceptance, the stream first gets `message_start` (Anthropic Messages API) or `response.created` (Responses API). A Chat Completions stream gets its first chunk only with the first text.

**Addresses (M-12, M-13, M-14, M-24).**

- The address of the management panel is `http://127.0.0.1:<port>/`.
- The address of the sign-in entry is `http://127.0.0.1:<port>/#sign-in`. The page puts the sign-in entry in the element with the ID `sign-in`. After the entry of the password, the page goes back to the same fragment.
- The management endpoints are `POST /manage/<action>`, one for each action of FR-053.
- The server writes the page as HTML, with forms. A small script on the page only updates the time left of a sign-in wait.
- Residual risk, accepted: a browser does not isolate a cookie by port. Thus, the browser also sends the panel session cookie to other services on `127.0.0.1`. Such a service can then use the panel, and it can read the local API key. A local process can already connect to each loopback port, so this risk adds little. On an HTTP loopback address, each fix changes FR-009 or the page design.
- The MCP path of each ACP session is `/mcp/<32 random bytes as hex>`.

**Attribution block of Claude Code (FR-019).** When the first `system` block is a text block whose text starts with `x-anthropic-billing-header:`, `dialect::messages` removes it.

- Evidence: Claude Code 2.1.299 makes the block with a function that returns `x-anthropic-billing-header: cc_version=<version>.<n>; cc_entrypoint=<entrypoint>;` and more fields. Claude Code itself finds the block by `startsWith("x-anthropic-billing-header:")`.
- Source: the bundle in the executable `~/Library/Application Support/Claude/claude-code/2.1.299/…/MacOS/claude`, read on 2026-10-18. The proof level is 2.
- The client check of SC-003 confirms the start with a run.

**Terminal lines (FR-002, FR-005, FR-035).**

- Each start: `acp2api: <version>: management panel at http://127.0.0.1:<port>/`.
- A start that makes a local API key: `acp2api: local API key: <key>`.
- A start that makes a management password: `acp2api: management password: <password>`.
- Each request: `acp2api: `, then the fields of the request line of FR-056, in this order:
  - the time, the request ID, the API, and the model name;
  - the short ID of the ACP session, and `new`, `continue`, or `rebuild`;
  - the duration in ms, and the input and output tokens;
  - the result, the error code, and the names of the unknown fields.
- Each change of state, also the first state after a start: `acp2api: <time> state <state>`. `<state>` is the English name of the panel text, for example `Running` or `Quota wait`. A lockout line adds `: ` and the notice M-12.
- A step that checks the state of acp2api reads the last state line.

**Request ID header (FR-056).** Each response of an API endpoint has the header `x-request-id`. Its value is the request ID of the request line.

**Times.** Each time in a message, a log line, the terminal, and `state.json` is RFC 3339 in UTC, to the second.

**Quota reset time (FR-036).** The parser accepts RFC 3339 and the captured `/usage` form, such as `resets Oct 24, 2026, 12:34 UTC`.

Each other form gives no usable reset time. The conductor then follows the fallback of FR-036. No capture proves the format of a Google 429 error.

## Seams

| Seam | Category | Port | Adapters (production; tests) | How tests cross |
|---|---|---|---|---|
| The official runtime | External | the process contract: two executables, ACP on stdio, error lines on stderr, MCP calls back | the real build; the fake build (`fake-acp-agent` and a descriptor) | a real child process of the test host |
| Process control | Local operating system | `ProcessLauncher` | `OsLauncher`; `GuardedLauncher`, which wraps `OsLauncher` | through the guard, with the real OS |
| The clock and the timers | In-process, not deterministic | `Clock` | `SystemClock`; `HarnessClock` | the virtual time of the hub |
| Release facts | Data fixed at build time | `ReleaseFacts` | `CompiledReleaseFacts` (`release/builds.toml`, `CARGO_PKG_VERSION`); `RunFileReleaseFacts` | the run file |
| The browser of the owner | External program | `Browser` | `SystemBrowser` (`open` on macOS, `ShellExecuteW` on Windows); `RecordingBrowser` | a record in the hub |
| The stop in the terminal | Local operating system | `StopSignal` | `TerminalStop` (Ctrl+C, and on Windows also Ctrl+Break); `HubStop` | a stop command of the hub |
| API clients | External callers of a published contract | HTTP on 127.0.0.1; inside it, the `Dialect` port with three adapters | Claude Code, Codex, Chat Completions clients, and the browser; `ApiClient` and `Owner` | real HTTP |
| The MCP client in the official runtime | External caller | MCP over HTTP | the official runtime; `FakeRuntime` | real HTTP |

- The test seam of the product is the test host executable. It differs from the release executable only in `main`.
- These items have one adapter each, so they get no port (deep-modules): the file system, the terminal, the account lock, randomness, and the listeners.
- The tests use local stand-ins for them:
  - the folders of the test root, through `Environment`;
  - the real stdout and real OS locks in the isolated account folder;
  - real random values and real sockets.
- `tdd` puts its unit tests at these seams:
  - the pure functions of `conversation`;
  - `Core::handle`, with a `Moment` as its argument, so a unit test needs no clock adapter;
  - `Dialect::parse`;
  - the parsers of `store` and of `runtime_driver::stderr`;
  - `messages`.

## Merge decision

**Recommendation.** Take design C (ports and adapters) as the base. Add five parts of the other designs and two own changes. The main cost is the virtual-time protocol. It is the most complex code of the harness, and it puts four kinds of holds into the product.

Design C is the base, because its score table has the fewest design red flags found: one, against two for each other design.

Parts from other designs:

- **From designs A and B: the descriptor in place of `localharness_external`.** Design C appended a trailer to the executables, which can break the signature on macOS (its assumption A1).
- **From design B: the puppet fake.** All behavior and all waits of the fake are in the harness. Thus the fake executable is a small fixed relay with no deadlines of its own.
- **From design B: the copy of the fake directly in the data folder in `Setup::normal()`.** Design C let acp2api search for the fake and copy it. But #36 comes before the search of #58, so the step of design C needs the work of a later slice.
- **From design B: the rule R1 and the type `Last`.** R1 puts each change of state before the response. The dialect rejects a prefill before it constructs `Last`.
- **Account ownership: revised after the owner rejected temporary-root scope.** The comparison below replaces this part of the original merge.
- **From design A: the check copy of a compatibility check.**

Own changes:

- **ACP through `agent-client-protocol-schema` and an own line loop.** Designs A and C used the connection of the ACP library. Design B used an own line loop for FR-028. Section "One-way decisions", item 6, gives the reasons.
- **An own MCP server in place of `rmcp`.** All three designs used `rmcp`. Section "One-way decisions", item 6, gives the reasons.

Rejected parts:

- **Design A: the variable `ACP2API_TEST_KIT` in the release executable.** It replaces the list of accepted builds, and ADR-0006 rejects such a setting. Design A also ran the 200 ms window in real time.
- **Design B: the start of acp2api in the test process.** It needs a second start path for the scenarios of the executable. It also needs fields only for tests in the public `Setup`, and a scheduler that differs from production.
- **Design C: N in the release facts.** No scenario needs a small N, because the fake makes N ACP sessions cheaply.

Score tables:

**Design A: the smallest interface.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | `gateway::answer` and `wire::read`/`write` hide the state rules and the three wire formats | |
| information leakage | deep-modules | found | `outside` in the release executable knows the hub protocol and the fake marker | a change of the harness protocol edits the product |
| temporal decomposition | deep-modules | absent | `wire::read` and `wire::write` are one module with one body of knowledge | |
| pass-through method | deep-modules | absent | `post_api` adds the key check | |
| importable internals | deep-modules | absent | the crate has no library, so nothing can import it | |
| split ownership | separate-before-serializing-shared-state | found | the keeper task owns the holder of the runtime process, and `Core` owns the mode, which also says if a runtime process exists | two copies of one fact |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | one start path | |
| hand-synced list | make-illegal-states-unrepresentable | absent | one `Message` enum | |
| depth | deep-modules | deep | three entry points of `gateway` hide about 30 FRs | |

**Design B: the simplest common call.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | `App::answer` hides about 40 FRs | |
| information leakage | deep-modules | found | `Setup.listener` and `Setup.spawn_only` exist only for tests, and the tests need a current-thread scheduler | the public interface carries test concerns |
| temporal decomposition | deep-modules | absent | modules by body of knowledge | |
| pass-through method | deep-modules | absent | | |
| importable internals | deep-modules | absent | only `start` is public | |
| split ownership | separate-before-serializing-shared-state | absent | one `Core` | |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | found | the start in the test process and the executable mode both start acp2api for tests | two harness paths with two kinds of time |
| hand-synced list | make-illegal-states-unrepresentable | absent | `errors::row` is the one table | |
| depth | deep-modules | deep | one call for each handler | |

**Design C: ports and adapters.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | `Browser` and `StopSignal` are small, but each one is a real seam with two adapters | |
| information leakage | deep-modules | found | `Clock::hold` and `HoldKind` exist only for virtual time | an input path without a hold makes the settle wrong |
| temporal decomposition | deep-modules | absent | modules by body of knowledge | |
| pass-through method | deep-modules | absent | | |
| importable internals | deep-modules | absent | `pub(crate)` modules | |
| split ownership | separate-before-serializing-shared-state | absent | the conductor is the one writer | |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | the tests always cross at the test host | |
| hand-synced list | make-illegal-states-unrepresentable | absent | `Failure::row` is the one map | |
| depth | deep-modules | deep | `run` is one entry point; the conductor has `admit`, `owner`, and `view` | |

**The merged design.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | as design C; the fake executable is a small relay, but it is a test adapter, not a product module | |
| information leakage | deep-modules | found | four hold points in the product: `HttpRequest`, `RuntimeLine`, `McpRequest`, and `TurnEvent` | a fifth input path without a hold makes the settle wrong; the settle self-test of #36 and the 10 s limit of `settle` find it |
| temporal decomposition | deep-modules | absent | modules by body of knowledge | |
| pass-through method | deep-modules | absent | `post_messages` only selects the dialect, as an axum route must | |
| importable internals | deep-modules | absent | only `run`, `Ports`, `Environment`, the port traits, and `os` are public | |
| split ownership | separate-before-serializing-shared-state | absent | the conductor core is the one writer of the state of acp2api | |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | one test start path, one ACP path, and one MCP path | |
| hand-synced list | make-illegal-states-unrepresentable | absent | `Row` and `Message` are each one enum; `builds.toml` is one list | |
| depth | deep-modules | deep | `run`; the conductor with four methods; `Dialect` with three methods; the pure functions of `conversation` | |

The types of section "Shape" are stubs that compile with rustc 1.96.0 (`rustc --edition 2024 --crate-type lib --emit=metadata`), together with the three usage examples. The stubs are in a temporary folder outside the repository.

### Account-lock comparison after revision

The three focused designs first chose the same two-lock protocol. One designer repeated the task with native IPC ownership as its structural constraint.
That repeat removed the address file, but it could not safely recover a stale macOS socket without another ownership mechanism.
All designs came from one model. Their agreement is weak evidence; the bounded macOS probe supplies the execution evidence stated above.

| Design | Interface and state owner | Decision |
|---|---|---|
| L-A: smallest interface | `run` owns the lifecycle; an explicit `account_root` value selects the account | Take its separate account value and owned-root records |
| L-B: simplest common call | `run` owns the lifecycle; callers see no lock API | Base: the smallest initial interface, with no found red flag |
| L-C: fewest new test seams | Private claim and publish states use real OS resources | Take its publisher identity check; expose no new port |
| L-B2: native IPC repeat | A live local server owns publication state | Reject: stale macOS socket removal can admit two owners |
| Merged | One private lifecycle, one account-root value, five unchanged ports | Fixed account ownership plus separate temporary-file ownership |

The merge keeps L-B's common call. It takes the separate account value from L-A because data paths can follow environment overrides.
It takes L-C's process creation identity to reject stale publication without PID-based lock ownership.
The ownership records preserve cleanup across temporary-root changes. Their format and permissions need design approval as stored-data decisions.

**L-A, L-B, L-C, and merged account design score.** The four designs have the same score in each row.

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | One lifecycle hides admission, publication, and cleanup. |  |
| information leakage | deep-modules | absent | Only environment values cross the caller boundary. |  |
| temporal decomposition | deep-modules | absent | One private owner contains the sequence. |  |
| pass-through method | deep-modules | absent | No public lock wrapper. |  |
| importable internals | deep-modules | absent | No public lock handles or cleanup permits. |  |
| split ownership | separate-before-serializing-shared-state | absent | The lifecycle owns both locks and publication. |  |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | Production and tests call the same run. |  |
| hand-synced list | make-illegal-states-unrepresentable | absent | One versioned publication record. |  |
| depth | deep-modules | deep | The caller has one start operation. |  |

**L-B2 score.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | One lifecycle hides admission, publication, and cleanup. |  |
| information leakage | deep-modules | absent | Only environment values cross the caller boundary. |  |
| temporal decomposition | deep-modules | absent | One private owner contains the sequence. |  |
| pass-through method | deep-modules | absent | No public lock wrapper. |  |
| importable internals | deep-modules | absent | No public lock handles or cleanup permits. |  |
| split ownership | separate-before-serializing-shared-state | found | A stale pathname and a live listener can acquire different owners. | Two recovery contenders can each serve. |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | Production and tests call the same run. |  |
| hand-synced list | make-illegal-states-unrepresentable | absent | One in-memory publication state. |  |
| depth | deep-modules | deep | One query hides the protocol; macOS recovery remains incomplete. |  |

### Admission revision: three independent designs

The approved review added checks before acceptance and conditions that can coexist. A separate admission helper would split capacity and cancellation ownership.
The redesign keeps the conductor as the sole owner (redesign-from-first-principles). The three designers saw the requirements and frame, but no competing design.
The third constraint was "the clearest typed lifecycle". No new dependency needed a port; the existing five ports stay unchanged.

| Design | Structure | Main strength | Decision |
|---|---|---|---|
| A: smallest interface | One ordered table holds every queued or occupied request. Counts derive from variants. | No duplicate queue index or capacity count. | Base of the merge. |
| B: simplest common call | One FIFO and a separate occupied map; a sign-in revision fences check results. | One admission call hides each step. | Keep the call and cancellation handoff; reject revision fencing. |
| C: typed lifecycle | One FIFO and occupied map, with distinct pending, accepted, and drain payloads. | Pending work cannot emit accepted output. | Take non-cloneable payloads and the cancellation lease. |

All three use the same caller: `match conductor.admit(turn, form, hold).await { Refused(r) => error(r), Accepted(t) => respond(t) }`.
Their private structures differ: A derives queue membership from one table; B and C transfer entries between two collections.
Thus this comparison needed no repeated designer. A bounded table scan costs little for 32 queued requests and at most three occupied places.
Other place-consuming work can also occupy the table. It uses the same capacity rule.

The merge uses A's table and C's response capabilities. It keeps the existing exclusive runtime mode and separate sign-in and quota conditions.
B's sign-in revision could reject a current successful check after another failure. FR-038 instead permits that success to clear sign-in required.
The merge uses valid completion order, with runtime-generation and operation-identity checks. Lockout signals remain unconditional.
No implementation caller exists, so there is no migration and no second admission path.

**Admission design A score.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | One admission call hides checks, capacity, and cancellation. | |
| information leakage | deep-modules | absent | The caller sees only refusal or a ticket. | |
| temporal decomposition | deep-modules | absent | No caller-visible admission stages. | |
| pass-through method | deep-modules | absent | Admission owns policy and state. | |
| importable internals | deep-modules | absent | Work variants remain private. | |
| split ownership | separate-before-serializing-shared-state | absent | One core owns the table and conditions. | |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | One admission call. | |
| hand-synced list | make-illegal-states-unrepresentable | absent | Counts derive from one table. | |
| depth | deep-modules | deep | One call hides the whole request admission. | A short table scan on dispatch. |

**Admission design B score.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | One admission call owns the lifecycle. | |
| information leakage | deep-modules | absent | The caller sees no check or place. | |
| temporal decomposition | deep-modules | absent | Stages stay private. | |
| pass-through method | deep-modules | absent | Admission applies the state rules. | |
| importable internals | deep-modules | absent | Both collections remain private. | |
| split ownership | separate-before-serializing-shared-state | absent | One core moves each entry. | |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | One admission call. | |
| hand-synced list | make-illegal-states-unrepresentable | absent | Queued and occupied collections hold distinct ownership stages. | Transfers must remain atomic. |
| depth | deep-modules | deep | The common caller needs no lifecycle knowledge. | Revision fencing conflicts with FR-038. |

**Admission design C score.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | One admission call owns each transition. | |
| information leakage | deep-modules | absent | Response capabilities stay private. | |
| temporal decomposition | deep-modules | absent | The caller starts no separate check. | |
| pass-through method | deep-modules | absent | The core owns policy and dispatch. | |
| importable internals | deep-modules | absent | Lifecycle payloads remain private. | |
| split ownership | separate-before-serializing-shared-state | absent | One core owns both collections. | |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | One admission call. | |
| hand-synced list | make-illegal-states-unrepresentable | absent | Pending and accepted payloads cannot coexist for one owned entry. | Transfers must remain atomic. |
| depth | deep-modules | deep | Types hide output and cancellation capabilities. | More private payload types. |

**Merged admission score.**

| Design red flag | Principle | Result | Evidence | Cost |
|---|---|---|---|---|
| shallow module | deep-modules | absent | The unchanged call owns admission and capacity. | |
| information leakage | deep-modules | absent | The HTTP caller sees no pending state. | |
| temporal decomposition | deep-modules | absent | One call spans every admission stage. | |
| pass-through method | deep-modules | absent | Each entry point owns its policy. | |
| importable internals | deep-modules | absent | Work types stay in the core. | |
| split ownership | separate-before-serializing-shared-state | absent | One core owns work, conditions, and deadlines. | |
| two ways to do one task | migrate-callers-then-delete-legacy-apis | absent | Release and test host share the same path. | |
| hand-synced list | make-illegal-states-unrepresentable | absent | One table derives queue and capacity counts. | |
| depth | deep-modules | deep | One call hides checks, waits, cancellation, and overlap. | A bounded scan and private payload types. |

## Trade-offs

- We accept a second `main` in exchange for a release executable with no test seam.
  - SC-001 does not cover `SystemClock`, `SystemBrowser`, `TerminalStop`, `Environment::from_process`, or the data of `CompiledReleaseFacts`.
  - Unit tests cover the parse of `builds.toml` and of the environment. SC-003 and SC-005 run the release executable.
- We accept the virtual-time protocol and four hold points. In exchange, each time scenario has an exact result on each CI runner, also the 200 ms window.
- We accept an own JSON-RPC line loop and an own MCP server in exchange for three gains:
  - the order of lines that FR-028 needs;
  - a count of the handled lines;
  - no dependency on two libraries with three major versions in 2026.
- We accept that `Setup::normal()` puts the fake build directly as the copy. In exchange, #36 can freeze the step before the search of #58 exists. FR-042 and FR-043 get their own `Setup` methods.
- We accept two permanent account locks and ownership records. They preserve FR-006 across temporary-root changes and prevent cleanup across accounts.
- We accept a puppet fake in exchange for a fixed fake executable. The behavior of the fake is Rust code of the harness, and a later slice adds code there.
- We accept a page that the server writes as HTML with forms. In exchange, the fake browser can read each text with no script engine. The time left of a sign-in wait needs a small script.
- We accept times in UTC in the terminal and in the messages in exchange for no date library. The page can show the local time with a script.
- We accept that a write of `config.toml` from the panel removes the comments, in exchange for no extra TOML library.

## Other designs

### Design A: the smallest interface

Interface: four modules on the request path, each with one to three entry points.

```rust
pub fn read(api: Api, body: &[u8]) -> Result<Ask, Refusal>;                 // wire
pub async fn write(api: Api, answer: Answer, clock: &Clock) -> Response;    // wire
impl Gateway { pub async fn answer(&self, ask: Ask) -> Answer; pub async fn act(&self, a: OwnerAction) -> Result<Done, NotDone>; }
impl Runtime { pub async fn start(..) -> ..; pub fn run(&self, work: Work) -> Running; pub async fn stop(self); }
```

Usage: `let answer = app.gateway.answer(wire::read(api, &body)?).await; wire::write(api, answer, &app.clock).await`.

- Structure: one gateway owns the state rules, the builds, and the life cycle of the runtime process. The release executable reads `ACP2API_TEST_KIT`. That kit replaces the release facts, turns on the guard, links a clock with an offset to the hub, and records the browser.
- Main strength: the smallest interface of the request path, and one environment variable for all test control.
- Why it lost: the release executable gets a hidden switch for the accepted builds, which ADR-0006 rejects. Its clock is real time plus jumps, so the 200 ms window of FR-028 runs in real time with a margin of 50 ms.

### Design B: the simplest common call

Interface: one call for each handler, and one public start of the product.

```rust
pub async fn start(setup: Setup) -> Result<Running, StartError>;   // Setup: args, listener, os, release, clock, terminal, browser, spawn_only
impl App { async fn answer<A: Api>(&self, request: Request) -> Response; }
impl Engine { async fn run(&self, ask: Ask) -> Run; }
```

Usage: `.route("/v1/messages", post(answer::<Messages>))`, and the step `w.plan.reach = Reach::Normal;`.

- Structure: the tests start acp2api in the test process, with a manual clock on a current-thread tokio runtime with park hooks. A second mode runs the executable with the real clock for the scenarios of the process. The core is one struct behind one mutex.
- Main strength: the simplest step code, and a puppet fake that never changes.
- Why it lost: it has two ways to start acp2api in tests, with two kinds of time. Its public `Setup` has fields only for tests. The merged design takes its puppet fake, its descriptor, R1, and `Last`.

## Preparatory refactorings and migration shape

No code exists. Thus no refactoring comes first, and no caller moves.

The order of work follows the blocking edges of the issues. This design changes no slice boundary, but it changes the size of issue #36:

- #36 builds these parts:
  - the two crates, and `run` with the five ports;
  - the start path from a copy;
  - the request path of the Anthropic Messages API with no stream;
  - the full harness core.
- The full harness core has these parts:
  - the hub, and `TimeLine` with `settle` and `advance_to`;
  - `HarnessClock`, `GuardedLauncher` with its counts, `RunFileReleaseFacts`, `RecordingBrowser`, and `HubStop`;
  - the fake executable with its full command set and its caps;
  - `FakeRuntime` with its defaults, and `ApiClient` with its SSE reader;
  - `Setup::normal()`, the settle self-test, and the caps.
- Reason: after the freeze of #36, each harness line that a frozen scenario uses can only stay. Thus each harness mechanism that later slices extend must exist in #36. Later slices only add variants, methods, and step files.
- #36 also needs the setting names now, because the seed of `config.toml` is a fixture of #36.

## Revision decisions

The owner accepted these ten choices on 2026-10-18. Their definitions are in the referenced sections.

| Choice | Definition after revision |
|---|---|
| Initial sign-in check and model names | FR-012, FR-038; section "Start and exit" |
| Unknown configuration keys and TOML syntax errors | FR-004 and M-26 |
| Absent configuration fields | FR-004; section "Stored data" |
| Hashes for all management passwords | FR-009; section "Stored data" |
| Account scope independent of temporary roots | FR-006; section "Account ownership" |
| Four test terms | `GLOSSARY.md`: fake ACP agent, fake API client, test host, virtual time |
| Test host and virtual time decision | ADR-0008 |
| Disk measurement | #36 measures `target/` after the first build; above 3 GB, arrange an external-drive location |
| Reset-time forms | "Quota reset time" under "Decided values"; unknown forms follow FR-036 |
| Final independent review | One `prd-review` round with a cold reader and an attacker, after revision |

The disk had about 9.5 GiB free at the review. The first-build measurement, not an estimate, decides the next storage action.

The owner also approved every recommendation of the final review. FR-006 and M-28 close the account questions below.
FR-011, FR-020, FR-025 to FR-028, FR-033, FR-036 to FR-040, FR-049, and FR-054 hold the revised product contracts.
The feature files cover the clarified arguments, preconditions, and exact time boundaries. The admission revision above carries the changed contracts into the conductor.

### 2026-10-19: repair the shared test harness

The owner approved the four repair groups found during the independent verification of #36, then requested continued local implementation.

- Cause: the wake counter cleared only when another sleep registered. Thus, a terminal owner blocked the settle after its work ended.
- Port change: `Clock::sleep_until` returns `Hold` instead of `()`. The two product call sites retain or release that ownership at the end of their work.
- Other repairs: per-session state in the fake, owned cleanup of old runs, captured wire shapes, and the default step limit in the execution path. These repairs change test infrastructure, not PRD behavior or the expected results of existing scenarios.
- Scope: issue #36 owns the repair and the replacement freeze record. History keeps the original freeze and the failing evidence. The change affects each later slice that uses the shared harness, for example streaming, cancellation, runtime recovery, and quota deadlines.

The production clock returns an empty `Hold`; the harness clock reports delivery and completion separately.
This removes the dependency on another timer registration and keeps the five-port structure unchanged.

### 2026-10-19: preserve credential delivery across a failed first start

Decision: startup validates the settings and generates each absent credential first. It writes and flushes the new credentials to the terminal before atomic persistence and the other startup work that can fail. A terminal error ends the start without persistence.
Reason: when the port was in use, startup ended after it saved the password hash. A retry then showed no password, because the saved field existed.

This order implements FR-004 and FR-005 with no plaintext password and no change of the configuration format.
It affects #37 and the later credential and startup work in #55 and #64.
The browser remains at the successful first-start point. Terminal delivery proves successful writes and a flush, not that the owner read them.

### 2026-10-19: keep the role of each system part

`SystemPart` keeps its original `System` or `Developer` role, because FR-019 starts a named part with its role label.
The system parts come from the Messages `system` field, the Responses `instructions` field, and the system and developer messages before the first message of another role.
This change affects no external behavior, field, or stored format.

### 2026-10-19: retain tool kind and namespace independently

A namespace can contain function tools and custom tools.
The `InNamespace` variant could not also keep the member kind, so `ToolKind` and the optional namespace are separate fields.
Each returned call keeps its kind, its namespace, and its original member name.
Normalization puts the namespace name and description before the description and grammar of each member.

The MCP adapter gives each tool its MCP name, so that the model sees the name of each tool (FR-022):

- A plain tool keeps its name.
- A namespace member gets `<namespace>__<name>`.
- If that name is too long or is already in use, the adapter cuts it and adds a hash suffix.

With the runtime prefix `mcp_client-tools_`, each member name has at most 64 characters.
The runtime driver keeps the tools of its session, and maps the title of a permission request back to one of them with these MCP names.
This correction adds no port, external dependency, or stored format.

## Open questions

1. **A first start that never publishes its panel address.**
   - Approved answer: FR-006 permits a cancellable wait without a total deadline. M-24 always needs a validated address.
2. **A coordination failure while an owner still holds the account.**
   - Approved answer: FR-006 and M-28 define the failed start and its path diagnostic. The account locks remain intact.

OD-1 still blocks the value of N and its three scenarios. OD-2 still blocks the targets of SC-006.
The first build and its measurements resolve them. This design sets neither value.
Native recovery must prove that earlier runtime trees ended before cleanup. This remains part of FR-045 and SC-002, not a weaker recovery contract.
