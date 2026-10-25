((key) => {
  const old = globalThis[key];
  if (old && typeof old.cancel === 'function') old.cancel();
  const state = { result: null, cancel: () => finish(null) };
  globalThis[key] = state;
  const overlay = document.createElement('div');
  overlay.style.cssText = 'position:fixed;pointer-events:none;z-index:2147483647;border:2px solid #ffd633;background:rgba(255,214,51,.10);border-radius:4px;box-shadow:0 0 0 1px rgba(0,0,0,.25);display:none';
  const tooltip = document.createElement('div');
  tooltip.style.cssText = 'position:absolute;bottom:100%;left:0;margin-bottom:4px;padding:3px 6px;background:rgba(20,20,22,0.92);color:#ffd633;font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;font-size:11px;font-weight:600;line-height:1.2;border-radius:4px;pointer-events:none;white-space:nowrap;box-shadow:0 2px 6px rgba(0,0,0,0.3);border:1px solid rgba(255,214,51,0.3);';
  overlay.appendChild(tooltip);
  document.documentElement.appendChild(overlay);
  const sensitive = 'input,textarea,select,option,script,style,noscript,[contenteditable],[hidden],[aria-hidden="true"]';
  const clean = (value, max) => String(value || '').replace(/[\u0000-\u001f\u007f]/g, ' ').trim().slice(0, max);
  const escape = value => value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  function target(event) {
    return event.composedPath().find(node => node instanceof Element && node !== overlay && node !== tooltip);
  }
  function selector(element) {
    const parts = [];
    for (let node = element; node instanceof Element && parts.length < 6; node = node.parentElement) {
      let part = node.localName;
      if (node.id) { parts.unshift(part + '#' + CSS.escape(node.id)); break; }
      const parent = node.parentElement;
      if (parent) {
        const siblings = Array.from(parent.children).filter(sibling => sibling.localName === node.localName);
        if (siblings.length > 1) part += ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')';
      }
      parts.unshift(part);
    }
    return clean(parts.join(' > '), 1000);
  }
  function text(element) {
    if (element.closest(sensitive)) return '';
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    const parts = [];
    let length = 0, count = 0, node;
    while ((node = walker.nextNode()) && count++ < 500 && length < 1200) {
      if (node.parentElement?.closest(sensitive)) continue;
      const value = clean(node.textContent, 1200 - length);
      if (value) { parts.push(value); length += value.length + 1; }
    }
    return clean(parts.join(' '), 1200);
  }
  function finish(result) {
    document.removeEventListener('pointermove', move, true);
    document.removeEventListener('click', choose, true);
    document.removeEventListener('keydown', keydown, true);
    overlay.remove();
    state.result = result || { cancelled: true };
    state.cancel = null;
  }
  function move(event) {
    const element = target(event);
    if (!element) return;
    const rect = element.getBoundingClientRect();
    const tag = clean(element.localName, 80);
    const classes = element.className && typeof element.className === 'string'
      ? '.' + element.className.trim().split(/\s+/).filter(Boolean).slice(0, 2).join('.')
      : '';
    tooltip.textContent = '<' + tag + classes + '> (' + Math.round(rect.width) + ' × ' + Math.round(rect.height) + ')';
    Object.assign(overlay.style, {
      display: 'block',
      left: rect.left + 'px',
      top: rect.top + 'px',
      width: Math.max(0, rect.width) + 'px',
      height: Math.max(0, rect.height) + 'px',
    });
  }
  function choose(event) {
    const element = target(event);
    if (!element) return;
    event.preventDefault(); event.stopImmediatePropagation();
    const rect = element.getBoundingClientRect();
    const tag = clean(element.localName, 80);
    const content = text(element);
    const attrs = ['id', 'class', 'role', 'aria-label', 'data-testid'].flatMap(name => {
      const value = clean(element.getAttribute(name), 180);
      return value ? [' ' + name + '="' + escape(value) + '"'] : [];
    }).join('');
    let computedStyles = null;
    try {
      const computed = window.getComputedStyle(element);
      computedStyles = {
        display: clean(computed.display, 50),
        padding: clean(computed.padding, 50),
        margin: clean(computed.margin, 50),
        color: clean(computed.color, 50),
        background: clean(computed.backgroundColor || computed.background, 80),
        font: clean(computed.font || (computed.fontWeight + ' ' + computed.fontSize + ' ' + computed.fontFamily), 80),
      };
    } catch (_) {}
    finish({
      url: clean(location.href, 4096),
      title: clean(document.title, 300),
      selector: selector(element),
      tag,
      text: content,
      html: '<' + tag + attrs + '>' + escape(content) + '</' + tag + '>',
      bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      computedStyles: computedStyles || undefined,
    });
  }
  function keydown(event) {
    if (event.key === 'Escape') { event.preventDefault(); event.stopImmediatePropagation(); finish(null); }
  }
  document.addEventListener('pointermove', move, true);
  document.addEventListener('click', choose, true);
  document.addEventListener('keydown', keydown, true);
})(__PANDACODE_PICK_KEY__);
