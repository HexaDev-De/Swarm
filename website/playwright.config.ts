import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

const siteUrl = process.env.PANDACODE_SITE_URL;
const chrome = process.env.PANDACODE_TEST_CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';

export default defineConfig({
  testDir: './tests',
  testMatch: ['landing.spec.ts', 'pages.spec.ts'],
  fullyParallel: true,
  workers: 2,
  timeout: 30_000,
  expect: { timeout: 6_000 },
  reporter: 'list',
  outputDir: '/private/tmp/pandacode-landing-checks/results',
  use: {
    baseURL: siteUrl ?? 'http://127.0.0.1:4173',
    headless: true,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  projects: [
    { name: 'chrome-desktop', use: { browserName: 'chromium', viewport: { width: 1440, height: 1000 }, launchOptions: { executablePath: chrome } } },
    { name: 'chrome-mobile', use: { browserName: 'chromium', viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, deviceScaleFactor: 2, launchOptions: { executablePath: chrome } } },
    { name: 'webkit-desktop', use: { browserName: 'webkit', viewport: { width: 1440, height: 1000 } } },
    { name: 'webkit-mobile', use: { browserName: 'webkit', viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, deviceScaleFactor: 2 } },
    { name: 'chrome-reduced-motion', use: { browserName: 'chromium', viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce', launchOptions: { executablePath: chrome } } },
    { name: 'webkit-mobile-reduced-motion', use: { browserName: 'webkit', viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, reducedMotion: 'reduce' } },
  ],
  // With PANDACODE_SITE_URL set, test that server instead of starting one on 4173.
  webServer: siteUrl ? undefined : {
    command: 'npm run dev',
    cwd: fileURLToPath(new URL('./', import.meta.url)),
    url: 'http://127.0.0.1:4173',
    reuseExistingServer: true,
    timeout: 30_000,
  },
});
