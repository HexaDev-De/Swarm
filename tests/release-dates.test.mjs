import test from 'node:test';
import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';

// The website changelog dates every releases/X.Y.Z.md from this map (set-version.mjs adds to it).
test('every release note has a changelog date', async () => {
  const dates = JSON.parse(await readFile(new URL('../website/src/data/release-dates.json', import.meta.url), 'utf8'));
  const versions = (await readdir(new URL('../releases/', import.meta.url))).filter(name => /^\d+\.\d+\.\d+\.md$/.test(name)).map(name => name.slice(0, -3));
  assert.deepEqual(versions.filter(version => !/^\d{4}-\d{2}-\d{2}$/.test(dates[version] ?? '')), []);
});
