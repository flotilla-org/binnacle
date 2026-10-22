// The pivot demo end to end (#209): the shipments with ports by region down
// the rows and quarters across, each quarter spanning its total weight and
// count columns, the All columns pinned right. Cells are canvas pixels, so
// the status line's focus (Position and IDs) is the probe: a pivot's rows and
// columns both come from the data, by negative IDs.

import { expect, test, type Page } from "@playwright/test";
import { quarterShipments } from "../demo/sources/pivot";

const ROW = 24;
/** The column band: the quarters' row, then the value columns' own. */
const BAND = 2 * 28;
/** The demo's column width and the Gap after it. */
const COLUMN = 112 + 1;
/** The row band's width as the demo restores it. */
const ROWS_BAND = 180;

const status = (page: Page) => page.locator("#status");
/** A column header by its accessible name. */
const header = (page: Page, name: string) =>
  page.locator(".binnacle-aria").getByRole("columnheader", { name, exact: true });

const regions = new Set(quarterShipments.map((s) => s.region)).size;
const ports = new Set(quarterShipments.map((s) => `${s.region}/${s.port}`)).size;
const quarters = new Set(quarterShipments.map((s) => s.quarter)).size;

/** Clicks the cell at screen row `row` (0 first) of screen column `col`. */
async function cell(page: Page, row: number, col: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + ROWS_BAND + col * COLUMN + COLUMN / 2, b.y + BAND + row * ROW + ROW / 2);
}

/** The focus as the status line gives it: its Positions and its row and column IDs. */
async function focus(page: Page): Promise<string> {
  const text = (await status(page).textContent()) ?? "";
  return /focus (.*)$/.exec(text)?.[1] ?? "";
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 600 });
  await page.goto("/?source=pivot&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  // Each region heads its ports, and the Grand Total ends the rows; each quarter has two value columns, and each value an All column.
  await expect(status(page)).toContainText(`of ${regions + ports + 1}`);
  await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
});

test("ports by region down and quarters across, each quarter over its values, the All columns pinned right", async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await expect(page.locator("#grid")).toHaveScreenshot("pivot.png", { scale: "device" });
});

test("scrolled to the end, the last quarter meets the All columns pinned at the right", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await page.setViewportSize({ width: 800, height: 400 });
  await page.mouse.move(400, 300);
  await expect(async () => {
    await page.mouse.wheel(20 * COLUMN, 0);
    await expect(status(page)).toContainText("to 8 of 10", { timeout: 200 });
  }).toPass();
  await expect(page.locator("#grid")).toHaveScreenshot("pivot-scrolled.png", { scale: "device" });
});

test("a pivot column's header drags nothing: the columns follow their sort", async ({ page }) => {
  await cell(page, 1, 0);
  await expect(status(page)).toContainText("focus row 2, column 1");
  const before = await focus(page);
  expect(before).toMatch(/^row 2, column 1 \(-\d+ \/ -\d+\)$/);
  const b = (await page.locator(".binnacle-layer").boundingBox())!;
  const y = b.y + BAND - 14;
  await page.mouse.move(b.x + ROWS_BAND + COLUMN / 2, y);
  await page.mouse.down();
  await page.mouse.move(b.x + ROWS_BAND + 3 * COLUMN, y, { steps: 8 });
  await page.mouse.up();
  await cell(page, 1, 0);
  await expect(status(page)).toContainText(`focus ${before}`);
});

test("focus moves along a row's quarters, each pivot column by its own ID", async ({ page }) => {
  await cell(page, 1, 0);
  await expect(status(page)).toContainText("focus row 2, column 1");
  const first = await focus(page);
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText("focus row 2, column 2");
  expect(await focus(page)).not.toBe(first);
  await page.keyboard.press("ArrowLeft");
  await expect(status(page)).toContainText(`focus ${first}`);
});

test("Up from the first row enters the quarters, where Left steps out to Q1 and closes it to its total columns", async ({
  page,
  browserName,
}) => {
  await cell(page, 0, 0);
  await expect(status(page)).toContainText("focus row 1, column 1");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  // Q1's two value columns give way to its total of each value.
  await expect(header(page, "Q1 count")).toBeVisible();
  await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
  if (browserName === "chromium") {
    await expect(page.locator("#grid")).toHaveScreenshot("pivot-q1-closed.png", { scale: "device" });
  }
  await page.keyboard.press("Enter");
  await expect(header(page, "Q1 count")).toHaveCount(0);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText("focus row 1, column 2");
});

test("a click on a quarter's mark closes it and another opens it", async ({ page }) => {
  const b = (await page.locator(".binnacle-layer").boundingBox())!;
  const mark = { x: b.x + ROWS_BAND + 10, y: b.y + 14 };
  await page.mouse.click(mark.x, mark.y);
  await expect(header(page, "Q1 count")).toBeVisible();
  await page.mouse.click(mark.x, mark.y);
  await expect(header(page, "Q1 count")).toHaveCount(0);
});
