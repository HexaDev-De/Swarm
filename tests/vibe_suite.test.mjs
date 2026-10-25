if (typeof window === 'undefined') {
  globalThis.window = { __TAURI_INTERNALS__: {} };
}
import test from 'node:test';
import assert from 'node:assert/strict';
import { compactCrashStack, formatCrashForAgent, getActiveCrash, clearCrash, reportCrash } from '../src/auto-heal.ts';

test('compactCrashStack preserves concise stacks and compacts excessive frames', () => {
  const shortStack = 'Error: boom\n    at foo.ts:10\n    at bar.ts:20';
  assert.equal(compactCrashStack(shortStack), shortStack);

  const longLines = [
    'Error: build failed',
    '    at src/index.ts:10:5',
    '    at src/app.ts:25:3',
    '    at src/router.ts:40:9',
    '    at src/handler.ts:50:2',
    '    at src/db.ts:60:1',
    '    at node_modules/vite/dist/node/chunks/dep-1.js:100',
    '    at node_modules/vite/dist/node/chunks/dep-2.js:200',
    '    at node_modules/vite/dist/node/chunks/dep-3.js:300',
    '    at node_modules/vite/dist/node/chunks/dep-4.js:400',
    '    at node_modules/vite/dist/node/chunks/dep-5.js:500',
    '    at node_modules/vite/dist/node/chunks/dep-6.js:600',
    '    at node_modules/vite/dist/node/chunks/dep-7.js:700',
    '    at node_modules/vite/dist/node/chunks/dep-8.js:800',
    '    at node_modules/vite/dist/node/chunks/dep-9.js:900',
    '    at node_modules/vite/dist/node/chunks/dep-10.js:1000',
    '    at node_modules/vite/dist/node/chunks/dep-11.js:1100',
    '    at final_clean_frame.ts:99:1',
  ];
  const compacted = compactCrashStack(longLines.join('\n'));
  assert.ok(compacted.includes('Error: build failed'));
  assert.ok(compacted.includes('omitted'));
  assert.ok(compacted.includes('final_clean_frame.ts:99:1'));
});

test('formatCrashForAgent formats clean instructions for coding agent', () => {
  const report = {
    source: 'browser',
    message: 'Cannot read properties of undefined (reading "map")',
    file: 'src/components/List.tsx',
    line: 42,
    stack: 'TypeError: Cannot read properties of undefined (reading "map")\n    at List (src/components/List.tsx:42:15)',
    timestamp: Date.now(),
  };
  const prompt = formatCrashForAgent(report);
  assert.ok(prompt.includes('Fix this browser error (src/components/List.tsx:42)'));
  assert.ok(prompt.includes('TypeError: Cannot read properties of undefined'));
  assert.ok(prompt.includes('Please inspect the affected files'));
});

test('CrashReport store manages active crash lifecycle safely', () => {
  clearCrash();
  assert.equal(getActiveCrash(), null);

  reportCrash({
    source: 'build',
    message: 'cargo check failed',
    stack: 'error[E0425]: cannot find value `xyz` in this scope',
  });

  const active = getActiveCrash();
  assert.ok(active);
  assert.equal(active.source, 'build');
  assert.equal(active.message, 'cargo check failed');

  clearCrash();
  assert.equal(getActiveCrash(), null);
});

test('extractArtifacts extracts HTML and SVG blocks from assistant messages', async () => {
  const { extractArtifacts } = await import('../src/live-artifacts.ts');
  const messages = [
    { role: 'user', content: 'Design a button and an icon.' },
    {
      role: 'assistant',
      content: 'Here is the button:\n\n```html\n<div class="card"><button>Click me</button></div>\n```\n\nAnd here is the icon:\n\n```svg\n<svg viewBox="0 0 24 24"><path d="M12 2L2 22h20L12 2z"/></svg>\n```',
    },
  ];

  const artifacts = extractArtifacts(messages);
  assert.equal(artifacts.length, 2);
  assert.equal(artifacts[0].type, 'html');
  assert.ok(artifacts[0].code.includes('<button>Click me</button>'));
  assert.equal(artifacts[1].type, 'svg');
  assert.ok(artifacts[1].code.includes('<svg viewBox="0 0 24 24">'));
});

test('renderInlineCheckpoint generates rich checkpoint markup with diff stats', async () => {
  const { renderInlineCheckpoint } = await import('../src/vibe-time-machine.ts');
  const snapshot = {
    turnIndex: 7,
    sessionId: 'test-session',
    commitSha: 'a1b2c3d4e5',
    treeSha: 'f6e5d4c3b2',
    timestamp: Math.floor(Date.now() / 1000) - 120,
    summary: 'Refactor auth service',
    filesCopied: [],
    filesChanged: ['src/auth.ts', 'src/types.ts'],
    additions: 45,
    deletions: 12,
    messageId: 104,
  };

  const html = renderInlineCheckpoint(snapshot, false);
  assert.ok(html.includes('Turn 7'));
  assert.ok(html.includes('2 files (+45 -12)'));
  assert.ok(html.includes('data-vibe-act="preview"'));
  assert.ok(html.includes('data-vibe-act="rewind"'));
  assert.ok(html.includes('f6e5d4c'));
  assert.ok(html.includes('src/auth.ts'));
});

test('DEVICE_PRESETS includes standard responsive design targets', async () => {
  const { DEVICE_PRESETS } = await import('../src/design-canvas.ts');
  assert.ok(DEVICE_PRESETS.desktop);
  assert.equal(DEVICE_PRESETS.desktop.width, 1440);
  assert.equal(DEVICE_PRESETS.desktop.height, 900);

  assert.ok(DEVICE_PRESETS.iphone);
  assert.equal(DEVICE_PRESETS.iphone.width, 393);
  assert.equal(DEVICE_PRESETS.iphone.height, 852);
  assert.equal(DEVICE_PRESETS.iphone.bezel, 'phone');
});
