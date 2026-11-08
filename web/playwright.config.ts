import { defineConfig, devices } from '@playwright/test';

// Port overrides let the browser suite run beside an existing dev server:
//   E2E_UI_PORT=5273 E2E_FIXTURE_PORT=3102 npm run test:browser
const uiPort = Number(process.env.E2E_UI_PORT ?? 5173);
const fixturePort = Number(process.env.E2E_FIXTURE_PORT ?? 3002);
const baseURL = `http://127.0.0.1:${uiPort}`;
const fixtureURL = `http://127.0.0.1:${fixturePort}`;

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
      command: `npx vite --port ${uiPort} --strictPort`,
      url: baseURL,
      reuseExistingServer: false,
      timeout: 60_000,
      env: { VITE_RUNTIME_TEST: '1', E2E_FIXTURE_PORT: String(fixturePort) },
    },
  ],
});
