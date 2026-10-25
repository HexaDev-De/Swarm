import type { FileDiff, FileDiffMetadata, FileDiffOptions } from '@pierre/diffs';
import './diff-viewer.css';

export interface DiffViewerInput {
  patch: string;
  path?: string;
  binary?: boolean;
  truncated?: boolean;
  error?: string;
  layout?: 'unified' | 'split';
}

export interface DiffViewerController {
  /** Resolves after the first render or a safe fallback; never rejects. */
  ready: Promise<void>;
  /** Safe to call during the lazy import, and more than once. */
  dispose(): void;
}

// FileDiff's public, framework-free API: https://diffs.com/docs
// https://github.com/pierrecomputer/pierre/tree/main/packages/diffs
// Import this module only on opening a diff. Shiki loads the requested grammars
// on demand; no React, editor, worker pool, or eager language preloading is used.
const MAX_PATCH_LENGTH = 500_000;
const MAX_PATCH_LINES = 5_000;
const MAX_FILES = 40;
const RAW_PREVIEW_LENGTH = 100_000;
const RAW_PREVIEW_LINES = 1_200;
const SPLIT_MIN_WIDTH = 680;
const mountedViewers = new WeakMap<HTMLElement, DiffViewerController>();

function element<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function themeType(): 'light' | 'dark' {
  return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
}

function rawPreview(patch: string): { text: string; clipped: boolean } {
  const prefix = patch.slice(0, RAW_PREVIEW_LENGTH);
  let end = prefix.length;
  let lines = 0;
  for (let i = 0; i < prefix.length; i++) {
    if (prefix[i] === '\n' && ++lines === RAW_PREVIEW_LINES) { end = i; break; }
  }
  return { text: prefix.slice(0, end), clipped: end < patch.length };
}

function tooManyLines(patch: string): boolean {
  let lines = 1;
  for (let i = 0; i < patch.length; i++) {
    if (patch[i] === '\n' && ++lines > MAX_PATCH_LINES) return true;
  }
  return false;
}

