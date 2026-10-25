// Appended to src/preview.ts while capturing README screenshots (never shipped): a fuller
// sample workspace, with a finished conversation, tasks in every board column, and replies
// the capture script queues in window.__showcaseReply.
{
  const at = Date.now();
  const base = sessions[1];
  const task = (id, title, provider, model, stage, ago, extra = {}) => ({
    ...base, id, projectId: 'project-1', title, provider, model, status: 'idle', taskStage: stage,
    createdAt: at - ago - 60_000, updatedAt: at - ago, lastResponseAt: stage === 'pending' ? null : at - ago, resumeId: null, issue: undefined, ...extra,
  });
  sessions.push(
    task('session-5', 'Offline sync for the reading list', 'opencode', '', 'in_progress', 40 * 60_000, { status: 'running' }),
    task('session-6', 'Polish the onboarding flow', 'glm', 'glm-4.7', 'review', 3 * 3600_000),
    task('session-7', 'Fix flaky date tests', 'codex', 'gpt-6-astra', 'completed', 26 * 3600_000, { completedAt: at - 26 * 3600_000 }),
    task('session-8', 'Write the v2 migration guide', 'claude', 'opus', 'pending', 5 * 3600_000),
  );
  for (const id of ['session-5', 'session-6', 'session-7', 'session-8']) messages[id] = [];

  const tool = (id, name, input, output, ago) => ({ id: ++id_, sessionId: 'session-1', role: 'tool', content: encodeTool({ id, name, status: 'completed', input, output }), createdAt: at - ago });
  let id_ = 500;
  messages['session-1'] = [
    { id: ++id_, sessionId: 'session-1', role: 'user', content: 'Search feels a little slow. Can you make it respond instantly as I type, and keep the keyboard navigation working?', createdAt: at - 420_000 },
    { id: ++id_, sessionId: 'session-1', role: 'assistant', content: 'I’ll trace how the search input updates the results, then check the keyboard interactions.', createdAt: at - 410_000 },
    tool('show-read', 'Read', 'src/components/search.ts', 'export function createSearchIndex(items) { … }', 400_000),
    tool('show-grep', 'Grep', 'buildIndex', 'src/components/search.ts:14\nsrc/library/view.ts:88', 380_000),
    tool('show-edit', 'Edit', 'src/components/search.ts', 'Updated 2 hunks', 330_000),
    tool('show-test', 'Bash', 'npm test', 'PASS src/components/search.test.ts\n  ✓ filters results as the query changes\n  ✓ preserves keyboard selection\n\nTests: 42 passed, 42 total', 300_000),
    { id: ++id_, sessionId: 'session-1', role: 'assistant', createdAt: at - 290_000, content: [
      'Search now updates on every keystroke without rebuilding the index.',
      '',
      '### What changed',
      '',
      '- **One index per dataset.** `buildIndex()` moved out of the input handler and is cached by dataset version.',
      '- **Only visible rows render.** The results list is virtualized, so a 10,000-item library stays under 4 ms per frame.',
      '- **Keyboard navigation kept.** ↑ ↓ and Enter work as before, and the active row stays in view.',
      '',
      '```ts',
      'const index = useMemo(() => buildIndex(items), [items.version]);',
      'const results = useDeferredValue(search(index, query));',
      '```',
      '',
      'All 42 tests pass. Want me to open a pull request?',
    ].join('\n') },
  ];
}
// The finished turn carries its changed-files card, like a real run.
turnChanges.finish(messages['session-1'][0]);
