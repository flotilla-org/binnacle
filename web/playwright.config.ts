import { defineConfig, devices } from "@playwright/test";

// BINNACLE_E2E_PORT lets parallel worktrees run end-to-end tests at once.
const port = Number(process.env.BINNACLE_E2E_PORT ?? 5755);
/** The consumer fixture's built app, served by `vite preview` on the next port (see consumer.spec.ts). */
const consumer = port + 1;

export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  reporter: "list",
  use: { baseURL: `http://127.0.0.1:${port}` },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] }, testIgnore: "webkit.spec.ts" },
    { name: "firefox", use: { ...devices["Desktop Firefox"] }, testIgnore: "webkit.spec.ts" },
    // Only WebKit tints a selected canvas; the rest of the suite stays on the two above.
    { name: "webkit", use: { ...devices["Desktop Safari"] }, testMatch: "webkit.spec.ts" },
  ],
  webServer: [
    {
      command: `vite --port ${port} --strictPort --host 127.0.0.1`,
      url: `http://127.0.0.1:${port}`,
      reuseExistingServer: false,
    },
    {
      command: `pnpm -C consumer build && pnpm -C consumer preview --port ${consumer} --strictPort --host 127.0.0.1`,
      url: `http://127.0.0.1:${consumer}`,
      reuseExistingServer: false,
      timeout: 120_000,
    },
  ],
});
