// End-to-end tests against a running PrivateBin server.
//
// The server must run with the rate limit disabled (PRIVATEBIN__TRAFFIC__LIMIT=0),
// because the tests create pastes and comments back to back.
//   PB_URL      base URL of the server (default http://127.0.0.1:8080/)
//   PB_CHANNEL  optional browser channel, e.g. "chrome" to use a system Chrome
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'tests',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: process.env.PB_URL ?? 'http://127.0.0.1:8080/',
    browserName: 'chromium',
    channel: process.env.PB_CHANNEL || undefined,
    permissions: ['clipboard-read', 'clipboard-write'],
    trace: 'retain-on-failure',
  },
});
