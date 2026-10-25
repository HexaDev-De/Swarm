import { markdownHtml } from './dom';
import { captureFileLinkContext, decorateFileLinks, retainFileLink } from './file-links';
import { preserveTranscriptPosition } from './transcript-scroll';

type Paint = { target: HTMLElement; value: string; rendered: string; pending: boolean; timer: number; lastPaint: number; formatted: boolean; context?: object; scroll?: HTMLElement };
const paints = new WeakMap<HTMLElement, Paint>();
const FRAME_MS = 32;

/** Keep finished blocks and text nodes in place as the open Markdown block grows. */
function reconcile(parent: Node, next: Node): void {
  const children = [...next.childNodes];
  children.forEach((child, index) => {
    const old = parent.childNodes[index];
    if (!old) { parent.appendChild(child); return; }
    if (old.nodeType !== child.nodeType || old.nodeName !== child.nodeName) { parent.replaceChild(child, old); return; }
    if (old instanceof Text && child instanceof Text) {
      if (child.data.startsWith(old.data)) { if (child.data.length > old.length) old.appendData(child.data.slice(old.length)); }
      else if (old.data !== child.data) old.data = child.data;
    } else if (old instanceof Element && child instanceof Element) {
      retainFileLink(old, child);
      for (const attribute of [...old.attributes]) if (!child.hasAttribute(attribute.name)) old.removeAttribute(attribute.name);
      for (const attribute of [...child.attributes]) if (old.getAttribute(attribute.name) !== attribute.value) old.setAttribute(attribute.name, attribute.value);
      reconcile(old, child);
    } else if (old.nodeValue !== child.nodeValue) old.nodeValue = child.nodeValue;
  });
  while (parent.childNodes.length > children.length) parent.lastChild!.remove();
}

function schedule(state: Paint): void {
  if (state.pending || state.timer || state.rendered === state.value || !state.target.isConnected) return;
  const delay = state.formatted ? Math.max(0, FRAME_MS - (performance.now() - state.lastPaint)) : 0;
  if (!delay) void paint(state);
  else state.timer = window.setTimeout(() => { state.timer = 0; void paint(state); }, delay);
}

async function paint(state: Paint): Promise<void> {
  if (!state.target.isConnected) return;
  const value = state.value;
  state.pending = true;
  try {
    const html = await markdownHtml(value);
    if (!state.target.isConnected || state.value !== value) return;
    const fragment = document.createElement('template');
    fragment.innerHTML = html;
    decorateFileLinks(fragment.content, state.target);
    const restore = state.scroll ? preserveTranscriptPosition(state.scroll, state.target) : undefined;
    reconcile(state.target, fragment.content);
    state.target.classList.add('markdown');
    state.formatted = true; state.rendered = value; state.lastPaint = performance.now();
    restore?.();
  } catch {
    // Keep the response readable if a lazy renderer cannot be loaded.
    if (state.target.isConnected && state.value === value) {
      state.target.classList.remove('markdown'); state.target.textContent = value;
      state.formatted = false; state.rendered = value;
    }
  } finally { state.pending = false; schedule(state); }
}

/** One bounded paint per visible response; incoming chunks are never held for a turn to finish. */
export function streamMarkdown(target: HTMLElement, value: string, scroll?: HTMLElement): void {
  const context = captureFileLinkContext(target);
  let state = paints.get(target);
  if (!state) {
    state = { target, value: '', rendered: '', pending: false, timer: 0, lastPaint: 0, formatted: false, context };
    paints.set(target, state);
  }
  if (state.context !== context) {
    state.context = context; state.rendered = ''; state.formatted = false;
  }
  state.value = value; state.scroll = scroll;
  if (!value) {
    clearTimeout(state.timer); state.timer = 0; state.rendered = ''; state.formatted = false;
    target.replaceChildren(); target.classList.remove('markdown'); return;
  }
  if (!state.formatted) {
    const text = target.firstChild;
    if (text instanceof Text && value.startsWith(text.data)) text.appendData(value.slice(text.length));
    else target.textContent = value;
  }
  schedule(state);
}
