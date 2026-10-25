import { expect, test, type Page } from '@playwright/test';

/* The GitHub view: repositories on the left, threads / pull requests / issues on the right,
 * and one-click work that opens as an ordinary linked thread. Runs on the preview's sample
 * GitHub (src/preview-github-hub.ts). */
const hub = (page: Page) => page.getByRole('region', { name: 'GitHub', exact: true });
const repoList = (page: Page) => hub(page).getByRole('listbox', { name: 'Repositories' });
const tab = (page: Page, name: RegExp) => hub(page).getByRole('tab', { name });
const row = (page: Page, number: number) => hub(page).locator(`.gh-item[data-gh-number="${number}"]`);
/** A thread opened from GitHub: a side panel next to it, like the board's. */
const side = (page: Page) => page.getByRole('region', { name: 'Task details', exact: true });
async function expectSideThread(page: Page): Promise<void> {
  await expect(hub(page)).toBeVisible();
  await expect(side(page)).toBeVisible();
  const [hubBox, sideBox] = [(await hub(page).boundingBox())!, (await side(page).boundingBox())!];
  expect(sideBox.x).toBeGreaterThanOrEqual(hubBox.x + hubBox.width - 1);
}

async function openHub(page: Page): Promise<void> {
  await page.goto('/');
  await page.locator('#navigation .task-item').first().waitFor();
  await page.getByRole('button', { name: 'GitHub view' }).click();
  await expect(hub(page)).toBeVisible();
  await expect(repoList(page).getByRole('option')).toHaveCount(4);
}

test('lists repositories and shows the chosen one’s threads, pull requests and issues', async ({ page }) => {
  await openHub(page);
  await expect(page.locator('.sidebar')).toBeHidden();
  await expect(page.locator('.conversation-pane')).toBeHidden();
  const orbit = repoList(page).getByRole('option', { name: /orbit\s*orbit-labs/ });
  await expect(orbit).toHaveAttribute('aria-selected', 'true');
  await expect(orbit.locator('.gh-local-dot')).toBeVisible();
  await expect(hub(page).getByRole('heading', { name: 'orbit-labs/orbit' })).toBeVisible();
  await expect(hub(page).locator('.gh-where')).toContainText('Projects/orbit');
  await expect(tab(page, /Threads/)).toHaveText('Threads3');
  await expect(tab(page, /Pull requests/)).toHaveText('Pull requests4');
  await expect(tab(page, /Issues/)).toHaveText('Issues5');
  await expect(hub(page).locator('.gh-thread')).toHaveCount(3);

  await tab(page, /Pull requests/).click();
  await expect(row(page, 130)).toContainText('Debounce library search and cache the index');
  await expect(row(page, 124).locator('.gh-review')).toHaveText('Changes requested');
  await expect(row(page, 124).locator('.gh-checks')).toHaveAttribute('data-checks', 'failing');
  await hub(page).getByRole('searchbox', { name: 'Search this tab' }).fill('vite');
  await expect(hub(page).locator('.gh-item')).toHaveCount(1);
  await hub(page).getByRole('searchbox', { name: 'Search this tab' }).fill('');

  await hub(page).getByRole('searchbox', { name: 'Filter repositories' }).fill('dot');
  await expect(repoList(page).getByRole('option')).toHaveCount(1);
  await hub(page).getByRole('searchbox', { name: 'Filter repositories' }).fill('');
  // Arrow keys move through repositories and select them.
  await orbit.focus();
  await page.keyboard.press('ArrowDown');
  await expect(repoList(page).getByRole('option', { name: /orbit-api/ })).toHaveAttribute('aria-selected', 'true');
  await expect(hub(page).getByRole('heading', { name: 'orbit-labs/orbit-api' })).toBeVisible();
  await expect(tab(page, /Pull requests/)).toHaveAttribute('aria-selected', 'true');
  await expect(row(page, 52)).toBeVisible();
});

