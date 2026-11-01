// ULTRACODE V2 Frontend Controller & Workflow Inspector (Pillars 1-7)

export interface PreflightData {
  phases: number;
  estimatedAgents: number;
  subtasksCount: number;
  mode: string;
  concurrencyCeiling: number;
}

export interface LiveAgentEntry {
  id: string;
  role: string;
  status: 'queued' | 'running' | 'passed' | 'failed' | 'null_filtered';
  prompt_preview: string;
  owned_files: string[];
  elapsed_ms: number;
  tokens_used?: number;
  worktree?: string;
  error?: string;
}

export interface SwarmMatrixPayload {
  agents: LiveAgentEntry[];
  totalSpawned: number;
  maxAllowed: number;
  concurrencyCeiling: number;
}

let activeMatrixData: SwarmMatrixPayload = {
  agents: [],
  totalSpawned: 0,
  maxAllowed: 1000,
  concurrencyCeiling: 16,
};

let matrixListeners = new Set<(data: SwarmMatrixPayload) => void>();

export function ultracodeStageName(stage: string | null | undefined): string {
  switch (stage) {
    case 'recon': return 'Recon';
    case 'workers': return 'Workers';
    case 'qa_loop': return 'QA Loop';
    case 'critic_review': return 'Critic Review';
    default: return stage ? stage.charAt(0).toUpperCase() + stage.slice(1) : 'Recon';
  }
}

/** Human-Input Security Guard: Only genuinely typed or dictated input triggers keyword activation */
export function detectUltracodeKeyword(text: string, isHumanInput = true): boolean {
  if (!isHumanInput || !text) return false;
  return /(?:^|\s)ultracode:(?:\s|$)/i.test(text.trim());
}

/** Instant Disarm (Alt+W / Option+W) reverts keyword trigger to plain text */
export function disarmUltracodeKeyword(text: string): string {
  return text.replace(/(?:^|\s)ultracode:(?:\s|$)/gi, ' ').trim();
}

/** Check if multi-agent workflows are disabled by enterprise setting */
export function isWorkflowsDisabled(): boolean {
  try {
    return localStorage.getItem('pandacode.disableWorkflows') === 'true';
  } catch {
    return false;
  }
}

export function setWorkflowsDisabled(disabled: boolean): void {
  try {
    localStorage.setItem('pandacode.disableWorkflows', String(disabled));
  } catch {}
}

/** Updates live swarm matrix telemetry received from Rust backend */
export function updateSwarmMatrix(payload: SwarmMatrixPayload): void {
  activeMatrixData = payload;
  matrixListeners.forEach(fn => fn(activeMatrixData));
}

export function subscribeSwarmMatrix(listener: (data: SwarmMatrixPayload) => void): () => void {
  matrixListeners.add(listener);
  listener(activeMatrixData);
  return () => { matrixListeners.delete(listener); };
}

/** Renders the glowing purple keyword badge inside the Composer */
export function renderUltracodePill(container: HTMLElement, onDisarm: () => void): void {
  let existing = container.querySelector<HTMLElement>('.ultracode-keyword-pill');
  if (existing) return;

  const pill = document.createElement('div');
  pill.className = 'ultracode-keyword-pill';
  pill.innerHTML = `
    <span class="ultracode-pill-glow"></span>
    <span class="ultracode-pill-icon">⚡</span>
    <span class="ultracode-pill-title">ULTRACODE V2</span>
    <span class="ultracode-pill-hint">Armed · <kbd>Alt+W</kbd> to disarm</span>
    <button type="button" class="ultracode-pill-close" title="Disarm ULTRACODE (Alt+W)">✕</button>
  `;

  pill.querySelector('.ultracode-pill-close')?.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    onDisarm();
  });

  container.prepend(pill);
}

export function removeUltracodePill(container: HTMLElement): void {
  container.querySelector('.ultracode-keyword-pill')?.remove();
}

