import { expect, test, type Page } from '@playwright/test';

const goal = {
  threadId: 'thread-a', objective: 'Build and verify the game', status: 'active', tokenBudget: 50000,
  tokensUsed: 1234, timeUsedSeconds: 86, createdAt: 100, updatedAt: 200,
};
const dialog = (page: Page) => page.locator('#command-fixture');
function localDate(offset: number): string {
  const day = new Date(); day.setDate(day.getDate() + offset);
  return `${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, '0')}-${String(day.getDate()).padStart(2, '0')}`;
}
async function setup(page: Page, kind: 'goal' | 'panel' | 'banner', data: unknown = goal, held = false): Promise<void> {
  await page.goto('/');
  await expect(page.locator('.task-item.selected')).toBeVisible();
  await page.evaluate(async ({ kind, data, held }) => {
    const moduleUrl = '/src/command-center.ts';
    const module = await import(moduleUrl);
    const target = document.createElement('dialog'); target.id = 'command-fixture'; document.body.append(target);
    const fixture = (window as any).__command = { kind, data, held, heldExecute: false, actions: [], loads: [], pending: [], module, target };
    const load = async (cursor?: string) => {
      fixture.loads.push(cursor ?? null);
      if (fixture.held) return new Promise(resolve => fixture.pending.push(resolve));
      if (fixture.loadError) throw new Error(fixture.loadError);
      return structuredClone(fixture.data);
    };
    const execute = async (action: unknown, payload?: unknown) => {
      fixture.actions.push(structuredClone(payload === undefined ? action : { action, payload }));
      if (fixture.heldExecute) await new Promise(resolve => { fixture.release = resolve; });
      if (fixture.executeError) throw new Error(fixture.executeError);
      if (kind === 'goal') {
        const command = action as any;
        fixture.data = command.action === 'clear' ? null : command.action === 'set'
          ? { ...fixture.data, ...command, threadId: 'thread-a', status: 'active', tokensUsed: fixture.data?.tokensUsed ?? 0, timeUsedSeconds: fixture.data?.timeUsedSeconds ?? 0 }
          : { ...fixture.data, status: command.action === 'pause' ? 'paused' : 'active' };
      }
      return structuredClone(fixture.data);
    };
    if (kind === 'goal') fixture.controller = module.showGoalDialog(target, { taskId: 'task-a', load, execute });
    else if (kind === 'panel') fixture.controller = module.showCommandCenter(target, { title: 'Harness controls', load, execute });
    else {
      fixture.controller = module.createGoalBanner({ open: () => fixture.actions.push('open'), pause: async () => { fixture.actions.push('pause'); if (fixture.heldExecute) { await new Promise((resolve, reject) => { fixture.release = resolve; fixture.reject = reject; }); } }, resume: async () => { fixture.actions.push('resume'); } });
      fixture.controller.element.style.cssText = 'position:fixed;top:100px;left:400px;width:600px;z-index:100';
      document.body.append(fixture.controller.element); fixture.controller.update(data);
    }
  }, { kind, data, held });
}
const actions = (page: Page) => page.evaluate(() => (window as any).__command.actions);

test('command center goal edits use exact budgets and native state without starting on input', async ({ page }) => {
  await setup(page, 'goal');
  await expect(dialog(page).getByRole('status')).toContainText('1,234 / 50,000 tokens · 1m 26s');
  await dialog(page).getByRole('textbox', { name: 'Goal objective' }).fill('  Ship a keyboard-accessible game  ');
  await dialog(page).getByRole('spinbutton', { name: 'Goal token budget' }).fill('75000');
  expect(await actions(page)).toEqual([]);
  await dialog(page).getByRole('button', { name: 'Save goal', exact: true }).click();
  expect(await actions(page)).toEqual([{ action: 'set', objective: 'Ship a keyboard-accessible game', tokenBudget: 75000 }]);
  await expect(dialog(page).getByRole('status')).toContainText('1,234 / 75,000 tokens');
  await dialog(page).getByRole('button', { name: 'Pause', exact: true }).click();
  await expect(dialog(page).getByRole('status')).toContainText('Paused');
  await dialog(page).getByRole('button', { name: 'Resume', exact: true }).click();
  await expect(dialog(page).getByRole('status')).toContainText('Working toward goal');
  await dialog(page).getByRole('button', { name: 'Clear goal', exact: true }).click();
  await expect(dialog(page).getByRole('status')).toContainText('No active goal');
  expect((await actions(page)).slice(1)).toEqual([{ action: 'pause' }, { action: 'resume' }, { action: 'clear' }]);
});

