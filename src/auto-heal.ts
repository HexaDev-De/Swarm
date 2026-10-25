import { icon } from './dom.ts';

export interface CrashReport {
  source: 'browser' | 'terminal' | 'build';
  message: string;
  file?: string;
  line?: number;
  stack?: string;
  timestamp: number;
}

let activeCrash: CrashReport | null = null;
const listeners = new Set<() => void>();

export function getActiveCrash(): CrashReport | null {
  return activeCrash;
}

export function subscribeCrash(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function notify(): void {
  listeners.forEach(fn => fn());
}

export function reportCrash(report: Omit<CrashReport, 'timestamp'>): void {
  // Avoid duplicate error noise within 2 seconds
  if (
    activeCrash &&
    activeCrash.message === report.message &&
    Date.now() - activeCrash.timestamp < 2000
  ) {
    return;
  }
  activeCrash = {
    ...report,
    timestamp: Date.now(),
  };
  notify();
}

export function clearCrash(): void {
  activeCrash = null;
  notify();
}

export function compactCrashStack(raw: string): string {
  if (!raw) return '';
  const lines = raw.split('\n');
  if (lines.length <= 15) return raw.trim();

  // Keep top 6 and bottom 4 frames, folding noisy intermediate bundles
  const top = lines.slice(0, 6);
  const bottom = lines.slice(-4);
  return `${top.join('\n')}\n    ... [${lines.length - 10} internal framework frames omitted] ...\n${bottom.join('\n')}`.trim();
}

export function formatCrashForAgent(report: CrashReport): string {
  const fileRef = report.file ? ` (${report.file}${report.line ? `:${report.line}` : ''})` : '';
  const body = compactCrashStack(report.stack || report.message);

  return `Fix this ${report.source} error${fileRef}:
\`\`\`
${body}
\`\`\`
Please inspect the affected files, identify the root cause, and resolve it so execution runs cleanly.`;
}

export function renderAutoHealBanner(
  host: HTMLElement,
  onSend: (promptText: string) => void
): void {
  const existing = host.querySelector<HTMLElement>('#auto-heal-banner');
  if (!activeCrash) {
    if (existing) existing.remove();
    return;
  }

  const crash = activeCrash;
  const summary = crash.file
    ? `${crash.message.slice(0, 50)} in ${crash.file.split('/').pop()}${crash.line ? `:${crash.line}` : ''}`
    : crash.message.slice(0, 80);

  const html = `
    <div id="auto-heal-banner" class="auto-heal-pill" role="alert">
      <div class="auto-heal-summary" title="${escapeHtml(crash.message)}">
        ${icon('alert')}
        <span class="auto-heal-text">${escapeHtml(summary)}</span>
      </div>
      <div class="auto-heal-actions">
        <button type="button" class="auto-heal-fix-btn" data-action="auto-heal-fix" title="Send stack trace to agent to fix">
          ${icon('zap')}
          <span>Fix Error</span>
        </button>
        <button type="button" class="auto-heal-close-btn" data-action="auto-heal-dismiss" aria-label="Dismiss error">
          ${icon('close')}
        </button>
      </div>
    </div>
  `;

  if (existing) {
    existing.outerHTML = html;
  } else {
    host.insertAdjacentHTML('afterbegin', html);
  }

  const banner = host.querySelector<HTMLElement>('#auto-heal-banner');
  if (!banner) return;

  banner.querySelector('[data-action="auto-heal-fix"]')?.addEventListener('click', e => {
    e.preventDefault();
    e.stopPropagation();
    const prompt = formatCrashForAgent(crash);
    clearCrash();
    onSend(prompt);
  });

  banner.querySelector('[data-action="auto-heal-dismiss"]')?.addEventListener('click', e => {
    e.preventDefault();
    e.stopPropagation();
    clearCrash();
  });
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