/** Pre-Flight Cost & Swarm Estimation Bar */
export function renderPreflightBanner(
  host: HTMLElement,
  data: PreflightData,
  onLaunch: () => void,
  onCancel: () => void,
  onInspect: () => void,
): void {
  let existing = host.querySelector<HTMLElement>('#ultracode-preflight-banner');
  if (existing) existing.remove();

  const banner = document.createElement('div');
  banner.id = 'ultracode-preflight-banner';
  banner.className = 'ultracode-preflight-bar';
  banner.innerHTML = `
    <div class="preflight-info">
      <span class="preflight-badge">⚡ ULTRACODE V2 Ready</span>
      <span class="preflight-stat"><strong>${data.phases}</strong> Phases</span>
      <span class="preflight-dot">·</span>
      <span class="preflight-stat">Est. <strong>${data.estimatedAgents}</strong> Sub-Agents</span>
      <span class="preflight-dot">·</span>
      <span class="preflight-stat"><strong>~6x</strong> Deep Compute</span>
      <span class="preflight-dot">·</span>
      <span class="preflight-stat">Max Concurrency: <strong>${data.concurrencyCeiling}</strong></span>
    </div>
    <div class="preflight-actions">
      <button type="button" class="preflight-btn inspect-btn" title="Inspect generated script (Ctrl+G)">
        🔍 Inspect Script <kbd>Ctrl+G</kbd>
      </button>
      <button type="button" class="preflight-btn launch-btn">
        🚀 Launch Swarm
      </button>
      <button type="button" class="preflight-btn cancel-btn" title="Cancel workflow">
        ✕
      </button>
    </div>
  `;

  banner.querySelector('.inspect-btn')?.addEventListener('click', onInspect);
  banner.querySelector('.launch-btn')?.addEventListener('click', onLaunch);
  banner.querySelector('.cancel-btn')?.addEventListener('click', () => {
    banner.remove();
    onCancel();
  });

  host.appendChild(banner);
}

/** Generates standard ULTRACODE JavaScript orchestration workflow template */
export function generateWorkflowScript(taskTitle: string, subtasksCount = 4, concurrency = 16): string {
  return `/**
 * ULTRACODE V2 Deterministic Orchestration Workflow
 * Objective: "${taskTitle.replace(/"/g, '\\"')}"
 * Max Concurrency: ${concurrency}
 */

// Phase 0: Lay down shared interface contracts
await agent("Define shared contracts and export schemas in src/types.ts", {
  role: "Lead Architect",
  modelAlias: "best",
  reasoningEffort: "xhigh"
});

// Phase 1: Spatial Concurrency Fan-out (MapReduce Swarm)
const subtasks = Array.from({ length: ${subtasksCount} }, (_, i) => ({
  id: \`subtask-\${i + 1}\`,
  title: \`Module Implementation \${i + 1}\`,
  ownedFiles: [\`src/module_\${i + 1}.ts\`]
}));

// Fault-Tolerant Parallel Triad Execution
const results = await parallel(
  subtasks.map(task => async () => {
    return await adversarialTriad(task, {
      role: "Triad Swarm",
      modelAlias: "fast"
    });
  })
);

// Synthesize successful outcomes (null-filtered)
const verifiedOutcomes = results.filter(Boolean);
console.log(\`Successfully verified \${verifiedOutcomes.length}/\${subtasks.length} modules.\`);

// Phase 2: Clustered Compiler Self-Healing & Integration
await agent("Run global verification and finalize integration patches.", {
  role: "Integration Engineer",
  modelAlias: "best"
});
`;
}

