import { expect, test, type Page } from '@playwright/test';

// Settings → Agent teams: the classic Team setup switch, and named agent teams that appear in the
// model picker and orchestrate only their own models.
const errors = new WeakMap<Page, string[]>();
const calls = (page: Page, command: string): Promise<any[]> => page.evaluate(command => (window as any).__agentTeams.calls.filter((entry: any) => entry.command === command), command);
const settings = (page: Page) => page.getByRole('region', { name: 'Settings', exact: true });
const picker = (page: Page) => page.getByRole('dialog', { name: 'Harness and model', exact: true });
const modelButton = (page: Page) => page.getByRole('button', { name: 'Choose harness and model', exact: true });

test.afterEach(async ({ page }) => { expect(errors.get(page) ?? []).toEqual([]); });

async function install(page: Page): Promise<void> {
  const runtimeErrors: string[] = []; errors.set(page, runtimeErrors);
  page.on('pageerror', error => runtimeErrors.push(error.message));
  await page.route('**/src/preview.ts*', async route => {
    const response = await route.fetch(); const source = await response.text();
    const marker = /async function invoke\(command, args\) \{/;
    expect(source).toMatch(marker);
    await route.fulfill({ response, body: source.replace(marker, `async function invoke(command, args) {
      (window.__agentTeams ??= { calls: [] }).calls.push({ command, args: structuredClone(args) });
      return invokePreview(command, args);
    }
    async function invokePreview(command, args) {`) });
  });
  await page.goto('/');
  await page.locator('.task-item[data-id="session-2"]').click();
  await expect(page.locator('#prompt')).toHaveAttribute('data-session-id', 'session-2');
}

async function openAgents(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await settings(page).getByRole('tab', { name: 'Agent teams', exact: true }).click();
}
async function closeSettings(page: Page): Promise<void> {
  await settings(page).getByRole('button', { name: 'Back to the workspace', exact: true }).click();
  await expect(settings(page)).toBeHidden();
}

/** Builds a team from the first two listed models; the second one leads. */
async function createTeam(page: Page, name: string): Promise<{ lead: string; other: string }> {
  await settings(page).getByRole('button', { name: 'New team' }).click();
  await settings(page).getByRole('textbox', { name: 'Team name', exact: true }).fill(name);
  const boxes = settings(page).getByRole('group', { name: 'Team models' }).getByRole('checkbox');
  await expect(boxes.first()).toBeVisible();
  const [other, lead] = [(await boxes.nth(0).getAttribute('aria-label'))!.replace('Include ', ''), (await boxes.nth(1).getAttribute('aria-label'))!.replace('Include ', '')];
  await boxes.nth(0).check(); await boxes.nth(1).check();
  await settings(page).getByRole('radio', { name: `${lead} leads`, exact: true }).check();
  await expect(settings(page).getByText('2 models', { exact: true })).toBeVisible();
  await settings(page).getByRole('button', { name: 'Save team', exact: true }).click();
  await expect(settings(page).getByRole('listitem').filter({ hasText: name })).toBeVisible();
  return { lead, other };
}

test('the classic team setup switch brings back Architect & workers', async ({ page }) => {
  await install(page);
  await expect(page.getByRole('button', { name: 'Team', exact: true })).toBeVisible();
  await openAgents(page);
  await settings(page).getByRole('checkbox', { name: 'Classic team setup', exact: true }).check();
  await closeSettings(page);
  const classic = page.getByRole('button', { name: 'Architect & workers', exact: true });
  await expect(classic).toBeVisible();
  await classic.click();
  await expect(page.getByRole('heading', { name: 'Architect & workers', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Choose team profile', exact: true })).toBeVisible();
  // The choice survives a reload, and switching it off restores the automatic toggle.
  await page.reload();
  await page.locator('.task-item[data-id="session-2"]').click();
  await expect(page.getByRole('button', { name: 'Architect & workers', exact: true })).toBeVisible();
  await openAgents(page);
  await settings(page).getByRole('checkbox', { name: 'Classic team setup', exact: true }).uncheck();
  await closeSettings(page);
  await expect(page.getByRole('button', { name: 'Team', exact: true })).toBeVisible();
});

test('a named agent team is a choice in the model picker and lists only its models', async ({ page }) => {
  await install(page);
  await openAgents(page);
  const { lead, other } = await createTeam(page, 'Reviewers');
  await expect(settings(page).getByRole('listitem').filter({ hasText: 'Reviewers' })).toContainText(`2 models · led by ${lead}`);
  await closeSettings(page);

  await modelButton(page).click();
  const team = picker(page).getByRole('button', { name: 'Agent team · Reviewers', exact: true });
  await expect(team).toBeVisible();
  await team.click();
  await expect(picker(page)).toBeHidden();
  const configured = (await calls(page, 'architect_configure')).at(-1).args;
  expect(configured.config).toMatchObject({ auto: true, rosterOnly: true, name: 'Reviewers' });
  expect(configured.config.workers.map((worker: any) => worker.name)).toEqual([other, lead]);
  const [leadWorker] = configured.config.workers.filter((worker: any) => worker.name === lead);
  expect(configured.architect).toMatchObject({ provider: leadWorker.provider, model: leadWorker.model });
  await expect(modelButton(page)).toContainText('Reviewers');
  await expect(modelButton(page)).toContainText('2 models');
  await expect(page.getByRole('button', { name: 'Team', exact: true })).toHaveAttribute('aria-pressed', 'true');

  // The picker marks the team, not the lead's plain model, as current.
  await modelButton(page).click();
  await expect(picker(page).getByRole('button', { name: 'Agent team · Reviewers', exact: true })).toHaveAttribute('aria-current', 'true');
  await expect(picker(page).locator('[aria-current="true"]')).toHaveCount(1);
  // A plain model leaves the team for a single agent.
  await picker(page).locator('.model-menu-option:not(.model-menu-team):not([disabled])').first().click();
  await expect(picker(page)).toBeHidden();
  expect((await calls(page, 'architect_configure')).at(-1).args).toMatchObject({ config: null });
  await expect(page.getByRole('button', { name: 'Team', exact: true })).toHaveAttribute('aria-pressed', 'false');
  await expect(modelButton(page)).not.toContainText('Reviewers');
});

test('agent teams can be renamed, rechosen and deleted, and need a name and a model', async ({ page }) => {
  await install(page);
  await openAgents(page);
  await settings(page).getByRole('button', { name: 'New team' }).click();
  await settings(page).getByRole('button', { name: 'Save team', exact: true }).click();
  await expect(settings(page).getByRole('alert')).toHaveText('Give this team a name.');
  await settings(page).getByRole('textbox', { name: 'Team name', exact: true }).fill('Solo');
  await settings(page).getByRole('button', { name: 'Save team', exact: true }).click();
  await expect(settings(page).getByRole('alert')).toHaveText('Choose at least one model for this team.');
  await settings(page).getByRole('button', { name: 'Cancel', exact: true }).click();

  await createTeam(page, 'Builders');
  await settings(page).getByRole('button', { name: 'Edit Builders', exact: true }).click();
  const name = settings(page).getByRole('textbox', { name: 'Team name', exact: true });
  await expect(name).toHaveValue('Builders');
  await expect(settings(page).getByRole('group', { name: 'Team models' }).getByRole('checkbox', { checked: true })).toHaveCount(2);
  await name.fill('Shipping crew');
  await settings(page).getByRole('group', { name: 'Team models' }).getByRole('checkbox', { checked: true }).first().uncheck();
  await settings(page).getByRole('button', { name: 'Save team', exact: true }).click();
  const row = settings(page).getByRole('listitem').filter({ hasText: 'Shipping crew' });
  await expect(row).toContainText('1 model · led by');
  expect((await calls(page, 'architect_save_profile')).at(-1).args.setup.config.workers).toHaveLength(1);
  await settings(page).getByRole('button', { name: 'Delete Shipping crew', exact: true }).click();
  await expect(row).toBeHidden();
  expect((await calls(page, 'architect_profiles'))).not.toHaveLength(0);
  await closeSettings(page);
  await modelButton(page).click();
  await expect(picker(page).getByRole('button', { name: /^Agent team/ })).toHaveCount(0);
});
