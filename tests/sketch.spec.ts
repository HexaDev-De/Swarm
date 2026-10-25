import { expect, test, type Page } from '@playwright/test';

// The mockup editor: drawing, components, text, selection, history, export to an image
// attachment, and re-editing an attached sketch.
const previewBody = (source: string) => source + '\nwindow.__pandacodePreviewUrl = import.meta.url;\n';
const prompt = (page: Page) => page.getByRole('textbox', { name: 'Message your coding agent' });
const dialog = (page: Page) => page.locator('#dialog.sketch-dialog');
const canvas = (page: Page) => page.getByRole('img', { name: /Mockup canvas/ });
const chips = (page: Page) => page.locator('#attachments .prompt-chip');
const calls = (page: Page, command: string): Promise<any[]> => page.evaluate(command => ((window as any).__sketchCalls ?? []).filter((entry: any) => entry.command === command), command);

async function probe(page: Page): Promise<void> {
  await page.route('**/src/preview.ts*', async route => {
    const response = await route.fetch();
    const source = await response.text();
    const marker = /switch\s*\(command\)\s*\{/;
    expect(source).toMatch(marker);
    await route.fulfill({ response, body: previewBody(source.replace(marker, `(window.__sketchCalls ??= []).push({ command, args: structuredClone(args) }); switch (command) {`)) });
  });
  await page.goto('/');
  await expect(page.locator('.task-item.selected')).toHaveAttribute('data-id', 'session-1');
  await expect(prompt(page)).toBeEnabled();
}

async function openSketch(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Attach files for context' }).click();
  await page.getByRole('dialog', { name: 'Attach', exact: true }).getByRole('menuitem', { name: /Sketch a mockup…/ }).click();
  await expect(dialog(page)).toBeVisible();
  await expect(canvas(page)).toBeVisible();
}

async function drag(page: Page, from: [number, number], to: [number, number]): Promise<void> {
  const box = (await canvas(page).boundingBox())!;
  await page.mouse.move(box.x + box.width * from[0], box.y + box.height * from[1]);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * to[0], box.y + box.height * to[1], { steps: 5 });
  await page.mouse.up();
}

