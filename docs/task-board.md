# Project board

PandaCode offers Conversation and Board views, remembered on this device. Both
views use the same tasks, provider sessions, worktrees, approvals and messages.
The view toggle is borderless and shares the sidebar's frosted material over wallpapers.
Board view fills the window without a sidebar. Its header provides project
filtering, opening projects and archived tasks; Conversation restores the sidebar.
The board follows the project filter and opens one task at a time in a right
panel. It reuses the conversation renderer instead of loading every transcript.

Tasks have four durable stages: Pending, In progress, Review and Completed.
Adding a title and description queues a task without starting an agent. Start
sends the saved description with the task's selected harness, model and approval
policy. An accepted run enters In progress; success enters Review. Errors and
stops remain In progress, with a retry action. Follow-up messages from Review
start another run. The user marks reviewed work Completed or reopens it.

Completed tasks archive seven days after completion. Native storage catches up
on launch and checks while the app is open; quitting the app needs no background
process. Archived tasks remain recoverable through the existing archive view.
Reopening clears completion time. No Git commit or merge is implied by completion.

The board has four compact columns, quiet stage colors and provider/worktree
icons. Rounded task cards share the sidebar and thread panel's frosted material
when wallpapers are enabled. The right panel presents task details followed by comments from the user
and agent, with existing tool disclosures and composer controls. It can close
without stopping work. On narrow windows it overlays the board, with a visible
close control. With wallpapers enabled, the panel matches the sidebar's translucent
tint and backdrop blur. Both layouts have rounded corners and a small inset, with
no left divider or heavy shadow. Increased contrast or reduced
transparency restores a solid surface. Existing conversations remain available
through the view switch.

Verification covers queued-task persistence, guarded starts, successful and
failed runs, completion/reopening, the seven-day boundary, migration, preserved
drafts, light/dark layouts and keyboard access. Board cards update on metadata
changes, not on every streamed token.

The task list includes only 512 characters of each description. Full details
are fetched for the selected task and released when selecting another task;
Start reads the complete description from storage. A late metadata refresh
cannot replace those details with the short card preview. Editing is unavailable
until details have loaded, and a canceled, delayed creation never changes the
current selection.

The archive sweep runs once per minute while PandaCode is open and at launch.
Restoring a completed task grants a fresh seven-day retention period. Reopen
returns it to Review and clears completion time; a new reply starts work again.
