import { call, subscribe } from './bridge.ts';
import { icon, toast, escape } from './dom.ts';
import { markdownHtml } from './dom.ts';
import { streamMarkdown } from './streaming-markdown.ts';
import { resolveWallpaperUrl } from './wallpapers.ts';
import type { Session, Provider } from './types.ts';

export interface ArenaMatchup {
  id: string;
  sourceSessionId: string;
  taskA: Session;
  taskB: Session;
  prompt: string;
  createdAt: number;
}

export interface ArenaFileStat {
  path: string;
  additions: number;
  deletions: number;
}

export interface ArenaCompetitorFinished {
  arenaId: string;
  competitor: 'a' | 'b';
  sessionId: string;
  responseText: string;
  diff: string;
  files: ArenaFileStat[];
  previewFile?: string;
  success: boolean;
}

export interface ArenaVerdictResult {
  arenaId: string;
  winnerId: string;
  winnerBranch: string;
  winnerMerged: boolean;
  loserCleaned: boolean;
  message: string;
}

let activeMatchup: ArenaMatchup | null = null;

export function getActiveMatchup(): ArenaMatchup | null {
  return activeMatchup;
}

function renderModelOptions(providers?: Provider[], defaultProvider?: string, defaultModel?: string): string {
  if (providers && providers.length) {
    return providers.map(p => {
      const opts = (p.models && p.models.length ? p.models : [{ id: 'default', name: 'Default Model' }]).map(m => {
        const val = `${p.id}:${m.id}`;
        const isSelected = (defaultProvider === p.id && (defaultModel === m.id || (!defaultModel && m.id === 'default')));
        return `<option value="${escape(val)}" ${isSelected ? 'selected' : ''}>${escape(m.name || m.id)}</option>`;
      }).join('');
      return `<optgroup label="${escape(p.name)}">${opts}</optgroup>`;
    }).join('');
  }
  return `
    <optgroup label="Claude Code">
      <option value="claude:claude-opus-5.5" ${defaultProvider === 'claude' ? 'selected' : ''}>Claude Opus 5.5</option>
      <option value="claude:claude-sonnet-5.5">Claude Sonnet 5.5</option>
      <option value="claude:claude-fable-5.1">Claude Fable 5.1</option>
    </optgroup>
    <optgroup label="Codex">
      <option value="codex:gpt-6-astra" ${defaultProvider === 'codex' ? 'selected' : ''}>GPT 6 Astra</option>
      <option value="codex:gpt-6.1-sol">GPT-6.1 Sol</option>
      <option value="codex:gpt-6-sol">GPT-6 Sol</option>
    </optgroup>
    <optgroup label="Antigravity">
      <option value="antigravity:gemini-3.8-flash" ${defaultProvider === 'antigravity' ? 'selected' : ''}>Gemini 3.8 Flash</option>
      <option value="antigravity:gemini-3-flash">Gemini 3.x Flash</option>
      <option value="antigravity:gemini-2.0-pro">Gemini 2.0 Pro</option>
    </optgroup>
    <optgroup label="Kimi Code">
      <option value="kimi:kimi-k3.1" ${defaultProvider === 'kimi' ? 'selected' : ''}>Kimi k3.1</option>
      <option value="kimi:kimi-k3">Kimi k3</option>
    </optgroup>
    <optgroup label="DeepSeek">
      <option value="deepseek:deepseek-v4.1-flash" ${defaultProvider === 'deepseek' ? 'selected' : ''}>DeepSeek V4.1 flash</option>
      <option value="deepseek:deepseek-v4">DeepSeek V4</option>
    </optgroup>
    <optgroup label="GLM Coding Plan">
      <option value="glm:glm-5.3">GLM-5.3</option>
      <option value="glm:glm-5.3-flash">GLM-5.3-Flash</option>
    </optgroup>
    <optgroup label="xAI Grok Build">
      <option value="grok:grok-4.7">xAI Grok 4.7</option>
      <option value="grok:grok-4.6">xAI Grok 4.6</option>
    </optgroup>
  `;
}

