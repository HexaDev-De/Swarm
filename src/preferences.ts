/** Keep preferences from earlier releases without replacing newer choices. */
export function migratePreferences(storage: Storage): void {
  const marker = 'pandacode.preferences-migrated';
  if (storage.getItem(marker) === '1') return;
  // This prefix is a compatibility identifier, never displayed as product branding.
  const legacy = 'kingcode.';
  const keys = Array.from({ length: storage.length }, (_, index) => storage.key(index));
  for (const key of keys) {
    if (!key?.startsWith(legacy)) continue;
    const next = 'pandacode.' + key.slice(legacy.length);
    const value = storage.getItem(key);
    if (value !== null && storage.getItem(next) === null) storage.setItem(next, value);
  }
  storage.setItem(marker, '1');
}
