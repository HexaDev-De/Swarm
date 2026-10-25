# Conversation titles and activity

Automatically named conversations can receive a short title suggested by the agent already doing the work. The optional `pandacode_title` metadata tool uses that agent's current conversation context; it does not launch a second model or send the conversation to PandaCode servers. Suggestions are applied after a successful, saved response. A manual rename, an explicitly named board task, a stopped or superseded run, and internal worker coordination cannot overwrite the user's title.

Providers that cannot use the metadata tool continue normally. A conservative local fallback can clean up the initial provisional title; later topic changes rely on the agent's suggestion so a small follow-up cannot replace an established title. Fallback naming is not an additional AI summarization service. Title provenance is persisted by schema version 20 and carried through conversation forks.

During work, the elapsed-time section starts expanded. Consecutive tool calls form collapsed groups of up to five calls. Assistant updates divide those groups and remain in chronological order; the next response does not pull earlier narration behind a later tool call. Each group shows its call count, short categories, and running or failed counts. Opening it reveals the original input/output disclosures.

Incoming text preserves its existing Markdown nodes. Tool updates patch only the affected group, preserving its expansion and focused tool details. The live preview remains bounded to 30 activity entries and keeps the latest visible narrative when older tools are omitted; full recorded activity remains in saved history. The same grouping is used in conversation, board, and quick-overlay views.

Relevant checks: `tests/turn-activity.test.mjs`, `tests/tool-activity.test.mjs`, `tests/streaming.spec.ts`, `tests/ui.spec.ts`, `tests/overlay.spec.ts`, and native `session_titles` / MCP transport tests.
