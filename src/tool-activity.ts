import type { ToolActivity } from './types';
import { escape, icon } from './dom.ts';

const MAX_CONTENT = 100_000;
const MAX_DETAIL = 20_000;
const MAX_LABEL = 256;
const MAX_ID = 512;
const statuses = { running: 'Running', completed: 'Completed', failed: 'Failed' } as const;

/** Read the versioned record without treating arbitrary JSON or older labels as a tool. */
export function decodeToolActivity(content: string): ToolActivity | undefined {
  if (content.length > MAX_CONTENT || !content.trimStart().startsWith('{')) return;
  try {
    const value: unknown = JSON.parse(content);
    if (!value || typeof value !== 'object' || Array.isArray(value)) return;
    const record = value as Record<string, unknown>;
    if (record.type !== 'tool_activity' || record.version !== 1
      || typeof record.id !== 'string' || record.id.length > MAX_ID
      || typeof record.name !== 'string' || record.name.length > MAX_LABEL
      || (record.status !== 'running' && record.status !== 'completed' && record.status !== 'failed')
      || typeof record.input !== 'string' || record.input.length > MAX_DETAIL
      || typeof record.output !== 'string' || record.output.length > MAX_DETAIL) return;
    return { id: record.id, name: record.name, status: record.status, input: record.input, output: record.output };
  } catch {
    return;
  }
}

function bounded(value: string, limit = MAX_DETAIL): string {
  return value.length > limit ? value.slice(0, limit) + '\n[Display truncated]' : value;
}

function inputHint(input: string): string {
  const value = input.slice(0, MAX_DETAIL).trim();
  let hint = value.split('\n', 1)[0] ?? '';
  if (value.startsWith('{')) {
    try {
      const record = JSON.parse(value) as Record<string, unknown>;
      const field = record.command ?? record.cmd ?? record.file_path ?? record.filePath ?? record.path ?? record.query;
      hint = typeof field === 'string' ? field : Array.isArray(field) && field.every(part => typeof part === 'string') ? field.join(' ') : '';
    } catch { hint = ''; }
  }
  hint = hint.replace(/\s+/g, ' ').trim();
  return hint.length > 96 ? hint.slice(0, 95) + '…' : hint;
}

function section(kind: 'input' | 'output', content: string): string {
  const label = kind === 'input' ? 'Input' : 'Output';
  return `<section class="tool-activity-section" data-tool-section="${kind}"><h4>${label}</h4><pre tabindex="0" aria-label="Tool ${kind}">${escape(bounded(content))}</pre></section>`;
}

/** Caller owns the stable key and restores native details.open when replacing activity rows. */
export function renderToolActivity(content: string | ToolActivity, key: string): string {
  const tool = typeof content === 'string' ? decodeToolActivity(content) : content;
  if (!tool) return renderLegacyTool(content as string, key);
  const name = tool.name.slice(0, MAX_LABEL) || 'Tool';
  const hint = inputHint(tool.input);
  const body = (tool.input.trim() ? section('input', tool.input) : '')
    + (tool.output.trim() ? section('output', tool.output) : `<p class="tool-activity-note">${tool.status === 'running'
      ? tool.input.trim() ? 'Waiting for output…' : 'Waiting for tool details…'
      : tool.status === 'completed' ? 'No output was returned.' : 'No output was recorded.'}</p>`);
  return `<details class="tool-activity" data-tool-key="${escape(key)}" data-tool-status="${escape(tool.status)}"><summary>${icon(tool.status === 'failed' ? 'alert' : tool.status === 'completed' ? 'check' : 'terminal')}<span class="tool-activity-name" title="${escape(name)}">${escape(name)}</span>${hint ? `<span class="tool-activity-hint">${escape(hint)}</span>` : ''}<span class="tool-activity-status">${escape(statuses[tool.status])}</span>${icon('chevron')}</summary><div class="tool-activity-body">${body}</div></details>`;
}

function renderLegacyTool(content: string, key: string): string {
  const text = bounded(content).trim();
  const newline = text.indexOf('\n');
  const name = bounded(newline < 0 ? text : text.slice(0, newline), MAX_LABEL) || 'Tool activity';
  const remainder = newline < 0 ? '' : text.slice(newline + 1).trim();
  const body = remainder
    ? `<pre class="tool-activity-legacy-body" tabindex="0" aria-label="Recorded tool details">${escape(remainder)}</pre>`
    : '<p class="tool-activity-note">Details were not recorded for this earlier tool call.</p>';
  return `<details class="tool-activity" data-tool-key="${escape(key)}"><summary>${icon('terminal')}<span class="tool-activity-name">${escape(name)}</span>${icon('chevron')}</summary><div class="tool-activity-body">${body}</div></details>`;
}

/** Generic status messages are plain rows, not empty expandable tool calls. */
export function renderToolStatus(content: string): string {
  return `<div class="tool-activity-status-row">${icon('terminal')}<span>${escape(bounded(content))}</span></div>`;
}

export type ToolRun<T> = { kind: 'tools'; key: string; items: T[] } | { kind: 'message'; item: T };

/** Narration/status rows are boundaries; each group keeps its first call's stable key. */
export function groupToolRuns<T>(entries: T[], toolKey: (entry: T) => string | undefined): ToolRun<T>[] {
  const runs: ToolRun<T>[] = [];
  for (const entry of entries) {
    const key = toolKey(entry);
    if (key === undefined) { runs.push({ kind: 'message', item: entry }); continue; }
    const previous = runs[runs.length - 1];
    if (previous?.kind === 'tools' && previous.items.length < 5) previous.items.push(entry);
    else runs.push({ kind: 'tools', key, items: [entry] });
  }
  return runs;
}

export function renderToolGroup(entries: { key: string; content: string | ToolActivity }[]): string {
  if (!entries.length) return '';
  const tools = entries.map(entry => typeof entry.content === 'string' ? decodeToolActivity(entry.content) : entry.content);
  const running = tools.filter(tool => tool?.status === 'running').length;
  const failed = tools.filter(tool => tool?.status === 'failed').length;
  const categories = new Set<string>();
  for (const tool of tools) {
    const name = tool?.name.toLowerCase() ?? '';
    if (/bash|shell|exec|command|terminal/.test(name)) categories.add('Commands');
    else if (/edit|write|patch/.test(name)) categories.add('Edits');
    else if (/read|file|glob|list.*dir/.test(name)) categories.add('Files');
    else if (/search|grep|find/.test(name)) categories.add('Search');
    else if (/browser|navigate|screenshot/.test(name)) categories.add('Browser');
  }
  const label = `${entries.length} ${entries.length === 1 ? 'tool' : 'tools'}`;
  const status = [running ? `${running} running` : '', failed ? `${failed} failed` : ''].filter(Boolean).join(' · ');
  const hint = categories.size ? `<span class="tool-group-hint"> · ${escape([...categories].join(', '))}</span>` : '';
  return `<details class="tool-group" data-tool-group-key="${escape('group:' + entries[0].key)}"><summary class="tool-group-summary">${icon(failed ? 'alert' : 'terminal')}<span class="tool-group-label">${label}</span>${hint}${status ? `<span class="tool-group-status${failed ? ' has-failure' : ''}"> · ${status}</span>` : ''}${icon('chevron')}</summary><div class="tool-group-body">${entries.map(entry => renderToolActivity(entry.content, entry.key)).join('')}</div></details>`;
}