export function openArenaSetupModal(options: {
  sourceSession: Session;
  currentPrompt: string;
  providers?: Provider[];
  onStart(matchup: ArenaMatchup): void;
}): void {
  const overlay = document.createElement('div');
  overlay.className = 'arena-modal-overlay';

  const defaultAProvider = options.sourceSession.provider || 'antigravity';
  const defaultAModel = options.sourceSession.model || '';

  let defaultBProvider = 'claude';
  let defaultBModel = 'claude-3-7-sonnet';
  if (defaultAProvider === 'claude') {
    defaultBProvider = 'antigravity';
    defaultBModel = 'gemini-3.8-flash';
  }

  const optionsA = renderModelOptions(options.providers, defaultAProvider, defaultAModel);
  const optionsB = renderModelOptions(options.providers, defaultBProvider, defaultBModel);

  overlay.innerHTML = `
    <div class="arena-modal" role="dialog" aria-label="Dual-Harness Arena Setup">
      <div class="arena-modal-header">
        <div class="arena-modal-title">
          ${icon('swords')}
          <span>Dual-Harness Arena Race</span>
        </div>
        <button type="button" class="icon-button" data-action="close" aria-label="Close">${icon('close')}</button>
      </div>

      <p style="font-size:13px;color:var(--muted,#94a3b8);margin:0;">
        Run two AI models head-to-head in parallel isolated git worktrees on the exact same task. Review live progress side-by-side and keep the champion with 1 click.
      </p>

      <div id="arena-error-box" style="display:none;padding:10px 14px;background:rgba(239,68,68,0.15);border:1px solid #ef4444;border-radius:6px;color:#fca5a5;font-size:12px;line-height:1.4;"></div>

      <div class="arena-competitors">
        <div class="arena-competitor-card">
          <span class="arena-competitor-label">Competitor A</span>
          <select id="arena-model-a" class="arena-select">
            ${optionsA}
          </select>
        </div>

        <div class="arena-vs">VS</div>

        <div class="arena-competitor-card">
          <span class="arena-competitor-label">Competitor B</span>
          <select id="arena-model-b" class="arena-select">
            ${optionsB}
          </select>
        </div>
      </div>

      <div style="display:flex;flex-direction:column;gap:6px;">
        <label style="font-size:12px;font-weight:600;color:var(--text,#fff);" for="arena-prompt-input">Competition Task Prompt:</label>
        <textarea id="arena-prompt-input" rows="3" class="arena-textarea" placeholder="Describe what both competitors must build...">${escape(options.currentPrompt || '')}</textarea>
      </div>

      <div style="display:flex;justify-content:flex-end;gap:10px;margin-top:8px;">
        <button type="button" class="secondary-button" data-action="close">Cancel</button>
        <button type="button" class="primary-button" id="arena-start-btn" style="background:var(--accent);color:var(--on-accent);font-weight:700;">
          ${icon('swords')}
          <span>Start 2x Arena Race</span>
        </button>
      </div>
    </div>
  `;

  document.body.appendChild(overlay);

  const close = () => overlay.remove();
  overlay.querySelectorAll('[data-action="close"]').forEach(b => b.addEventListener('click', close));

  const errorBox = overlay.querySelector('#arena-error-box') as HTMLDivElement;
  const showError = (msg: string) => {
    if (errorBox) {
      errorBox.textContent = msg;
      errorBox.style.display = 'block';
    } else {
      toast(msg);
    }
  };

  overlay.querySelector('#arena-start-btn')?.addEventListener('click', async () => {
    if (errorBox) errorBox.style.display = 'none';
    const valA = (overlay.querySelector('#arena-model-a') as HTMLSelectElement).value.split(':');
    const valB = (overlay.querySelector('#arena-model-b') as HTMLSelectElement).value.split(':');
    const prompt = (overlay.querySelector('#arena-prompt-input') as HTMLTextAreaElement).value.trim();

    if (!prompt) {
      showError('Please enter a task prompt for the arena race.');
      return;
    }

    const startBtn = overlay.querySelector('#arena-start-btn') as HTMLButtonElement;
    try {
      startBtn.disabled = true;
      startBtn.innerHTML = `${icon('spark')} <span>Spawning parallel worktrees...</span>`;

      const matchup = await call<ArenaMatchup>('arena_start', {
        sourceSessionId: options.sourceSession.id,
        competitorAProvider: valA[0],
        competitorAModel: valA[1],
        competitorBProvider: valB[0],
        competitorBModel: valB[1],
        prompt,
      });

      activeMatchup = matchup;
      close();
      toast('Dual-Harness Arena Race started in isolated worktrees!');
      options.onStart(matchup);
    } catch (err: any) {
      startBtn.disabled = false;
      startBtn.innerHTML = `${icon('swords')} <span>Start 2x Arena Race</span>`;
      const msg = err instanceof Error ? err.message : String(err);
      showError(`Could not start arena race: ${msg}`);
    }
  });
}

