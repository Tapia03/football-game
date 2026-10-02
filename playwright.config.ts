import { defineConfig, devices } from '@playwright/test';

const isCI = process.env['CI'] !== undefined;

export default defineConfig({
  testDir: 'tests/e2e',
  // Golden screenshots live per browser (spec Section 0, item 8).
  snapshotPathTemplate: 'tests/golden/{projectName}/{arg}{ext}',
  fullyParallel: true,
  forbidOnly: isCI,
  retries: 0,
  reporter: isCI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: 'http://localhost:4173',
    trace: 'retain-on-failure',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    // DIAGNÓSTICO TEMPORÁRIO (fase-3): será revertido antes do PR.
    {
      name: 'firefox-forced',
      use: {
        ...devices['Desktop Firefox'],
        launchOptions: { firefoxUserPrefs: { 'webgl.force-enabled': true } },
      },
    },
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
