import test from 'node:test';
import assert from 'node:assert/strict';
import { age, issuePrompt, pullPrompt, threadTitle, PULL_ACTIONS } from '../src/github-hub-prompts.ts';

const pull = { number: 130, title: 'Debounce search', head: 'feat/search', base: 'main' };

test('every pull request action names the pull request, so its context is attached', () => {
  for (const action of ['review', 'checks', 'feedback', 'merge', 'thread']) assert.match(pullPrompt(action, pull), /#130\b/);
  assert.match(pullPrompt('review', pull), /gh pr checkout 130/);
  assert.match(pullPrompt('merge', pull), /don't merge/);
  assert.deepEqual(PULL_ACTIONS.map(item => item.id), ['thread', 'checks', 'feedback', 'merge']);
});

test('issues are solved on a branch, or opened with the prompt ready to finish', () => {
  assert.match(issuePrompt('solve', { number: 128, title: 'Slow search' }), /^Fix issue #128: Slow search\./);
  assert.equal(issuePrompt('thread', { number: 3, title: 'Frost dates' }), "Let's work on issue #3: ");
});

test('thread titles say what the thread is for and fit the sidebar', () => {
  assert.equal(threadTitle('issue', 'solve', { number: 128, title: 'Slow search' }), 'Fix #128: Slow search');
  assert.equal(threadTitle('pull', 'review', pull), 'Review #130: Debounce search');
  assert.ok(threadTitle('pull', 'merge', { number: 1, title: 'x'.repeat(300) }).length <= 120);
});

test('ages read like a tracker: minutes, hours, days', () => {
  const now = Date.parse('2026-10-18T12:00:00Z');
  assert.equal(age('2026-10-18T11:59:40Z', now), 'now');
  assert.equal(age('2026-10-18T11:35:00Z', now), '25m');
  assert.equal(age('2026-10-18T07:00:00Z', now), '5h');
  assert.equal(age('2026-10-15T12:00:00Z', now), '3d');
  assert.equal(age('not a date', now), '');
});

test('the connect screen says what to do for the error gh gave', async () => {
  const { connectAdvice } = await import('../src/github-hub-prompts.ts');
  assert.match(connectAdvice('Failed to start GitHub CLI: No such file or directory (os error 2)'), /brew install gh/);
  assert.match(connectAdvice('To get started with GitHub CLI, please run:  gh auth login'), /gh auth login/);
  assert.match(connectAdvice('error connecting to api.github.com'), /Check your connection/);
});

test('repositories pushed in the last 30 days are recent; older or undated ones go under Other', async () => {
  const { isRecent } = await import('../src/github-hub-prompts.ts');
  const now = Date.parse('2026-10-18T12:00:00Z');
  assert.equal(isRecent('2026-10-18T11:00:00Z', now), true);
  assert.equal(isRecent('2026-09-19T12:00:01Z', now), true);
  assert.equal(isRecent('2026-09-18T11:59:59Z', now), false);
  assert.equal(isRecent('', now), false);
});
