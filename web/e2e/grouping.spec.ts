// Grouping end to end (#200): the shipments demo opens grouped by region,
// each Group Item drawn in outline form in the row header in every preset;
// the column menu ungroups the rows and groups them by another column, focus
// staying on its shipment; and a saved layout keeps the grouping.

import { expect, test, type Page } from "@playwright/test";
import { REGIONS, shipments } from "../demo/sources/shipments-data";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");

/** The groups a column makes of the shipments. */
const groupsBy = (field: "region" | "crew") => new Set(shipments.map((s) => s[field])).size;

/** Clicks the cell at screen row `row` (0 first) of body column `col` (0 first), each 110 px wide. */
async function click(page: Page, row: number, col: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + 90 + col * 110 + 30, b.y + HEADER + row * ROW + ROW / 2);
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(status(page)).toContainText(`of ${shipments.length + groupsBy("region")}`);
});

test("the rows open grouped by region, the region column gone from the body", async ({ page }) => {
  expect(groupsBy("region")).toBe(REGIONS.length + 1);
  await expect(status(page)).toContainText("columns 1 to 4 of 4");
});

for (const theme of ["light", "catppuccin mocha", "mono"]) {
  test(`Group Items draw in outline form in ${theme}`, async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
    await page.goto(`/?source=shipments&theme=${encodeURIComponent(theme)}`);
    await expect(status(page)).toContainText(`of ${shipments.length + groupsBy("region")}`);
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
  await expect(status(page)).toContainText(`of ${shipments.length}  ·  columns 1 to 5 of 5`);
  await expect(status(page)).toContainText(new RegExp(`\\(${id} / port\\)`));

  // The crew column, two along now the region is back.
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText(`(${id} / crew)`);
  await page.keyboard.press("Alt+ArrowDown");
  const crew = page.getByRole("dialog", { name: "Filter crew" });
  await expect(crew.getByRole("button", { name: "Ungroup rows" })).toBeHidden();
  await crew.getByRole("button", { name: "Group rows by this column" }).click();
  await expect(status(page)).toContainText(`of ${shipments.length + groupsBy("crew")}  ·  columns 1 to 4 of 4`);
});

test("a saved layout keeps the grouping", async ({ page }) => {
  await click(page, 2, 1);
  await page.keyboard.press("Alt+ArrowDown");
  await page.getByRole("dialog", { name: "Filter port" }).getByRole("button", { name: "Ungroup rows" }).click();
  await expect(status(page)).toContainText(`of ${shipments.length}  ·`);
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.reload();
  await expect(status(page)).toContainText(`of ${shipments.length}  ·  columns 1 to 5 of 5`);

  await click(page, 2, 4);
  await page.keyboard.press("Alt+ArrowDown");
  await page.getByRole("dialog", { name: "Filter crew" }).getByRole("button", { name: "Group rows by this column" }).click();
  await expect(status(page)).toContainText(`of ${shipments.length + groupsBy("crew")}  ·`);
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.reload();
  await expect(status(page)).toContainText(`of ${shipments.length + groupsBy("crew")}  ·  columns 1 to 4 of 4`);
});
