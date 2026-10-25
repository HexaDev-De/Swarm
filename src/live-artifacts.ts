import { escape, icon, toast } from './dom.ts';

export interface ArtifactItem {
  id: string;
  title: string;
  type: 'html' | 'svg' | 'mermaid';
  code: string;
}

let activeArtifactIndex = 0;
let activeViewMode: 'preview' | 'code' = 'preview';
let activeViewport: 'desktop' | 'mobile' = 'desktop';
let isDrawerOpen = false;

export function extractArtifacts(messages: { role: string; content: string }[]): ArtifactItem[] {
  const artifacts: ArtifactItem[] = [];
  const codeBlockRegex = /```(html|svg|xml)\n([\s\S]*?)```/gi;

  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    if (msg.role !== 'assistant') continue;

    let match: RegExpExecArray | null;
    let idx = 0;
    while ((match = codeBlockRegex.exec(msg.content)) !== null) {
      idx++;
      const lang = match[1].toLowerCase();
      const code = match[2].trim();
      const isHtml = lang === 'html' || code.startsWith('<!DOCTYPE') || code.startsWith('<html') || code.includes('<div') || code.includes('<button');
      const isSvg = lang === 'svg' || code.startsWith('<svg');

      if (isHtml || isSvg) {
        artifacts.push({
          id: `artifact-${i}-${idx}`,
          title: isSvg ? `SVG Graphic ${idx}` : `UI Component ${idx}`,
          type: isSvg ? 'svg' : 'html',
          code,
        });
      }
    }
  }

  return artifacts;
}

export function isArtifactsDrawerOpen(): boolean {
  return isDrawerOpen;
}

export function toggleArtifactsDrawer(container: HTMLElement, force?: boolean): boolean {
  isDrawerOpen = force !== undefined ? force : !isDrawerOpen;
  container.classList.toggle('is-open', isDrawerOpen);
  return isDrawerOpen;
}

export function renderLiveArtifactsPill(artifacts: ArtifactItem[]): string {
  if (artifacts.length === 0) return '';
  return `
    <div class="live-artifact-floating-bar">
      <button type="button" class="live-artifact-pill" data-action="toggle-live-artifacts" title="Open sandboxed in-memory live preview (0 disk writes)">
        <span class="live-artifact-pill-spark">${icon('spark')}</span>
        <span>Live Preview (${artifacts.length} ${artifacts.length === 1 ? 'artifact' : 'artifacts'})</span>
      </button>
    </div>
  `;
}

export function renderLiveArtifactsDrawer(container: HTMLElement, artifacts: ArtifactItem[]): void {
  if (artifacts.length === 0) {
    container.innerHTML = '';
    container.hidden = true;
    container.classList.remove('is-open');
    return;
  }

  container.hidden = false;
  container.classList.toggle('is-open', isDrawerOpen);

  if (activeArtifactIndex >= artifacts.length) {
    activeArtifactIndex = 0;
  }
  const current = artifacts[activeArtifactIndex];

  // Prepare srcdoc for sandboxed iframe
  let srcdoc = current.code;
  if (current.type === 'svg' && !srcdoc.includes('<html')) {
    srcdoc = `<!DOCTYPE html><html><head><meta charset="utf-8"><style>body{margin:0;display:flex;align-items:center;justify-content:center;height:100vh;background:#0f172a;}</style></head><body>${current.code}</body></html>`;
  } else if (!srcdoc.includes('<html') && !srcdoc.includes('<!DOCTYPE')) {
    srcdoc = `<!DOCTYPE html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>body{font-family:system-ui,-apple-system,sans-serif;margin:0;padding:24px;background:#0f172a;color:#f8fafc;}</style></head><body>${current.code}</body></html>`;
  }

  container.innerHTML = `
    <div class="live-artifacts-panel">
      <!-- Header -->
      <div class="live-artifacts-header">
        <div class="live-artifacts-tabs">
          ${artifacts.map((a, i) => `
            <button type="button" class="live-artifact-tab ${i === activeArtifactIndex ? 'active' : ''}" data-artifact-idx="${i}">
              ${icon('spark')}<span>${escape(a.title)}</span>
            </button>
          `).join('')}
        </div>
        
        <div class="live-artifacts-controls">
          <div class="live-view-switch">
            <button type="button" class="live-ctrl-btn ${activeViewMode === 'preview' ? 'active' : ''}" data-view="preview" title="Live Preview">
              ${icon('eye')}
            </button>
            <button type="button" class="live-ctrl-btn ${activeViewMode === 'code' ? 'active' : ''}" data-view="code" title="View Code">
              ${icon('terminal')}
            </button>
          </div>

          <div class="live-viewport-switch">
            <button type="button" class="live-ctrl-btn ${activeViewport === 'desktop' ? 'active' : ''}" data-viewport="desktop" title="Desktop">
              ${icon('monitor')}
            </button>
            <button type="button" class="live-ctrl-btn ${activeViewport === 'mobile' ? 'active' : ''}" data-viewport="mobile" title="Mobile (375px)">
              ${icon('phone')}
            </button>
          </div>

          <button type="button" class="live-ctrl-btn" data-action="copy-artifact-code" title="Copy code snippet">
            ${icon('copy')}
          </button>

          <button type="button" class="live-ctrl-btn live-close-btn" data-action="close-artifacts" title="Close Live Preview">
            ${icon('close')}
          </button>
        </div>
      </div>

      <!-- Content Area -->
      <div class="live-artifacts-body viewport-${activeViewport}">
        ${activeViewMode === 'preview' ? `
          <div class="live-preview-wrapper ${activeViewport === 'mobile' ? 'mobile-frame' : ''}">
            <iframe class="live-artifact-iframe" sandbox="allow-scripts" srcdoc="${escape(srcdoc)}"></iframe>
          </div>
        ` : `
          <div class="live-code-wrapper">
            <pre class="live-code-pre"><code>${escape(current.code)}</code></pre>
          </div>
        `}
      </div>

      <div class="live-artifacts-footer">
        <span class="live-footer-note">${icon('zap')} In-Memory Sandbox · 0 files written to workspace disk</span>
      </div>
    </div>
  `;

  // Bind clicks
  container.onclick = async e => {
    const target = (e.target as Element).closest<HTMLElement>('[data-artifact-idx], [data-view], [data-viewport], [data-action]');
    if (!target) return;

    if (target.dataset.artifactIdx !== undefined) {
      activeArtifactIndex = Number(target.dataset.artifactIdx);
      renderLiveArtifactsDrawer(container, artifacts);
      return;
    }

    if (target.dataset.view !== undefined) {
      activeViewMode = target.dataset.view as 'preview' | 'code';
      renderLiveArtifactsDrawer(container, artifacts);
      return;
    }

    if (target.dataset.viewport !== undefined) {
      activeViewport = target.dataset.viewport as 'desktop' | 'mobile';
      renderLiveArtifactsDrawer(container, artifacts);
      return;
    }

    const action = target.dataset.action;
    if (action === 'copy-artifact-code') {
      try {
        await navigator.clipboard.writeText(current.code);
        toast('Copied code to clipboard');
      } catch {
        toast('Could not copy code');
      }
      return;
    }

    if (action === 'close-artifacts') {
      toggleArtifactsDrawer(container, false);
      return;
    }
  };
}