test('Solve starts a linked thread that works on the issue', async ({ page }) => {
  await openHub(page);
  await tab(page, /Issues/).click();
  await row(page, 128).hover();
  await row(page, 128).getByRole('button', { name: 'Solve' }).click();
  await expectSideThread(page);
  const sent = page.locator('#transcript .user-message').last();
  await expect(sent).toContainText('Fix issue #128: Search index rebuilds on every keystroke');
  await expect(page.locator('#navigation .task-item.selected')).toContainText('Fix #128: Search index rebuilds on every keystroke');
  // Closing the side thread leaves GitHub as it was; the issue now links to its thread.
  await side(page).getByRole('button', { name: 'Close task details', exact: true }).click();
  await expect(side(page)).toBeHidden();
  await expect(hub(page)).toBeVisible();
  await expect(row(page, 128).getByRole('button', { name: 'Thread' })).toBeAttached();
  await expect(row(page, 128).getByRole('button', { name: 'Open in thread' })).toHaveCount(0);
  await row(page, 128).hover();
  await row(page, 128).getByRole('button', { name: 'Thread' }).click();
  await expectSideThread(page);
  await expect(page.locator('#navigation .task-item.selected')).toContainText('Fix #128');
  // Chatting continues right there, and Escape closes the panel.
  await page.locator('#prompt').fill('Also add a regression test');
  await page.locator('#prompt').press('Enter');
  await expect(page.locator('#transcript .user-message').last()).toContainText('Also add a regression test');
  await expect(hub(page)).toBeVisible();
});

test('pull requests open for review, or in a thread with the prompt ready', async ({ page }) => {
  await openHub(page);
  await tab(page, /Pull requests/).click();
  await row(page, 130).hover();
  await row(page, 130).getByRole('button', { name: 'Review' }).click();
  await expect(page.locator('#transcript .user-message').last()).toContainText('Review pull request #130 (feat/instant-search → main)');
  await expect(page.locator('#navigation .task-item.selected')).toContainText('Review #130');
  await expectSideThread(page);

  await row(page, 127).hover();
  await row(page, 127).getByRole('button', { name: 'More actions for #127' }).click();
  const menu = page.locator('.choice-menu');
  await expect(menu).toContainText('Merge when ready');
  await menu.getByText('Open in thread', { exact: true }).click();
  await expect(page.locator('#prompt')).toHaveValue('Let’s work on pull request #127: '.replace('’', "'"));
  await expect(page.locator('#prompt')).toBeFocused();
  await expect(page.locator('#transcript .user-message')).toHaveCount(0);
});

test('a thread starts from the Threads tab', async ({ page }) => {
  await openHub(page);
  const compose = hub(page).getByRole('textbox', { name: 'New thread' });
  await compose.fill('Add a reading-time estimate to each article');
  await compose.press('Enter');
  await expect(page.locator('#transcript .user-message').last()).toContainText('Add a reading-time estimate to each article');
  await expectSideThread(page);
  await expect(page.locator('#breadcrumb')).toContainText('orbit');
});

test('a repository that is only on GitHub can be cloned, then worked on', async ({ page }) => {
  await openHub(page);
  await repoList(page).getByRole('option', { name: /meadow/ }).click();
  await expect(hub(page).locator('.gh-empty')).toContainText('Not on this computer yet');
  await hub(page).locator('.gh-head').getByRole('button', { name: 'Clone to this computer' }).click();
  await expect(hub(page).locator('.gh-where')).toContainText('new-project/meadow');
  await expect(repoList(page).getByRole('option', { name: /meadow/ }).locator('.gh-local-dot')).toBeVisible();
  await expect(hub(page).getByRole('textbox', { name: 'New thread' })).toBeVisible();
  await tab(page, /Issues/).click();
  await row(page, 3).hover();
  await row(page, 3).getByRole('button', { name: 'Open in thread' }).click();
  await expect(page.locator('#prompt')).toHaveValue("Let's work on issue #3: ");
  await expectSideThread(page);
  // The side thread already knows its project: no welcome heading or mark.
  await expect(side(page).locator('.new-task-welcome')).toBeHidden();
});

test('the GitHub view stays out of the way when GitHub is turned off', async ({ page }) => {
  await page.addInitScript(() => { localStorage.setItem('pandacode.github', 'off'); localStorage.setItem('pandacode.view', 'github'); });
  await page.goto('/');
  await page.locator('#navigation .task-item').first().waitFor();
  await expect(page.getByRole('button', { name: 'GitHub view' })).toBeHidden();
  await expect(page.locator('.conversation-pane')).toBeVisible();
});

