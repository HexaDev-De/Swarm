import type { Message, TurnChanges, TurnChangeDiff } from './types';

type Snapshot = { sessionId: string; messageId: number; summary: TurnChanges; details: TurnChangeDiff[] };
const key = 'pandacode.preview.turn-changes';
const limit = 48;
const samples = [
  {
    path: 'src/components/search.ts', status: 'modified' as const, additions: 2, deletions: 1,
    patch: 'diff --git a/src/components/search.ts b/src/components/search.ts\n--- a/src/components/search.ts\n+++ b/src/components/search.ts\n@@ -1,4 +1,5 @@\n export function search(query: string) {\n-  return items.filter(item => item.includes(query));\n+  const normalized = query.trim().toLowerCase();\n+  return items.filter(item => item.toLowerCase().includes(normalized));\n }\n // Browser preview sample only.\n',
  },
  {
    path: 'src/lib/search-index.ts', status: 'added' as const, additions: 3, deletions: 0,
    patch: 'diff --git a/src/lib/search-index.ts b/src/lib/search-index.ts\nnew file mode 100644\n--- /dev/null\n+++ b/src/lib/search-index.ts\n@@ -0,0 +1,3 @@\n+// Browser preview sample only.\n+export const createSearchIndex = (items: string[]) =>\n+  new Map(items.map(item => [item.toLowerCase(), item]));\n',
  },
  {
    path: 'README.md', status: 'modified' as const, additions: 1, deletions: 1,
    patch: 'diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1,2 +1,2 @@\n # Browser preview sample\n-Simple search demo.\n+Search supports trimmed, case-insensitive queries.\n',
  },
];

/** Immutable sample diffs demonstrate review without touching the user's checkout. */
export function createPreviewTurnChanges(storage: Pick<Storage, 'getItem' | 'setItem'> = localStorage) {
  let snapshots: Snapshot[] = [];
  try {
    const raw = storage.getItem(key);
    const saved = JSON.parse(raw && raw.length <= 256 * 1024 ? raw : '[]');
    if (Array.isArray(saved)) snapshots = saved.slice(-limit).filter((entry): entry is Snapshot =>
      entry && typeof entry.sessionId === 'string' && Number.isSafeInteger(entry.messageId)
      && typeof entry.summary?.id === 'string' && Array.isArray(entry.summary.files) && entry.summary.files.length <= 100
      && Array.isArray(entry.details) && entry.details.length <= 100
      && entry.details.every((detail: TurnChangeDiff) => typeof detail?.path === 'string' && typeof detail.patch === 'string' && detail.patch.length <= 64 * 1024));
  } catch { /* Private or invalid storage still permits an in-memory preview. */ }
  function finish(user: Message): void {
    if (user.role !== 'user' && user.role !== 'changes') return;
    let saved = snapshots.find(snapshot => snapshot.sessionId === user.sessionId && snapshot.messageId === user.id);
    if (!saved) {
      saved = { sessionId: user.sessionId, messageId: user.id,
        summary: { attributed: true, id: crypto.randomUUID(), files: samples.map(({ path, status, additions, deletions }) => ({ path, status, additions, deletions })), additions: 6, deletions: 2, truncated: false, concurrent: false },
        details: samples.map(({ path, patch }) => ({ path, patch, binary: false, truncated: false })),
      };
      snapshots.push(saved); snapshots = snapshots.slice(-limit);
      try { storage.setItem(key, JSON.stringify(snapshots)); } catch { /* In-memory snapshots remain available. */ }
    }
    user.turnChanges = structuredClone(saved.summary);
  }
  function diff(sessionId: unknown, changeId: unknown, path: unknown): TurnChangeDiff {
    const snapshot = snapshots.find(entry => entry.sessionId === sessionId && entry.summary.id === changeId);
    if (!snapshot) throw new Error('This preview change snapshot is no longer available.');
    const detail = snapshot.details.find(detail => detail.path === path);
    if (!detail) throw new Error('File not found in this change snapshot.');
    return structuredClone(detail);
  }
  function remove(sessionIds: string[]): void {
    snapshots = snapshots.filter(snapshot => !sessionIds.includes(snapshot.sessionId));
    try { storage.setItem(key, JSON.stringify(snapshots)); } catch { /* Preview storage is optional. */ }
  }
  return { finish, diff, remove };
}
