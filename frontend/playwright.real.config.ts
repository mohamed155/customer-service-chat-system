import { defineConfig, devices } from '@playwright/test';

const backendPort = process.env['E2E_BACKEND_PORT'] ?? '18080';

export default defineConfig({
  testDir: './e2e',
  testMatch: ['tenant-team-management.real.spec.ts', 'customer-profiles.real.spec.ts'],
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  retries: process.env['CI'] ? 1 : 0,
  reporter: process.env['CI'] ? 'github' : 'list',
  use: {
    baseURL: 'http://127.0.0.1:4201',
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'real-backend-chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      command: 'cargo run -p server',
      cwd: '../backend',
      url: `http://127.0.0.1:${backendPort}/health`,
      env: { ...process.env, PORT: backendPort, CI_REAL_BACKEND: 'true' },
      reuseExistingServer: !process.env['CI'],
      timeout: 180_000,
    },
    {
      command:
        'pnpm ng serve dashboard --configuration e2e --host 127.0.0.1 --port 4201 --proxy-config e2e/proxy.conf.cjs',
      url: 'http://127.0.0.1:4201',
      env: { ...process.env, PORT: '4201', CI_REAL_BACKEND: 'true' },
      reuseExistingServer: !process.env['CI'],
      timeout: 180_000,
    },
  ],
});
