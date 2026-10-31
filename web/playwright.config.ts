import { defineConfig, devices } from "@playwright/test";

// BINNACLE_E2E_PORT lets parallel worktrees run end-to-end tests at once.
const port = Number(process.env.BINNACLE_E2E_PORT ?? 5755);
/** The consumer fixture's built app, served by `vite preview` on the next port (see consumer.spec.ts). */
const consumer = port + 1;
/** The shared fake table server the described source reads (see described.spec.ts). */
const fake = port + 2;
const fixtures = "../crates/binnacle-described/tests";

export default defineConfig({
  testDir: "e2e",
  fullyParallel: true,
  reporter: "list",
  use: { baseURL: `http://127.0.0.1:${port}` },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] }, testIgnore: "webkit.spec.ts" },
    { name: "firefox", use: { ...devices["Desktop Firefox"] }, testIgnore: "webkit.spec.ts" },
    // The whole suite, plus checks for what only WebKit does (it tints a selected canvas).
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
  webServer: [
    {
      command: `vite --port ${port} --strictPort --host 127.0.0.1`,
      url: `http://127.0.0.1:${port}`,
      reuseExistingServer: false,
      env: { BINNACLE_FAKE_TABLES: `http://127.0.0.1:${fake}` },
    },
    {
      command: `pnpm -C consumer build && pnpm -C consumer preview --port ${consumer} --strictPort --host 127.0.0.1`,
      url: `http://127.0.0.1:${consumer}`,
      reuseExistingServer: false,
      timeout: 120_000,
    },
    {
      // Built first, so the per-test fakes (e2e/fixtures/fake-tables.ts) find the binary too.
      command:
        "cargo build -p binnacle-testkit --features fake-tables --bin fake-tables && " +
        `cargo run -q -p binnacle-testkit --features fake-tables --bin fake-tables -- --listen 127.0.0.1:${fake} ` +
        `--fixture ${fixtures}/fixtures/crews.ndjson --fixture ${fixtures}/data/rows-hosts.ndjson --generate big:10050`,
      url: `http://127.0.0.1:${fake}/tables`,
      reuseExistingServer: false,
      timeout: 300_000,
    },
  ],
});
