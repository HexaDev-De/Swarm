import { test, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync } from 'node:fs';

/* Captures the README screenshots from the browser preview: full windows and close-ups at
 * 2x, each framed with a soft shadow on a transparent background and saved as WebP in docs/screenshots/. The sample
 * data comes from showcase.js and the replacements below, applied only while capturing. */
const out = new URL('../../docs/screenshots/', import.meta.url).pathname;
const raw = '/tmp/pandacode-screenshots/raw/';
const showcase = readFileSync(new URL('./showcase.js', import.meta.url), 'utf8');

// Preview-only wording reads as placeholder text in a screenshot; swap in realistic copy.
const copy: [RegExp | string, string][] = [
  [/Sample Claude Code model metadata\./g, 'Anthropic model via Claude Code.'],
  [/Sample Codex model metadata\./g, 'OpenAI model via Codex.'],
  [/Sample configured Kimi model\./g, 'Moonshot model via Kimi Code.'],
  [/Sample output — no command was executed\./g, ''],
  [/\/\/ Browser preview — sample source\\n/g, ''],
  [/const output = (["'])This is the browser preview\./, 'const output = window.__showcaseReply?.shift() ?? $1This is the browser preview.'],
  // Stream a little slower, so a capture lands mid-reply.
  ['setTimeout(tick, 24)', 'setTimeout(tick, 90)'],
];
const architectCopy: [RegExp | string, string][] = [
  [/`### Sample architect review[\s\S]*?\$\{sampleNotice\}`/,
    '`### Command palette shipped\\n\\nThe architect split the work between ${count} workers: one built the palette and its fuzzy search, the other wired every shortcut and the cheat sheet. Both checked in, and the combined changes are ready for review.`'],
];

async function prepare(page: Page, theme: 'light' | 'dark', scene: string): Promise<void> {
  await page.addInitScript(([t, s]) => {
    localStorage.setItem('pandacode.theme', t);
    localStorage.setItem('pandacode.wallpaperPreferences', JSON.stringify({ enabled: true, visibility: 20, blur: 0, set: 'automatic', pinned: s }));
  }, [theme, scene]);
  const rewrite = (pattern: string, replacements: [RegExp | string, string][], append = '') => page.route(pattern, async route => {
    const response = await route.fetch();
    let body = await response.text();
    for (const [from, to] of replacements) body = body.replace(from, to);
    await route.fulfill({ response, body: body + append });
  });
  await rewrite('**/src/preview.ts*', copy, `\n${showcase}\n`);
  await rewrite('**/src/preview-architect.ts*', architectCopy);
  await rewrite('**/src/preview-turn-changes.ts*', [[/Browser preview sample only\./g, 'Matching ignores case and surrounding spaces.']]);
  await page.goto('/');
  await page.locator('#navigation .task-item').first().waitFor();
}

/** Hides what only the browser preview shows (its status label, notices, toasts). */
async function tidy(page: Page): Promise<void> {
  await page.evaluate(() => document.querySelectorAll<HTMLElement>('.preview-status, .preview-notice, #toast').forEach(node => { node.style.visibility = 'hidden'; }));
  await page.mouse.move(2, 940);
  await page.waitForTimeout(400);
}
const open = (page: Page, title: string) => page.locator('#navigation .task-item', { hasText: title }).click();
const reply = (page: Page, text: string) => page.evaluate(value => { ((window as any).__showcaseReply ??= []).push(value); }, text);

async function shot(page: Page, name: string, clip?: { x: number; y: number; width: number; height: number }, settle = true): Promise<void> {
  if (settle) await tidy(page);
  else await page.evaluate(() => document.querySelectorAll<HTMLElement>('.preview-status, .preview-notice, #toast').forEach(node => { node.style.visibility = 'hidden'; }));
  await page.screenshot({ path: `${raw}${name}.png`, clip });
}
async function element(page: Page, name: string, selector: string, pad = 0, maxHeight = Infinity): Promise<void> {
  await tidy(page);
  const box = (await page.locator(selector).first().boundingBox())!;
  await page.screenshot({ path: `${raw}${name}.png`, clip: { x: Math.max(0, box.x - pad), y: Math.max(0, box.y - pad), width: box.width + pad * 2, height: Math.min(maxHeight, box.height + pad * 2) } });
}

/** Frames a capture: rounded corners and a soft shadow on a transparent background, so it
 * sits cleanly on any page (GitHub's light and dark themes alike). */
async function frame(page: Page, name: string, { dark = true, zoom = false } = {}): Promise<void> {
  const image = readFileSync(`${raw}${name}.png`).toString('base64');
  const pad = zoom ? 40 : 56;
  const edge = dark ? 'rgba(255,255,255,.1)' : 'rgba(0,0,0,.08)';
  await page.setViewportSize({ width: 1700, height: 1200 });
  await page.setContent(`<!doctype html><html><body style="margin:0;background:transparent"><div id="frame" style="display:inline-block;padding:${pad}px ${pad}px ${pad + 16}px"><img src="data:image/png;base64,${image}" style="display:block;border-radius:${zoom ? 14 : 12}px;box-shadow:0 24px 60px -24px rgba(0,0,0,.42),0 6px 16px -8px rgba(0,0,0,.18),0 0 0 1px ${edge}"></div></body></html>`);
  // Shown at half its pixel size, so the 2x frame keeps every captured pixel.
  await page.locator('#frame img').evaluate(async (img: HTMLImageElement) => { await img.decode(); img.style.width = `${img.naturalWidth / 2}px`; });
  await page.locator('#frame').screenshot({ path: `${raw}${name}-framed.png`, omitBackground: true });
  execFileSync('cwebp', ['-quiet', '-q', '88', '-m', '6', ...(zoom ? [] : ['-resize', '2000', '0']), `${raw}${name}-framed.png`, '-o', `${out}${name}.webp`]);
}

test.beforeAll(() => { rmSync(raw, { recursive: true, force: true }); mkdirSync(raw, { recursive: true }); mkdirSync(out, { recursive: true }); });

test('conversation', async ({ page }) => {
  await prepare(page, 'dark', 'mon-night');
  await open(page, 'Make search feel instant');
  await page.locator('#transcript .agent-message').last().waitFor();
  await shot(page, 'conversation');
  await page.getByRole('button', { name: 'Choose harness and model', exact: true }).click();
  await page.locator('.model-menu').waitFor();
  await page.waitForTimeout(500);
  await element(page, 'model-picker', '.model-menu');
  await page.keyboard.press('Escape');
  await frame(page, 'conversation');
  await frame(page, 'model-picker', { zoom: true });
});

test('streaming', async ({ page }) => {
  await prepare(page, 'dark', 'wed-night');
  await open(page, 'Add keyboard shortcuts');
  await reply(page, 'I’ll add a command palette on **⌘K** and move the navigation into it.\n\n1. **Palette.** A fuzzy finder over projects, tasks and commands, with recent items first.\n2. **Shortcuts.** Every action gets a binding, listed in a `?` cheat sheet.\n3. **Focus.** Arrow keys move through results; Escape returns focus where it was.\n\nStarting with the palette component and its tests.');
  await page.locator('#prompt').fill('Make every navigation action reachable from the keyboard, with a ⌘K palette');
  await page.locator('#prompt').press('Enter');
  // Mid-reply: the live row is still streaming when the capture is taken.
  await page.locator('#transcript .live-text').filter({ hasText: 'Shortcuts.' }).waitFor();
  const pane = (await page.locator('.conversation-pane').boundingBox())!;
  await shot(page, 'streaming', { x: pane.x + 150, y: 60, width: pane.width - 300, height: 470 }, false);
  await frame(page, 'streaming', { zoom: true });
});

test('board', async ({ page }) => {
  await prepare(page, 'dark', 'sat-night');
  await page.getByRole('button', { name: 'Board view' }).click();
  await page.locator('.board-card, .board-task').first().waitFor();
  await shot(page, 'board');
  await frame(page, 'board');
});

test('agent team', async ({ page }) => {
  await prepare(page, 'dark', 'thu-night');
  await open(page, 'Refine the navigation');
  await page.locator('[data-action="team-toggle"]').click();
  await page.locator('[data-action="team-toggle"][aria-pressed="true"]').waitFor();
  await page.waitForTimeout(600);
  await page.locator('#prompt').fill('Ship a command palette with fuzzy search, and a shortcut for every navigation action');
  await page.locator('#prompt').press('Enter');
  await page.getByText('Team complete').waitFor({ timeout: 30_000 });
  await shot(page, 'agent-team');
  await frame(page, 'agent-team');
});

test('simulator and diff', async ({ page }) => {
  await prepare(page, 'dark', 'sun-night');
  await open(page, 'Make search feel instant');
  if (!(await page.locator('#inspector').isVisible())) await page.locator('#inspector-toggle').click();
  await page.locator('.inspector-tabs [data-action="tab-simulator"]').click();
  await page.waitForTimeout(3500);
  await shot(page, 'simulator');
  await page.locator('#inspector-toggle').click();
  await page.getByRole('button', { name: 'Open diff' }).first().click();
  await page.waitForTimeout(1200);
  await element(page, 'diff', '#dialog .dialog-plate, #dialog', 0, 330);
  await frame(page, 'simulator');
  await frame(page, 'diff', { zoom: true });
});

test('bot', async ({ page }) => {
  await prepare(page, 'dark', 'fri-night');
  await page.getByRole('region', { name: 'Bots', exact: true }).getByRole('button', { name: 'Add bot', exact: true }).click();
  const bots = page.getByRole('dialog', { name: 'Bots' });
  await bots.getByRole('button', { name: /Chief of Staff/ }).click();
  await bots.getByRole('textbox', { name: 'Name' }).fill('Scout');
  await bots.getByRole('button', { name: 'Create bot' }).click();
  await page.locator('#breadcrumb .bot-crumb').waitFor();
  await reply(page, 'Here’s your morning digest:\n\n- **orbit:** the instant-search PR (#130) merged and CI is green.\n- **Issues:** #131, the shortcut cheat sheet, has two new comments from Devon.\n- **Calendar:** demo with Sam on Friday at 14:00. The deck is in your Drafts.\n\nShall I send this every weekday at 9?');
  await page.locator('#prompt').fill('Good morning! What did I miss?');
  await page.locator('#prompt').press('Enter');
  await page.locator('#transcript .agent-message').last().waitFor();
  await page.waitForTimeout(6000);
  // The preview attaches sample file changes to every turn; a digest has none.
  await page.evaluate(() => document.querySelectorAll<HTMLElement>('.turn-changes-row').forEach(node => { node.style.display = 'none'; }));
  await shot(page, 'bot');
  await page.locator('#breadcrumb').getByRole('button', { name: 'Scout settings', exact: true }).click();
  await page.getByRole('dialog', { name: 'Scout settings' }).getByRole('tab', { name: /Channels/ }).click();
  await page.waitForTimeout(700);
  await element(page, 'bot-settings', '#dialog .dialog-plate, #dialog');
  await frame(page, 'bot');
  await frame(page, 'bot-settings', { zoom: true });
});
