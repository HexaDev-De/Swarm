# File mentions

Type `@` in the conversation composer, a board task description, or the quick
overlay's new-task prompt to search project files. Arrow keys select a match;
Enter or Tab inserts it without sending the prompt. Escape closes the menu.
File paths containing spaces or punctuation are quoted and escaped, for example
`@"docs/design notes.md"`. Email addresses do not open the picker.

References remain editable prompt text. The selected harness reads the file
through its normal tools in the task's working directory. Typing or selecting
a reference does not read or attach its contents, so large files do not inflate
the prompt automatically. Existing explicit file and image attachments retain
their separate behavior (see [attachments](attachments.md)).

Typed queries also match folders, listed after files of the same rank and
inserted with a trailing slash (`@src/components/`). An empty query lists files
only. Folders that build output or version control would hide are never offered.

## Scope and performance

`search_files` resolves the registered project and optional task natively. A
worktree task searches its own checkout; mismatched projects/tasks or unavailable
checkouts reject rather than searching a different folder. The quick overlay
searches the selected project for its new task.

Discovery includes untracked files and useful dotfiles, respects ignore rules,
and skips symlinks, Git internals, dependency folders and common build outputs.
Case-insensitive matching prioritizes exact filenames, filename prefixes,
filename substrings, then path substrings. The interface displays up to 40
matches, or 30 in the quick overlay.

Search runs off the interface thread. A lazy per-workspace index expires after
three seconds; at most three roots are retained. A scan stops at 20,000 files,
80,000 directory entries, 2 MiB of indexed path text, depth 48, or a 500 ms
budget. Blocking filesystem operations cannot be preempted. Very large projects
can therefore have an incomplete suggestion list; explicit paths remain usable.
No file contents, background watcher, or continuously running indexer is added.

The composer debounces requests, retains at most 12 recent query results for five
seconds, ignores stale responses, and reuses matching suggestion rows while
typing. Switching tasks/projects or disposing the composer clears its cache and
listeners. The picker supports reduced motion and reduced transparency.

## Validation — September 19, 2026

- All 27 native workspace tests passed, including eight search tests for scope,
  worktrees, ignore rules, path escaping, traversal limits and cache refresh.
- Native Clippy passed with warnings denied. A 3,000-file debug fixture measured
  21 ms for its initial scan and about 5.5 ms per cached query on the development
  Mac; these are local measurements, not a general performance guarantee.
- Nine focused browser tests passed in both Chrome and WebKit. The 30 existing
  slash-command/board regressions and all 32 overlay tests passed in Chrome;
  three focused overlay checks also passed in WebKit.
- Desktop, narrow board and minimum-size overlay screenshots were inspected.
  No browser errors were observed; menu placement and keyboard ownership passed.
- TypeScript, the production frontend build and local macOS app packaging passed.
  The local ad-hoc app signature passed strict verification.
