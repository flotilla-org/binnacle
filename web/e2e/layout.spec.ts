// Saving and restoring a layout end to end (#154): the records demo keeps a
// sort and a pinned column through a reload and puts them back with Restore
// after they change; the slow demo restores before its data arrives, so the
// saved filter reads "filtering…" until the late answer; the transposed demo
// keeps a pinned record.

import { expect, test, type Page } from "@playwright/test";
import { rows } from "../demo/sources/records-data";
import { column, FIRST } from "./records-columns";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");
const state = (page: Page) => page.locator("#tools .layout-state");

async function click(page: Page, x: number, y: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + x, b.y + y);
}

const cell = (page: Page, row: number, x: number) => click(page, x, HEADER + row * ROW + ROW / 2);

/** The record first by weight, ascending (nulls last, ties in the records' order). */
const lightest = rows.reduce((best, r) => (r.weight !== null && (best.weight === null || r.weight < best.weight) ? r : best)).id;

test.describe("records", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records&theme=light");
    await expect(status(page)).toContainText("columns 1 to 5 of 5");
    // Sort by weight and pin the name column left.
    await click(page, column(2), HEADER / 2);
    await cell(page, 0, column(1));
    await expect(status(page)).toContainText(`focus row 1, column 2 (${lightest} / name)`);
    await page.keyboard.press("Alt+Shift+ArrowLeft");
    // The pinned name column leads (another row, so not a double-click).
    await cell(page, 1, FIRST);
    await expect(status(page)).toContainText("focus row 2, column 2");
    await expect(status(page)).toContainText("/ name)");
    await page.getByRole("button", { name: "Save layout" }).click();
    await expect(state(page)).toHaveText("saved");
  });

  test("a saved layout comes back on reload: the sort and the pinned column", async ({ page }) => {
    await page.reload();
    await expect(state(page)).toHaveText("restored");
    await expect(status(page)).toContainText("columns 1 to 5 of 5");
    await cell(page, 0, FIRST);
    await expect(status(page)).toContainText(`focus row 1, column 2 (${lightest} / name)`);
  });

  test("Restore puts back the saved layout after the sort and the pin change", async ({ page }) => {
    // Back in the grid, on the pinned name column (in another row than the last click, so not a
    // double-click, which would start an edit), to unpin it.
    await cell(page, 2, FIRST);
    await expect(status(page)).toContainText("focus row 3, column 2");
    await page.keyboard.press("Alt+Shift+Backspace");
    // Descending, then no sort: the records' own order.
    await click(page, column(2), HEADER / 2);
    await click(page, column(2), HEADER / 2);
    await cell(page, 0, FIRST);
    await expect(status(page)).toContainText("focus row 1, column 1 (0 / id)");
    await page.getByRole("button", { name: "Restore layout" }).click();
    await expect(state(page)).toHaveText("restored");
    await cell(page, 1, FIRST);
    await expect(status(page)).toContainText("focus row 2, column 2");
    await expect(status(page)).toContainText("/ name)");
    await cell(page, 0, FIRST);
    await expect(status(page)).toContainText(`focus row 1, column 2 (${lightest} / name)`);
  });
});

test("the slow demo restores before its data arrives: the saved filter reads filtering… until the answer", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=slow&pace=slow");
  await expect(status(page)).not.toContainText("loading");
  const field = page.getByRole("search").first().getByRole("textbox").first();
  const count = page.locator(".binnacle-row-count");
  await field.fill("bergen");
  const kept = rows.filter((r) => Object.values(r).some((v) => v != null && String(v).toLowerCase().includes("bergen"))).length;
  await expect(count).toHaveText(`${kept.toLocaleString("en")} of 5,000`);
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.reload();
  await expect(state(page)).toHaveText("restored");
  await expect(count).toHaveText("filtering…");
  await expect(field).toHaveValue("bergen");
  await expect(count).toHaveText(`${kept.toLocaleString("en")} of 5,000`);
});

test("the transposed demo keeps a pinned record", async ({ page }) => {
  const FIELDS_BAND = 90;
  const COLUMN = 112 + 1;
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=transposed&theme=light");
  await expect(status(page)).toContainText("rows 1 to 5 of 5");
  await cell(page, 1, FIELDS_BAND + 3 * COLUMN);
  await expect(status(page)).toContainText("focus row 2, column 4 (name / 3)");
  await page.keyboard.press("Alt+Shift+ArrowLeft");
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.reload();
  await expect(state(page)).toHaveText("restored");
  await cell(page, 1, FIELDS_BAND);
  await expect(status(page)).toContainText("focus row 2, column 4 (name / 3)");
});