interface ToolCallState {
  id: string;
  name: string;
  input: string;
  output: string;
  running: boolean;
}

export interface CompetitorState {
  status: 'running' | 'finished' | 'failed';
  elapsedSeconds: number;
  responseText: string;
  toolCalls: ToolCallState[];
  diff: string;
  files: ArenaFileStat[];
  previewFile?: string;
  activeTab: 'response' | 'diff' | 'preview';
}

export interface MatchupRuntime {
  stateA: CompetitorState;
  stateB: CompetitorState;
  timerId: number | null;
  unlistenAgent: (() => void) | null;
  unlistenFinished: (() => void) | null;
  listenersStarted: boolean;
  activeContainer: HTMLElement | null;
  renderColumn?: (side: 'a' | 'b') => void;
  renderTab?: (side: 'a' | 'b') => void;
  updateTimer?: (side: 'a' | 'b') => void;
  rerenderCol?: (side: 'a' | 'b') => void;
}

const activeMatchupRuntimes = new Map<string, MatchupRuntime>();

export function getMatchupRuntime(matchupId: string): MatchupRuntime | undefined {
  return activeMatchupRuntimes.get(matchupId);
}

export function cleanupMatchup(matchupId: string): void {
  const runtime = activeMatchupRuntimes.get(matchupId);
  if (runtime) {
    if (runtime.timerId !== null) {
      clearInterval(runtime.timerId);
      runtime.timerId = null;
    }
    runtime.unlistenAgent?.();
    runtime.unlistenFinished?.();
    runtime.activeContainer = null;
    activeMatchupRuntimes.delete(matchupId);
  }
}

function formatElapsed(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
}

