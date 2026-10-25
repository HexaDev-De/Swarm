/** Estimated API cost from local agent logs, grouped into the viewer's local calendar days. */
import type { ProviderId } from './types';

/** `[unixHour, modelIndex, input, cacheRead, cacheWrite, output, costUsd, savingsUsd]` */
export type LedgerRow = [number, number, number, number, number, number, number, number];
export interface Ledger {
  scannedAt: number;
  pricing: { source: 'live' | 'saved' | 'bundled'; fetchedAt: number | null };
  sources: { provider: ProviderId; files: number }[];
  models: { provider: ProviderId; name: string; priced: boolean }[];
  rows: LedgerRow[];
  sessions: { provider: ProviderId; first: number; last: number }[];
}
/** `[unixHour, provider, prompts]` sent from PandaCode. */
export type PromptActivity = [number, ProviderId, number][];

export type Range = 'week' | 'month' | 'year';
export const RANGE_DAYS: Record<Range, number> = { week: 7, month: 30, year: 365 };
export const RANGE_LABEL: Record<Range, string> = { week: 'Past 7 days', month: 'Past 30 days', year: 'Past year' };

export interface Totals { input: number; cacheRead: number; cacheWrite: number; output: number; cost: number; savings: number; tokens: number; unpricedTokens: number; }
export interface AgentCost extends Totals { provider: ProviderId; sessions: number; }
export interface ModelCost { provider: ProviderId; model: string; priced: boolean; cost: number; tokens: number; }
export interface Summary {
  range: Range;
  days: string[];
  totals: Totals;
  sessions: number;
  agents: AgentCost[];
  models: ModelCost[];
  /** Cost per agent for each entry in `days` (or each week for a year). */
  series: { provider: ProviderId; values: number[] }[];
  buckets: { start: string; end: string }[];
  dailyCost: { date: string; cost: number; tokens: number }[];
}

const HOUR_MS = 3_600_000;
export function dayKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}
export function hourDay(hour: number): string { return dayKey(new Date(hour * HOUR_MS)); }
function emptyTotals(): Totals { return { input: 0, cacheRead: 0, cacheWrite: 0, output: 0, cost: 0, savings: 0, tokens: 0, unpricedTokens: 0 }; }
function addRow(totals: Totals, row: LedgerRow, priced: boolean): Totals {
  const tokens = row[2] + row[3] + row[4] + row[5];
  return {
    input: totals.input + row[2], cacheRead: totals.cacheRead + row[3], cacheWrite: totals.cacheWrite + row[4], output: totals.output + row[5],
    cost: totals.cost + row[6], savings: totals.savings + row[7], tokens: totals.tokens + tokens, unpricedTokens: totals.unpricedTokens + (priced ? 0 : tokens),
  };
}

/** The last `count` local days ending today, oldest first. */
export function lastDays(count: number, today = new Date()): string[] {
  return Array.from({ length: count }, (_, i) => dayKey(new Date(today.getFullYear(), today.getMonth(), today.getDate() - (count - 1 - i))));
}