test('the GitHub view is reachable from Go to anything', async ({ page }) => {
  await page.goto('/');
  await page.locator('#navigation .task-item').first().waitFor();
  await page.locator('.search-trigger').click();
  await page.getByRole('textbox', { name: 'Search tasks and commands' }).fill('github');
  await page.locator('#command-results').getByRole('button', { name: 'GitHub' }).click();
  await expect(hub(page)).toBeVisible();
});

test('when gh fails, the view says why and how to fix it', async ({ page }) => {
  await page.route('**/src/preview.ts*', async route => {
    const response = await route.fetch();
    const body = (await response.text()).replace(/case "github_repos":/, 'case "github_repos": throw new Error("Failed to start GitHub CLI: No such file or directory (os error 2)");');
    await route.fulfill({ response, body });
  });
  await page.goto('/');
  await page.locator('#navigation .task-item').first().waitFor();
  await page.getByRole('button', { name: 'GitHub view' }).click();
  const empty = hub(page).locator('.gh-empty');
  await expect(empty).toContainText('Connect GitHub');
  await expect(empty).toContainText('brew install gh');
  await expect(empty.locator('.gh-error-detail')).toHaveText('Failed to start GitHub CLI: No such file or directory (os error 2)');
  await expect(empty.getByRole('button', { name: 'Try again' })).toBeVisible();
});

test('repositories not pushed to in 30 days wait under a collapsed Other', async ({ page }) => {
  await openHub(page);
  await expect(repoList(page).getByRole('option', { name: /tauri/ })).toHaveCount(0);
  const toggle = hub(page).getByRole('button', { name: /Other/ });
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(toggle).toContainText('1');
  const other = hub(page).getByRole('listbox', { name: 'Other repositories', includeHidden: true });
  await expect(other).toBeHidden();
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  // Ages and the Other count share one right edge, with or without the on-this-computer dot.
  const edges = await hub(page).locator('.gh-repo time, .gh-other-toggle small').evaluateAll(els => els.map(el => Math.round(el.getBoundingClientRect().right)));
  expect(new Set(edges).size).toBe(1);
  await other.getByRole('option', { name: /tauri/ }).click();
  await expect(hub(page).getByRole('heading', { name: 'you/tauri' })).toBeVisible();
  // The choice is remembered, and a search looks through both lists.
  await page.reload();
  // The app reopens in the GitHub view.
  await expect(hub(page)).toBeVisible();
  await expect(hub(page).getByRole('button', { name: /Other/ })).toHaveAttribute('aria-expanded', 'true');
  await hub(page).getByRole('button', { name: /Other/ }).click();
  await hub(page).getByRole('searchbox', { name: 'Filter repositories' }).fill('tau');
  await expect(hub(page).getByRole('listbox', { name: 'Other repositories' }).getByRole('option', { name: /tauri/ })).toBeVisible();
});

test('pull requests show their CI status, and Actions lists runs with a fix for failures', async ({ page }) => {
  await openHub(page);
  await tab(page, /Pull requests/).click();
  await expect(row(page, 130).locator('.gh-status')).toHaveText('Passed');
  await expect(row(page, 127).locator('.gh-status')).toHaveText('Pending');
  await expect(row(page, 124).locator('.gh-status')).toHaveAttribute('data-status', 'failed');

  await tab(page, /Actions/).click();
  await expect(tab(page, /Actions/)).toHaveText('Actions5');
  const run = (id: number) => hub(page).locator(`.gh-run[data-gh-run="${id}"]`);
  await expect(run(9120).locator('.gh-status')).toHaveText('In progress');
  await expect(run(9118).locator('.gh-status')).toHaveText('Queued');
  await expect(run(9105)).toContainText('feat/instant-search');
  await expect(run(9105).getByRole('button', { name: 'Fix' })).toHaveCount(0);
  await run(9087).hover();
  await run(9087).getByRole('button', { name: 'Fix' }).click();
  await expectSideThread(page);
  await expect(page.locator('#transcript .user-message').last()).toContainText('gh run view 9087 --log-failed');
  await expect(page.locator('#navigation .task-item.selected')).toContainText('Fix CI on devon/contrast');
});
