import { call } from './bridge.ts';
import { icon, toast } from './dom.ts';

export interface VibeSnapshot {
  turnIndex: number;
  sessionId: string;
  commitSha: string | null;
  treeSha: string | null;
  timestamp: number;
  summary: string;
  filesCopied: string[];
  filesChanged: string[];
  additions: number;
  deletions: number;
  messageId: number | null;
}

let activePreviewTurn: number | null = null;
let filmstripOpen = false;
const sessionSnapshotsCache = new Map<string, VibeSnapshot[]>();
let lastSnapshottedTurn = new Map<string, number>();

export function getActivePreviewTurn(): number | null {
  return activePreviewTurn;
}

export function isFilmstripDrawerOpen(): boolean {
  return filmstripOpen;
}

export function toggleFilmstripDrawer(force?: boolean): boolean {
  filmstripOpen = force !== undefined ? force : !filmstripOpen;
  const host = document.getElementById('vibe-scrubber-host');
  if (host) {
    host.classList.toggle('filmstrip-open', filmstripOpen);
  }
  return filmstripOpen;
}

export async function loadTurnSnapshots(sessionId: string): Promise<VibeSnapshot[]> {
  try {
    const list = await call<VibeSnapshot[]>('vibe_get_turn_snapshots', { sessionId });
    sessionSnapshotsCache.set(sessionId, list);
    return list;
  } catch {
    return sessionSnapshotsCache.get(sessionId) || [];
  }
}

export function getCachedSnapshots(sessionId: string): VibeSnapshot[] {
  return sessionSnapshotsCache.get(sessionId) || [];
}

export async function snapshotCurrentTurn(sessionId: string, turnIndex: number, summary = ''): Promise<void> {
  const last = lastSnapshottedTurn.get(sessionId);
  if (last === turnIndex) return;
  lastSnapshottedTurn.set(sessionId, turnIndex);
  try {
    const snap = await call<VibeSnapshot>('vibe_snapshot_turn', { sessionId, turnIndex, summary });
    const cached = sessionSnapshotsCache.get(sessionId) || [];
    const idx = cached.findIndex(s => s.turnIndex === turnIndex);
    if (idx >= 0) {
      cached[idx] = snap;
    } else {
      cached.push(snap);
      cached.sort((a, b) => a.turnIndex - b.turnIndex);
    }
    sessionSnapshotsCache.set(sessionId, cached);
  } catch (e) {
    console.warn('Vibe snapshot failed:', e);
  }
}

