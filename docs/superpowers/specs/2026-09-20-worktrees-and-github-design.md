# Worktree workflow and GitHub integration

## Requested outcome

One place to see and act on a task's branch, a worktree lifecycle that does not
strand drafts or leave orphaned checkouts, and optional GitHub features driven by
the user's own `gh` CLI: issue and pull-request mentions in the prompt, importing
issues to the board, linked-work chips, and opening pull requests from a task.

## Worktrees

- **One control.** The context bar above the prompt appears on every task
  (compact on active conversations and in the Board task panel). Its branch
  chip is a menu: current branch and checkout path, then
  context-dependent actions. The statusbar branch button is replaced by the
  checkout folder name (copies its path); the task menu, inspector `+`, and
  command palette open the same dialog.
- **Convert in place.** On a task with no history, *Work in a new worktree…*
  attaches a fresh worktree to the current task (`attach_worktree`), keeping the
  draft, title, harness, and model. A task with history offers *New task in a
  worktree…*, which inherits its settings (existing behavior).
- **Create form.** Live preview of the final branch (`pandacode/<name>`), a name
  suggested from the task title or linked issue, *Start from* any local branch
  (default: the current branch), and *Use existing branch* when the name already
  exists. Validation matches the backend: `[A-Za-z0-9_/-]`, no `..`.
- **Cleanup offer.** Archiving or completing a worktree task checks the checkout
  (`worktree_status`: exists, clean, upstream/ahead, merged into the main
  checkout). A clean checkout is offered for removal (folder and branch; the
  branch is kept when unmerged and the dialog says so). A dirty checkout is left
  alone with the reason shown. *Remove worktree…* in the chip menu uses the same
  guard and archives the task. Removal runs `git worktree remove`, optional
  `git branch -d`, then `git worktree prune`. Nothing is ever forced.
- **Missing folder.** The chip shows *Worktree missing* with *Recreate* (the
  branch still exists) or *Archive task*. The branch name is stored on the
  session (`worktree_branch`) so chips and recreation do not need Git.

## GitHub through `gh`

- **Connection.** No tokens. Native code runs the user's `gh` with argument
  arrays, bounded output, a null stdin, and timeouts. `gh repo view` from the
  workspace resolves `owner/repo`, default branch, and URL; without a GitHub
  remote every GitHub affordance is hidden for that project. A Settings switch
  (device-local, on by default) disables all `gh` calls. Settings shows `gh`
  missing / not signed in (with the `gh auth login` hint) / connected as *login*.
- **Mentions.** The `@` picker gains issues and pull requests (open, ten each,
  cached briefly). Selecting inserts plain `#123`. At send time every `#123` in
  the message (picked or typed, up to five) is resolved once and appended as a
  `GitHub context:` block — title, state, author, body excerpt (4 KiB), URL —
  framed as content, not instructions, like browser context.
- **Import.** Board header *Import from GitHub*: filters (open, assigned to me,
  label, search), up to fifty issues with checkboxes and labels. Import creates
  Pending tasks (title, body plus URL) linked to their issue. Issues already
  linked in the project are marked and skipped; an unstarted linked task whose
  issue changed can be updated. No background sync.
- **Links.** `task_links(session_id, kind issue|pull, number, url, title, state,
  checked_at)`; one issue and one pull request per task, serialized on `Session`
  as `issue` and `pullRequest`. Cards, the task header, and the chip menu show
  `#123 · open` / `PR #45 · merged`; clicking opens the URL. States refresh in
  one GraphQL batch per project on window focus, at most every five minutes.
- **Pull requests.** *Push & open pull request…* for a task on a non-default
  branch: title from the task, body from its description plus `Closes #123`
  when an issue is linked, base = default branch. Refuses with a pointer to the
  Changes tab when the tree is dirty. Runs `git push -u origin <branch>` then
  `gh pr create`; the PR is linked to the task.

## Architecture boundaries

`workspace.rs` gains a shared bounded process runner, branch listing, push,
worktree status/attach/remove/recreate. New `github.rs` owns `gh` detection,
repository resolution, search, import listing, reference resolution, batched
link states, and PR creation; it never touches the store except through the
commands in `lib.rs`. `storage.rs` adds migration 18 (`task_links`,
`sessions.worktree_branch`). Frontend: `worktrees.ts` (context bar, chip menu,
dialogs, cleanup offer), `github.ts` (status, mentions, context, import, PR,
chips), small extensions to `file-mentions.ts` (mention entries with a custom
insertion), `task-board.ts` (chips, import button), `settings.ts` (GitHub row),
`preview.ts` mocks for every new command.

## Validation

Rust: attach/remove/recreate worktrees, branch listing, dirty and unmerged
guards, `gh` JSON parsing with truncation, GraphQL batch parsing, migration.
Browser (`worktrees.spec.ts`, `github.spec.ts` on preview mocks): single chip
per view, in-place conversion keeps the draft, base branch and existing-branch
paths, cleanup offer on archive/complete, missing worktree recovery, mention
section and send-time context, import with dedupe, PR dialog, chips, and the
switch hiding everything. Existing suites, typecheck, build, and Clippy stay
green.
