/** Sidebar tree: pinned tasks first, then one folder per project with its tasks indented beneath it. */
import type { Project, Session } from './types';

/** Tasks a folder shows before "Show more". The selected task is always shown. */
export const FOLDER_LIMIT = 6;
/** With one project selected its list fills the sidebar, so it shows more before "Show more". */
export const PROJECT_LIMIT = 20;

export interface FolderGroup {
  project: Project;
  tasks: Session[];
  /** Newest prompt or response among the folder's tasks; orders folders. */
  latest: number;
}

export interface TreeInput {
  projects: readonly Project[];
  /** Visible, unpinned tasks already sorted in sidebar order. */
  tasks: readonly Session[];
  activity(task: Session): number | null;
  /** Keep folders without tasks (hidden while searching). */
  includeEmpty: boolean;
}

/** Folders with recent work first; within a folder, tasks keep their sidebar order. */
export function folderGroups({ projects, tasks, activity, includeEmpty }: TreeInput): FolderGroup[] {
  const byProject = new Map<string, Session[]>();
  for (const task of tasks) byProject.set(task.projectId, [...(byProject.get(task.projectId) ?? []), task]);
  return projects
    .map(project => {
      const own = byProject.get(project.id) ?? [];
      return { project, tasks: own, latest: Math.max(0, ...own.map(task => activity(task) ?? task.createdAt)) };
    })
    .filter(group => group.tasks.length || includeEmpty)
    .sort((a, b) => Number(b.tasks.length > 0) - Number(a.tasks.length > 0) || b.latest - a.latest || a.project.name.localeCompare(b.project.name));
}

/** The rows a folder shows: its first tasks, plus the selected one if it would otherwise be hidden. */
export function shownTasks(tasks: readonly Session[], expanded: boolean, selectedId: string, limit = FOLDER_LIMIT): Session[] {
  if (expanded || tasks.length <= limit) return [...tasks];
  const head = tasks.slice(0, limit);
  const selected = tasks.find(task => task.id === selectedId);
  return selected && !head.includes(selected) ? [...head, selected] : head;
}

/** Folder open/closed state lives per project across launches. */
export function readCollapsed(storage: Pick<Storage, 'getItem'>, key: string): Set<string> {
  try {
    const value: unknown = JSON.parse(storage.getItem(key) ?? '[]');
    return new Set(Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : []);
  } catch { return new Set(); }
}
