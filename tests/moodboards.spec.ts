import { expect, test, type Page } from '@playwright/test';

const pixel = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC', 'base64');
const dialog = (page: Page) => page.locator('#dialog.moodboards-dialog');
async function open(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Project moodboards', exact: true }).click();
  await expect(dialog(page)).toBeVisible();
}
async function create(page: Page, name: string): Promise<void> {
  await dialog(page).getByRole('button', { name: 'New moodboard', exact: true }).click();
  await dialog(page).getByRole('textbox', { name: 'Moodboard name', exact: true }).fill(name);
  await dialog(page).getByRole('button', { name: 'Create moodboard', exact: true }).click();
  await expect(dialog(page).getByRole('textbox', { name: 'Board name', exact: true })).toHaveValue(name);
}
async function add(page: Page, name = 'Reference.png'): Promise<void> {
  await dialog(page).locator('.moodboard-file-input').setInputFiles({ name, mimeType: 'image/png', buffer: pixel });
  await expect(dialog(page).getByRole('textbox', { name: `Caption for ${name}`, exact: true })).toBeVisible();
  await expect(dialog(page).locator('.moodboard-image-preview img').first()).toHaveAttribute('src', /^data:image\/jpeg;base64,/);
}

test.describe('moodboards', () => {
  test.beforeEach(async ({ page }) => { page.on('pageerror', error => console.error(error)); await page.goto('/'); await expect(page.locator('.task-item.selected .task-title')).toHaveText('Make search feel instant'); await expect(page.locator('#prompt')).toBeEnabled(); });

  test('persists a board, image and caption after reload; attaches the validated board to the task', async ({ page }) => {
    await open(page); await create(page, 'Visual direction'); await add(page);
    await dialog(page).getByRole('textbox', { name: 'Caption for Reference.png' }).fill('Use this quiet layout and generous margins.');
    // Dismiss by Escape without manually blurring the caption: it still saves.
    await page.keyboard.press('Escape'); await expect(dialog(page)).toBeHidden();
    await open(page);
    await expect(dialog(page).getByRole('textbox', { name: 'Caption for Reference.png' })).toHaveValue('Use this quiet layout and generous margins.');
    await page.reload(); await expect(page.locator('.task-item.selected .task-title')).toHaveText('Make search feel instant'); await open(page);
    await expect(dialog(page).getByRole('textbox', { name: 'Board name', exact: true })).toHaveValue('Visual direction');
    await expect(dialog(page).getByRole('textbox', { name: 'Caption for Reference.png' })).toHaveValue('Use this quiet layout and generous margins.');
    await dialog(page).getByRole('button', { name: 'Attach to task', exact: true }).click();
    await expect(dialog(page)).toBeHidden();
    await expect(page.locator('#attachments')).toContainText('Visual direction');
  });

  test('supports rename, preview with Escape, image removal and explicit board deletion', async ({ page }) => {
    await open(page); await create(page, 'Draft'); await add(page);
    await dialog(page).getByRole('textbox', { name: 'Board name', exact: true }).fill('Polished direction');
    await dialog(page).getByRole('textbox', { name: 'Board name', exact: true }).press('Enter');
    await expect(dialog(page).getByRole('navigation', { name: 'Moodboards' })).toContainText('Polished direction');
    await dialog(page).getByRole('button', { name: 'View Reference.png', exact: true }).click();
    const preview = page.getByRole('dialog', { name: 'Image preview', exact: true });
    await expect(preview).toBeVisible(); await expect(preview.locator('img')).toHaveAttribute('src', /^data:image\/jpeg;base64,/);
    await page.keyboard.press('Escape'); await expect(preview).toBeHidden(); await expect(dialog(page)).toBeVisible();
    await dialog(page).getByRole('button', { name: 'Remove Reference.png', exact: true }).click();
    await expect(dialog(page).locator('.moodboard-image-card')).toHaveCount(0);
    await dialog(page).getByRole('button', { name: 'Delete moodboard', exact: true }).click();
    await expect(dialog(page)).toContainText('Delete this board and its images?');
    await dialog(page).getByRole('button', { name: 'Keep board', exact: true }).click();
    await expect(dialog(page).getByRole('textbox', { name: 'Board name', exact: true })).toHaveValue('Polished direction');
    await dialog(page).getByRole('button', { name: 'Delete moodboard', exact: true }).click();
    await dialog(page).getByRole('button', { name: 'Delete', exact: true }).click();
    await expect(dialog(page)).toContainText('Your visual starting point.');
  });

  test('accepts pasted screenshots and keeps the caption scoped while switching boards', async ({ page }) => {
    await open(page); await create(page, 'One');
    await dialog(page).locator('.moodboard-canvas').evaluate((element, encoded) => {
      const bytes = Uint8Array.from(atob(encoded), character => character.charCodeAt(0));
      const clipboard = new DataTransfer(); clipboard.items.add(new File([bytes], 'Pasted screenshot.png', { type: 'image/png' }));
      element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: clipboard, bubbles: true, cancelable: true }));
    }, pixel.toString('base64'));
    await expect(dialog(page).getByRole('textbox', { name: 'Caption for Pasted screenshot.png' })).toBeVisible();
    await create(page, 'Two');
    await dialog(page).getByRole('button', { name: 'One 1 image', exact: true }).click();
    await expect(dialog(page).getByRole('textbox', { name: 'Caption for Pasted screenshot.png' })).toBeVisible();
    await dialog(page).getByRole('textbox', { name: 'Caption for Pasted screenshot.png' }).fill('Only board one');
    await dialog(page).getByRole('button', { name: 'Two 0 images', exact: true }).click();
    await expect(dialog(page).locator('.moodboard-image-card')).toHaveCount(0);
    await dialog(page).getByRole('button', { name: 'One 1 image', exact: true }).click();
    await expect(dialog(page).getByRole('textbox', { name: 'Caption for Pasted screenshot.png' })).toHaveValue('Only board one');
  });

  test('sends an attached board by ID and restores the draft when the board was deleted', async ({ page }) => {
    await open(page); await create(page, 'Before implementation'); await add(page);
    const boardId = await dialog(page).locator('.moodboard-list-item[aria-pressed="true"]').getAttribute('data-mood-board');
    await dialog(page).getByRole('button', { name: 'Attach to task', exact: true }).click();
    const prompt = page.getByRole('textbox', { name: 'Message your coding agent' });
    await prompt.fill('Follow the attached visual direction'); await prompt.press('Enter');
    await expect(page.getByRole('button', { name: 'Stop task', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Stop task', exact: true })).toBeHidden();
    const saved = await page.evaluate(async () => {
      const module = await import('/src/preview.ts');
      return module.invoke('get_messages', { sessionId: 'session-1' });
    });
    const message = saved.messages.find((message: any) => message.content.startsWith('Follow the attached visual direction'));
    expect(message.content).toContain(boardId); expect(message.content).toContain('pandacode_read_moodboard_image');
    await open(page); await dialog(page).getByRole('button', { name: 'Attach to task', exact: true }).click();
    await page.evaluate(async id => {
      const module = await import('/src/preview.ts');
      const tasks = await module.invoke('list_sessions');
      const task = tasks.find((task: any) => task.id === 'session-1');
      await module.invoke('delete_moodboard', { projectId: task.projectId, boardId: id });
    }, boardId);
    await prompt.fill('Keep this draft after the missing reference fails'); await prompt.press('Enter');
    await expect(page.locator('#toast')).toContainText('Moodboard is unavailable');
    await expect(prompt).toHaveValue('Keep this draft after the missing reference fails');
    await expect(page.locator('#attachments')).toContainText('Before implementation');
    await expect(page.getByRole('button', { name: 'Stop task', exact: true })).toBeHidden();
    await expect(page.getByRole('log', { name: 'Conversation', exact: true })).not.toContainText('Keep this draft after the missing reference fails');
  });

  test('rejects unsupported images and stays within the viewport at large UI text size', async ({ page }) => {
    await page.evaluate(() => { document.documentElement.style.fontSize = '23px'; });
    await open(page); await create(page, 'A very long visual reference moodboard name for layout testing');
    await dialog(page).locator('.moodboard-file-input').setInputFiles({ name: 'script.svg', mimeType: 'image/svg+xml', buffer: Buffer.from('<svg><script>alert(1)</script></svg>') });
    await expect(dialog(page).getByRole('status')).toContainText('Choose PNG, JPEG or WebP');
    await expect(dialog(page).locator('.moodboard-image-card')).toHaveCount(0);
    const bounds = await dialog(page).boundingBox(); const viewport = page.viewportSize()!;
    expect(bounds!.x).toBeGreaterThanOrEqual(0); expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(viewport.width);
    expect(bounds!.y).toBeGreaterThanOrEqual(0); expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(viewport.height);
    expect(await dialog(page).evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
  });
});
