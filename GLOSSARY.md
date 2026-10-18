# Glossary of acp2api

This file defines the domain terms and the workflow terms of the project. The setup skill writes the workflow terms between the markers. The skill `domain-modeling` adds the domain terms. For the format, see `.claude/skills/domain-modeling/references/glossary-format.md`.

Columns:

- Term: the word to use in the files of the project.
- Chinese term: the word to use in the conversation with the user.
- Definition: what the term is in this project.
- Avoid: the words not to use for this concept, in the files or in the conversation.

<!-- workflow-setup:start -->
## Workflow terms

The setup skill `/workflow-setup` writes the text between the two markers. Its next run replaces the text, or it shows a change there as a conflict. Do not change this text here. Each entry gives the fixed Chinese term of a workflow term. If a file defines the term, the definition names that file.

### Workflow

| Term | Chinese term | Definition | Avoid |
|---|---|---|---|
| principle | 原则 | A file that defines one engineering rule and its self-check. Each principle file is a reference of the shared `principles` skill. Other skills refer to a principle by its name. The principle file is the only definition, so this glossary does not give a second one. | 准则, 规范, 最佳实践, guideline, best practice |
| workflow | 工作流 | The group of development skills that the setup skill installs into a project. They come from Matt Pocock's skills. The skills can call each other. The group has no fixed order of steps. | 剧本, playbook (a pstack term for a fixed list of steps), 流程 |
| subagent | 子代理 | An agent that the main conversation sends to do one subtask alone. It has its own context. It gives only its result back to the main conversation. | 子智能体, 助手, helper |
| brief | 任务说明 | The file that gives the task of one subagent. `implement` writes one for each issue (`references/brief.md`). | |
| document map | 文档地图 | The table that names one document for each kind of fact in a project that uses the workflow. Its single definition is `.claude/skills/domain-modeling/references/document-map.md`. | home document list, 文档清单 |
| domain model | 领域模型 | In a project that uses the workflow, the vocabulary in `GLOSSARY.md` and the decisions in the ADRs. The skill `domain-modeling` builds it. | |
| domain term | 领域术语 | In a project that uses the workflow, a term in its `GLOSSARY.md` for a concept that is specific to that project. A technical name is not a domain term. | project term |
| workflow term | 工作流术语 | A term that a principle or a workflow skill defines. The setup skill writes its entry into the glossary of each project that uses the workflow. Defined in `.claude/skills/domain-modeling/references/glossary-format.md`. | |
| functional requirement (FR) | 功能需求 / FR | One product obligation in a PRD, with an ID such as `FR-001`. It has an initial condition, an observable result, and a pass rule. | user story, 用户故事 |
| success criterion (SC) | SC | One intended outcome of a feature in a PRD, with an ID such as `SC-001` and a pass rule. | |
| pass rule | 通过规则 | The rule in an FR or an SC that decides pass or fail. It is finite and observable, so two readers reach the same result. | acceptance criterion, 验收标准 |
| Blocked draft | `Blocked draft` | The PRD status that `to-prd` sets when a blocker makes a condition of the status `Draft` false. Defined in the skill `to-prd` ("Stop rules"). | blocked PRD |
| blocker | 阻塞项 | An open decision or a measurement gap that makes a condition of the status `Draft` false. Defined in the skill `to-prd` ("Stop rules"). Between issues, the term is "blocking edge". Stop reason 3 of `no-stops-after-approval` has its own meaning of the word. | blocking issue (for an issue that blocks another issue) |
| open decision | open decision | A choice that nobody made. The PRD records it, and no skill assumes it. Defined in the skill `to-prd` ("Constraints"). | pending decision, TBD |
| FR tag | FR 标签 | A Gherkin tag, for example `@FR-001`, that names the functional requirement of the PRD that a scenario covers. Its single definition is `.claude/skills/to-feature/references/gherkin.md`. | requirement tag, 需求标签 |
| slice tag | 切片标签 | The Gherkin tag `@issue-<number>` that names the GitHub issue that holds a scenario. `to-tickets` adds it. Its single definition is `.claude/skills/to-feature/references/gherkin.md`. | issue tag, ticket tag |
| run-control tag | 运行控制标签 | The Gherkin tag `@wip`. It keeps a scenario out of the default test run until the scenario is in an issue and that issue is done. `to-tickets` adds it, and `bdd` removes it. Its single definition is `.claude/skills/to-feature/references/gherkin.md`. `to-feature` adds it again in one case, which the section "Revise scenarios" of that file gives. | status tag, run control (alone) |
| freeze commit | 冻结提交 | The commit that records the freeze of the acceptance contract of one issue. `bdd` makes it, and `implement` and `test-audit` can make a new one after an approved change. Its single definition is `.claude/skills/bdd/references/freeze.md`. | handoff, handoff ID (the global skill `handoff` has that name) |
| contract check | 契约检查 | The command that compares the files of a freeze commit with the working tree. Its single definition is `.claude/skills/bdd/references/freeze.md`. | |
| written design | 设计文档 | The file `docs/design/<name>.md` that the skill `architect` writes after the user approves a design. It holds the one-way decisions, the interface, the seams, and the migration shape. `to-tickets` and `implement` read it. | rationale, design package, refactoring note, 设计稿 |
| blocking edge | 阻塞关系 | The link that makes one issue wait for another issue. The first issue must finish before the second issue starts. | blocker (for an issue) |
| refactoring issue | 重构 issue | An issue without scenarios and without a slice tag. It keeps the behavior. Defined in the skill `to-tickets` (`references/slicing.md`). | prefactor issue, cleanup issue |
| removal issue | 删除 issue | The refactoring issue that removes the old interface of a Parallel Change. Defined in the skill `to-tickets` (`references/slicing.md`). | |
| trusted red | 可信红 | A red run of a scenario that fails at its assertion for the reason that the scenario states. Defined in the skill `bdd`. | |
| existing green | 已有绿 | A scenario that passes before any behavior is added to the product, with proof that its test can fail. Defined in the skill `bdd`. | |
| integration branch | 集成分支 | The one branch that holds the work of all issues of one parent issue until the final pull request. Defined in the skill `implement`. | |
| issue branch | issue 分支 | The branch of one issue, from the tip of the integration branch. `implement` merges it after the checks of the issue pass. Defined in the skill `implement`. | |
| safety fact | 安全事实 | A fact that makes a change safe, for example "this call only drops cache entries that are already dead". A change has one or two. `blast-radius` proves each with a run. Defined in the skill `blast-radius`. | |
| reshape flag | 重塑标记 | A finding of `no-comments`. It names a symbol in our own code whose surprise a deleted comment excused, or that a kept suppression hides. It also names the change that the code needs. `code-review` checks each open flag as a lead. Defined in the skill `no-comments` (`references/brief.md`). | MUST KILL |
| merge decision | 合并决定 | The result of `code-review`: `BLOCK`, `NEEDS DECISION`, or `MERGE`. Defined in the skill `code-review` (step 5). `pr` reads it. | verdict (for the result of `code-review`) |
| Merge Danger | Merge Danger | The section of the body of a pull request with the door of the change, its blast radius in one word, and the merge decision. Defined in the skill `pr` (`references/body.md`). | risk section |
| audit table | 审计表 | The table in which `test-audit` gives each test in scope one mark (`keep`, `repair`, `merge`, or `delete`) and one evidence line. Defined in the skill `test-audit` (`references/audit.md`). | ledger (a word to avoid for the records file), 台账 |
| map | 地图 | The issue with the label `wayfinder:map` that indexes one effort that is too large for one session. Its sub-issues are its decision issues. It gives each decision in one line with a link, not its reasons. The document map and the example map are different concepts. Defined in the skill `wayfinder` (`references/map.md`). | plan document, roadmap, epic |
| destination | 终点 | The decision or the document of the workflow that ends the effort of a map, for example a PRD, a written design, or an ADR. The user fixes it first, and it fixes the scope of the map. Defined in the skill `wayfinder`. | goal (alone), end state |
| decision issue | 决策 issue | A sub-issue of a map that holds one question. Its answer is a decision, or a fact that a decision needs. Its label gives its type: grilling, prototype, research, or task. Defined in the skill `wayfinder` (`references/map.md` and `references/types.md`). | ticket, decision ticket |

