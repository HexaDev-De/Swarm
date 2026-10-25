const scrollRounding = new WeakMap<HTMLElement, { top: number; remainder: number }>();

function restoreScrollTop(transcript: HTMLElement, top: number): void {
  transcript.scrollTop = top;
  const actual = transcript.scrollTop, remainder = top - actual;
  // Some webviews round scrollTop to whole pixels. Carry only rounding error,
  // never a clamped scroll, so many async code-block paints cannot add up to a jump.
  scrollRounding.set(transcript, { top: actual, remainder: Math.abs(remainder) < 1 ? remainder : 0 });
}

function viewport(transcript: HTMLElement): { top: number; bottom: number; scale: number } {
  const rect = transcript.getBoundingClientRect();
  // Surfaces use border-box sizing. Keep fractional layout pixels: offsetHeight
  // rounds them, which magnifies anchoring errors over a long prepended page.
  const height = parseFloat(getComputedStyle(transcript).height);
  return { top: rect.top, bottom: rect.bottom, scale: height > 0 && rect.height > 0 ? rect.height / height : 1 };
}

/** Preserve the visible message or tool while content is inserted above it. */
export function captureTranscriptAnchor(transcript: HTMLElement): () => void {
  const { top, bottom, scale } = viewport(transcript);
  const scrollTop = transcript.scrollTop, height = transcript.scrollHeight;
  const previous = scrollRounding.get(transcript);
  const remainder = previous?.top === scrollTop ? previous.remainder : 0;
  const attributes = ['data-message-id', 'data-tool-key', 'data-turn-key', 'data-change-key'];
  const anchors = [...transcript.querySelectorAll<HTMLElement>(attributes.map(name => `[${name}]`).join(','))]
    .filter(node => node.getClientRects().length)
    .map(node => ({ node, rect: node.getBoundingClientRect() }))
    .filter(({ rect }) => rect.bottom > top && rect.top < bottom)
    .sort((a, b) => Math.abs(a.rect.top - top) - Math.abs(b.rect.top - top))
    .map(({ node, rect }) => {
      const attribute = attributes.find(name => node.hasAttribute(name))!;
      return { selector: `[${attribute}="${CSS.escape(node.getAttribute(attribute)!)}"]`, offset: (rect.top - top) / scale };
    });
  return () => {
    for (const anchor of anchors) {
      const node = transcript.querySelector<HTMLElement>(anchor.selector);
      if (!node?.getClientRects().length) continue;
      const current = viewport(transcript);
      restoreScrollTop(transcript, transcript.scrollTop + (node.getBoundingClientRect().top - current.top) / current.scale - anchor.offset + remainder);
      return;
    }
    restoreScrollTop(transcript, scrollTop + transcript.scrollHeight - height + remainder);
  };
}

/** Capture immediately before an async Markdown paint, respecting new user scrolls. */
export function preserveTranscriptPosition(transcript: HTMLElement, changed?: HTMLElement): () => void {
  const follow = transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 140
    && !document.activeElement?.closest('details');
  if (follow) return () => { transcript.scrollTop = transcript.scrollHeight; };
  // Streaming at the bottom cannot move content above it. Avoid scanning loaded
  // history on every token while the reader is elsewhere in the conversation.
  if (changed && changed.getBoundingClientRect().top >= transcript.getBoundingClientRect().bottom) return () => {};
  return captureTranscriptAnchor(transcript);
}
