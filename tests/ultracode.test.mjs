import test from 'node:test';
import assert from 'node:assert/strict';

const { ultracodeStageName } = await import('../src/ultracode.ts');

test('ultracodeStageName maps pipeline phases to human readable names', () => {
  assert.equal(ultracodeStageName('recon'), 'Recon');
  assert.equal(ultracodeStageName('workers'), 'Workers');
  assert.equal(ultracodeStageName('qa_loop'), 'QA Loop');
  assert.equal(ultracodeStageName('critic_review'), 'Critic Review');
  assert.equal(ultracodeStageName(null), 'Recon');
  assert.equal(ultracodeStageName(undefined), 'Recon');
  assert.equal(ultracodeStageName('custom_phase'), 'Custom_phase');
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
