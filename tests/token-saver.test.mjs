import test from 'node:test';
import assert from 'node:assert/strict';

const memory = new Map();
globalThis.localStorage = { getItem: key => memory.get(key) ?? null, setItem: (key, value) => memory.set(key, String(value)), removeItem: key => memory.delete(key) };
const { REMIND_EVERY, contextFor, defaults, estimate, guidance, setDefaults, threadSaver, updateThread } = await import('../src/token-saver.ts');
const noRtk = { rtk: false }, withRtk = { rtk: true };

test('guidance rides along when it changes and as a periodic reminder, never every message', () => {
  memory.clear();
  assert.equal(contextFor('t1', noRtk), '', 'nothing is sent while the saver is off');
  updateThread('t1', { packs: ['caveman'], level: 'full' });
  const first = contextFor('t1', noRtk);
  assert.match(first, /^\n\nResponse style \((?:PandaCode|Swarm) token saver/);
  assert.match(first, /Respond like smart caveman/);
  assert.match(first, /plain full sentences for security warnings/);
  for (let i = 1; i < REMIND_EVERY; i++) assert.equal(contextFor('t1', noRtk), '', `message ${i} stays quiet`);
  assert.notEqual(contextFor('t1', noRtk), '', 'then it is restated');
  updateThread('t1', { level: 'lite' });
  assert.match(contextFor('t1', noRtk), /Answer first\. No greeting/);
  updateThread('t1', { packs: [] });
  assert.match(contextFor('t1', noRtk), /: off\. Answer in your normal style again\./);
  assert.equal(contextFor('t1', noRtk), '', 'the switch-off is said once');
});

test('RTK guidance needs rtk installed, and packs combine in one block', () => {
  memory.clear();
  updateThread('t2', { packs: ['rtk'] });
  assert.equal(contextFor('t2', noRtk), '', 'without rtk the pack is inert');
  assert.match(contextFor('t2', withRtk), /Prefix shell commands with rtk/);
  updateThread('t2', { packs: ['lean-code', 'quiet-tools', 'rtk'] });
  const block = guidance(['lean-code', 'quiet-tools', 'rtk'], 'full', withRtk);
  assert.match(block, /least code that fully solves/);
  assert.match(block, /rg -n pattern path/);
  assert.doesNotMatch(block, /caveman/i);
});

test('threads start from the saved default and keep their own changes', () => {
  memory.clear();
  assert.deepEqual(threadSaver('fresh').packs, []);
  setDefaults({ packs: ['caveman', 'quiet-tools'], level: 'ultra' });
  assert.deepEqual(threadSaver('new-thread').packs, ['caveman', 'quiet-tools']);
  assert.equal(threadSaver('new-thread').level, 'ultra');
  updateThread('new-thread', { packs: ['lean-code'] });
  assert.deepEqual(threadSaver('new-thread').packs, ['lean-code']);
  assert.deepEqual(defaults().packs, ['caveman', 'quiet-tools']);
  setDefaults(null);
  assert.deepEqual(threadSaver('another').packs, []);
});

test('the estimate compounds, ignores unavailable tools, and stays plausible', () => {
  assert.equal(estimate([], 'full', noRtk), 0);
  assert.equal(estimate(['rtk'], 'full', noRtk), 0);
  const all = estimate(['caveman', 'lean-code', 'quiet-tools', 'rtk'], 'ultra', withRtk);
  assert.ok(all > 50 && all < 75, String(all));
  assert.ok(estimate(['caveman'], 'lite', noRtk) < estimate(['caveman'], 'ultra', noRtk));
});
