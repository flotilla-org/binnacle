// The transposed source end to end: the records on their side, each record a
// column and each field a row, through `transpose(fromRows(...))`. Cells are
// canvas pixels, so the status line's focus (Position, row ID, column ID) is
// the probe: the row ID is the field, the column ID the record's index.

import { expect, test, type Page } from "@playwright/test";
import { matches, rows } from "../demo/sources/records-data";

const HEADER = 28;
const ROW = 24;
/** Inside the first record's column: the row-number band fits the field names and a sort marker. */
const FIRST = 90;
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

/** Clicks the row header of the field at screen row `row` (0 first), with Shift if `shift`. */
async function rowHeader(page: Page, row: number, shift = false) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  if (shift) await page.keyboard.down("Shift");
  await page.mouse.click(b.x + 8, b.y + HEADER + row * ROW + ROW / 2);
  if (shift) await page.keyboard.up("Shift");
}

/** The records' indices sorted by weight, nulls last and ties in base order. */
function byWeight(direction: 1 | -1): number[] {
  return rows
    .map((_, i) => i)
    .sort((i, j) => {
      const [a, b] = [rows[i]!.weight, rows[j]!.weight];
      if (a === null || b === null) return Number(a === null) - Number(b === null);
      return direction * (a - b) || i - j;
    });
}

test("a click on a field's row header sorts the records, as columns, by it", async ({ page }) => {
  const aria = page.locator(".binnacle-aria");
  const weight = aria.getByRole("rowheader", { name: "weight" });
  await rowHeader(page, 2);
  await expect(weight).toHaveAttribute("aria-sort", "ascending");
  await expect(aria.getByRole("status")).toHaveText("Columns sorted by weight, ascending");
  const ascending = byWeight(1);
  await cell(page, 1, 0);
  await expect(status(page)).toContainText(focus(1, "name", 0, ascending[0]!));
  await cell(page, 1, 1);
  await expect(status(page)).toContainText(focus(1, "name", 1, ascending[1]!));
  await rowHeader(page, 2);
  await expect(weight).toHaveAttribute("aria-sort", "descending");
  await cell(page, 0, 0);
  await expect(status(page)).toContainText(focus(0, "id", 0, byWeight(-1)[0]!));
  // A column header selects its record rather than sorting.
  const b = await page.locator(".binnacle-layer").boundingBox();
  await page.mouse.click(b!.x + FIRST + COLUMN, b!.y + HEADER / 2);
  await expect(weight).toHaveAttribute("aria-sort", "descending");
  await rowHeader(page, 2);
  await expect(weight).not.toHaveAttribute("aria-sort");
  await expect(aria.getByRole("status")).toHaveText("Columns not sorted");
});

test("a sorted field's row header shows its marker", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await rowHeader(page, 2);
  await rowHeader(page, 3, true);
  const status = page.locator(".binnacle-aria").getByRole("status");
  await expect(status).toHaveText("Columns sorted by weight, ascending, then crew, ascending");
  await expect(page.locator(".grid-box")).toHaveScreenshot("transposed-sorted-light.png", { scale: "device" });
});

test("each field row draws its own way: the id and weight end-aligned, the crew as a bar", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await expect(page.locator(".grid-box")).toHaveScreenshot("transposed-light.png", { scale: "device" });
});

test("records pick and select as columns, and a field row selects whole", async ({ page }) => {
  const b = (await page.locator(".binnacle-layer").boundingBox())!;
  await page.keyboard.down("ControlOrMeta");
  await page.mouse.click(b.x + FIRST + COLUMN, b.y + HEADER / 2);
  await page.mouse.click(b.x + FIRST + 2 * COLUMN, b.y + HEADER / 2);
  await page.keyboard.up("ControlOrMeta");
  await expect(status(page)).toContainText("2 columns picked");
  await cell(page, 1, 0);
  await page.keyboard.press("Shift+Space");
  await expect(status(page)).toContainText(`selected 1 × ${rows.length.toLocaleString("en")}`);
  await page.keyboard.press("Control+Space");
  await expect(status(page)).toContainText("selected 5 × 1");
});
