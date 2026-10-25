# Approval behaviours

The composer permission selector has four choices per harness. Its selected label
remains visible, and each menu row describes what the harness is allowed to do.
The selected mode is saved with the task. New user tasks, including board tasks,
worktrees, and the quick launcher, start in **Full access** unless a mode is
explicitly supplied. Switching harnesses or applying a team profile preserves
the task's selected mode. Existing tasks and saved launcher choices are not
migrated to Full access. Stop an active turn before changing its permissions.

Workers and agent-created tasks/worktrees inherit their parent's current mode;
conversation forks retain the source mode. Team profiles store model choices,
not permission grants. Retrying a worker after reducing its parent's permissions
uses the new, reduced mode.

| Stored mode | Claude Code | Codex | Kimi Code |
| --- | --- | --- | --- |
| `ask` | Standard: normal approval rules | Standard: workspace sandbox, user reviews escalation requests | Standard: manual approval mode |
| `auto` | Accept edits: automatically allow file edits | Approve for me: Codex reviews approval requests inside the workspace sandbox | Ask when needed: routine operations proceed; sensitive actions still ask |
| `plan` | Plan mode: explore and prepare an implementation plan | Read only: read-only sandbox, no permission escalation | Plan mode: planning workflow with manual approvals |
| `full` | Full access: bypass tool approval prompts | Full access: no sandbox and no approval prompts | Full access: unattended tool approval |

Harness-enforced exceptions and administrator policies still apply.

Full access maps to the following supported native mechanisms:

| Harness | Native behavior |
| --- | --- |
| Claude Code | `--permission-mode bypassPermissions` with `--allow-dangerously-skip-permissions` |
| GLM Coding Plan | The same Claude Code permission flags, with the isolated Z.AI provider configuration |
| Codex | `danger-full-access` sandbox, `never` approval policy, and `dangerFullAccess` on each turn |
| Kimi Code | Its advertised ACP `auto` mode (unattended execution); PandaCode waits for mode acknowledgement |
| OpenCode | Process-local `permission: { "*": "allow" }` for the selected built-in agent and matching ACP approvals |
| Antigravity | Accepts the offered ACP one-time tool permission; the server's own policy remains authoritative |

Antigravity has no universal ACP setting that can disable server-side sandboxing
or managed restrictions. PandaCode does not invent one. Structured questions still
require an answer; Full access does not choose answers on the user's behalf.

These policies are applied through native flags or the harness protocol. The UI
does not simulate acceptance by clicking pending approval buttons. Returning
to Standard explicitly restores that policy on the next turn, including resumed
conversations. Unsupported modes or failed policy changes stop the turn instead
of silently continuing under a different policy. Plan modes can permit their
harness's own planning files; only Codex's Read only option is described as a
strict write restriction.

Claude receives an explicit `--permission-mode` on each launch (`default`,
`acceptEdits`, `plan`, or `bypassPermissions`). Codex receives approval policy,
reviewer and sandbox settings on thread start/resume and each turn. Kimi must
advertise the required ACP mode and acknowledge `session/set_mode` before the
prompt is sent.

Pi has no native approval system, so a PandaCode extension enforces the modes
in Pi's RPC session: Standard asks before edits, commands and other non-read
tools; Accept edits allows edit/write and asks for the rest; Plan mode allows
read-only tools and declines everything else; Full access runs every tool without
asking.

DeepSeek (`dsh --profile acp`) enforces modes with its own sandbox, chosen by
`DSH_PERMISSION_MODE` in its process environment. dsh asks only when a tool wants to
escalate past the sandbox, never before each workspace edit. Standard runs
`workspace-write` and asks on escalation; Accept edits is the same sandbox and also
approves escalations from dsh's edit/write/str_replace_editor tools; Plan mode runs
`read-only`, declines every escalation and prefixes the prompt with the plan
instruction; Full access runs `danger-full-access`, which never asks. A dsh session
keeps the sandbox it was created with, so after a mode change PandaCode starts a
fresh dsh session (dsh's own history of the task does not carry over). A
`permission.defaultPreset` in `~/.dsh/settings.yaml` overrides the environment for
new sessions.

Kimi's actual mode behaviour is verified against its installed engine source:
`default` is manual, `yolo` auto-approves routine operations, and `auto` is fully
unattended. Its ACP mode descriptions can lag behind those engine semantics, so
PandaCode supplies clearer labels.

References: [Claude permission modes](https://code.claude.com/docs/en/permissions),
[Kimi interaction modes](https://moonshotai.github.io/kimi-code/en/guides/interaction.html).
OpenCode: [permission configuration](https://opencode.ai/docs/permissions/).
ACP: [tool permissions](https://agentclientprotocol.com/protocol/v1/tool-calls).
Codex mappings were checked against its installed app-server schema for
`ThreadStartParams`, `ThreadResumeParams`, and `TurnStartParams`, including the
`approvalsReviewer` enum and explicit turn sandbox policies.
