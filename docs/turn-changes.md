# Changes after a turn

Completed turns show a compact file summary below the response in Conversation,
the Board task panel, and architect worker history. Expand folders to see file
names and added/deleted line counts. **Open diff** opens immutable saved patches;
**Run** appears when the checkout contains `pandacode-run.sh` (see
[Run and preview](run-project.md)). Empty captures do not add a card or claim that
no files changed. Older whole-checkout snapshots are retained in storage but are
not presented as task-owned changes: their authorship cannot be reconstructed.

The read-only viewer uses `@pierre/diffs` for syntax highlighting, old/new line
numbers, word-level changes, wrapping, and unified/split layout. The Changes
inspector separately shows current workspace Git changes; Stage/Unstage never
changes a historical snapshot.

## What is recorded

Tracking captures only named file writes from the task, using its actual checkout
or worktree. The runner never uses Git status or a whole-folder diff as evidence
of task ownership. Unchanged pre-existing edits and unrelated paths are excluded.

The stable `pandacode_track_files` tool gives agents a pre-write handshake: `begin`
with exact relative paths captures baseline bytes; `complete` with that capture ID
and the subset actually edited freezes the result. The instructions request this
for each write batch, including shell scripts. Codex, Claude, and ACP edit events
also carry structured write paths as a fallback. Native asynchronous notifications
can arrive too late to capture a fast edit, so the explicit handshake is preferred.
Read-only tools never establish ownership. Incomplete, unreported, or ambiguous
writes can be omitted rather than assigned to the wrong task.

Snapshots freeze at tool completion, so later external edits do not leak into a
completed diff. Detected overlapping task writes to the same file and gaps between
this task's edits are omitted. A file snapshot cannot identify an external editor
or unreported process writing the same file *during* a declared capture window;
separate worktrees are needed for absolute isolation.

Each completion appends a `changes` message with an `attributed: true` summary.
Native goal turns retain separate boundaries. Patches are stored in SQLite and
fetched only when opened; they never read current working-tree bytes.

Architect summaries also roll up attributed edits from their own settled workers,
including work left by failed or stopped workers. Stored source IDs prevent
counting the same worker capture twice. Unrelated tasks and legacy workspace
snapshots are excluded. Combined per-file patches preserve each captured edit
batch, so line counts describe edit activity rather than a Git diff against HEAD.

## Resource limits

- At most 200 named file paths per turn, 512 tracked tools, 1 MiB per file and
  32 MiB of captured bytes. Captures use temporary files, a 500 ms cooperative
  deadline and a 750 ms bounded wait, with at most eight capture workers.
- Patch limits are 256 KiB per file and 2 MiB per turn. Binary/oversized line
  counts remain unknown. Unsafe paths and symlink escapes are rejected.
- Saved patch bodies have a 128 MiB retention budget. Summaries survive eviction;
  opening an evicted patch explains why details are unavailable.
- Partial captures are labeled. Diff code loads on demand, and streamed text does
  not trigger filesystem reads, capture work, or diff rendering.

Requests for saved details validate the owning task, snapshot ID and recorded
path. None of this changes the Git index, references, or repository hooks.
