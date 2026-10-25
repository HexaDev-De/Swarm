import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { gzipSync } from 'node:zlib';

test('production entry stays below the 80 KiB compressed startup budget', async () => {
  const html = await readFile(new URL('../dist/index.html', import.meta.url), 'utf8');
  const entry = html.match(/<script[^>]+src="([^"]+)"/);
  assert.ok(entry, 'built entry script exists');
  const source = await readFile(new URL(`../dist${entry[1]}`, import.meta.url));
  assert.ok(gzipSync(source).length < 80 * 1024, 'initial JavaScript must stay under 80 KiB gzip');
  assert.ok(!html.includes('cdn.'), 'no third-party runtime scripts');
});

test('production entry does not eagerly initialize a terminal or a local HTTP server', async () => {
  const html = await readFile(new URL('../dist/index.html', import.meta.url), 'utf8');
  assert.ok(!/<link[^>]+modulepreload[^>]+(?:xterm|terminal)/.test(html));
  const config = JSON.parse(await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  assert.equal(config.build.frontendDist, '../dist');
  assert.equal(config.app.windows.length, 1);
  assert.ok(config.app.security.csp.includes("script-src 'self'"));
  // WebAssembly (the Markdown parser) may compile; JavaScript eval stays blocked.
  assert.match(config.app.security.csp, /script-src 'self' 'wasm-unsafe-eval';/);
  assert.doesNotMatch(config.app.security.csp, /(^|\s)'unsafe-eval'/);
});
