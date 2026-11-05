import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  // *.real.spec.ts need a live backend; they run via playwright.real.config.ts.
  testIgnore: ['**/*.real.spec.ts'],
  outputDir: 'test-results',
  fullyParallel: true,
  forbidOnly: Boolean(process.env['CI']),
  retries: process.env['CI'] ? 2 : 0,
  expect: { timeout: 10_000 },
  reporter: process.env['CI'] ? 'github' : 'list',
  use: {
    baseURL: 'http://127.0.0.1:4201',
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  // Serve the production build (`pnpm exec ng build dashboard`) rather than
  // `ng serve`: the dev server compiles lazy routes on demand, which made
  // multi-step specs flaky when several workers hit it at once.
  webServer: {
    command: 'node e2e/support/static-server.mjs dist/dashboard/browser 4201',
    url: 'http://127.0.0.1:4201',
    reuseExistingServer: !process.env['CI'],
    timeout: 60_000,
  },
});