### Principles and checks

| Term | Chinese term | Definition | Avoid |
|---|---|---|---|
| self-check | 自检判据 | A specific check in a principle. It shows if a person or an agent really applied the principle. Example: "If every imported function returns undefined, does the test still pass?" | 标准, 指标, criterion, metric |
| single definition | 单一定义处 | A concept or a rule has its definition in only one file. All other locations refer to that file. | 唯一真相源, single source of truth |
| acceptance contract | 验收契约 | Defined in the principle `no-stops-after-approval`. | |
| freeze | 冻结 | Defined in the principle `no-stops-after-approval`. | |
| sign | 迹象 | Something that an agent can see in code, in a diff, or in a run, and that shows that a principle was probably skipped. Code smells and design red flags are signs. A sign is a lead, not a finding (see `.claude/skills/principles/SKILL.md`). | symptom (`fix-root-causes` uses this word for the visible failure of a bug) |
| system boundary | 系统边界 | Defined in the principle `boundary-discipline`. | "boundary" alone, trust boundary, 边界 |
| external user | 外部用户 | Defined in the principle `migrate-callers-then-delete-legacy-apis`. | |
| depth | 深度 | Defined in the principle `deep-modules`. | |
| locality | 局部性 | Defined in the principle `deep-modules`. | |
| leverage | 杠杆 | Defined in the principle `deep-modules`. | |
| design step | 设计步骤 | Defined in the principle `redesign-from-first-principles`. | design phase |
| build step | 实现步骤 | Defined in the principle `redesign-from-first-principles`. | build phase, implementation phase |
| bolt-on | 外挂式改动 | Defined in the principle `redesign-from-first-principles`. | |
| unit | 单元 | Defined in the principle `sequence-verifiable-units`. | |
| lever | 杠杆脚本 | Defined in the principle `build-the-lever`. | helper script, 辅助脚本 |
| fences | 围栏 | Defined in the principle `build-the-lever`. | |
| reproduction check | 复现检查 | Defined in the principle `fix-root-causes`. | repro |
| census | 普查 | Defined in the principle `fix-root-causes`. | |
| proof level | 证明等级 | Defined in the principle `prove-it-works`. | evidence level (the research skill uses it for a different scale), confidence |
| limiter | 瓶颈 | Defined in the principle `prove-it-works`. | bottleneck |
| stop reason | 停止理由 | Defined in the principle `no-stops-after-approval`. | pause reason, 暂停理由 |
| draft follow-up issue | 待建后续 issue | Defined in the principle `no-stops-after-approval`. | |
| agent decision | 代理决定 | Defined in the principle `no-stops-after-approval`. | open decision (a choice that nobody made) |
| project rule | 项目规则 | Defined in the index of the skill `principles` (`.claude/skills/principles/SKILL.md`). | |
| report | 报告 | Defined in the index of the skill `principles` (`.claude/skills/principles/SKILL.md`). | |
| the code that the change touches | 改动涉及的代码 | Defined in the principle `subtract-before-you-add`. | the code that you change, the code that the issue changes |
| isolated copy | 隔离副本 | Defined in the principle `guard-one-way-doors`. | |
| behavior | 行为 | Defined in the principle `test-behavior-not-implementation`. | |
<!-- workflow-setup:end -->

