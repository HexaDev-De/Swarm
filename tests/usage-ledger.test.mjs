import test from 'node:test';
import assert from 'node:assert/strict';
import { averageLimit, calendarFor, dayKey, formatShare, formatUsd, lastDays, summarize } from '../src/usage-ledger.ts';
import { niceTicks, smoothPath } from '../src/usage-cost-chart.ts';

const today = new Date(2026, 8, 26, 15);
const hourOn = (daysBack, hour = 14) => Math.floor(new Date(2026, 8, 26 - daysBack, hour).getTime() / 3_600_000);
const ledger = {
  scannedAt: 0, pricing: { source: 'bundled', fetchedAt: null }, sources: [{ provider: 'claude', files: 2 }, { provider: 'codex', files: 1 }],
  models: [{ provider: 'claude', name: 'claude-opus-5-5', priced: true }, { provider: 'codex', name: 'gpt-6-astra', priced: true }, { provider: 'codex', name: 'fugu', priced: false }],
  rows: [
    [hourOn(0), 0, 10, 900, 50, 40, 3, 1],
    [hourOn(0, 9), 1, 20, 100, 0, 30, 2, 0.5],
    [hourOn(3), 2, 100, 0, 0, 0, 0, 0],
    [hourOn(20), 0, 5, 5, 0, 0, 1, 0],
    [hourOn(200), 1, 1, 1, 1, 1, 10, 0],
  ],
  sessions: [{ provider: 'claude', first: hourOn(20), last: hourOn(0) }, { provider: 'codex', first: hourOn(200), last: hourOn(200) }, { provider: 'codex', first: hourOn(3), last: hourOn(0) }],
};

test('ranges cover whole local days ending today', () => {
  assert.deepEqual(lastDays(3, today), ['2026-09-24', '2026-09-25', '2026-09-26']);
  assert.equal(dayKey(new Date(2026, 0, 5)), '2026-01-05');
});

test('a week sums cost, tokens, sessions and unpriced tokens only inside the range', () => {
  const week = summarize(ledger, 'week', today);
  assert.equal(week.totals.cost, 5);
  assert.equal(week.totals.tokens, 1000 + 150 + 100);
  assert.equal(week.totals.unpricedTokens, 100);
  assert.equal(week.totals.cacheRead, 1000);
  assert.equal(week.totals.savings, 1.5);
  assert.equal(week.sessions, 2);
  assert.deepEqual(week.agents.map(agent => [agent.provider, agent.cost, agent.sessions]), [['claude', 3, 1], ['codex', 2, 1]]);
  assert.deepEqual(week.models.map(model => model.model), ['claude-opus-5-5', 'gpt-6-astra', 'fugu']);
  assert.equal(week.series.find(line => line.provider === 'claude').values.at(-1), 3);
  assert.equal(week.dailyCost.at(-1).cost, 5);
});

test('a year groups cost into weeks that end today', () => {
  const year = summarize(ledger, 'year', today);
  assert.equal(year.totals.cost, 16);
  assert.equal(year.buckets.length, Math.ceil(365 / 7));
  assert.equal(year.buckets.at(-1).end, '2026-09-26');
  assert.equal(year.series.find(line => line.provider === 'codex').values.reduce((a, b) => a + b, 0), 12);
});

test('calendars use log tokens, fall back to PandaCode prompts, and are absent without data', () => {
  const prompts = [[hourOn(1), 'kimi', 3], [hourOn(1, 8), 'kimi', 2], [hourOn(1), 'claude', 9]];
  assert.equal(calendarFor('claude', ledger, prompts).unit, 'tokens');
  assert.deepEqual(calendarFor('kimi', ledger, prompts), { unit: 'prompts', daily: [{ date: '2026-09-25', tokens: 5 }] });
  assert.equal(calendarFor('grok', ledger, prompts), null);
});

test('average limit ignores agents without a reported window', () => {
  assert.equal(averageLimit([22, 68]), 45);
  assert.equal(averageLimit([]), null);
  assert.equal(averageLimit([Number.NaN, 30]), 30);
});

test('money and shares format for small and zero values', () => {
  assert.equal(formatUsd(6146.271), '$6,146.27');
  assert.equal(formatUsd(0.004), '<$0.01');
  assert.equal(formatShare(1, 5000), '<0.1%');
  assert.equal(formatShare(0, 0), '0%');
});

test('axis ticks are round and the smoothed line never dips below the baseline', () => {
  assert.deepEqual(niceTicks(2600), [0, 1000, 2000, 3000]);
  assert.deepEqual(niceTicks(0), [0]);
  const path = smoothPath([[0, 180], [100, 20], [200, 180], [300, 180], [400, 60]]);
  const ys = [...path.matchAll(/-?\d+(?:\.\d+)?,(-?\d+(?:\.\d+)?)/g)].map(match => Number(match[1]));
  assert.ok(ys.every(y => y <= 180 && y >= 20), `control points stay within the data: ${ys}`);
});
