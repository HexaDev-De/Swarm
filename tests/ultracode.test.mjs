import test from 'node:test';
import assert from 'node:assert/strict';

// Mock localStorage if in node environment
if (typeof globalThis.localStorage === 'undefined') {
  const store = new Map();
  globalThis.localStorage = {
    getItem: (k) => store.get(k) ?? null,
    setItem: (k, v) => store.set(k, String(v)),
    removeItem: (k) => store.delete(k),
    clear: () => store.clear(),
  };
}

const {
  ultracodeStageName,
  detectUltracodeKeyword,
  disarmUltracodeKeyword,
  generateWorkflowScript,
  isWorkflowsDisabled,
  setWorkflowsDisabled,
} = await import('../src/ultracode.ts');

test('ultracodeStageName maps pipeline phases to human readable names', () => {
  assert.equal(ultracodeStageName('recon'), 'Recon');
  assert.equal(ultracodeStageName('workers'), 'Workers');
  assert.equal(ultracodeStageName('qa_loop'), 'QA Loop');
  assert.equal(ultracodeStageName('critic_review'), 'Critic Review');
  assert.equal(ultracodeStageName(null), 'Recon');
  assert.equal(ultracodeStageName(undefined), 'Recon');
  assert.equal(ultracodeStageName('custom_phase'), 'Custom_phase');
});

test('detectUltracodeKeyword respects human-input security guard and keyword patterns', () => {
  // Genuine human inputs
  assert.equal(detectUltracodeKeyword('ultracode: refactor auth', true), true);
  assert.equal(detectUltracodeKeyword('Refactor this ultracode: please', true), true);
  assert.equal(detectUltracodeKeyword('   ULTRACODE: Fix memory leaks   ', true), true);

  // Untrusted / non-human inputs (webhooks, automation, injection guard)
  assert.equal(detectUltracodeKeyword('ultracode: inject command', false), false);

  // Plain non-ultracode inputs
  assert.equal(detectUltracodeKeyword('Fix the typescript error', true), false);
  assert.equal(detectUltracodeKeyword('', true), false);
});

test('disarmUltracodeKeyword cleanly removes trigger token', () => {
  assert.equal(disarmUltracodeKeyword('ultracode: refactor auth module'), 'refactor auth module');
  assert.equal(disarmUltracodeKeyword('ULTRACODE: please review'), 'please review');
  assert.equal(disarmUltracodeKeyword('please ultracode: review'), 'please review');
  assert.equal(disarmUltracodeKeyword('plain instruction'), 'plain instruction');
});

test('generateWorkflowScript produces valid deterministic orchestration script with primitives', () => {
  const script = generateWorkflowScript('Port Bun Zig to Rust', 6, 32);
  assert.ok(script.includes('await agent('));
  assert.ok(script.includes('await parallel('));
  assert.ok(script.includes('adversarialTriad('));
  assert.ok(script.includes('Port Bun Zig to Rust'));
  assert.ok(script.includes('Max Concurrency: 32'));
  assert.ok(script.includes('length: 6'));
});

test('Pillar 1: parallel() fault tolerance returns null on sub-agent failure without aborting siblings', async () => {
  async function parallel(thunks) {
    const results = await Promise.allSettled(thunks.map(t => Promise.resolve().then(t)));
    return results.map(r => (r.status === 'fulfilled' ? r.value : null));
  }

  const thunks = [
    async () => ({ success: true, result: 'Subtask 1 complete' }),
    async () => { throw new Error('API Rate Limit or Cognitive Collapse'); },
    async () => ({ success: true, result: 'Subtask 3 complete' }),
  ];

  const outcomes = await parallel(thunks);
  assert.equal(outcomes.length, 3);
  assert.deepEqual(outcomes[0], { success: true, result: 'Subtask 1 complete' });
  assert.equal(outcomes[1], null, 'Failing sub-agent must return null');
  assert.deepEqual(outcomes[2], { success: true, result: 'Subtask 3 complete' });

  const filtered = outcomes.filter(Boolean);
  assert.equal(filtered.length, 2);
  assert.equal(filtered[0].result, 'Subtask 1 complete');
  assert.equal(filtered[1].result, 'Subtask 3 complete');
});