/** Read-only, bounded diff rendering. Replacing a viewer cancels its old mount. */
export function mountDiffViewer(host: HTMLElement, input: DiffViewerInput): DiffViewerController {
  mountedViewers.get(host)?.dispose();
  let disposed = false;
  let renderers: FileDiff[] = [];
  let resizeObserver: ResizeObserver | undefined;
  let themeObserver: MutationObserver | undefined;
  let resizeFrame = 0;
  let requestedLayout = input.layout ?? 'unified';
  let layout: 'unified' | 'split' = 'unified';
  let wrap = true;
  let files: FileDiffMetadata[] = [];
  const listeners = new AbortController();

  const root = element('section', 'diff-viewer');
  root.setAttribute('aria-label', input.path ? `Changes to ${input.path}` : 'File changes');
  root.dataset.diffState = 'loading';
  const toolbar = element('div', 'diff-viewer-toolbar');
  const title = element('span', 'diff-viewer-title', input.path ?? 'Changes');
  title.title = input.path ?? 'Changes';
  const controls = element('div', 'diff-viewer-controls');
  const layouts = element('div', 'diff-viewer-layout');
  layouts.setAttribute('role', 'group');
  layouts.setAttribute('aria-label', 'Diff layout');
  const unified = element('button', 'diff-viewer-button', 'Unified');
  const split = element('button', 'diff-viewer-button', 'Split');
  const wrapping = element('button', 'diff-viewer-button diff-viewer-wrap', 'Wrap');
  for (const [button, label] of [[unified, 'Unified diff'], [split, 'Split diff'], [wrapping, 'Wrap lines']] as const) {
    button.type = 'button';
    button.setAttribute('aria-label', label);
    button.title = label;
    button.disabled = true;
  }
  layouts.append(unified, split);
  controls.append(layouts, wrapping);
  toolbar.append(title, controls);
  const status = element('p', 'diff-viewer-status', 'Loading diff…');
  status.setAttribute('role', 'status');
  status.setAttribute('aria-live', 'polite');
  const content = element('div', 'diff-viewer-content');
  content.tabIndex = 0;
  content.setAttribute('role', 'region');
  content.setAttribute('aria-label', 'Read-only diff');
  content.setAttribute('aria-busy', 'true');
  root.append(toolbar, status, content);
  host.replaceChildren(root);

  const options = (): FileDiffOptions<undefined, undefined> => ({
    theme: { dark: 'pierre-dark', light: 'pierre-light' },
    themeType: themeType(),
    preferredHighlighter: 'shiki-js',
    diffStyle: layout,
    overflow: wrap ? 'wrap' : 'scroll',
    diffIndicators: 'classic',
    disableLineNumbers: false,
    disableFileHeader: files.length === 1,
    stickyHeader: false,
    // Patches contain only recorded context. Never fetch current files to fill
    // historical gaps, or offer expand buttons for lines we do not possess.
    hunkSeparators: 'metadata',
    lineDiffType: 'word-alt',
    maxLineDiffLength: 1_000,
    tokenizeMaxLineLength: 2_000,
    tokenizeMaxLength: 100_000,
    disableErrorHandling: true,
    enableLineSelection: false,
    // This CSS is constant; no patch content is ever interpolated into it.
    unsafeCSS: ':host { --diffs-font-family: var(--mono, monospace); --diffs-font-size: 0.86rem; --diffs-line-height: 1.45rem; }',
  });

  function cleanRenderers(): void {
    for (const renderer of renderers) renderer.cleanUp();
    renderers = [];
  }

  function showFallback(message: string, showPatch = true): void {
    if (disposed) return;
    cleanRenderers();
    resizeObserver?.disconnect();
    themeObserver?.disconnect();
    content.replaceChildren();
    content.setAttribute('aria-busy', 'false');
    root.dataset.diffState = 'fallback';
    controls.hidden = true;
    status.hidden = false;
    status.textContent = message;
    if (showPatch && input.patch) {
      const preview = rawPreview(input.patch);
      const pre = element('pre', 'diff-viewer-raw', preview.text);
      pre.setAttribute('aria-label', 'Plain text patch');
      content.append(pre);
      if (preview.clipped) content.append(element('p', 'diff-viewer-note', 'Preview shortened to keep the app responsive.'));
    }
  }

  function updateControls(): void {
    const narrow = root.getBoundingClientRect().width < SPLIT_MIN_WIDTH;
    layout = requestedLayout === 'split' && !narrow ? 'split' : 'unified';
    root.dataset.diffLayout = layout;
    unified.disabled = false;
    split.disabled = narrow;
    split.title = narrow ? 'Widen the panel to use a split diff' : 'Split diff';
    wrapping.disabled = false;
    unified.setAttribute('aria-pressed', String(layout === 'unified'));
    split.setAttribute('aria-pressed', String(layout === 'split'));
    wrapping.setAttribute('aria-pressed', String(wrap));
  }

  function rerender(): void {
    if (disposed || !renderers.length) return;
    const scrollTop = content.scrollTop;
    try {
      updateControls();
      for (const renderer of renderers) {
        renderer.setOptions(options());
        renderer.rerender();
      }
      content.scrollTop = scrollTop;
    } catch {
      showFallback('Unable to format this diff. Showing the recorded patch.');
    }
  }

  unified.addEventListener('click', () => { requestedLayout = 'unified'; rerender(); }, { signal: listeners.signal });
  split.addEventListener('click', () => { requestedLayout = 'split'; rerender(); }, { signal: listeners.signal });
  wrapping.addEventListener('click', () => { wrap = !wrap; rerender(); }, { signal: listeners.signal });
  layouts.addEventListener('keydown', event => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key) || split.disabled) return;
    event.preventDefault();
    const button = event.key === 'ArrowLeft' || event.key === 'Home' ? unified : split;
    button.focus();
    button.click();
  }, { signal: listeners.signal });

  async function initialize(): Promise<void> {
    if (input.error) { showFallback(input.error); return; }
    if (input.binary || /^(?:GIT binary patch|Binary files .+ differ)$/m.test(input.patch)) {
      showFallback('Binary file changed. A text diff is not available.', false); return;
    }
    if (!input.patch.trim()) { showFallback('No text changes to display.', false); return; }
    if (input.patch.length > MAX_PATCH_LENGTH || tooManyLines(input.patch)) {
      showFallback('Large diff — showing a limited plain text preview.'); return;
    }
    try {
      const { FileDiff: Renderer, parsePatchFiles, preloadHighlighter, getFiletypeFromFileName } = await import('@pierre/diffs');
      if (disposed) return;
      files = parsePatchFiles(input.patch, undefined, true).flatMap(patch => patch.files);
      if (!files.length) { showFallback('No line changes in this patch.'); return; }
      if (files.length > MAX_FILES) { showFallback('Large diff — showing a limited plain text preview.'); return; }
      // Load only the two matching themes and languages present in this patch.
      // Shiki uses one shared highlighter across simultaneously open viewers.
      await preloadHighlighter({
        themes: ['pierre-dark', 'pierre-light'],
        langs: [...new Set(files.map(file => file.lang ?? getFiletypeFromFileName(file.name)))],
        preferredHighlighter: 'shiki-js',
      });
      if (disposed) return;
      updateControls();
      for (const file of files) {
        const frame = element('div', 'diff-viewer-file');
        frame.setAttribute('aria-label', file.name);
        content.append(frame);
        const renderer = new Renderer(options());
        renderers.push(renderer);
        renderer.render({ fileDiff: file, containerWrapper: frame });
        if (!file.hunks.length) {
          const message = file.type === 'rename-pure'
            ? `Renamed ${file.prevName ?? 'file'} to ${file.name}. Contents unchanged.`
            : file.mode !== file.prevMode && file.prevMode
              ? `File mode changed from ${file.prevMode} to ${file.mode ?? 'unknown'}.`
              : 'File metadata changed. No line changes.';
          frame.append(element('p', 'diff-viewer-note', message));
        }
      }
      root.dataset.diffState = 'ready';
      content.setAttribute('aria-busy', 'false');
      status.hidden = !input.truncated;
      status.textContent = input.truncated ? 'This patch was shortened when recorded. Only available changes are shown.' : '';
      themeObserver = new MutationObserver(() => {
        if (!disposed) for (const renderer of renderers) renderer.setThemeType(themeType());
      });
      themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
      resizeObserver = new ResizeObserver(() => {
        if (disposed || resizeFrame) return;
        resizeFrame = requestAnimationFrame(() => {
          resizeFrame = 0;
          if (disposed) return;
          const previousLayout = layout;
          updateControls();
          if (layout !== previousLayout) rerender();
        });
      });
      resizeObserver.observe(root);
    } catch {
      showFallback(input.truncated
        ? 'This patch was shortened when recorded. Showing the available plain text patch.'
        : 'Unable to format this diff. Showing the recorded patch.');
    }
  }

  const controller: DiffViewerController = {
    ready: initialize(),
    dispose() {
      if (disposed) return;
      disposed = true;
      listeners.abort();
      resizeObserver?.disconnect();
      themeObserver?.disconnect();
      cancelAnimationFrame(resizeFrame);
      cleanRenderers();
      root.remove();
      if (mountedViewers.get(host) === controller) mountedViewers.delete(host);
    },
  };
  mountedViewers.set(host, controller);
  return controller;
}