test('command center goal keeps typing through discovery and validates optional token budget', async ({ page }) => {
  await setup(page, 'goal', null, true);
  const objective = dialog(page).getByRole('textbox', { name: 'Goal objective' });
  await objective.fill('Use the draft I am typing');
  const retained = await objective.elementHandle();
  await expect(dialog(page).getByRole('button', { name: 'Start goal' })).toBeDisabled();
  await page.evaluate(goal => (window as any).__command.pending[0](goal), goal);
  await expect(objective).toHaveValue('Use the draft I am typing');
  expect(await retained!.evaluate(node => node === document.querySelector('#command-fixture textarea'))).toBe(true);
  const budget = dialog(page).getByRole('spinbutton', { name: 'Goal token budget' });
  await budget.fill('2.5');
  await dialog(page).getByRole('button', { name: 'Save goal' }).click();
  expect(await actions(page)).toEqual([]);
  await budget.fill('');
  await dialog(page).getByRole('button', { name: 'Save goal' }).click();
  expect(await actions(page)).toEqual([{ action: 'set', objective: 'Use the draft I am typing', tokenBudget: null }]);
});

test('command center goal retries errors with the same fields and ignores closed dialog responses', async ({ page }) => {
  await setup(page, 'goal');
  const objective = dialog(page).getByRole('textbox', { name: 'Goal objective' });
  await objective.fill('Keep this edited objective');
  await page.evaluate(() => { (window as any).__command.executeError = 'Goal service is temporarily unavailable'; });
  await dialog(page).getByRole('button', { name: 'Save goal' }).click();
  await expect(dialog(page).getByRole('alert')).toContainText('temporarily unavailable');
  await expect(objective).toHaveValue('Keep this edited objective');
  await page.evaluate(() => { const fixture = (window as any).__command; fixture.executeError = ''; fixture.heldExecute = true; });
  await dialog(page).getByRole('button', { name: 'Save goal' }).click();
  await expect(dialog(page).getByRole('button', { name: 'Updating…' })).toBeDisabled();
  await expect(objective).toBeDisabled();
  await dialog(page).getByRole('button', { name: 'Close', exact: true }).click();
  await page.evaluate(() => {
    const fixture = (window as any).__command;
    fixture.module.showCommandCenter(fixture.target, { title: 'Different task', load: async () => ({ kind: 'jobs', jobs: [] }), execute: async () => {} });
    fixture.release();
  });
  await expect(dialog(page).getByRole('heading')).toHaveText('Different task');
  await expect(dialog(page)).toContainText('No background jobs are running.');
  await expect(dialog(page).getByRole('textbox', { name: 'Goal objective' })).toHaveCount(0);
});

test('command center goal banner retains nodes as the native goal advances', async ({ page }) => {
  await setup(page, 'banner');
  const banner = page.getByRole('region', { name: 'Task goal' });
  const open = banner.getByRole('button', { name: 'Manage task goal' });
  const original = await open.elementHandle();
  await open.focus();
  await page.evaluate(goal => (window as any).__command.controller.update({ ...goal, tokensUsed: 2600, timeUsedSeconds: 132 }), goal);
  await expect(open).toContainText('2,600 / 50,000 tokens · 2m 12s');
  expect(await original!.evaluate(node => node === (window as any).__command.controller.element.querySelector('.goal-banner-open'))).toBe(true);
  await expect(open).toBeFocused();
  await banner.getByRole('button', { name: 'Pause goal' }).click();
  expect(await actions(page)).toEqual(['pause']);
  await page.evaluate(goal => (window as any).__command.controller.update({ ...goal, status: 'budgetLimited' }), goal);
  await expect(banner).toContainText('Token budget reached');
  await banner.getByRole('button', { name: 'Resume goal' }).click();
  await page.evaluate(goal => (window as any).__command.controller.update({ ...goal, status: 'complete' }), goal);
  await expect(banner).toContainText('Goal completed');
  await expect(banner.getByRole('button', { name: /Pause goal|Resume goal/ })).toBeHidden();
  await open.click();
  expect(await actions(page)).toEqual(['pause', 'resume', 'open']);
  await page.evaluate(() => (window as any).__command.controller.update(null));
  await expect(banner).toBeHidden();
});

