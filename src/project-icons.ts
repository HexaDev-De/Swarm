/* Each project's own app icon (its favicon or app icon, found natively by `project_icon`),
 * loaded once per project and cached for the session. Rows render the default icon until
 * it arrives, then the sidebar redraws. */
import { escape } from './dom';

const icons = new Map<string, string | null>();
const pending = new Set<string>();

/** The project's icon markup: its own image when it has one, else a quiet default tile. */
export function projectIconHtml(projectId: string, name: string, load: (projectId: string) => Promise<string | null>, loaded: () => void): string {
  const known = icons.get(projectId);
  if (known === undefined && !pending.has(projectId)) {
    pending.add(projectId);
    load(projectId)
      .then(icon => { icons.set(projectId, icon && icon.startsWith('data:image/') ? icon : null); })
      .catch(() => { icons.set(projectId, null); })
      .finally(() => { pending.delete(projectId); loaded(); });
  }
  if (known) return `<img class="task-app-icon" src="${escape(known)}" alt="" aria-hidden="true" draggable="false" decoding="sync">`;
  const initial = (name.trim()[0] ?? '·').toUpperCase();
  return `<span class="task-app-icon task-app-default" data-initial="${escape(initial)}" aria-hidden="true"></span>`;
}

/** Forget a project's icon so it is looked up again (after it changes on disk). */
export function forgetProjectIcon(projectId: string): void { icons.delete(projectId); }