/** Syntax highlighter for the JS workflow script */
function highlightJsScript(code: string): string {
  const keywords = /\b(await|async|const|let|var|function|return|if|else|for|while|import|export|from|new|true|false|null)\b/g;
  const primitives = /\b(agent|parallel|pipeline|adversarialTriad)\b/g;
  const strings = /(["'\`])(?:(?=(\\?))\2.)*?\1/g;
  const comments = /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)/g;

  return code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(comments, '<span class="tok-comment">$1</span>')
    .replace(strings, '<span class="tok-string">$&</span>')
    .replace(primitives, '<span class="tok-primitive">$&</span>')
    .replace(keywords, '<span class="tok-keyword">$1</span>');
}

/** Ctrl+G Workflow Script & Live Swarm Matrix Modal */
export function openWorkflowInspectorModal(
  taskTitle: string,
  initialTab: 'script' | 'matrix' = 'matrix'
): void {
  let existing = document.querySelector<HTMLDialogElement>('#ultracode-inspector-dialog');
  if (existing) existing.remove();

  const dialog = document.createElement('dialog');
  dialog.id = 'ultracode-inspector-dialog';
  dialog.className = 'ultracode-inspector-modal';

  let currentTab = initialTab;

  function renderBody(): void {
    const scriptTabActive = currentTab === 'script';
    const scriptCode = generateWorkflowScript(taskTitle, 4, activeMatrixData.concurrencyCeiling);

    dialog.innerHTML = `
      <div class="ultracode-modal-header">
        <div class="ultracode-modal-title">
          <span class="title-icon">⚡</span>
          <h2>ULTRACODE V2 Workflow Inspector</h2>
        </div>
        <div class="ultracode-tab-switch">
          <button type="button" class="tab-btn ${scriptTabActive ? 'active' : ''}" data-tab="script">
            📜 Generated Script
          </button>
          <button type="button" class="tab-btn ${!scriptTabActive ? 'active' : ''}" data-tab="matrix">
            📊 Live Swarm Matrix (${activeMatrixData.agents.length})
          </button>
        </div>
        <button type="button" class="modal-close-btn" title="Close inspector (Esc)">✕</button>
      </div>

      <div class="ultracode-modal-content">
        ${scriptTabActive ? `
          <div class="script-view-pane">
            <div class="script-toolbar">
              <span class="script-meta">Async Deterministic JavaScript Workflow Runtime</span>
              <span class="script-concurrency">Concurrency Limit: <strong>${activeMatrixData.concurrencyCeiling} slots</strong></span>
            </div>
            <pre class="script-code-block"><code>${highlightJsScript(scriptCode)}</code></pre>
          </div>
        ` : `
          <div class="matrix-view-pane">
            <div class="matrix-toolbar">
              <div class="matrix-stats">
                <span class="stat-pill total">Total Spawned: <strong>${activeMatrixData.totalSpawned} / ${activeMatrixData.maxAllowed}</strong></span>
                <span class="stat-pill active">Concurrency Ceiling: <strong>${activeMatrixData.concurrencyCeiling}</strong></span>
                <span class="stat-pill passed">Completed: <strong>${activeMatrixData.agents.filter(a => a.status === 'passed').length}</strong></span>
                <span class="stat-pill filtered">Null-Filtered: <strong>${activeMatrixData.agents.filter(a => a.status === 'null_filtered').length}</strong></span>
              </div>
            </div>
            <div class="matrix-grid">
              ${activeMatrixData.agents.length === 0 ? `
                <div class="matrix-empty">No sub-agents active. Launch an ULTRACODE task to view the live swarm matrix.</div>
              ` : activeMatrixData.agents.map((agent, i) => `
                <div class="swarm-agent-card status-${agent.status}">
                  <div class="card-header">
                    <span class="agent-index">#${i + 1}</span>
                    <span class="agent-role" title="${agent.role}">${agent.role}</span>
                    <span class="agent-badge badge-${agent.status}">${agent.status.toUpperCase()}</span>
                  </div>
                  <div class="card-prompt" title="${agent.prompt_preview}">
                    ${agent.prompt_preview || 'No prompt preview available'}
                  </div>
                  <div class="card-footer">
                    <span class="card-duration">⏱️ ${Math.round(agent.elapsed_ms / 100) / 10}s</span>
                    ${agent.owned_files?.length ? `<span class="card-files">📁 ${agent.owned_files.length} files</span>` : ''}
                  </div>
                </div>
              `).join('')}
            </div>
          </div>
        `}
      </div>
    `;

    dialog.querySelectorAll<HTMLButtonElement>('.tab-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        currentTab = (btn.dataset.tab as 'script' | 'matrix') || 'script';
        renderBody();
      });
    });

    dialog.querySelector('.modal-close-btn')?.addEventListener('click', () => {
      dialog.close();
      dialog.remove();
    });
  }

  renderBody();
  document.body.appendChild(dialog);
  dialog.showModal();

  const unsubscribe = subscribeSwarmMatrix(() => {
    if (currentTab === 'matrix') {
      renderBody();
    }
  });

  dialog.addEventListener('close', () => {
    unsubscribe();
    dialog.remove();
  }, { once: true });
}
