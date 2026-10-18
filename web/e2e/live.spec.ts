// The live source end to end: values tick and flash without any input, once
// paused the grid goes back to idling, and the quick filter takes new tickers
// as they are listed. Cells are canvas pixels, so the probe for change is
// whether the grid's image changes.

import { expect, test, type Page } from "@playwright/test";

const grid = (page: Page) => page.locator(".binnacle-layer canvas");

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=live");
  await expect(page.locator("#status")).toContainText("of 200");
});

test("the live table changes on its own", async ({ page }) => {
  const first = await grid(page).screenshot();
  await expect.poll(async () => (await grid(page).screenshot()).equals(first)).toBe(false);
});

test("paused, the flashes fade and the grid stays still", async ({ page }) => {
  await page.getByRole("button", { name: "Pause" }).click();
  await expect(page.getByRole("button", { name: "Resume" })).toHaveAttribute("aria-pressed", "true");
  // Longer than a flash takes to fade.
  await page.waitForTimeout(1000);
  const still = await grid(page).screenshot();
  await page.waitForTimeout(600);
  expect((await grid(page).screenshot()).equals(still)).toBe(true);
});

test("the quick filter takes each new ticker as it is listed", async ({ page }) => {
  const count = page.getByRole("search").locator(".binnacle-row-count");
  await page.getByRole("textbox", { name: "Filter rows" }).fill("anchor");
  // Every eighth of the first 200 is an anchor.
  await expect(count).toHaveText("25 rows");
  // The first ticker listed, the 201st, is ANCHOR 25; the next, a bosun, stays out.
  await expect(count).toHaveText("26 rows", { timeout: 10_000 });
  await page.waitForTimeout(2500);
  await expect(count).toHaveText("26 rows");
});
