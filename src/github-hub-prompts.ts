/* What the GitHub view asks an agent to do. Each prompt names the pull request or issue as
 * `#123`, so sending it attaches that item's title, state and description as GitHub context
 * (github.ts `contextFor`); the prompt itself only says what to do with it. Agents run in
 * the repository's checkout, where `gh` already knows the repository. */
export type PullAction = 'review' | 'checks' | 'feedback' | 'merge' | 'thread';
export type IssueAction = 'solve' | 'thread';

interface PullRef { number: number; title: string; base: string; head: string }
interface IssueRef { number: number; title: string }

export const PULL_ACTIONS: { id: Exclude<PullAction, 'review'>; label: string; icon: string }[] = [
  { id: 'thread', label: 'Open in thread', icon: 'chat' },
  { id: 'checks', label: 'Fix failing checks', icon: 'check' },
  { id: 'feedback', label: 'Address review comments', icon: 'edit' },
  { id: 'merge', label: 'Merge when ready', icon: 'branch' },
];

export function pullPrompt(action: PullAction, pull: PullRef): string {
  const ref = `#${pull.number}`;
  switch (action) {
    case 'review': return `Review pull request ${ref} (${pull.head} → ${pull.base}). Check it out with \`gh pr checkout ${pull.number}\`, read the whole diff against ${pull.base} and run the tests. Report bugs, risky changes and missing tests with file and line references, and suggest fixes. Don't push or comment on GitHub unless I ask.`;
    case 'checks': return `Pull request ${ref} has failing checks. Check it out with \`gh pr checkout ${pull.number}\`, see what failed with \`gh pr checks ${pull.number}\`, reproduce it locally, fix it and push to ${pull.head}. Then tell me what was wrong.`;
    case 'feedback': return `Address the review comments on pull request ${ref}. Read them with \`gh pr view ${pull.number} --comments\`, make the requested changes on ${pull.head}, run the tests, push, and summarize what changed for each comment.`;
    case 'merge': return `Get pull request ${ref} merged if it's ready: checks pass (\`gh pr checks ${pull.number}\`), it's approved and it merges cleanly into ${pull.base}. If so, merge it with \`gh pr merge ${pull.number} --squash --delete-branch\`. If anything blocks it, don't merge; tell me what's blocking and how to fix it.`;
    case 'thread': return `Let's work on pull request ${ref}: `;
  }
}

export function issuePrompt(action: IssueAction, issue: IssueRef): string {
  const ref = `#${issue.number}`;
  return action === 'solve'
    ? `Fix issue ${ref}: ${issue.title}. Work on a new branch, make the change, add or update tests and run them. When it's done, summarize what changed and offer to open a pull request that closes ${ref}.`
    : `Let's work on issue ${ref}: `;
}

/** A failed workflow run: the agent reads the failing logs, reproduces and fixes it. */
export function runPrompt(run: { id: number; workflow: string; branch: string }): string {
  return `The "${run.workflow}" workflow run ${run.id} on ${run.branch} failed. See what failed with \`gh run view ${run.id} --log-failed\`, check out ${run.branch}, reproduce the failure locally, fix it and run the tests. Summarize what was wrong and what you changed; don't push unless I ask.`;
}
export const runTitle = (run: { workflow: string; branch: string }) => `Fix ${run.workflow} on ${run.branch}`.slice(0, 120);

/** Thread titles say what the thread is for, short enough for the sidebar. */
export function threadTitle(kind: 'pull' | 'issue', action: PullAction | IssueAction, item: { number: number; title: string }): string {
  const verb = kind === 'issue'
    ? (action === 'solve' ? 'Fix' : 'Issue')
    : ({ review: 'Review', checks: 'Fix checks on', feedback: 'Address review on', merge: 'Merge', thread: 'PR' } as Record<string, string>)[action];
  return `${verb} #${item.number}: ${item.title}`.slice(0, 120);
}

/** How long ago, the way a tracker says it: now, 25m, 5h, 3d, 2mo. */
export function age(iso: string | number, now = Date.now()): string {
  const at = typeof iso === 'number' ? iso : Date.parse(iso);
  if (!Number.isFinite(at)) return '';
  const minutes = Math.max(0, Math.round((now - at) / 60_000));
  if (minutes < 1) return 'now';
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 1440) return `${Math.round(minutes / 60)}h`;
  if (minutes < 60 * 24 * 30) return `${Math.round(minutes / 1440)}d`;
  return `${Math.round(minutes / 43_200)}mo`;
}

/** How far back a push keeps a repository in the main list; older ones fold under Other. */
export const RECENT_DAYS = 30;
export function isRecent(pushedAt: string, now = Date.now()): boolean {
  const at = Date.parse(pushedAt);
  return Number.isFinite(at) && now - at <= RECENT_DAYS * 86_400_000;
}

/** What to do when GitHub can't be reached; the view shows gh's own message underneath. */
export function connectAdvice(error: string): string {
  if (/not found|no such file|cannot find|failed to start|program not found/i.test(error)) return 'Swarm uses the GitHub CLI. Install it (for example <code>brew install gh</code>), then try again.';
  if (/auth login|not logged|authenticat|token/i.test(error)) return 'Sign in to the GitHub CLI by running <code>gh auth login</code> in a terminal, then try again.';
  return 'Swarm couldn’t reach GitHub through the GitHub CLI. Check your connection, then try again.';
}
