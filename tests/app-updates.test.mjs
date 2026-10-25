import test from 'node:test';
import assert from 'node:assert/strict';

// The pure parts of the updater: when a downloaded update may restart the app, and what About says.
const { readyToRestart, describeUpdate, IDLE_BEFORE_RESTART } = await import('../src/app-updates-policy.ts');

test('a downloaded update restarts only when no agent works and the user stepped away', () => {
  const now = 10_000_000;
  assert.equal(readyToRestart(false, now - IDLE_BEFORE_RESTART, now), true);
  assert.equal(readyToRestart(false, now - IDLE_BEFORE_RESTART + 1, now), false);
  assert.equal(readyToRestart(true, now - 60 * 60_000, now), false);
});

test('About explains each state in one line', () => {
  const now = Date.parse('2026-10-18T12:00:00Z');
  assert.equal(describeUpdate({ kind: 'current', checkedAt: now - 5 * 60_000 }, true, now), 'Up to date · checked 5m ago');
  assert.equal(describeUpdate({ kind: 'downloading', version: '0.3.1', percent: 42 }, true), 'Downloading 0.3.1 · 42%');
  assert.match(describeUpdate({ kind: 'ready', version: '0.3.1' }, true), /^0\.3\.1 is ready\. (?:PandaCode|Swarm) restarts when your agents are done/);
  assert.equal(describeUpdate({ kind: 'ready', version: '0.3.1' }, false), '0.3.1 is ready. Restart to finish.');
  assert.equal(describeUpdate({ kind: 'idle' }, false), 'Automatic updates are off.');
  assert.match(describeUpdate({ kind: 'error', message: 'offline' }, true), /offline$/);
});