test('command center goal banner scopes pending controls and errors to the original task', async ({ page }) => {
  await setup(page, 'banner');
  await page.evaluate(() => { (window as any).__command.heldExecute = true; });
  const banner = page.getByRole('region', { name: 'Task goal' });
  await banner.getByRole('button', { name: 'Pause goal' }).click();
  await expect(banner.getByRole('button', { name: 'Pause goal' })).toBeDisabled();
  await page.evaluate(goal => (window as any).__command.controller.update({ ...goal, threadId: 'another-task', objective: 'A separate goal' }), goal);
  await expect(banner.getByRole('button', { name: 'Pause goal' })).toBeEnabled();
  await page.evaluate(() => (window as any).__command.reject(new Error('Original task failed')));
  await expect(banner.getByRole('alert')).toBeHidden();
  await expect(banner).toContainText('A separate goal');
});

test('command center usage distinguishes unavailable values and renders untrusted text literally', async ({ page }) => {
  await setup(page, 'panel', { kind: 'usage', accountLabel: '<img src=x onerror=alert(1)>', plan: 'Pro', windows: [
    { id: 'short', label: '5-hour limit', usedPercent: 37, resetsAt: 1900000000 }, { id: 'week', label: 'Weekly limit', usedPercent: null },
  ], stats: [{ label: 'Total tokens', value: '81,420' }], daily: [{ date: localDate(-11), tokens: 800 }, { date: localDate(-4), tokens: 120 }, { date: 'not-a-date', tokens: 5 }], notes: ['Shared across your Codex account.'] });
  await expect(dialog(page).getByRole('progressbar')).toHaveCount(1);
  await expect(dialog(page).getByRole('progressbar', { name: '5-hour limit usage' })).toHaveAttribute('value', '37');
  await expect(dialog(page)).toContainText('Unavailable');
  await expect(dialog(page)).toContainText('<img src=x onerror=alert(1)>');
  await expect(dialog(page).locator('img')).toHaveCount(0);
  const calendar = dialog(page).getByRole('listbox', { name: 'Daily token use' });
  await expect(calendar.getByRole('option', { name: /: 120 tokens$/ })).toHaveAttribute('aria-selected', 'true');
  await expect(calendar.getByRole('option', { name: /: 800 tokens$/ })).toHaveAttribute('data-level', '4');
  await expect(calendar.getByRole('option', { name: /: 120 tokens$/ })).toHaveAttribute('data-level', '2');
  await expect(dialog(page).locator('.heatmap-summary')).toContainText('920 tokens · 2 active days');
  await calendar.focus(); await page.keyboard.press('ArrowLeft');
  await expect(calendar.getByRole('option', { name: /: 800 tokens$/ })).toHaveAttribute('aria-selected', 'true');
  await expect(dialog(page).locator('.heatmap-readout')).toContainText('800 tokens');
  await page.keyboard.press('End');
  await expect(calendar.locator('[aria-selected=true]')).toHaveAttribute('aria-label', /: No tokens$/);
  expect(await actions(page)).toEqual([]);
});

test('command center import selects exact items and preserves selection through filtering and failures', async ({ page }) => {
  await setup(page, 'panel', { kind: 'imports', items: [
    { id: 'migration-a', title: 'Repository instructions', description: 'README rules', source: 'Claude Code' },
    { id: 'migration-b', title: 'Project skills', description: 'Local skills', source: 'Kimi Code' },
  ] });
  const commit = dialog(page).getByRole('button', { name: 'Import selected' });
  await expect(commit).toBeDisabled();
  await dialog(page).getByRole('checkbox', { name: 'Repository instructions README rules Claude Code' }).check();
  await dialog(page).getByRole('searchbox').fill('skills');
  await expect(dialog(page).getByRole('checkbox')).toHaveCount(1);
  await page.evaluate(() => { (window as any).__command.executeError = 'Import unavailable'; });
  await commit.click();
  expect(await actions(page)).toEqual([{ action: 'import', payload: { ids: ['migration-a'] } }]);
  await expect(dialog(page).getByRole('alert')).toHaveText('Import unavailable');
  await dialog(page).getByRole('searchbox').fill('');
  await expect(dialog(page).getByRole('checkbox', { name: 'Repository instructions README rules Claude Code' })).toBeChecked();
  await expect(dialog(page).getByRole('checkbox', { name: 'Project skills Local skills Kimi Code' })).not.toBeChecked();
});

