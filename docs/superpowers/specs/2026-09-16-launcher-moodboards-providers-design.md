# Launcher, moodboards, and provider connections

## Requested outcome

Make PandaCode accessible while working in other apps, give agents reusable visual references, and support OpenCode, GLM Coding Plan through Claude Code, and Google's official Antigravity ACP server.

## Design and implementation

- **Launcher:** a separate, lazily created Tauri webview, not a second workspace. Command/Control + Shift + Space toggles it. Settings can change or disable the global binding. Show compact running/waiting task, worker, and provider-subagent summaries and launch a prompt using the last project and task configuration. Clicking an entry opens its live conversation inside the overlay; an explicit Open in app control can show the full workspace. Idle/recent/completed history is excluded, and only in-memory live runs establish active status. Escape returns from a conversation to the list, then hides the launcher; focus loss also hides it while work continues. Persist shortcut and launch context; preserve the old binding when a replacement fails.
- **Moodboards:** project-owned boards and app-owned raster image copies. Create/rename/delete boards; import, drop, or paste images; add captions and remove items. Open from the toolbar and attach a board reference to the task's composer. Agents receive project-scoped list/read/image tools with actual image content, not only filenames. Enforce input size/count limits and safe image decoding, retain local source files, and load previews lazily.
- **Providers:** distinct provider identities with real model names. OpenCode and Antigravity use ACP negotiation, event streaming, tools, approval prompts, image capabilities and session resumption. GLM uses Claude's existing transport with process-scoped Z.ai configuration and a macOS Keychain credential. Never rewrite the user's normal Claude settings or expose API keys in frontend settings responses.
- **Connections UI:** one Settings entry opens focused configuration for the new providers. OpenCode uses its existing CLI login. Antigravity requires Google's ACP server, not an invented flag on the interactive `agy` CLI. Executable overrides are explicit absolute paths; no shell strings.

## Architecture boundaries

Root owns workspace/settings integration and trusted-webview command registration. Separate feature modules own launcher, moodboards, and protocol adapters. The browser guest has no app commands. The launcher only receives its four required commands. Large transcripts, diff viewers and board UI are not loaded by the launcher.

## Validation

Native tests cover persistence, image and project boundaries, protocol negotiation/streaming/resume, permission modes, shortcut rollback and secret-free settings serialization. Browser checks cover launcher interaction, Settings, moodboard CRUD/attachments and provider connections. Run frontend typecheck/build, existing Node tests, relevant browser suites and Rust tests. Package and stage the app without terminating running tasks.

## Authoritative integration references

- https://v2.tauri.app/plugin/global-shortcut/
- https://opencode.ai/v2/docs/cli/acp/
- https://docs.z.ai/devpack/tool/claude
- https://antigravity.google/docs/ide/extensions
- https://github.com/agentclientprotocol/registry/blob/main/antigravity-acp/agent.json

No inference requests or account purchases are part of verification. Protocol fixtures and local discovery test integrations without starting paid work.


## Inline overlay conversations — correction

Selecting a running task, worker, or subagent opens its conversation inside the
launcher window. It does not hide the launcher, emit workspace navigation, or
focus the main window. Back and Escape return to the active-agent list; only an
explicit Open in app control uses the existing workspace navigation endpoint.

The compact reader loads lazily and shares the sanitized, incremental Markdown
renderer. The latest assistant response stays visible as tool calls arrive;
previous progress and tool details remain expandable. Completed work stays
readable in the selected reader while disappearing from the active list.

A read-only native endpoint returns the selected task's bounded recent history
and existing live text buffer, or the specifically selected subagent snapshot.
Provider events trigger coalesced reads at most five times per second while
visible. A database revision skips unchanged history, with a low-frequency
refresh recovering missed lifecycle events. Back, task changes, and dismissal
cancel queued rendering; late reads cannot replace the current selection.

Validate parent/worker/subagent isolation, mid-stream entry, completion, delayed
read races, keyboard back navigation, small-window scrolling, and explicit-only
workspace opening in Chrome and WebKit. Native tests verify live/persisted
snapshot consistency, history revisions, and ownership without window actions.


## Harness and model selection in the launcher

The composer includes a harness/model button beside the project selector. It
opens the existing searchable model picker within the overlay, with a compact
blurred surface, company logos, full discovered names, and unavailable harnesses
disabled. Catalog discovery is lazy and cached; the workspace renderer remains
unloaded. Escape closes the picker before dismissing the launcher.

The native preference stores an explicit overlay model selection separately
from workspace launch context. Background context sync can update the project
and permissions but cannot replace the chosen harness/model. The next launch
waits for a pending selection save; persistence failure preserves the prior
selection. Switching model clears incompatible reasoning effort. A saved team
keeps its workers while the explicit choice applies to that new architect only;
reusable team profiles are unchanged. Existing preferences without a selection
continue to follow the workspace default.

Validate discovered names, selection persistence, exact launch settings, save
failures, slow catalog/snapshot races, keyboard dismissal, and narrow-window
layout. Native validation covers preference migration, atomic writes, and team
launches using the explicit choice.