export function renderArenaSplitView(
  container: HTMLElement,
  matchup: ArenaMatchup,
  onVerdict: (winnerSessionId: string) => void
): () => void {
  let runtime = activeMatchupRuntimes.get(matchup.id);
  if (!runtime) {
    runtime = {
      stateA: {
        status: 'running',
        elapsedSeconds: 0,
        responseText: '',
        toolCalls: [],
        diff: '',
        files: [],
        activeTab: 'response',
      },
      stateB: {
        status: 'running',
        elapsedSeconds: 0,
        responseText: '',
        toolCalls: [],
        diff: '',
        files: [],
        activeTab: 'response',
      },
      timerId: null,
      unlistenAgent: null,
      unlistenFinished: null,
      listenersStarted: false,
      activeContainer: null,
    };
    activeMatchupRuntimes.set(matchup.id, runtime);
  }

  const stateA = runtime.stateA;
  const stateB = runtime.stateB;

  function renderLayout(): void {
    container.innerHTML = `
      <div class="arena-battleground">
        <!-- Race Banner Header -->
        <div class="arena-battleground-header">
          <div class="arena-matchup-meta">
            <span class="arena-badge">${icon('swords')} 2X ARENA RACE</span>
            <span class="arena-prompt-preview" title="${escape(matchup.prompt)}">${escape(matchup.prompt)}</span>
          </div>
          <div class="arena-header-actions">
            <button type="button" class="arena-discard-header-btn" data-action="discard-race" title="Cancel and discard both races">
              ${icon('trash')}
              <span>Discard Both</span>
            </button>
          </div>
        </div>

        <!-- 50/50 Dual Columns -->
        <div class="arena-split-container">
          <!-- Competitor A -->
          <div class="arena-column" id="arena-col-a"></div>

          <!-- Competitor B -->
          <div class="arena-column" id="arena-col-b"></div>
        </div>

        <!-- Floating Verdict Bar -->
        <div class="arena-verdict-floating-bar">
          <button type="button" class="arena-verdict-btn pick-a" data-action="pick-a">
            ${icon('trophy')}
            <span>Select Competitor A as Winner</span>
          </button>
          <button type="button" class="arena-verdict-btn discard" data-action="discard-race">
            ${icon('close')}
            <span>Discard Both</span>
          </button>
          <button type="button" class="arena-verdict-btn pick-b" data-action="pick-b">
            ${icon('trophy')}
            <span>Select Competitor B as Winner</span>
          </button>
        </div>
      </div>
    `;

    renderCompetitorColumn('a');
    renderCompetitorColumn('b');
    bindVerdictActions();
  }

  function renderCompetitorColumn(side: 'a' | 'b'): void {
    const col = container.querySelector<HTMLElement>(`#arena-col-${side}`);
    if (!col) return;

    const task = side === 'a' ? matchup.taskA : matchup.taskB;
    const compState = side === 'a' ? stateA : stateB;
    const label = side.toUpperCase();

    const statusPill = compState.status === 'running'
      ? `<span class="arena-status-pill running"><span class="arena-status-dot"></span>Running (${formatElapsed(compState.elapsedSeconds)})</span>`
      : compState.status === 'finished'
      ? `<span class="arena-status-pill finished">${icon('check')}Finished (${formatElapsed(compState.elapsedSeconds)})</span>`
      : `<span class="arena-status-pill failed">${icon('alert')}Failed</span>`;

    const totalAdditions = compState.files.reduce((acc, f) => acc + f.additions, 0);
    const totalDeletions = compState.files.reduce((acc, f) => acc + f.deletions, 0);
    const diffStatBadge = (totalAdditions > 0 || totalDeletions > 0)
      ? `<span class="arena-diff-stat-pill">+${totalAdditions} -${totalDeletions}</span>`
      : '';

    col.innerHTML = `
      <div class="arena-column-header">
        <div class="arena-competitor-info">
          <div class="arena-competitor-tag">
            <span class="arena-competitor-letter">${label}</span>
            <strong>${escape(task.provider.toUpperCase())}</strong>
            <span class="arena-model-name">${escape(task.model)}</span>
          </div>
          <div class="arena-branch-chip" title="Git worktree branch">
            ${icon('gitBranch')}
            <span>${escape(task.worktreeBranch || `arena-${side}`)}</span>
          </div>
        </div>
        ${statusPill}
      </div>

      <!-- Segmented Tabs -->
      <div class="arena-tab-bar">
        <button type="button" class="arena-tab-item ${compState.activeTab === 'response' ? 'active' : ''}" data-side="${side}" data-tab="response">
          ${icon('message')}
          <span>Response</span>
        </button>
        <button type="button" class="arena-tab-item ${compState.activeTab === 'diff' ? 'active' : ''}" data-side="${side}" data-tab="diff">
          ${icon('diff')}
          <span>Code Diff ${diffStatBadge}</span>
        </button>
        <button type="button" class="arena-tab-item ${compState.activeTab === 'preview' ? 'active' : ''}" data-side="${side}" data-tab="preview">
          ${icon('eye')}
          <span>Live Preview</span>
        </button>
      </div>

      <!-- Column Body Content -->
      <div class="arena-column-body" id="arena-body-${side}"></div>
    `;

    // Render active tab content
    renderTabContent(side);

    // Bind tab switching
    col.querySelectorAll<HTMLButtonElement>('.arena-tab-item').forEach(tabBtn => {
      tabBtn.addEventListener('click', () => {
        const tab = tabBtn.dataset.tab as 'response' | 'diff' | 'preview';
        compState.activeTab = tab;
        renderCompetitorColumn(side);
      });
    });
  }

  runtime.rerenderCol = renderCompetitorColumn;

  function renderToolsHtml(compState: CompetitorState): string {
    if (compState.toolCalls.length === 0) return '';
    return `
      <div class="arena-tool-calls-container">
        <div class="arena-tool-header">${icon('command')} Tool Executions (${compState.toolCalls.length})</div>
        ${compState.toolCalls.map(t => `
          <details class="arena-tool-details" ${t.running ? 'open' : ''}>
            <summary class="arena-tool-summary">
              <span class="arena-tool-name">${escape(t.name)}</span>
              ${t.running ? '<span class="arena-tool-running-tag">Executing...</span>' : '<span class="arena-tool-done-tag">Done</span>'}
            </summary>
            <div class="arena-tool-body">
              ${t.input ? `<pre class="arena-tool-pre"><code>${escape(t.input)}</code></pre>` : ''}
              ${t.output ? `<pre class="arena-tool-output"><code>${escape(t.output)}</code></pre>` : ''}
            </div>
          </details>
        `).join('')}
      </div>
    `;
  }

  function renderTabContent(side: 'a' | 'b'): void {
    const body = container.querySelector<HTMLElement>(`#arena-body-${side}`);
    if (!body) return;

    const compState = side === 'a' ? stateA : stateB;

    if (compState.activeTab === 'response') {
      const markdownContent = compState.responseText.trim();
      const showPlaceholder = !markdownContent && compState.status === 'running';

      body.innerHTML = `
        <div class="arena-response-scroll" id="arena-scroll-${side}">
          <div id="arena-tools-box-${side}">${renderToolsHtml(compState)}</div>
          ${showPlaceholder ? `
            <div class="arena-running-placeholder" id="arena-placeholder-${side}">
              <span class="arena-pulse-spinner">${icon('spark')}</span>
              <p>Agent running in isolated worktree...</p>
              <span class="arena-timer-text">${formatElapsed(compState.elapsedSeconds)}</span>
            </div>
          ` : ''}
          ${(!markdownContent && compState.status !== 'running') ? '<p class="arena-empty-text">No text response recorded.</p>' : ''}
          <div class="arena-markdown-body" id="arena-markdown-body-${side}"></div>
        </div>
      `;

      const mdTarget = body.querySelector<HTMLElement>(`#arena-markdown-body-${side}`);
      const scrollEl = body.querySelector<HTMLElement>(`#arena-scroll-${side}`);
      if (mdTarget && markdownContent) {
        streamMarkdown(mdTarget, compState.responseText, scrollEl || undefined);
      }
    } else if (compState.activeTab === 'diff') {
      if (!compState.diff.trim() && compState.files.length === 0) {
        body.innerHTML = `
          <div class="arena-empty-tab">
            ${icon('diff')}
            <p>${compState.status === 'running' ? 'Diff will be captured as files are modified...' : 'No git changes recorded for this turn.'}</p>
          </div>
        `;
        return;
      }

      const filesListHtml = compState.files.length > 0 ? `
        <div class="arena-diff-file-list">
          ${compState.files.map(f => `
            <div class="arena-diff-file-row">
              <span class="arena-file-path">${escape(f.path)}</span>
              <span class="arena-file-stat-adds">+${f.additions}</span>
              <span class="arena-file-stat-dels">-${f.deletions}</span>
            </div>
          `).join('')}
        </div>
      ` : '';

      const formattedDiffHtml = formatUnifiedDiff(compState.diff);

      body.innerHTML = `
        <div class="arena-diff-scroll">
          ${filesListHtml}
          <div class="arena-diff-viewer">${formattedDiffHtml}</div>
        </div>
      `;
    } else if (compState.activeTab === 'preview') {
      if (compState.previewFile) {
        const previewUrl = resolveWallpaperUrl(compState.previewFile);
        body.innerHTML = `
          <div class="arena-preview-container">
            <div class="arena-preview-topbar">
              <span>Preview: <code>${escape(compState.previewFile.split(/[/\\]/).pop() || '')}</code></span>
              <button type="button" class="icon-button" data-action="reload-preview" title="Reload preview">${icon('refresh')}</button>
            </div>
            <iframe class="arena-preview-frame" sandbox="allow-scripts allow-same-origin" src="${escape(previewUrl)}"></iframe>
          </div>
        `;

        body.querySelector('[data-action="reload-preview"]')?.addEventListener('click', () => {
          const iframe = body.querySelector<HTMLIFrameElement>('.arena-preview-frame');
          if (iframe) iframe.src = previewUrl;
        });
      } else {
        body.innerHTML = `
          <div class="arena-empty-tab">
            ${icon('monitor')}
            <p>${compState.status === 'running' ? 'Checking worktree for HTML preview entrypoint...' : 'No HTML entrypoint (index.html / preview.html) found in this worktree.'}</p>
          </div>
        `;
      }
    }
  }

  function formatUnifiedDiff(diff: string): string {
    if (!diff) return '<p class="arena-empty-diff">No diff content.</p>';
    const lines = diff.split('\n');
    const renderedLines = lines.map(line => {
      if (line.startsWith('+++') || line.startsWith('---')) {
        return `<div class="diff-line diff-file-header">${escape(line)}</div>`;
      } else if (line.startsWith('@@')) {
        return `<div class="diff-line diff-hunk">${escape(line)}</div>`;
      } else if (line.startsWith('+')) {
        return `<div class="diff-line diff-add">${escape(line)}</div>`;
      } else if (line.startsWith('-')) {
        return `<div class="diff-line diff-del">${escape(line)}</div>`;
      }
      return `<div class="diff-line diff-ctx">${escape(line)}</div>`;
    }).join('');

    return `<pre class="arena-diff-code"><code>${renderedLines}</code></pre>`;
  }

  function bindVerdictActions(): void {
    const handleVerdict = async (winnerId: string, loserId: string) => {
      try {
        const isDiscard = winnerId === 'discard';
        const res = await call<ArenaVerdictResult>('arena_verdict', {
          arenaId: matchup.id,
          winnerTaskId: winnerId,
          loserTaskId: loserId,
          discardTaskId: isDiscard ? matchup.taskA.id : null,
        });
        activeMatchup = null;
        cleanupMatchup(matchup.id);
        toast(isDiscard ? 'Arena race safely discarded.' : `Victory! ${res.message}`);
        window.dispatchEvent(new CustomEvent('pandacode-git-refresh'));
        onVerdict(winnerId);
      } catch (err) {
        toast(`Verdict failed: ${err instanceof Error ? err.message : String(err)}`);
      }
    };

    container.querySelector('[data-action="pick-a"]')?.addEventListener('click', () => {
      void handleVerdict(matchup.taskA.id, matchup.taskB.id);
    });

    container.querySelector('[data-action="pick-b"]')?.addEventListener('click', () => {
      void handleVerdict(matchup.taskB.id, matchup.taskA.id);
    });

    container.querySelectorAll('[data-action="discard-race"]').forEach(btn => {
      btn.addEventListener('click', () => {
        void handleVerdict('discard', matchup.taskB.id);
      });
    });
  }

  function updateTimerBadge(side: 'a' | 'b'): void {
    if (!runtime?.activeContainer || !runtime.activeContainer.isConnected) return;
    const col = runtime.activeContainer.querySelector<HTMLElement>(`#arena-col-${side}`);
    if (!col) return;
    const compState = side === 'a' ? stateA : stateB;
    const pill = col.querySelector<HTMLElement>('.arena-status-pill.running');
    if (pill) {
      pill.innerHTML = `<span class="arena-status-dot"></span>Running (${formatElapsed(compState.elapsedSeconds)})`;
    }
  }

  // Bind runtime handlers to current active container
  runtime.activeContainer = container;
  runtime.renderColumn = renderCompetitorColumn;
  runtime.renderTab = renderTabContent;
  runtime.updateTimer = updateTimerBadge;
  runtime.rerenderCol = renderCompetitorColumn;

  async function startListeners(): Promise<void> {
    if (runtime!.listenersStarted) return;
    runtime!.listenersStarted = true;

    // 1. Live second timer
    runtime!.timerId = window.setInterval(() => {
      let dirtyA = false;
      let dirtyB = false;

      if (stateA.status === 'running') {
        stateA.elapsedSeconds++;
        dirtyA = true;
      }
      if (stateB.status === 'running') {
        stateB.elapsedSeconds++;
        dirtyB = true;
      }

      if (dirtyA) runtime?.updateTimer?.('a');
      if (dirtyB) runtime?.updateTimer?.('b');
    }, 1000);

    // 2. Stream agent events
    runtime!.unlistenAgent = await subscribe('agent-event', (evt: any) => {
      const sid = evt?.sessionId || evt?.session_id;
      if (!sid) return;

      const side = sid === matchup.taskA.id ? 'a' : sid === matchup.taskB.id ? 'b' : null;
      if (!side) return;

      const compState = side === 'a' ? stateA : stateB;
      const kind = evt.kind;

      if (kind === 'text') {
        compState.responseText += (evt.text || '');
        const host = runtime?.activeContainer;
        if (host && host.isConnected && compState.activeTab === 'response') {
          const bodyEl = host.querySelector<HTMLElement>(`#arena-markdown-body-${side}`);
          const scrollEl = host.querySelector<HTMLElement>(`#arena-scroll-${side}`);
          if (bodyEl) {
            const placeholder = host.querySelector<HTMLElement>(`#arena-placeholder-${side}`);
            if (placeholder) placeholder.remove();
            streamMarkdown(bodyEl, compState.responseText, scrollEl || undefined);
          } else {
            runtime?.renderTab?.(side);
          }
        }
      } else if (kind === 'tool') {
        const toolData = evt.tool;
        if (toolData) {
          const existing = compState.toolCalls.find(t => t.id === toolData.id);
          if (existing) {
            existing.output = toolData.output || '';
            existing.running = !toolData.output;
          } else {
            compState.toolCalls.push({
              id: toolData.id || `tool-${Date.now()}`,
              name: toolData.name || 'tool',
              input: toolData.input || '',
              output: toolData.output || '',
              running: !toolData.output,
            });
          }
          const host = runtime?.activeContainer;
          if (host && host.isConnected && compState.activeTab === 'response') {
            const toolsBox = host.querySelector<HTMLElement>(`#arena-tools-box-${side}`);
            if (toolsBox) {
              toolsBox.innerHTML = renderToolsHtml(compState);
            }
          }
        }
      } else if (kind === 'status' && (evt.text === 'done' || evt.text === 'complete')) {
        compState.status = 'finished';
        runtime?.renderColumn?.(side);
      }
    });

    // 3. Competitor finished event
    runtime!.unlistenFinished = await subscribe('arena:competitor_finished', (payload: ArenaCompetitorFinished) => {
      if (payload.arenaId !== matchup.id) return;
      const side = payload.competitor as 'a' | 'b';
      const compState = side === 'a' ? stateA : stateB;

      compState.status = payload.success ? 'finished' : 'failed';
      if (payload.responseText) compState.responseText = payload.responseText;
      compState.diff = payload.diff || '';
      compState.files = payload.files || [];
      compState.previewFile = payload.previewFile;

      runtime?.renderColumn?.(side);
    });
  }

  // Initial render & subscription start
  renderLayout();
  void startListeners();

  return () => {
    if (runtime && runtime.activeContainer === container) {
      runtime.activeContainer = null;
      runtime.renderColumn = undefined;
      runtime.renderTab = undefined;
      runtime.updateTimer = undefined;
      runtime.rerenderCol = undefined;
    }
  };
}
