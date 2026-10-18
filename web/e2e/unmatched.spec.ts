// The records demo end to end: an edit that stops a row matching the quick
// filter leaves the row in place, dimmed, with "Re-apply" in the filter bar,
// until the filter is applied again.

import { expect, test, type Page } from "@playwright/test";
import { rows } from "../demo/sources/records-data";
import { column } from "./records-columns";

const HEADER = 28;
const ROW = 24;

const en = (n: number) => n.toLocaleString("en");
const bar = (page: Page) => page.getByRole("search");
const field = (page: Page) => bar(page).getByRole("textbox", { name: "Filter rows" });
const shown = (page: Page) => bar(page).locator(".binnacle-row-count");
const reapply = (page: Page) => bar(page).locator(".binnacle-reapply");
const committed = (page: Page) => page.locator("#committed");

/** The hawsers: no port or number holds the text, so renaming one stops it matching. */
const hawsers = rows.filter((r) => r.name.includes("hawser"));

/** Filters to the hawsers and renames the one at screen row `row` (0 first) to "Zed". */
async function renameAHawser(page: Page, row: number) {
  await field(page).fill("hawser");
  await expect(shown(page)).toHaveText(`${en(hawsers.length)} of ${en(rows.length)}`);
  const box = await page.locator(".binnacle-layer").boundingBox();
  if (!box) throw new Error("no grid");
  await page.mouse.click(box.x + column(1), box.y + HEADER + row * ROW + ROW / 2);
  await page.keyboard.type("Zed");
  await page.keyboard.press("Enter");
  await expect(committed(page)).toHaveText(`name of ${hawsers[row]!.id} = "Zed"`);
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
});

test("a renamed row stays until Re-apply filters it out", async ({ page }) => {
  await page.goto("/?source=records");
  await expect(shown(page)).toHaveText(`${en(rows.length)} rows`);
  await expect(reapply(page)).toBeHidden();
  await renameAHawser(page, 1);
  // Still there and counted, and the bar offers to apply the filter again.
  await expect(shown(page)).toHaveText(`${en(hawsers.length)} of ${en(rows.length)}`);
  await expect(reapply(page)).toBeVisible();
  await expect(reapply(page)).toHaveAccessibleName("Re-apply the filter: 1 edited row no longer matches");
  await reapply(page).click();
  await expect(shown(page)).toHaveText(`${en(hawsers.length - 1)} of ${en(rows.length)}`);
  await expect(reapply(page)).toBeHidden();
});

test("changing the filter also takes the renamed row out", async ({ page }) => {
  await page.goto("/?source=records");
  await renameAHawser(page, 0);
  await field(page).fill("hawse");
  await expect(shown(page)).toHaveText(`${en(hawsers.length - 1)} of ${en(rows.length)}`);
  await expect(reapply(page)).toBeHidden();
});

test.describe("screenshots", () => {
  test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

  for (const theme of ["light", "catppuccin mocha"]) {
    test(`a renamed row dimmed in ${theme}`, async ({ page }) => {
      await page.goto(`/?source=records&theme=${encodeURIComponent(theme)}`);
      await renameAHawser(page, 1);
      // Focus moves off the renamed row, so the dimming is plain to see.
      await page.keyboard.press("ArrowDown");
      await page.mouse.move(0, 0);
      await expect(page.locator(".with-filter-bar")).toHaveScreenshot(`unmatched-${theme.replace(" ", "-")}.png`);
    });
  }
});
