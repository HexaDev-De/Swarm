# Provider integrations

PandaCode launches each installed CLI directly. Claude, Codex, Kimi, OpenCode,
Antigravity, and Grok Build own their authentication; sign in using their official
CLI or extension.
The optional Z.AI Coding Plan and MiMo Token Plan API keys are stored in the macOS
Keychain (separate entries) and injected only into their isolated Claude subprocesses. The app discovers executable paths
from PATH and common per-user installation folders; provider settings report the
actual path and version. The `default` model delegates model selection to the CLI.

Model menus load metadata on demand from the installed CLI. Each choice keeps its
invocation ID separate from its display name and description, so choosing a named
model does not rewrite an alias or change the provider's configuration. Codex uses
`model/list` and preserves its `displayName` and description. Claude uses the SDK's
initialization `models` response (`value`, `displayName`, `description`, and, when
available, `resolvedModel`). Its resolved model ID supplies the release version
even when the CLI's display name is only “Sonnet”; unresolved aliases stay generic
instead of claiming a guessed version. See [SDK model metadata](https://code.claude.com/docs/en/agent-sdk/typescript#modelinfo).

Claude discovery sends only an initialization control request, with conversation
persistence, hooks, and MCP startup disabled. It sends no user prompt and performs
no inference. Codex discovery creates no thread. Both processes have bounded output
(1 MiB total, 256 KiB per frame), deadlines (15 seconds for Claude, 8 for Codex), and
process-group cleanup on completion, cancellation, or failure. Only picker metadata
crosses the desktop bridge; account information in initialization is discarded.
Unavailable catalogs leave the configured/default choice and custom model IDs usable.
The frontend caches successful catalogs for five minutes without a background poll.

Kimi reads `provider list --json` without creating an ACP session. A typed whitelist
extracts only the configured aliases, model IDs, and display names; provider
configuration and credentials are discarded rather than sent to the UI. Its process
is limited to 8 seconds and 1 MiB of output. Provider-supplied `displayName` values
take precedence; entries without display metadata show the exact wire model name.
See [Kimi model configuration](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/config-files#models).

## Claude login and environment credentials

Open **Settings → Agents → Set up → Claude Code** to see the credential source
detected in PandaCode's environment. **Check login** separately checks Claude's
saved login using `claude auth status --json`; it sends no prompt. The card shows
credential variable names and the origin of a custom endpoint, never keys,
tokens, account details, URL paths, or URL query values.

**Use current environment** is the default and preserves existing setups. To
prefer the account saved by `claude auth login`, choose **Prefer saved Claude
login**. New Claude turns, model/command discovery, and harness consoles then
remove inherited `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`,
`CLAUDE_CODE_OAUTH_TOKEN`, custom API endpoints/headers, cloud-provider switches,
Anthropic profile/federation selectors, and model-routing overrides. The
preference does not change the system environment, saved Claude credentials,
active processes, GLM, or MiMo. Switch back to **Use current environment** to
restore inherited configuration for newly launched Claude processes.

Claude settings files, `apiKeyHelper`, active profiles, and organization policies
can still affect the credential used. Check `/status` in a newly opened Claude
console to confirm the account and login method. A detected saved login does not
verify its plan, quota, expiry, or access to a particular model. See Claude's
[authentication precedence](https://code.claude.com/docs/en/authentication#authentication-precedence)
and [environment variable reference](https://code.claude.com/docs/en/env-vars).

## Conversation continuity

An idle task can switch harnesses from the model picker without changing its
project, checkout, or saved transcript. PandaCode starts a fresh native session for
the chosen harness and transfers bounded text history with the next normal
message, using the recovery mechanism below. A **Conversation carried forward**
disclosure explains the transfer. Earlier replies retain their original provider
labels, including in paginated history, conversation forks, and the quick overlay.
Private reasoning, tool execution state, and earlier image bytes do not transfer.
Running tasks and active architect/worker configurations retain their existing
configuration guards. Provider-specific goal/settings state and a previous
harness's native resume ID are not reused by a different harness.

The saved provider session ID is reused across ordinary turns and app restarts.
Changing a task's team settings retains Claude/Kimi's native context. Codex only
supports registering dynamic tools when a thread is created; enabling or disabling
its team therefore queues a conversation transfer before replacing its native ID.
Saving the same setup or selecting single-agent mode again does not reset anything.

If a task has history but lacks a native session ID, PandaCode also queues that
history automatically. Schema 14 repairs tasks affected by the earlier architect
reset notice, including tasks that already received a replacement native ID.
The next normal prompt includes the earlier user/assistant messages as historical
JSON, followed by the current request. The visible transcript is unchanged. Tool,
system, and diff records are excluded; history is limited to the newest 200
messages and 64 KiB encoded, and truncation is disclosed in the handoff.

Recovery is acknowledged only after a successful, persisted turn with a saved
native session ID. Startup failures, cancellation, and app restarts keep the
transfer pending. Once delivered, later prompts use native resumption without
resending the transcript. Native slash commands, goal start/resume, harness
console launch, and forking require a normal message first when recovery is
pending because those paths cannot deliver a historical prompt. Goal inspection,
pause, and clear remain available without acknowledging the transfer.

## Reasoning effort and context

Effort choices come from the selected model's reported capabilities. Claude's
initialization `supportedEffortLevels` supplies the choices, and explicit selections
are passed with `--effort` on new and resumed runs. Codex supplies
`supportedReasoningEfforts` and `defaultReasoningEffort`; the selected ID is applied
in thread configuration and `turn/start.effort`. Catalog validation runs before
sending the prompt. The installed app-server's generated v2 JSON schemas were used
to verify these fields; see [app-server reference](https://developers.openai.com/codex/app-server).

The installed Kimi ACP adapter exposes a `thinking` configuration option with
on/off values. Only those advertised settings are offered; a model declaring
`always_thinking` cannot select Off. Although Kimi configuration can name numeric
effort levels, this adapter cannot apply them. The app waits for configuration
acknowledgement before prompting. See [Kimi ACP](https://moonshotai.github.io/kimi-code/en/reference/kimi-acp)
and [model configuration](https://moonshotai.github.io/kimi-code/en/configuration/config-files).

**Default** sends no PandaCode override. The harness retains control, including any
saved effort in a resumed session. It does not promise to reset external harness
settings. Claude can resolve capabilities for its default alias; an unresolved
Codex/Kimi default requires choosing a catalog model to expose effort choices.

Context capacity is shown only when explicitly reported. Codex usage uses
`tokenUsage.last.totalTokens` with `modelContextWindow`, not cumulative totals.
Claude counts the latest main-agent message's input, cache reads, cache creation
and output; its result may supply that model's context-window size. Subagent and
cumulative per-model billing totals are excluded. Kimi configured `maxContextSize`
supplies capacity; ACP `usage_update` can supply current usage when emitted by the
installed version. Unknown usage is hidden. Counts are last reported snapshots and
may decrease after compaction; the UI does not estimate them from conversation text.

Each active turn owns one process. Completed turns close stdin and release that
process; the next turn resumes the saved provider session. Stop sends the provider's
interruption message, then terminates its process group after a short grace period.
Closing the app also drops the input channel and stops the group. Runtime output
uses bounded frames (4 MiB maximum) and a 16 KiB stderr tail. No Node/Python SDK host
or network bridge runs inside PandaCode.

## Response streaming

Complete newline-delimited JSON events are forwarded as soon as stdout supplies
them; the adapter does not wait for the turn to finish. Claude launches with
`--include-partial-messages`; Codex app-server and Kimi ACP stream by default.
`message_start` precedes the first nonempty `text` delta of each main assistant
message, with bounded `data.messageId` and optional `data.phase` (Codex commentary
or final answer). Native persistence flushes the preceding assistant message at
this boundary without storing a marker row.
If message IDs interleave, a resumed ID starts a new contiguous display segment;
text never appends silently to another message's live buffer.

The latest assistant message stays visible outside the activity disclosure while
tools run. A tool call or `message_start` records its place in history without
clearing that visible message. The next nonempty text delta replaces it in the
same live message element, then subsequent deltas append normally. Only then does
the previous message appear in activity history (or as an earlier Board comment).
Repeated boundaries and empty deltas do not create duplicates or blank the slot;
completion materializes the retained response once before loading saved history.

Claude and Codex completion snapshots supply only a missing suffix whose prefix
matches the emitted stream. Replayed snapshots and empty deltas do not duplicate
text or open empty messages. Tracking retains at most 128 message IDs, byte counts,
and incremental digests; it does not keep another copy of response bodies. A
completion that changes an already-emitted prefix is not appended as unrelated
replacement text. Claude excludes child-agent transcript frames while retaining
top-level Agent/Task tools, their results, and child permission requests. Codex
filters notifications by active thread and turn so child events cannot contaminate
or prematurely finish the parent response.

Kimi honors ACP `messageId` when supplied. The installed Kimi 0.26 adapter omits it
and already filters child-agent events, so PandaCode derives boundaries when text
resumes after a new tool call or thinking segment. Late updates to an existing tool
do not split subsequent text; history replay during session loading is excluded.
See [Claude streaming](https://code.claude.com/docs/en/agent-sdk/streaming-output),
[Codex app-server events](https://developers.openai.com/codex/app-server), and
[ACP message IDs and prompt turns](https://agentclientprotocol.com/protocol/v1/prompt-turn#message-ids).

Tool events send a bounded snapshot containing the tool's name, status, input and
output across the desktop bridge. Input previews are limited to 4 KiB and output
previews to 8 KiB, with UTF-8-safe truncation notices. Each adapter keeps at most 64
tool snapshots to join starts, sparse updates and results. The UI shows separate
Input and Output sections and preserves one tool row through completion and reload.
Opaque event metadata still uses a small scalar allowlist; pending approval requests
remain native until answered so the provider receives its complete input correctly.

Each active task owns its protocol state, input channel, process group and stderr
reader. Stopping or aborting a run cleans up that group and its reader without
interrupting other tasks. Newline detection scans each stdout byte once, avoiding
quadratic rescans when several providers emit large tool-result frames together.
Admission has no global simultaneous-task limit. It rejects duplicate starts for
the same task before adding another user message or spawning another process.

Claude joins `tool_use` and `tool_result` by call ID and assembles bounded partial
tool input before emitting it. Codex extracts command, working directory, exit code,
output, file changes and MCP results from item notifications. Kimi merges ACP tool
updates, retaining input previews until terminal result content arrives. Already
saved label-only entries cannot recover details that earlier versions discarded.

| Provider | Entry point | Persistence | Interruption |
| --- | --- | --- | --- |
| Codex | `codex app-server` | `thread/start`, then `thread/resume` | `turn/interrupt` |
| Claude Code | `claude -p --input-format stream-json --output-format stream-json --include-partial-messages --verbose --permission-prompt-tool stdio` | CLI session ID, `--resume=<id>` | control request subtype `interrupt` |
| Kimi Code | `kimi acp` | `session/new`, then advertised `session/resume` or `session/load` | `session/cancel` |

Codex uses newline-delimited app-server requests and item notifications. The native
adapter covers thread creation/resumption, text streaming, tool events, and file,
command, and permission approval requests. Ask and Auto both use `workspace-write`
with `on-request` approvals routed to the user: operations allowed by Codex's sandbox
can run, while escalation requests appear in PandaCode. Neither mode disables its
sandbox. Codex's own rules still apply. The protocol was checked against the installed
CLI's generated schemas, version 0.150.1. See [official app-server documentation](https://developers.openai.com/codex/app-server).

Claude uses the installed executable's bidirectional control protocol. Ask retains
print mode's normal manual permissions; Auto selects `acceptEdits`, which approves
workspace edits while other protected actions still request permission. The adapter
waits for the initialization response, streams text deltas, and avoids repeating the
final full assistant message. The raw control protocol is an implementation interface
used by Anthropic's SDK and should be tested when upgrading Claude. See
[headless usage](https://code.claude.com/docs/en/headless),
[stream events](https://code.claude.com/docs/en/agent-sdk/streaming-output), and
[the official SDK's control transport](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/query.py).

Kimi uses ACP version 1 with no client filesystem/terminal capabilities advertised;
Kimi performs those operations through its own tools. The adapter explicitly selects
ACP's `default` mode, which maps to manual permissions, before each prompt; it fails
if that mode is unavailable. Auto currently keeps Kimi's manual
approval behavior: its `--auto` flag enables unattended approval of far more than
file edits, so PandaCode does not map to it. Permission responses select an option ID
actually offered by the server. Kimi 0.26.0 is the newer Kimi Code executable, distinct
from the legacy Python Kimi CLI and its Wire protocol. See
[Kimi ACP](https://moonshotai.github.io/kimi-code/en/reference/kimi-acp),
[permission modes](https://moonshotai.github.io/kimi-code/en/guides/interaction.html),
[ACP mode mapping](https://github.com/MoonshotAI/kimi-code/blob/main/packages/acp-server/src/modes.ts),
[ACP initialization](https://agentclientprotocol.com/protocol/v1/initialization),
[prompt turns](https://agentclientprotocol.com/protocol/v1/prompt-turn), and
[permission requests](https://agentclientprotocol.com/protocol/v1/tool-calls).

The first release supports boolean tool approvals. Structured provider questions
and custom elicitation forms are not implemented. Kimi's multiple-choice questions
are cancelled rather than selecting an answer through a generic Approve button.
Other unsupported reverse requests receive explicit errors; Claude is asked to put
questions in its text response so the user can answer in the next turn. Existing CLI hooks, MCP configuration, and
provider rules remain provider-owned and can affect startup time and memory.

Run `cargo test` for parser and permission tests. Live integration tests are ignored
by default because they consume provider usage. To explicitly exercise a provider:

```sh
PANDACODE_SMOKE_PROVIDER=codex cargo test smoke_provider_from_env -- --ignored --nocapture
```

Use `claude` or `kimi` for the other providers. The test creates a temporary Git
repository, asks for one fixed token, resumes the conversation, and checks that the
provider remembers that token. It rejects all tool approval requests and validates
that the same provider session is reused. Successful output prints only the fixed
token, event count, and presence of a session ID.

Set `PANDACODE_SMOKE_MODEL` to test a specific model without modifying the user's
provider settings. Live checks on September 12, 2026 verified successful Claude
`sonnet` and Codex `gpt-5.6-sol` generation and resumption across both turns. The
configured Codex default required a newer CLI. Claude's configured default reported
unavailable usage credits. Explicit model overrides worked without changing those
settings. Kimi required authentication; that response verifies startup and error
handling but does not establish successful Kimi generation or resumption.

A read-only `model/list` query against Codex 0.150.1 returned `gpt-5.6-sol`
(catalog default), `gpt-5.6-terra`, `gpt-5.6-luna`, `gpt-5.5`, and
`gpt-5.3-codex-spark`. This catalog differs from the user's configured default;
PandaCode leaves that configuration intact and accepts explicit model IDs.

Read-only catalog checks do not consume model inference usage:

```sh
cargo test installed_claude_catalog -- --ignored --nocapture
cargo test installed_codex_catalog -- --ignored --nocapture
cargo test installed_kimi_catalog -- --ignored --nocapture
```

## Subagent activity

Delegation snapshots use `kind: "subagent"`, empty `text`, and `data` containing
`id`, `parentId`, `name`, `prompt`, `status`, `response`, `tools`, `startedAt`, and
`updatedAt`. Times are Unix milliseconds. Status is `running`, `waiting`,
`completed`, `failed`, or `stopped`. Native code namespaces provider identities by
parent run and persists child history separately from the parent's messages.

- **Codex:** the installed executable's generated app-server schema includes
  `collabAgentToolCall`, `subAgentActivity`, and child `thread/started` source
  linkage. PandaCode records those identities, routes their text/tool notifications
  into isolated parsers, and uses `agentsStates.message` to recover results when
  child notifications are absent. Completing a spawn call does not complete its
  child. Child failures/completions never terminate the parent's turn. Source:
  `codex app-server generate-json-schema --experimental --out <temporary-directory>`
  (`ItemStartedNotification`, `ThreadStartedNotification`, and
  `ThreadStatusChangedNotification`), plus the
  [official app-server documentation](https://developers.openai.com/codex/app-server).
- **Claude:** `Agent` (and legacy `Task`) tool calls establish children;
  `parent_tool_use_id` routes their streamed text and tool results. Background
  `task_started`, `task_progress`, `task_notification`, and terminal
  `task_updated.patch.status` events update the same child. Background Bash tasks
  are excluded. Final result snapshots reconcile with previously streamed text.
  Sources: [subagent protocol guidance](https://code.claude.com/docs/en/agent-sdk/subagents)
  and [official SDK lifecycle types](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/types.py).
- **Kimi:** identifiable `Agent` calls expose their prompt, progress, and returned
  result. The current Kimi Code ACP implementation subscribes only to the main
  agent, so live child-internal messages/tools and independent background-child
  completion are unavailable through this transport. PandaCode does not invent
  those details, parse terminal text for fake identities, or start another server.
  Calls remain correlated after ACP replaces the tool title with its description.
  Sources: [ACP session subscriptions](https://github.com/MoonshotAI/kimi-code/blob/main/packages/acp-server/src/session.ts),
  [tool notification mapping](https://github.com/MoonshotAI/kimi-code/blob/main/packages/acp-server/src/events-map.ts),
  and [Agent tool parameters](https://github.com/MoonshotAI/kimi-code/blob/main/docs/en/reference/tools.md).

Each run retains at most 64 child previews at a time. When that cache fills,
its oldest terminal child retires to make room for later delegates; active children
are never evicted (up to 64 simultaneous children). Each retained child keeps a 32 KiB response preview and its latest 16 tools (2 KiB input / 4 KiB output
per tool); prompt previews are 4 KiB. Consumed child frames move into the parser
instead of copying potentially multi-megabyte envelopes. Rendering receives only
bounded snapshots, and storage checkpoints ongoing updates at most once per
second. Parent exit/stop and app recovery settle children still marked active.

Deterministic protocol tests cover interleaved child identities, nested linkage,
permission waiting/resumption, final-suffix recovery and replay deduplication,
background lifecycle, unrelated/stale turn exclusion, and output/count bounds.
No provider inference requests are required by these tests.

## OpenCode, GLM Coding Plan, and Antigravity

OpenCode launches `opencode acp` using ACP v1. Models come from its installed
`models --verbose` catalog with real display names, provider/model IDs, context
limits, and variant IDs. Catalog output is bounded and whitelisted so provider
headers and arbitrary options never reach the UI. Live `configOptions` are
authoritative: PandaCode selects mode, then model, then advertised reasoning effort,
waiting for each acknowledgement before sending the prompt. Session loading
suppresses replayed history, and resumption uses the advertised `resume` or
`loadSession` capability. Unsupported capabilities fail before prompting.

OpenCode's process-local `OPENCODE_CONFIG_CONTENT` sets Standard to approval for
actions with read/search allowed; Auto edits also allows edits; Plan denies
non-read actions; Full access allows actions. The selected built-in `build` or
`plan` agent gets the same policy override, including on resumed sessions. No
OpenCode configuration files are rewritten.

GLM uses Claude Code's existing bidirectional streaming protocol with Z.AI's
`https://api.z.ai/api/anthropic` endpoint. Its default is `glm-4.7`; the other
choices are documented Coding Plan model IDs, not fictional Claude aliases. Model
availability depends on the user's Z.AI plan; custom IDs remain available. Claude's
normal model catalog cannot advertise Z.AI plan entitlements, so this picker uses
the published Coding Plan list and does not claim measured context/effort metadata.
Explicit `--model` choices override the default environment value.

**GLM settings isolation:** Claude settings-file environment values override
inherited shell environment. Therefore GLM uses `--setting-sources ""` and
non-secret command-line settings to prevent user/project Claude credentials or
endpoints replacing the Coding Plan connection. It does not inherit those settings
files' hooks/plugins/permissions; PandaCode's explicitly supplied MCP tools remain
available. Claude-managed enterprise policies still apply. The key is never
written into argv or a settings file; it is resolved from the native Keychain or
`ZAI_API_KEY`. Competing Anthropic/cloud auth variables are removed, and Claude's
subprocess credential scrubbing is enabled.

Antigravity launches Google's official `agy_acp_server.par` (`agy_acp_server.exe`
on Windows), which can be selected in Settings. The standalone `agy` terminal
CLI is not misidentified as an ACP server. The executable and provider sign-in
must already be installed. ACP initialization/session metadata supplies its model
labels and supported controls; image, HTTP MCP, resume, and planning support are
checked explicitly. No private API, unofficial adapter, or browser-cookie
authentication is used. Antigravity's own policy applies, with ACP approval
requests shown in Standard; Auto edits approves explicit edit requests and Full
access approves offered one-time permissions. Structured questions are not
auto-selected. Modes not advertised by the server are not invented.

**Connection check.** In Settings, expand Antigravity's installation details and
choose **Check connection**. This separately verifies ACP initialization and an
empty session, reports a required sign-in or failed startup, and allows retry. It
does not send a prompt, sign in automatically, or verify model quota. A detected
executable alone is never presented as a verified connection.

ACP model discovery allows up to 60 seconds for cold startup. Successful metadata
is cached for five minutes by executable identity and shared with slash-command
discovery. Default-only results do not overwrite a known catalog; explicit
refresh invalidates both native and UI caches, including older pending results.
During an Antigravity turn, startup, session creation/resumption, setting changes,
and waiting for the response have separate status messages.

Both ACP adapters stream text/tool/usage updates, publish available slash commands,
route explicit approval responses using offered option IDs, and preserve task
context across restart. They advertise no client filesystem or terminal capabilities:
those actions run through each harness's own tools. Provider metadata discovery
sends no inference request; ACP-only metadata discovery can create an empty
harness session.

Verified interfaces (2026-09-16): [OpenCode ACP](https://opencode.ai/v2/docs/cli/acp/),
[OpenCode models CLI](https://dev.opencode.ai/docs/cli/#models),
[OpenCode permissions](https://opencode.ai/docs/permissions/),
[ACP session configuration](https://agentclientprotocol.com/protocol/v1/session-config-options),
[Z.AI Claude setup](https://docs.z.ai/devpack/tool/claude),
[Claude environment precedence](https://code.claude.com/docs/en/env-vars#precedence),
[official Google ACP registry entry](https://raw.githubusercontent.com/agentclientprotocol/registry/refs/heads/main/antigravity-acp/agent.json),
and [Antigravity authentication](https://antigravity.google/docs/ide/extensions).

## MiMo Token Plan

Xiaomi's MiMo models reach PandaCode the same way GLM does: through the installed
Claude Code, pointed at MiMo's Anthropic-compatible endpoint with a MiMo key
(documented at https://mimo.mi.com/docs/en-US/tokenplan/integration/claudecode).
The Token Plan is a fixed subscription with a dedicated cluster per account —
`https://token-plan-{cn|sgp|ams}.xiaomimimo.com/anthropic`, keys `tp-…` (individual) or
`ttp-…` (team); the pay-as-you-go API is `https://api.xiaomimimo.com/anthropic` with
`sk-…` keys. Settings → Agents → Provider connections has one MiMo card: a cluster
picker (the four endpoints above, nothing free-form — the URL is shown for the chosen
one) and the key. The key goes into the platform secure store under its own account
(`mimo-token-plan`) or comes from `MIMO_API_KEY`; the cluster choice is a plain
setting (`mimoEndpoint`). Unknown cluster IDs are dropped on load and refused on save.

A MiMo task cannot start until a cluster is chosen and a key exists; both errors name
the setting to fix. The launch mirrors GLM's isolation: `--setting-sources ""` plus a
process-local `--settings` block that fixes `ANTHROPIC_BASE_URL`, blanks
`ANTHROPIC_API_KEY`/OAuth/cloud variables, sets the Sonnet/Opus slots to
`mimo-v2.6-pro` and the Haiku slot to `mimo-v2.6-flash` (background work at a third of
the credit cost), and enables `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` as MiMo's
guide recommends. The key travels only in `ANTHROPIC_AUTH_TOKEN`, never in argv, and
`ZAI_API_KEY`/`MIMO_API_KEY` are scrubbed from the child. Slash-command discovery and
the harness console use the same flags. Everything else — streaming, approvals,
`--permission-mode`, effort, resume, MCP tools, image blocks — is Claude Code's normal
stream-json protocol.

The picker lists the documented plan models (`mimo-v2.6-pro`, its `[1m]` 1M-context
variant, `mimo-v2.6-flash`, and the V2.5 pair that retires on October 21, 2026) with
descriptions only; MiMo publishes no per-plan entitlements, so no context or effort
metadata is claimed and custom IDs stay usable. Endpoints and models verified against
the MiMo docs on 2026-09-22.

## Grok Build

xAI's Grok Build CLI (`grok`, installed with `curl -fsSL https://x.ai/cli/install.sh | bash`)
exposes ACP v1 on `grok agent stdio`, which PandaCode drives with the shared ACP
adapter. The launch line is `grok --no-auto-update --permission-mode <mode> agent
[--always-approve] [--model ID] [--reasoning-effort E] stdio`: global flags precede
`agent`, agent flags precede the `stdio` transport, and the permission mode
overrides whatever the user's `~/.grok/config.toml` saved for the TUI (a saved
`always-approve` would otherwise silence every prompt).

**Sign-in.** Grok Build requires `authenticate` before any session call, even with
cached credentials. PandaCode picks the advertised `defaultAuthMethodId` (normally
`cached_token`, backed by `~/.grok/auth.json`) and only then sends `session/new`
or `session/resume`. Without a cached method, or when it fails, the turn stops with
“Grok Build is not signed in. Run `grok login` in a terminal, then send again.” No
browser sign-in is ever triggered from the app.

**Models and effort.** `initialize` returns `_meta.modelState` with every model, its
`totalContextTokens`, and its reasoning efforts (`low`…`xhigh`, with the default
marked), so discovery never creates a session. `session/new` also returns
`configOptions` (`model`, `reasoning_effort` in the `thought_level` category), and the
adapter applies the chosen model and effort through `session/set_config_option`
before prompting; the same values ride the launch flags so a resumed session that
does not re-advertise options keeps them. Custom models from `config.toml` appear
with their configured names.

**Approvals.** Standard → `--permission-mode default` (Grok asks through
`session/request_permission`; PandaCode shows the request). Accept edits →
`acceptEdits` plus automatic approval of `edit` requests. Full access →
`bypassPermissions` with `--always-approve`. Grok's ACP agent has no read-only
session mode, so **Plan mode** launches in `default` mode, prefixes the prompt with a
read-only instruction, allows requests the agent marks `read_only` (or of kind
`read`/`search`/`fetch`/`think`), and declines everything else with the offered
`reject_once` option, noting “Plan mode declined: …” in the activity stream.

**Turn data.** Streaming text arrives as `agent_message_chunk`; reasoning arrives as
`agent_thought_chunk` and is summarized as a single “Reasoning” status per stretch
rather than echoed. Every `session/update` carries `_meta.totalTokens` (the context
currently held), which feeds the context indicator against the active model's
`totalContextTokens`; `_x.ai/session_notification` `model_changed` keeps the active
model current, and the `session/prompt` result's `_meta.totalTokens` closes the turn.
Other `_x.ai/*` notifications (queue, settings, announcements, hooks) are ignored.
Tool calls, diffs, slash commands (`available_commands_update`, including installed
skills), HTTP MCP for PandaCode tools, cancel, and resume all use the common ACP paths.

**Not yet.** The agent advertises `promptCapabilities.image: false`, so the composer
offers documents and folders but refuses images for Grok tasks with a note; the
mockup sketcher is hidden for them. Slash-command discovery and the harness console
(`grok --resume <id> --permission-mode …`) follow the same flags; `plan` maps to the
TUI's native plan mode there.

Verified against `grok` 1.0.30/1.0.40 on 2026-09-21 by driving `grok agent stdio`
directly; the recorded frames back the unit tests in `providers/acp.rs`, and an
ignored smoke test (`installed_grok_negotiates_settings_without_prompt`) replays the
real handshake up to the prompt boundary without spending tokens.
