import { defineConfig, devices } from '@playwright/test';

const isCI = process.env['CI'] !== undefined;

export default defineConfig({
  testDir: 'tests/e2e',
  // Golden screenshots live per browser (spec Section 0, item 8).
  snapshotPathTemplate: 'tests/golden/{projectName}/{arg}{ext}',
  fullyParallel: true,
  forbidOnly: isCI,
  // Every page draws the match with software WebGL as fast as it can: with
  // one worker per core a local run saturates the machine (minutes, flaky
  // timeouts). Two workers finish the three browsers in under a minute.
  ...(isCI ? {} : { workers: 2 }),
  retries: 0,
  reporter: isCI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: 'http://localhost:4173',
    trace: 'retain-on-failure',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
  webServer: {
    // Tests run against the production bundle, not the dev server.
    command: 'npm run build && npm run preview',
    url: 'http://localhost:4173',
    reuseExistingServer: !isCI,
    timeout: 300_000,
  },
});
