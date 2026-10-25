# Agent tools and integrations

PandaCode launches the installed Codex CLI through its app-server. Codex supplies
shell commands, file editing, image inspection, search, subagents, skills and
configured MCP integrations according to its version, configuration and policy.
PandaCode does not replace those tools with simulated implementations or enable
features the user's configuration has disabled.

## App tools

New Codex tasks register `pandacode_*` dynamic functions: project/task discovery,
task history and subagent activity, creating/starting/following up/renaming/
archiving tasks, board stages, worktrees, showing tasks/files/terminals, local
ports, browser navigation/reading/clicking/typing/screenshots, and project
moodboard discovery/metadata/image reading. See [Moodboards](moodboards.md).

Task operations remain within the current project. File previews use the task's
checkout and existing native path validation. Creating a worktree creates a task
under its original project. Starting another agent uses the same admission and
concurrency limits as the UI. Starting/following up on the current running task
is rejected. Use Codex's shell tool for command execution and its image tool to
inspect the local PNG returned by `pandacode_capture_browser`.

Host callbacks are outside Codex's shell sandbox, so PandaCode enforces its own
approval boundary: mutations and browser/UI access request approval in Standard
and Auto modes, are unavailable in Plan mode, and run without a prompt only when
the user selected Full access. Browser access requires the calling task to be
visible; background tasks cannot hijack another task's browser. The browser
interface accepts a finite action and data, never agent-provided JavaScript or
generic native IPC. Remote page text is untrusted content. Remote webviews have
no application IPC access.

The installed app-server persists dynamic tools with newly created threads.
Its `thread/resume` API cannot add them to a conversation created without them.
Older tasks retain their original coding/MCP tools; start a new Codex task to use
newly added PandaCode app tools. Claude, GLM, Kimi, OpenCode, and Antigravity receive
these host tools through the authenticated, run-local MCP bridge where the
provider supports HTTP MCP. Codex uses its dynamic tool interface. Provider-owned
tools and integrations continue through their own protocols and configuration.

## Integrations and questions

Settings → Tools & integrations inspects Codex's MCP server/tool inventory,
project skills, and installed apps. Discovery is on demand, bounded, and does
not send a model prompt, create a thread, execute tools, change configuration,
or log into services. Codex may initialize configured servers during discovery.
Unknown, disabled and unauthenticated states are shown distinctly. The inventory
does not claim that all built-in tools or desktop services are enumerable.

Configure MCP servers with Codex CLI, following the linked official guide. The
next task run inherits the updated configuration. PandaCode now handles Codex's
structured questions and supported MCP form/URL elicitations. Requests are
isolated by run and task, drafts survive streaming, and Stop/resolution clears
them. Secret inputs use password fields and are not saved in conversation
history. Unsupported form constraints return an explicit protocol error instead
of silently accepting incomplete answers. URL authorization always requires a
separate explicit open and confirmation action.

Callbacks run asynchronously (eight executing at once, at most 64 admitted per
run), with bounded arguments/results, timeouts and cancellation. A slow callback
cannot block provider stdout. Integration discovery is a short-lived process,
limited to one at a time, and cleans up children on completion/cancellation.

## Services that are not portable

This bridge does not reuse the Codex desktop app's private account, usage-reset,
automation, voice, CUA or hosted image-generation backends. Those require their
own supported integrations and credentials. No desktop session tokens or raw
provider configuration are copied or sent to the frontend.

Protocol verified against installed Codex CLI 0.154.0's generated experimental
schemas and the official [app-server documentation](https://learn.chatgpt.com/docs/app-server)
and [MCP guide](https://developers.openai.com/codex/mcp/).
