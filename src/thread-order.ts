import type { Session } from './types';
import './thread-order.css';

export function compareThreadOrder(a: Session, b: Session): number {
  const rank = (task: Session) => task.sidebarOrder ?? Number.MAX_SAFE_INTEGER;
  const activity = (task: Session) => task.lastPromptAt == null ? task.lastResponseAt ?? task.createdAt : Math.max(task.lastPromptAt, task.lastResponseAt ?? 0);
  return rank(a) - rank(b) || activity(b) - activity(a) || a.id.localeCompare(b.id);
}

export function threadGroupRows(row: HTMLElement): HTMLElement[] {
  return [...(row.closest('[data-thread-group]')?.querySelectorAll<HTMLElement>('.task-item') ?? [])].filter(item => item.dataset.pinned === row.dataset.pinned);
}

/** Native drag events keep the actual row as the drag image; no per-row listeners. */
export function bindThreadReordering(nav: HTMLElement, options: {
  busy(): boolean;
  reorder(ids: string[], focusedId: string): void;
  dragging(active: boolean): void;
  menu(row: HTMLElement): void;
}): void {
  let source: HTMLElement | null = null, target: HTMLElement | null = null, after = false;
  let scroll = 0, speed = 0;
  const clearTarget = () => { target?.classList.remove('drop-before', 'drop-after'); target = null; };
  const finish = () => {
    const active = Boolean(source);
    source?.classList.remove('task-dragging'); source = null; clearTarget();
    cancelAnimationFrame(scroll); scroll = 0; speed = 0;
    if (active) options.dragging(false);
  };
  const autoScroll = () => { nav.scrollTop += speed; scroll = speed && source ? requestAnimationFrame(autoScroll) : 0; };
  nav.addEventListener('dragstart', event => {
    const row = (event.target as Element).closest<HTMLElement>('.task-item');
    if (!row || options.busy() || !event.dataTransfer) { event.preventDefault(); return; }
    source = row; event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('application/x-pandacode-thread', row.dataset.id!);
    row.classList.add('task-dragging'); options.dragging(true);
  });
  nav.addEventListener('dragover', event => {
    const row = (event.target as Element).closest<HTMLElement>('.task-item');
    if (!source || !row || options.busy() || row.dataset.pinned !== source.dataset.pinned || row.closest('[data-thread-group]') !== source.closest('[data-thread-group]')) { clearTarget(); speed = 0; return; }
    event.preventDefault(); if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    clearTarget(); target = row;
    const box = row.getBoundingClientRect(); after = event.clientY > box.top + box.height / 2;
    if (row !== source) row.classList.add(after ? 'drop-after' : 'drop-before');
    const bounds = nav.getBoundingClientRect();
    speed = event.clientY < bounds.top + 32 ? -7 : event.clientY > bounds.bottom - 32 ? 7 : 0;
    if (speed && !scroll) scroll = requestAnimationFrame(autoScroll);
  });
  nav.addEventListener('drop', event => {
    if (!source || !target || options.busy()) { finish(); return; }
    event.preventDefault(); event.stopPropagation();
    const id = source.dataset.id!, rows = threadGroupRows(source), current = rows.map(row => row.dataset.id!);
    const ids = current.filter(value => value !== id);
    if (target !== source) {
      ids.splice(ids.indexOf(target.dataset.id!) + Number(after), 0, id);
      if (ids.join('\0') !== current.join('\0')) options.reorder(ids, id);
    }
    finish();
  });
  nav.addEventListener('dragend', finish);
  nav.addEventListener('dragleave', event => { if (!nav.contains(event.relatedTarget as Node | null)) { clearTarget(); speed = 0; } });
  window.addEventListener('blur', finish);
  nav.addEventListener('contextmenu', event => {
    const row = (event.target as Element).closest<HTMLElement>('.task-item');
    if (row) { event.preventDefault(); options.menu(row); }
  });
  nav.addEventListener('keydown', event => {
    const row = (event.target as Element).closest<HTMLElement>('.task-item');
    if (!row) return;
    if (event.key === 'ContextMenu' || event.shiftKey && event.key === 'F10') {
      event.preventDefault(); options.menu(row); return;
    }
    if (!event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return;
    event.preventDefault(); if (options.busy()) return;
    const ids = threadGroupRows(row).map(item => item.dataset.id!), index = ids.indexOf(row.dataset.id!);
    const next = index + (event.key === 'ArrowUp' ? -1 : 1);
    if (next < 0 || next >= ids.length) return;
    [ids[index], ids[next]] = [ids[next], ids[index]];
    options.reorder(ids, row.dataset.id!);
  });
}