test('command center conversation paging deduplicates items and resumes only the selected task', async ({ page }) => {
  await setup(page, 'panel', { kind: 'conversations', mode: 'resume', cursor: 'page-two', items: [{ id: 'one', title: 'First conversation' }] });
  await dialog(page).getByRole('radio', { name: 'First conversation' }).check();
  await page.evaluate(() => { (window as any).__command.data = { kind: 'conversations', mode: 'resume', cursor: null, items: [{ id: 'one', title: 'First conversation' }, { id: 'two', title: 'Second conversation' }] }; });
  await dialog(page).getByRole('button', { name: 'Load more conversations' }).click();
  await expect(dialog(page).getByRole('radio')).toHaveCount(2);
  await expect(dialog(page).getByRole('radio', { name: 'First conversation' })).toBeChecked();
  await dialog(page).getByRole('radio', { name: 'Second conversation' }).check();
  await dialog(page).getByRole('button', { name: 'Resume conversation' }).click();
  expect(await actions(page)).toEqual([{ action: 'resume', payload: { id: 'two' } }]);
  expect(await page.evaluate(() => (window as any).__command.loads)).toEqual([null, 'page-two']);
});

test('command center preferences sends changed fields only and retains edits on failure', async ({ page }) => {
  await setup(page, 'panel', { kind: 'preferences', fields: [
    { id: 'personality', label: 'Response style', type: 'select', value: 'none', options: [{ value: 'none', label: 'Default' }, { value: 'friendly', label: 'Friendly' }] },
    { id: 'feature', label: 'Experimental feature', type: 'toggle', value: false },
    { id: 'managed', label: 'Managed preference', type: 'text', value: 'Required', disabled: true },
  ] });
  await expect(dialog(page).getByRole('button', { name: 'Save changes' })).toBeDisabled();
  await expect(dialog(page).getByRole('textbox', { name: 'Managed preference' })).toBeDisabled();
  await dialog(page).getByRole('combobox', { name: 'Response style' }).selectOption('friendly');
  await page.evaluate(() => { (window as any).__command.executeError = 'Preference is managed by your organization'; });
  await dialog(page).getByRole('button', { name: 'Save changes' }).click();
  expect(await actions(page)).toEqual([{ action: 'set_preferences', payload: { values: { personality: 'friendly' } } }]);
  await expect(dialog(page).getByRole('combobox', { name: 'Response style' })).toHaveValue('friendly');
  await expect(dialog(page).getByRole('alert')).toContainText('managed');
});

test('command center jobs targets one process and renders result panels', async ({ page }) => {
  await setup(page, 'panel', { kind: 'jobs', jobs: [{ id: 'task-a:123', title: 'Local test server', status: 'Running', detail: 'Port 3000', canStop: true }, { id: 'task-b:321', title: 'Protected job', status: 'Running', canStop: false }] });
  await expect(dialog(page).getByRole('button', { name: /^Stop / })).toHaveCount(1);
  await page.evaluate(() => { (window as any).__command.data = { kind: 'result', title: 'Server stopped', description: 'The selected process has exited.', status: 'Stopped' }; });
  await dialog(page).getByRole('button', { name: 'Stop Local test server' }).click();
  expect(await actions(page)).toEqual([{ action: 'stop', payload: { id: 'task-a:123' } }]);
  await expect(dialog(page)).toContainText('The selected process has exited.');
});

test('command center remains readable at enlarged UI size and reduced motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await setup(page, 'goal');
  await page.setViewportSize({ width: 550, height: 760 });
  await page.evaluate(() => { document.documentElement.style.fontSize = '20px'; });
  const bounds = await dialog(page).boundingBox();
  expect(bounds!.x).toBeGreaterThanOrEqual(0);
  expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(550);
  expect(await dialog(page).evaluate(node => node.scrollWidth <= node.clientWidth)).toBe(true);
  await expect(dialog(page).getByRole('button', { name: 'Save goal' })).toBeVisible();
  await dialog(page).getByRole('button', { name: 'Save goal' }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: '/private/tmp/pandacode-command-center-goal.png' });
});