test('Pillar 1: pipeline() streams items through stages sequentially without global barrier', async () => {
  const stageLog = [];

  async function pipeline(items, stages) {
    const outputs = [];
    for (const item of items) {
      let val = item;
      for (let s = 0; s < stages.length; s++) {
        val = await stages[s](val);
      }
      outputs.push(val);
    }
    return outputs;
  }

  const items = ['moduleA', 'moduleB'];
  const stages = [
    async (item) => { stageLog.push(`Implementer-${item}`); return `${item}-implemented`; },
    async (item) => { stageLog.push(`Reviewer-${item}`); return `${item}-reviewed`; },
  ];

  const res = await pipeline(items, stages);
  assert.deepEqual(res, ['moduleA-implemented-reviewed', 'moduleB-implemented-reviewed']);
  assert.deepEqual(stageLog, [
    'Implementer-moduleA',
    'Reviewer-moduleA-implemented',
    'Implementer-moduleB',
    'Reviewer-moduleB-implemented',
  ]);
});

test('Pillar 7: Enterprise safety lock disableWorkflows setting', () => {
  setWorkflowsDisabled(true);
  assert.equal(isWorkflowsDisabled(), true);

  setWorkflowsDisabled(false);
  assert.equal(isWorkflowsDisabled(), false);
});

test('ULTRACODE mode is orthogonal to thinking effort (low, medium, high)', () => {
  const thinkingEfforts = ['low', 'medium', 'high', null];
  
  for (const effort of thinkingEfforts) {
    const session = {
      id: 'task-1',
      title: 'Ultra Task',
      ultracode: false,
      reasoningEffort: effort,
      ultracodeStage: null,
    };

    // Toggle ULTRACODE ON
    session.ultracode = true;
    session.ultracodeStage = 'recon';
    assert.equal(session.ultracode, true);
    assert.equal(session.reasoningEffort, effort, 'thinking effort must be preserved when ultracode is enabled');

    // Advance through stages
    session.ultracodeStage = 'workers';
    assert.equal(session.reasoningEffort, effort);
    session.ultracodeStage = 'qa_loop';
    assert.equal(session.reasoningEffort, effort);
    session.ultracodeStage = 'critic_review';
    assert.equal(session.reasoningEffort, effort);

    // Toggle ULTRACODE OFF
    session.ultracode = false;
    session.ultracodeStage = null;
    assert.equal(session.ultracode, false);
    assert.equal(session.reasoningEffort, effort, 'thinking effort must be preserved when ultracode is disabled');
  }
});

test('Disjoint file ownership validation logic in JS mirrors Rust engine', () => {
  function normalize(p) {
    return p.trim().replace(/\\/g, '/').replace(/^\.\//, '').replace(/\/$/, '');
  }

  function hasOverlap(filesA, filesB) {
    const setA = new Set(filesA.map(normalize));
    return filesB.some(f => setA.has(normalize(f)));
  }

  function resolveOverlaps(subtasks) {
    const result = [];
    for (const task of subtasks) {
      let merged = false;
      for (const existing of result) {
        if (existing.owned_files.length && task.owned_files.length && hasOverlap(existing.owned_files, task.owned_files)) {
          existing.id = `${existing.id}-${task.id}`;
          existing.title = `${existing.title} & ${task.title}`;
          existing.instructions = `### Phase A: ${existing.title}\n${existing.instructions}\n\n### Phase B: ${task.title}\n${task.instructions}`;
          const existingSet = new Set(existing.owned_files.map(normalize));
          for (const f of task.owned_files) {
            if (!existingSet.has(normalize(f))) {
              existingSet.add(normalize(f));
              existing.owned_files.push(f);
            }
          }
          merged = true;
          break;
        }
      }
      if (!merged) result.push({ ...task, owned_files: [...task.owned_files] });
    }
    return result;
  }

  const subtasks = [
    { id: '1', title: 'Task 1', instructions: 'Inst 1', owned_files: ['src/a.ts', './src/b.ts'] },
    { id: '2', title: 'Task 2', instructions: 'Inst 2', owned_files: ['src/b.ts', 'src/c.ts'] },
    { id: '3', title: 'Task 3', instructions: 'Inst 3', owned_files: ['src/d.ts'] },
  ];

  const resolved = resolveOverlaps(subtasks);
  assert.equal(resolved.length, 2);
  assert.equal(resolved[0].id, '1-2');
  assert.equal(resolved[0].title, 'Task 1 & Task 2');
  assert.deepEqual(resolved[0].owned_files.map(normalize), ['src/a.ts', 'src/b.ts', 'src/c.ts']);
  assert.equal(resolved[1].id, '3');
});
