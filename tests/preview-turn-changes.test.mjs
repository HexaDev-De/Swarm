import test from 'node:test';
import assert from 'node:assert/strict';
import { createPreviewTurnChanges } from '../src/preview-turn-changes.ts';

function fixture() {
  const data = new Map();
  const storage = { getItem: key => data.get(key) ?? null, setItem: (key, value) => data.set(key, value) };
  return { storage, changes: createPreviewTurnChanges(storage) };
}
const message = (sessionId = 'task', id = 1) => ({ id, sessionId, role: 'user', content: 'Sample request', createdAt: 1 });

test('preview changes retain an immutable per-turn snapshot and enforce detail ownership', () => {
  const { changes } = fixture();
  const user = message(); changes.finish(user);
  const original = structuredClone(user.turnChanges);
  assert.equal(original.files.length, 3);
  assert.equal(original.additions, 6); assert.equal(original.deletions, 2);
  const path = original.files[0].path;
  const detail = changes.diff('task', original.id, path);
  assert.match(detail.patch, /^diff --git /);
  assert.throws(() => changes.diff('other', original.id, path), /not found|belong|available/);
  assert.throws(() => changes.diff('task', original.id, '../outside'), /not found|belong|available/);
  detail.patch = 'tampered'; user.turnChanges.files.length = 0;
  const restored = message(); changes.finish(restored);
  assert.deepEqual(restored.turnChanges, original);
  assert.notEqual(changes.diff('task', original.id, path).patch, 'tampered');
});

test('preview change summaries and patches survive preview reload without reading current files', () => {
  const { changes, storage } = fixture();
  const user = message(); changes.finish(user);
  const before = changes.diff('task', user.turnChanges.id, 'src/components/search.ts');
  const reloaded = createPreviewTurnChanges(storage), saved = message(); reloaded.finish(saved);
  assert.deepEqual(saved.turnChanges, user.turnChanges);
  assert.deepEqual(reloaded.diff('task', saved.turnChanges.id, before.path), before);
  const next = message('task', 2); reloaded.finish(next);
  assert.notEqual(next.turnChanges.id, saved.turnChanges.id);
});

test('preview change storage is bounded and rejects unavailable snapshots explicitly', () => {
  const { changes, storage } = fixture();
  let oldest;
  for (let id = 1; id <= 60; id++) { const user = message('task', id); changes.finish(user); oldest ??= user.turnChanges; }
  assert.throws(() => changes.diff('task', oldest.id, oldest.files[0].path), /not found|available/);
  assert(storage.getItem('pandacode.preview.turn-changes').length < 256 * 1024);
  assert.throws(() => changes.diff('task', 'unknown', 'README.md'), /not found|available/);
});

test('preview changes attach to autonomous goal markers but not assistant prose', () => {
  const { changes } = fixture();
  const marker = { ...message(), role: 'changes', content: '' }; changes.finish(marker);
  assert.equal(marker.turnChanges.files.length, 3);
  const assistant = { ...message('task', 2), role: 'assistant' }; changes.finish(assistant);
  assert.equal(assistant.turnChanges, undefined);
});