test('draws shapes and components, edits text, and keeps history in step', async ({ page }) => {
  await probe(page);
  await openSketch(page);
  await expect(page.getByRole('button', { name: 'Rectangle (R)' })).toHaveAttribute('aria-pressed', 'true');
  await drag(page, [0.1, 0.1], [0.4, 0.4]);
  await expect(canvas(page)).toHaveAccessibleName('Mockup canvas, 1 shape, rect selected');
  // Finishing a shape hands back to Select.
  await expect(page.getByRole('button', { name: 'Select (V)' })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Add button' }).click();
  await page.getByRole('button', { name: 'Add nav bar' }).click();
  await expect(canvas(page)).toHaveAccessibleName('Mockup canvas, 3 shapes, navbar selected');
  // Enter edits the selected component's label.
  await canvas(page).focus();
  await page.keyboard.press('Enter');
  const editor = dialog(page).getByRole('textbox', { name: 'Label' });
  await expect(editor).toBeFocused();
  await editor.fill('Acme');
  await page.keyboard.press('Enter');
  await expect(editor).toHaveCount(0);
  await expect(dialog(page)).toBeVisible();
  // The text tool places a caption; Escape ends typing without closing the sketch.
  await page.getByRole('button', { name: 'Text (T)' }).click();
  const box = (await canvas(page).boundingBox())!;
  await page.mouse.click(box.x + box.width * 0.6, box.y + box.height * 0.8);
  const text = dialog(page).getByRole('textbox', { name: 'Text', exact: true });
  await expect(text).toBeFocused();
  await page.keyboard.type('Empty state');
  await page.keyboard.press('Escape');
  await expect(text).toHaveCount(0);
  await expect(dialog(page)).toBeVisible();
  await expect(canvas(page)).toHaveAccessibleName('Mockup canvas, 4 shapes, text selected');
  // Undo and redo walk the same steps; Delete removes the selection.
  await page.getByRole('button', { name: 'Undo (⌘Z)' }).click();
  await expect(canvas(page)).toHaveAccessibleName(/3 shapes/);
  await page.getByRole('button', { name: 'Redo (⇧⌘Z)' }).click();
  await expect(canvas(page)).toHaveAccessibleName('Mockup canvas, 4 shapes');
  // Clicking the caption selects it again; Backspace removes it.
  await page.mouse.click(box.x + box.width * 0.6 + 12, box.y + box.height * 0.8 + 6);
  await expect(canvas(page)).toHaveAccessibleName('Mockup canvas, 4 shapes, text selected');
  await page.keyboard.press('Backspace');
  await expect(canvas(page)).toHaveAccessibleName('Mockup canvas, 3 shapes');
  await expect(page.getByRole('button', { name: 'Delete selection' })).toBeDisabled();
});

test('attaches the export as an image chip, reopens it for editing, and updates in place', async ({ page }) => {
  await probe(page);
  await openSketch(page);
  await page.getByRole('button', { name: 'Attach to message' }).click();
  await expect(dialog(page).getByRole('status')).toHaveText('Draw something first.');
  await drag(page, [0.1, 0.2], [0.5, 0.6]);
  await page.getByRole('button', { name: 'Add card' }).click();
  await page.getByRole('button', { name: 'Attach to message' }).click();
  await expect(dialog(page)).toBeHidden();
  await expect(chips(page)).toHaveCount(1);
  await expect(chips(page).first()).toHaveAttribute('data-source', 'sketch');
  await expect(chips(page).first()).toContainText('Mockup.png');
  await expect(chips(page).first()).toContainText('Mockup · 1280 × 800');
  const imported = await calls(page, 'import_prompt_image');
  expect(imported).toHaveLength(1);
  expect(imported[0].args.dataUrl).toMatch(/^data:image\/png;base64,/);
  expect(imported[0].args.name).toBe('Mockup.png');
  // The export is the scene alone at export scale: a real PNG larger than a blank one.
  const bytes = Buffer.from(imported[0].args.dataUrl.split(',')[1], 'base64');
  expect(bytes.subarray(0, 8)).toEqual(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
  expect(bytes.readUInt32BE(16)).toBe(2048);
  expect(bytes.readUInt32BE(20)).toBe(1280);

  await page.getByRole('button', { name: 'Edit sketch Mockup.png' }).click();
  await expect(dialog(page)).toBeVisible();
  await expect(page.locator('#dialog h2')).toHaveText('Edit mockup');
  await expect(canvas(page)).toHaveAccessibleName(/2 shapes/);
  await page.getByRole('button', { name: 'Add button' }).click();
  await page.getByRole('button', { name: 'Update mockup' }).click();
  await expect(dialog(page)).toBeHidden();
  await expect(chips(page)).toHaveCount(1);
  expect(await calls(page, 'import_prompt_image')).toHaveLength(2);
  await expect(page.locator('#toast')).toContainText('Mockup updated');
});

test('a canceled sketch keeps its draft for the task and the canvas preset moves shapes back inside', async ({ page }) => {
  await probe(page);
  await openSketch(page);
  await drag(page, [0.7, 0.7], [0.95, 0.9]);
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(dialog(page)).toBeHidden();
  await expect(chips(page)).toHaveCount(0);
  await openSketch(page);
  await expect(canvas(page)).toHaveAccessibleName(/1 shape/);
  await dialog(page).getByRole('combobox', { name: 'Canvas size' }).selectOption('phone');
  const shape = await page.evaluate(() => {
    const canvas = document.querySelector<HTMLCanvasElement>('.sketch-paper canvas')!;
    return { width: Math.round(canvas.getBoundingClientRect().width), height: Math.round(canvas.getBoundingClientRect().height) };
  });
  expect(shape.height).toBeGreaterThan(shape.width);
  await expect(canvas(page)).toHaveAccessibleName(/1 shape/);
  await page.keyboard.press('Escape');
  await expect(dialog(page)).toBeHidden();
});
