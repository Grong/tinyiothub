import { defineConfig, devices } from '@playwright/test';

const baseURL = 'http://127.0.0.1:5173';
const fixtureURL = 'http://127.0.0.1:3002';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 45_000,
  expect: { timeout: 10_000 },
  reporter: [['list']],
  use: {
    baseURL,
    viewport: { width: 1280, height: 900 },
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], browserName: 'chromium' },
    },
  ],
  webServer: [
    {
      command: 'node e2e/fixture-server.ts',
      url: `${fixtureURL}/__fixtures__/health`,
      reuseExistingServer: false,
      timeout: 30_000,
    },
    {
      command: 'npx vite --port 5173 --strictPort',
      url: baseURL,
      reuseExistingServer: false,
      timeout: 60_000,
      env: { VITE_RUNTIME_TEST: '1' },
    },
  ],
});
