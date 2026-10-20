// The transposed source end to end: the records on their side, each record a
// column and each field a row, through `transpose(fromRows(...))`. Cells are
// canvas pixels, so the status line's focus (Position, row ID, column ID) is
// the probe: the row ID is the field, the column ID the record's index.

import { expect, test, type Page } from "@playwright/test";
import { matches, rows } from "../demo/sources/records-data";
import { FIRST } from "./records-columns";

const HEADER = 28;
const ROW = 24;
/** The demo's column width and the Gap after it. */
const COLUMN = 112 + 1;

const status = (page: Page) => page.locator("#status");

/** Clicks the visible cell at screen row `row` (0 first) of screen column `col`. */
async function cell(page: Page, row: number, col: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + FIRST + col * COLUMN, b.y + HEADER + row * ROW + ROW / 2);
}

const focus = (row: number, field: string, col: number, id: number) =>
  `focus row ${row + 1}, column ${(col + 1).toLocaleString("en")} (${field} / ${id})`;

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 400 });
  await page.goto("/?source=transposed&theme=light");
  await expect(status(page)).toContainText(`rows 1 to 5 of 5`);
  await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
});

test("each record is a column and each field a row", async ({ page }) => {
  await cell(page, 1, 2);
  await expect(status(page)).toContainText(focus(1, "name", 2, 2));
  await cell(page, 4, 0);
  await expect(status(page)).toContainText(focus(4, "port", 0, 0));
});

test("the filter box keeps the matching records as columns", async ({ page }) => {
  await page.getByRole("searchbox", { name: "Filter records" }).fill("bergen");
  const kept = rows.map((_, i) => i).filter((i) => matches(rows[i]!, "bergen"));
  await expect(status(page)).toContainText(`of ${kept.length.toLocaleString("en")}`);
  await cell(page, 0, 2);
  await expect(status(page)).toContainText(focus(0, "id", 2, kept[2]!));
  await page.getByRole("searchbox", { name: "Filter records" }).fill("");
  await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
});

test("each field row draws its own way: the id and weight end-aligned, the crew as a bar", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await expect(page.locator(".grid-box")).toHaveScreenshot("transposed-light.png", { scale: "device" });
});
