// Grouping end to end (#200): the shipments demo opens grouped by region,
// each Group Item drawn in outline form in the row header in every preset;
// the column menu ungroups the rows and groups them by another column, focus
// staying on its shipment; and a saved layout keeps the grouping.

import { expect, test, type Page } from "@playwright/test";
import { REGIONS, shipments } from "../demo/sources/shipments-data";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");
const en = (n: number) => n.toLocaleString("en");

/** The groups a column makes of the shipments. */
const groupsBy = (field: "region" | "crates") => new Set(shipments.map((s) => s[field])).size;

/** Clicks the id cell at screen row `row` (0 first), then steps right `right` columns. */
async function click(page: Page, row: number, right: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + 110, b.y + HEADER + row * ROW + ROW / 2);
  await expect(status(page)).toContainText("/ id)");
  for (let i = 0; i < right; i++) await page.keyboard.press("ArrowRight");
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(shipments.length + groupsBy("region"))}`);
});

test("the rows open grouped by region, the region column gone from the body", async ({ page }) => {
  expect(groupsBy("region")).toBe(REGIONS.length + 1);
  await expect(status(page)).toContainText("columns 1 to 5 of 5");
});

for (const theme of ["light", "catppuccin mocha", "mono"]) {
  test(`Group Items draw in outline form in ${theme}`, async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
    await page.goto(`/?source=shipments&theme=${encodeURIComponent(theme)}`);
    await expect(status(page)).toContainText(`of ${en(shipments.length + groupsBy("region"))}`);
    await expect(page.locator(".grid-box")).toHaveScreenshot(`grouped-${theme.replace(" ", "-")}.png`, { scale: "device" });
  });
}

test("the column menu ungroups the rows and groups them by its column, focus staying on its shipment", async ({ page }) => {
  await click(page, 2, 1);
  const focus = /focus row \d+, column 2 \((\d+) \/ port\)/;
  await expect(status(page)).toContainText(focus);
  const id = (await status(page).textContent())!.match(focus)![1];
  await page.keyboard.press("Alt+ArrowDown");
  const menu = page.getByRole("dialog", { name: "Filter port" });
  await expect(menu.getByRole("button", { name: "Group rows by this column" })).toBeHidden();
  await menu.getByRole("button", { name: "Ungroup rows" }).click();
  await expect(menu).toBeHidden();
  await expect(status(page)).toContainText(`of ${en(shipments.length)}  ·  columns 1 to 6 of 6`);
  await expect(status(page)).toContainText(new RegExp(`\\(${id} / port\\)`));

  // The crates column, three along.
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText(`(${id} / crates)`);
  await page.keyboard.press("Alt+ArrowDown");
  const crates = page.getByRole("dialog", { name: "Filter crates" });
  await expect(crates.getByRole("button", { name: "Ungroup rows" })).toBeHidden();
  await crates.getByRole("button", { name: "Group rows by this column" }).click();
  await expect(status(page)).toContainText(`of ${en(shipments.length + groupsBy("crates"))}  ·  columns 1 to 5 of 5`);
});

test("a saved layout keeps the grouping", async ({ page }) => {
  await click(page, 2, 1);
  await page.keyboard.press("Alt+ArrowDown");
  await page.getByRole("dialog", { name: "Filter port" }).getByRole("button", { name: "Ungroup rows" }).click();
  await expect(status(page)).toContainText(`of ${en(shipments.length)}  ·`);
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(shipments.length)}  ·  columns 1 to 6 of 6`);

  await click(page, 2, 5);
  await page.keyboard.press("Alt+ArrowDown");
  await page.getByRole("dialog", { name: "Filter crates" }).getByRole("button", { name: "Group rows by this column" }).click();
  await expect(status(page)).toContainText(`of ${en(shipments.length + groupsBy("crates"))}  ·`);
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(shipments.length + groupsBy("crates"))}  ·  columns 1 to 5 of 5`);
});