## Domain terms

| Term | Chinese term | Definition | Avoid |
|---|---|---|---|
| API client | API 客户端 | A program that sends requests to acp2api through the OpenAI Chat Completions API, the OpenAI Responses API, or the Anthropic Messages API. Examples are Claude Code, Codex, and SillyTavern. | harness (not each API client is a harness), client (alone, because in ACP acp2api is the Client) |
| conversation | 对话 | The system prompt and the messages that an API client sends in one request, with the tool calls and the tool results in them. Each request carries the full conversation. | chat, thread, message list, session (an ACP session is a different thing) |
| official runtime | 官方运行时 | The executable that Google publishes for Antigravity and that speaks ACP (Agent Client Protocol): the Antigravity ACP server, `agy_acp_server`. acp2api is its ACP Client. | runtime (alone), Antigravity (alone), agent, backend, `agy` (the Antigravity CLI, which has no ACP mode) |
| client tool | 客户端工具 | A tool that an API client declares in a request and executes itself. | harness tool, external tool |
| built-in tool | 内置工具 | A tool that the official runtime has and executes itself, for example a file edit, a terminal command, or a web search. | Antigravity tool, native tool, agent tool |
| server tool | 服务端工具 | A tool that the provider of an API executes on its own servers, when a request to that API declares it. Examples are `web_search_20250305` of the Anthropic Messages API and the hosted `web_search` of the OpenAI Responses API. | hosted tool, server-side tool, built-in tool (a tool of the official runtime) |
| cancel at permission | 权限处取消 | The method in which acp2api cancels the prompt turn at the permission request of a client tool, and returns the tool call to the API client. The tool result then goes to the model as text in a later prompt. | held call (a different method), cancel and text result |
| tool filter | 工具过滤 | The undocumented option `_meta.antigravity.enabledTools` of an ACP session of the official runtime. It limits the built-in tools that the model of the session can use. | tool lock, tool allowlist |
| rebuild | 重建 | The start of a new ACP session whose first prompt holds the earlier part of a conversation as text. A new ACP session for a conversation with no earlier part is not a rebuild. | replay (`session/load` replays the history of an ACP session), reset, fork |
| local API key | 本地 API 密钥 | The secret that an API client sends to acp2api with each request, so that acp2api accepts the request. | API key (alone, because the official runtime also signs in with a Google API key), token |
| management panel | 管理面板 | The web page of acp2api on 127.0.0.1 where the user sees the state of acp2api and changes its settings. | dashboard, admin page, web UI, console |
| management password | 管理密码 | The secret that the user enters to open the management panel and to change the state or the settings of acp2api. | admin password, panel key, secret key, local API key (a different secret) |
| lockout | 锁死 | The state in which acp2api sends no request to the official runtime, after a response that shows a risk to the Google account. Only the user ends it. | ban (an action of Google on the account), pause, circuit breaker (it closes again without the user) |
| lockout signal | 锁死信号 | A response or an event of the official runtime that shows a risk to the Google account. It starts a lockout. | ban signal (a ban is an action of Google), trigger |
| quota wait | 配额等待 | The state in which acp2api starts no prompt turn on the official runtime until the reset time of an exhausted Google quota. It ends without the user. | cooldown, rate limit (a limit of Google, not a state of acp2api), pause, lockout (only the user ends a lockout) |
| sign-in required | 需要登录 | The state in which the official runtime has no valid sign-in, so acp2api starts no prompt turn on it. It ends when a sign-in succeeds. | signed out, logged out, unauthenticated, lockout (a sign-in error is not a lockout signal) |

## Technical names

The project approves these names as exceptions to ASD-STE100. This section gives no definitions, because each name already gives its standard definition.

| Name | Chinese term | Source | Avoid |
|---|---|---|---|
| ACP session | ACP 会话 | ACP specification, `docs/protocol/v1/session-setup.mdx` in `agentclientprotocol/agent-client-protocol` | session (alone), Cascade (the word of agy-bridge for a conversation of its engine), conversation (the messages of one request) |
| DNS rebinding | DNS 重绑定 | C. Jackson, A. Barth, A. Bortz, W. Shao, and D. Boneh, "Protecting Browsers from DNS Rebinding Attacks", ACM CCS 2007 | DNS rebind attack |
| permission request | 权限请求 | ACP specification, `docs/protocol/v1/tool-calls.mdx`, section "Requesting Permission" (`session/request_permission`), in `agentclientprotocol/agent-client-protocol` | permission prompt, approval request |
| prompt turn | prompt 轮次 | ACP specification, `docs/protocol/v1/prompt-turn.mdx` in `agentclientprotocol/agent-client-protocol` | turn (alone) |
| slash command | 斜杠命令 | ACP specification, `docs/protocol/v1/slash-commands.mdx` in `agentclientprotocol/agent-client-protocol` | command (alone) |
