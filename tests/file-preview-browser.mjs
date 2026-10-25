// Called by the opt-in native fixture: cargo test real_browser_bootstrap_and_assets -- --ignored.
import assert from 'node:assert/strict';
import http from 'node:http';
import { chromium, webkit } from 'playwright';

const url = process.argv[2];
const restoredUrl = process.argv[3];
assert.ok(url, 'The native fixture must provide its preview URL.');
assert.ok(restoredUrl, 'The native fixture must provide its reauthorized preview URL.');
async function assertPreview(page, label) {
  assert.equal(await page.locator('#preview-heading').textContent(), 'Workspace preview', `${label}: HTML`);
  await page.waitForFunction(() => document.body.dataset.loaded === 'true' && document.querySelector('#preview-image')?.naturalWidth === 32);
  assert.equal(await page.locator('#preview-heading').evaluate(element => getComputedStyle(element).color), 'rgb(212, 170, 25)', `${label}: CSS`);
  assert.equal(await page.title(), 'Preview ready', `${label}: JavaScript`);
  assert.ok(!page.url().includes('__pandacode_preview'), `${label}: clean final URL`);
  assert.equal(await page.evaluate(async () => (await fetch('/.env')).status), 404, `${label}: hidden-file protection`);
}
const unrelated = http.createServer((_request, response) => {
  response.setHeader('Content-Type', 'text/html');
  response.end('<h1>Unrelated page</h1>');
});
await new Promise(resolve => unrelated.listen(0, '127.0.0.1', resolve));
try {
  for (const [name, engine] of [['Chromium', chromium], ['WebKit', webkit]]) {
    const executablePath = name === 'Chromium'
      ? process.env.PANDACODE_TEST_CHROME ?? (process.platform === 'darwin' ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : undefined)
      : undefined;
    const browser = await engine.launch({ headless: true, executablePath });
    try {
      const initialContext = await browser.newContext();
      const page = await initialContext.newPage();
      // WebKit's Strict cookie policy differs after an unrelated site. A raw
      // HTTP test cannot catch this; commit a previous document before previewing.
      await page.goto(`http://localhost:${unrelated.address().port}/`);
      assert.equal((await page.goto(url)).status(), 200, `${name}: bootstrap status`);
      await assertPreview(page, name);
      await page.reload();
      await assertPreview(page, `${name}: reload`);
      const cleanUrl = new URL(page.url());
      cleanUrl.search = '?restored=two%20words';
      cleanUrl.hash = '#preview-heading';
      await initialContext.close();
      // Switching tasks destroys the incognito guest and its cookie store. A
      // reload within one context cannot exercise the resulting authorization loss.
      for (let cycle = 1; cycle <= 2; cycle++) {
        const context = await browser.newContext();
        try {
          const restoredPage = await context.newPage();
          const label = `${name}: restored context ${cycle}`;
          const unauthorized = await context.request.get(cleanUrl.href);
          assert.equal(unauthorized.status(), 403, `${label}: clean URL requires authentication`);
          assert.equal(await unauthorized.text(), '', `${label}: unauthorized response reproduces blank page`);
          assert.equal((await restoredPage.goto(restoredUrl)).status(), 200, `${label}: reauthorized bootstrap status`);
          await assertPreview(restoredPage, label);
          assert.equal(restoredPage.url(), cleanUrl.href, `${label}: path, query and fragment preserved`);
          assert.equal((await restoredPage.reload()).status(), 200, `${label}: reload status`);
          await assertPreview(restoredPage, `${label}: reload`);
          assert.equal(restoredPage.url(), cleanUrl.href, `${label}: reload preserves URL`);
        } finally { await context.close(); }
      }
      console.log(`${name}: HTML, CSS, JavaScript, SVG, cookies, reload, fresh-context restoration, query/fragment and hidden-file protection passed.`);
    } finally { await browser.close(); }
  }
} finally { await new Promise(resolve => unrelated.close(resolve)); }