function timeAgo(timestampSeconds: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = Math.max(0, now - timestampSeconds);
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/** Render inline checkpoint divider pill inside the chat transcript after a turn */
export function renderInlineCheckpoint(snapshot: VibeSnapshot, isLatest: boolean): string {
  const isPreviewing = activePreviewTurn === snapshot.turnIndex;
  const shortSha = (snapshot.treeSha || snapshot.commitSha || '').slice(0, 7);
  const filesCount = snapshot.filesChanged.length;
  const changesStats = filesCount > 0
    ? `${filesCount} ${filesCount === 1 ? 'file' : 'files'} (${snapshot.additions > 0 ? `+${snapshot.additions}` : ''}${snapshot.deletions > 0 ? ` -${snapshot.deletions}` : ''})`
    : 'No file changes';

  const previewTitle = isPreviewing ? 'Currently previewing this snapshot' : `Preview files at Turn ${snapshot.turnIndex}`;
  const rewindTitle = `Rewind workspace and conversation to Turn ${snapshot.turnIndex} permanently`;

  return `
    <div class="vibe-turn-checkpoint ${isPreviewing ? 'is-previewing' : ''} ${isLatest ? 'is-latest' : ''}" data-vibe-turn="${snapshot.turnIndex}">
      <div class="vibe-checkpoint-divider">
        <span class="vibe-divider-line"></span>
        <div class="vibe-checkpoint-pill" tabindex="0" role="region" aria-label="Turn ${snapshot.turnIndex} checkpoint">
          <span class="vibe-checkpoint-dot" aria-hidden="true"></span>
          <span class="vibe-checkpoint-title">Turn ${snapshot.turnIndex}</span>
          <span class="vibe-checkpoint-meta">${changesStats} · ${timeAgo(snapshot.timestamp)}</span>
          <div class="vibe-checkpoint-actions">
            ${isPreviewing
              ? `<button type="button" class="vibe-pill-btn active-pill" data-vibe-act="return" title="Return to latest turn">${icon('return')}<span>Latest</span></button>`
              : `<button type="button" class="vibe-pill-btn" data-vibe-act="preview" data-turn="${snapshot.turnIndex}" title="${previewTitle}">${icon('eye')}<span>Preview</span></button>`
            }
            <button type="button" class="vibe-pill-btn vibe-pill-rewind" data-vibe-act="rewind" data-turn="${snapshot.turnIndex}" title="${rewindTitle}">
              ${icon('rewind')}<span>Rewind</span>
            </button>
          </div>
          
          <!-- Rich Hover Card -->
          <div class="vibe-checkpoint-hovercard" role="tooltip">
            <div class="vibe-hovercard-top">
              <span class="vibe-hovercard-turn">Turn ${snapshot.turnIndex} Checkpoint</span>
              ${shortSha ? `<span class="vibe-hovercard-sha"><code>${shortSha}</code></span>` : ''}
            </div>
            ${snapshot.summary ? `<p class="vibe-hovercard-summary">${escapeHtml(snapshot.summary)}</p>` : ''}
            ${filesCount > 0 ? `
              <div class="vibe-hovercard-files">
                <div class="vibe-hovercard-files-header">Modified Files:</div>
                ${snapshot.filesChanged.slice(0, 5).map(f => `
                  <div class="vibe-hovercard-file-row">
                    <span class="vibe-file-icon">${icon('file')}</span>
                    <span class="vibe-file-name" title="${escapeHtml(f)}">${escapeHtml(f)}</span>
                  </div>
                `).join('')}
                ${filesCount > 5 ? `<div class="vibe-hovercard-more">+${filesCount - 5} more files</div>` : ''}
              </div>
            ` : '<div class="vibe-hovercard-no-files">Dev / conversational snapshot</div>'}
            <div class="vibe-hovercard-hint">Click Preview to inspect working tree · Click Rewind to restore permanently</div>
          </div>
        </div>
        <span class="vibe-divider-line"></span>
      </div>
    </div>
  `;
}

/** Render collapsible Time Machine filmstrip drawer into #vibe-scrubber-host */
export function renderVibeScrubber(
  container: HTMLElement,
  sessionId: string,
  snapshots: VibeSnapshot[],
  onStatusChange?: () => void
): void {
  if (snapshots.length === 0 && activePreviewTurn === null) {
    container.innerHTML = '';
    return;
  }

  const latestTurn = snapshots.length > 0 ? snapshots[snapshots.length - 1].turnIndex : 0;

  // 1. Floating Amber Preview Banner if currently previewing an older turn
  let bannerHtml = '';
  if (activePreviewTurn !== null) {
    bannerHtml = `
      <div class="vibe-preview-banner" role="status">
        <div class="vibe-banner-content">
          <span class="vibe-banner-pulse"></span>
          <span class="vibe-banner-tag">PREVIEWING HISTORY</span>
          <span class="vibe-banner-title">
            Viewing Historical Snapshot: <strong>Turn ${activePreviewTurn}</strong> of ${latestTurn}
          </span>
        </div>
        <div class="vibe-banner-actions">
          <button type="button" class="vibe-banner-btn vibe-banner-restore" data-vibe-act="restore-confirm" data-turn="${activePreviewTurn}">
            ${icon('zap')}<span>Restore Turn ${activePreviewTurn} Permanently</span>
          </button>
          <button type="button" class="vibe-banner-btn vibe-banner-return" data-vibe-act="return">
            ${icon('return')}<span>Return to Latest (Turn ${latestTurn})</span>
          </button>
        </div>
      </div>
    `;
  }

  // 2. Filmstrip drawer content (collapsible)
  const cardsHtml = snapshots.map((s, idx) => {
    const isLatest = idx === snapshots.length - 1;
    const isPreviewing = activePreviewTurn === s.turnIndex;
    const isCurrent = activePreviewTurn === null && isLatest;

    const shortSha = (s.treeSha || s.commitSha || '').slice(0, 7);
    const filesCount = s.filesChanged.length;

    let statusBadge = '<span class="vibe-card-status">Historical</span>';
    if (isPreviewing) {
      statusBadge = '<span class="vibe-card-status previewing">Previewing</span>';
    } else if (isCurrent) {
      statusBadge = '<span class="vibe-card-status current">Current (Latest)</span>';
    }

    return `
      <div class="vibe-filmstrip-card ${isPreviewing ? 'previewing' : ''} ${isCurrent ? 'current' : ''}" data-turn-index="${s.turnIndex}">
        <div class="vibe-card-header">
          <span class="vibe-card-turn">Turn ${s.turnIndex}</span>
          ${statusBadge}
        </div>
        <div class="vibe-card-meta">
          <span class="vibe-card-time">${timeAgo(s.timestamp)}</span>
          ${shortSha ? `<span class="vibe-card-sha">${shortSha}</span>` : ''}
        </div>
        <div class="vibe-card-summary" title="${escapeHtml(s.summary)}">
          ${escapeHtml(s.summary || `Checkpoint for Turn ${s.turnIndex}`)}
        </div>
        <div class="vibe-card-stats">
          <span>${filesCount} ${filesCount === 1 ? 'file' : 'files'}</span>
          ${s.additions > 0 ? `<span class="vibe-stat-add">+${s.additions}</span>` : ''}
          ${s.deletions > 0 ? `<span class="vibe-stat-del">-${s.deletions}</span>` : ''}
        </div>
        <div class="vibe-card-actions">
          ${isPreviewing
            ? `<button type="button" class="vibe-card-btn active" data-vibe-act="return">${icon('return')}<span>Return</span></button>`
            : `<button type="button" class="vibe-card-btn" data-vibe-act="preview" data-turn="${s.turnIndex}">${icon('eye')}<span>Preview</span></button>`
          }
          <button type="button" class="vibe-card-btn vibe-card-rewind" data-vibe-act="rewind" data-turn="${s.turnIndex}">
            ${icon('rewind')}<span>Rewind</span>
          </button>
        </div>
      </div>
    `;
  }).join('');

  const filmstripHtml = `
    <div class="vibe-filmstrip-drawer ${filmstripOpen ? 'is-open' : ''}">
      <div class="vibe-filmstrip-topbar">
        <div class="vibe-filmstrip-title">
          ${icon('rewind')}
          <span>Vibe Time-Machine Filmstrip</span>
          <span class="vibe-filmstrip-count">${snapshots.length} ${snapshots.length === 1 ? 'turn' : 'turns'} captured</span>
        </div>
        <button type="button" class="vibe-filmstrip-close-btn" data-vibe-act="close-filmstrip" title="Close Filmstrip">
          ${icon('close')}
        </button>
      </div>
      <div class="vibe-filmstrip-track">
        ${cardsHtml}
      </div>
    </div>
  `;

  container.innerHTML = `${bannerHtml}${filmstripHtml}`;

  // Attach click handlers
  bindVibeActions(container, sessionId, snapshots, onStatusChange);
}

function bindVibeActions(
  container: HTMLElement,
  sessionId: string,
  snapshots: VibeSnapshot[],
  onStatusChange?: () => void
): void {
  container.onclick = async e => {
    const target = (e.target as Element).closest<HTMLElement>('[data-vibe-act]');
    if (!target) return;
    const act = target.dataset.vibeAct;
    const turn = target.dataset.turn ? Number(target.dataset.turn) : null;

    if (act === 'close-filmstrip') {
      toggleFilmstripDrawer(false);
      renderVibeScrubber(container, sessionId, snapshots, onStatusChange);
      return;
    }

    if (act === 'return') {
      try {
        await call('vibe_return_to_latest', { sessionId });
        activePreviewTurn = null;
        toast('⏩ Returned to latest workspace state.');
        window.dispatchEvent(new CustomEvent('pandacode-git-refresh'));
        window.dispatchEvent(new CustomEvent('pandacode-browser-reload'));
        renderVibeScrubber(container, sessionId, snapshots, onStatusChange);
        onStatusChange?.();
      } catch (err) {
        toast(`Could not return to latest: ${err instanceof Error ? err.message : String(err)}`);
      }
      return;
    }

    if (act === 'preview' && turn !== null) {
      try {
        await call('vibe_preview_turn_snapshot', { sessionId, turnIndex: turn });
        activePreviewTurn = turn;
        toast(`Previewing Turn ${turn} snapshot.`);
        window.dispatchEvent(new CustomEvent('pandacode-git-refresh'));
        window.dispatchEvent(new CustomEvent('pandacode-browser-reload'));
        renderVibeScrubber(container, sessionId, snapshots, onStatusChange);
        onStatusChange?.();
      } catch (err) {
        toast(`Could not preview turn: ${err instanceof Error ? err.message : String(err)}`);
      }
      return;
    }

    if ((act === 'rewind' || act === 'restore-confirm') && turn !== null) {
      const confirmMsg = `Permanent Rewind: Restore workspace and conversation to Turn ${turn}?\n\nAll files modified and messages sent after Turn ${turn} will be permanently removed.`;
      if (!window.confirm(confirmMsg)) return;

      try {
        await call('vibe_restore_turn_permanent', { sessionId, turnIndex: turn });
        activePreviewTurn = null;
        toast(`Restored workspace and conversation to Turn ${turn} permanently.`);
        window.dispatchEvent(new CustomEvent('pandacode-git-refresh'));
        window.dispatchEvent(new CustomEvent('pandacode-browser-reload'));
        window.dispatchEvent(new CustomEvent('pandacode-restore-turn', { detail: { turnIndex: turn } }));
        renderVibeScrubber(container, sessionId, snapshots, onStatusChange);
        onStatusChange?.();
      } catch (err) {
        toast(`Restore failed: ${err instanceof Error ? err.message : String(err)}`);
      }
      return;
    }
  };
}

/** Global listener for checkpoint divider clicks inside the transcript */
export function bindTranscriptCheckpointEvents(
  transcript: HTMLElement,
  sessionId: string,
  onStatusChange?: () => void
): void {
  transcript.addEventListener('click', async e => {
    const target = (e.target as Element).closest<HTMLElement>('[data-vibe-act]');
    if (!target) return;
    const act = target.dataset.vibeAct;
    const turn = target.dataset.turn ? Number(target.dataset.turn) : null;
    const snapshots = getCachedSnapshots(sessionId);
    const host = document.getElementById('vibe-scrubber-host');

    if (act === 'preview' && turn !== null) {
      try {
        await call('vibe_preview_turn_snapshot', { sessionId, turnIndex: turn });
        activePreviewTurn = turn;
        toast(`Previewing Turn ${turn} snapshot.`);
        window.dispatchEvent(new CustomEvent('pandacode-git-refresh'));
        window.dispatchEvent(new CustomEvent('pandacode-browser-reload'));
        if (host) renderVibeScrubber(host, sessionId, snapshots, onStatusChange);
        onStatusChange?.();
      } catch (err) {
        toast(`Could not preview turn: ${err instanceof Error ? err.message : String(err)}`);
      }
    } else if (act === 'return') {
      try {
        await call('vibe_return_to_latest', { sessionId });
        activePreviewTurn = null;
        toast('Returned to latest workspace state.');
        window.dispatchEvent(new CustomEvent('pandacode-git-refresh'));
        window.dispatchEvent(new CustomEvent('pandacode-browser-reload'));
        if (host) renderVibeScrubber(host, sessionId, snapshots, onStatusChange);
        onStatusChange?.();
      } catch (err) {
        toast(`Could not return to latest: ${err instanceof Error ? err.message : String(err)}`);
      }
    } else if (act === 'rewind' && turn !== null) {
      const confirmMsg = `Permanent Rewind: Restore workspace and conversation to Turn ${turn}?\n\nAll files modified and messages sent after Turn ${turn} will be permanently removed.`;
      if (!window.confirm(confirmMsg)) return;

      try {
        await call('vibe_restore_turn_permanent', { sessionId, turnIndex: turn });
        activePreviewTurn = null;
        toast(`Restored workspace and conversation to Turn ${turn} permanently.`);
        window.dispatchEvent(new CustomEvent('pandacode-git-refresh'));
        window.dispatchEvent(new CustomEvent('pandacode-browser-reload'));
        window.dispatchEvent(new CustomEvent('pandacode-restore-turn', { detail: { turnIndex: turn } }));
        if (host) renderVibeScrubber(host, sessionId, snapshots, onStatusChange);
        onStatusChange?.();
      } catch (err) {
        toast(`Restore failed: ${err instanceof Error ? err.message : String(err)}`);
      }
    }
  });
}
