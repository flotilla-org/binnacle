// The live source end to end: values tick and flash without any input, and
// once paused the grid goes back to idling. Cells are canvas pixels, so the
// probe is whether the grid's image changes.

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