/** Everything the overview and agent tabs show for one range, computed once per render. */
export function summarize(ledger: Ledger, range: Range, today = new Date()): Summary {
  const days = lastDays(RANGE_DAYS[range], today);
  const dayIndex = new Map(days.map((day, i) => [day, i]));
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate() - (days.length - 1)).getTime() / HOUR_MS;
  const bucketSize = range === 'year' ? 7 : 1;
  const bucketCount = Math.ceil(days.length / bucketSize);
  const bucketOf = (index: number) => Math.max(0, bucketCount - 1 - Math.floor((days.length - 1 - index) / bucketSize));
  const agentTotals = new Map<ProviderId, Totals>();
  const modelTotals = new Map<number, { cost: number; tokens: number }>();
  const series = new Map<ProviderId, number[]>();
  const daily = days.map(date => ({ date, cost: 0, tokens: 0 }));
  let totals = emptyTotals();
  for (const row of ledger.rows) {
    const index = dayIndex.get(hourDay(row[0]));
    const model = ledger.models[row[1]];
    if (index === undefined || !model) continue;
    totals = addRow(totals, row, model.priced);
    agentTotals.set(model.provider, addRow(agentTotals.get(model.provider) ?? emptyTotals(), row, model.priced));
    const previous = modelTotals.get(row[1]) ?? { cost: 0, tokens: 0 };
    modelTotals.set(row[1], { cost: previous.cost + row[6], tokens: previous.tokens + row[2] + row[3] + row[4] + row[5] });
    const values = series.get(model.provider) ?? Array<number>(bucketCount).fill(0);
    values[bucketOf(index)] += row[6]; series.set(model.provider, values);
    daily[index] = { ...daily[index], cost: daily[index].cost + row[6], tokens: daily[index].tokens + row[2] + row[3] + row[4] + row[5] };
  }
  const inRange = ledger.sessions.filter(session => session.last >= start);
  const agents = [...agentTotals].map(([provider, value]) => ({ ...value, provider, sessions: inRange.filter(session => session.provider === provider).length }))
    .sort((a, b) => b.cost - a.cost || b.tokens - a.tokens);
  const models = [...modelTotals].map(([index, value]) => ({ ...value, provider: ledger.models[index].provider, model: ledger.models[index].name, priced: ledger.models[index].priced }))
    .sort((a, b) => b.cost - a.cost || b.tokens - a.tokens);
  const buckets = Array.from({ length: bucketCount }, (_, bucket) => {
    const last = Math.min(days.length - 1, days.length - 1 - (bucketCount - 1 - bucket) * bucketSize);
    return { start: days[Math.max(0, last - bucketSize + 1)], end: days[last] };
  });
  return { range, days, totals, sessions: inRange.length, agents, models, series: [...series].map(([provider, values]) => ({ provider, values })), buckets, dailyCost: daily };
}

/** Daily totals for one agent's year calendar: tokens from logs when it has any, else PandaCode prompts. */
export function calendarFor(provider: ProviderId, ledger: Ledger | null, prompts: PromptActivity | null): { unit: 'tokens' | 'prompts'; daily: { date: string; tokens: number }[] } | null {
  const add = (map: Map<string, number>, date: string, value: number) => map.set(date, (map.get(date) ?? 0) + value);
  const tokens = new Map<string, number>();
  for (const row of ledger?.rows ?? []) if (ledger!.models[row[1]]?.provider === provider) add(tokens, hourDay(row[0]), row[2] + row[3] + row[4] + row[5]);
  if (tokens.size) return { unit: 'tokens', daily: [...tokens].map(([date, value]) => ({ date, tokens: value })) };
  const sent = new Map<string, number>();
  for (const [hour, owner, count] of prompts ?? []) if (owner === provider) add(sent, hourDay(hour), count);
  return sent.size ? { unit: 'prompts', daily: [...sent].map(([date, value]) => ({ date, tokens: value })) } : null;
}

/** Average of each reporting agent's most-used limit window. */
export function averageLimit(peaks: readonly number[]): number | null {
  const valid = peaks.filter(Number.isFinite);
  return valid.length ? valid.reduce((sum, value) => sum + value, 0) / valid.length : null;
}

const usd = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', minimumFractionDigits: 2, maximumFractionDigits: 2 });
const usdCompact = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', notation: 'compact', maximumFractionDigits: 1 });
const compact = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 });
const share = new Intl.NumberFormat(undefined, { style: 'percent', maximumFractionDigits: 1 });
export function formatUsd(value: number): string { return value > 0 && value < 0.01 ? '<$0.01' : usd.format(value); }
export function formatUsdAxis(value: number): string { return value >= 1000 ? usdCompact.format(value) : usd.format(value).replace(/\.00$/, ''); }
export function formatTokens(value: number): string { return compact.format(value); }
export function formatShare(part: number, whole: number): string {
  if (!whole || !part) return '0%';
  const ratio = part / whole;
  return ratio < 0.001 ? '<0.1%' : share.format(ratio);
}
