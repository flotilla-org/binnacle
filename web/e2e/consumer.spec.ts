// The package from a consumer's side: `consumer/` imports
// @flotilla-org/binnacle and its /react subpath through the workspace,
// type-checks against the shipped types and builds with Vite. Its built page
// must fetch the wasm as an asset Vite emitted and show a working grid.

import { expect, test } from "@playwright/test";

/** playwright.config.ts serves the consumer one port above the demo. */
const CONSUMER = `http://127.0.0.1:${Number(process.env.BINNACLE_E2E_PORT ?? 5755) + 1}`;

test("a Vite app built against the package loads the wasm and shows the grid", async ({ page }) => {
  const wasm: string[] = [];
  page.on("response", (r) => {
    if (r.url().endsWith(".wasm") && r.ok()) wasm.push(new URL(r.url()).pathname);
  });
  await page.goto(CONSUMER);
  await expect(page.locator("#status")).toHaveText("rows 1 to 3 of 3");
  expect(wasm).toHaveLength(1);
  expect(wasm[0]).toMatch(/^\/assets\/binnacle_wasm_bg-[\w-]+\.wasm$/);
  await expect(page.locator(".binnacle canvas")).toBeVisible();
});

test("the React wrapper hands onSelectionChange the focused row's own ID", async ({ page }) => {
  await page.goto(CONSUMER);
  await expect(page.locator("#status")).toHaveText("rows 1 to 3 of 3");
  const box = (await page.locator(".binnacle canvas").boundingBox())!;
  // The third body row, in the first column, past the row-number band.
  await page.mouse.click(box.x + 80, box.y + 28 + 2 * 24 + 12);
  await expect(page.locator("#focused")).toHaveText("Discovery.name (0 rows)");
});
