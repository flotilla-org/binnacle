// Filtering the columns by a row's values, end to end: in the records demo a
// row's menu (Alt+ArrowRight, or its header's button) filters the columns,
// shown as a "Columns where" chip with the column count beside the row count,
// and an edit that stops a column matching leaves it until Re-apply. In the
// transposed demo the filter bar filters the records (the columns), and the
// find count follows the records' own filter box (#139).

import { expect, test, type Page } from "@playwright/test";
import { rows, type RecordRow } from "../demo/sources/records-data";
import { column } from "./records-columns";

const HEADER = 28;
const ROW = 24;

const en = (n: number) => n.toLocaleString("en");
const bar = (page: Page) => page.locator(".binnacle-filter-bar");
const columnCount = (page: Page) => bar(page).locator(".binnacle-column-count");
const acrossChips = (page: Page) => bar(page).getByRole("list", { name: /^Filters on the / }).getByRole("listitem");
const reapply = (page: Page) => bar(page).locator(".binnacle-reapply");
const status = (page: Page) => page.locator("#status");
const menu = (page: Page, label: string) => page.getByRole("dialog", { name: `Filter columns by ${label}` });

/** Clicks the records demo's cell at screen row `row` (0 first) of column `col`. */
async function cell(page: Page, row: number, col: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + column(col), b.y + HEADER + row * ROW + ROW / 2);
}

test.describe("records", () => {
  /** The record on screen row 2, the third. */
  const third = rows[2]!;

  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records&theme=light");
    await expect(status(page)).toContainText("columns 1 to 5 of 5");
  });

  test("a row's menu filters the columns by its values, with a chip and the column count", async ({ page }) => {
    await cell(page, 2, 1);
    await page.keyboard.press("Alt+ArrowRight");
    const values = menu(page, "3").getByRole("group", { name: "3 values" });
    await expect(values.getByRole("checkbox", { name: `${third.name} 1` })).toBeChecked();
    await values.getByRole("checkbox", { name: `${third.name} 1` }).uncheck();
    await page.keyboard.press("Escape");
    await expect(acrossChips(page)).toHaveText([`3: not ${third.name}×`]);
    await expect(columnCount(page)).toHaveText("4 of 5 columns");
    await expect(status(page)).toContainText("columns 1 to 4 of 4");
    await acrossChips(page).getByRole("button", { name: /^Remove filter/ }).click();
    await expect(columnCount(page)).toBeHidden();
    await expect(status(page)).toContainText("columns 1 to 5 of 5");
  });

  test("an edit that stops a column matching leaves it, counted by Re-apply, until the filter is applied again", async ({
    page,
  }) => {
    // Only the name holds an "a" in the third record.
    const kept = (Object.keys(third) as (keyof RecordRow)[]).filter((k) => String(third[k] ?? "").includes("a"));
    expect(kept).toEqual(["name"]);
    await cell(page, 2, 1);
    await page.keyboard.press("Alt+ArrowRight");
    await menu(page, "3").getByRole("textbox", { name: "3 text" }).fill("a");
    await page.keyboard.press("Enter");
    await expect(columnCount(page)).toHaveText("1 of 5 columns");
    await cell(page, 2, 0);
    await page.keyboard.type("Zed");
    await page.keyboard.press("Enter");
    await expect(page.locator("#committed")).toHaveText(`name of ${third.id} = "Zed"`);
    await expect(reapply(page)).toBeVisible();
    await expect(reapply(page)).toHaveAccessibleName("Re-apply the filter: 1 edited column no longer matches");
    await expect(columnCount(page)).toHaveText("1 of 5 columns");
    await reapply(page).click();
    await expect(columnCount(page)).toHaveText("0 of 5 columns");
    await expect(reapply(page)).toBeHidden();
  });
});

test.describe("transposed", () => {
  const findBar = (page: Page) => page.getByRole("search", { name: "Find" });
  const counted = (page: Page) => findBar(page).locator(".binnacle-find-count");
  /** Whether some field of `r`, as text, holds `text` (ignoring case). */
  const holds = (r: RecordRow, text: string) =>
    Object.values(r).some((v) => v !== null && String(v).toLowerCase().includes(text));

  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 400 });
    await page.goto("/?source=transposed&theme=light");
    await expect(status(page)).toContainText(`columns 1 to`);
  });

  test("the filter bar filters the records, and a field's row menu narrows them by its values", async ({ page }) => {
    const galleys = rows.filter((r) => holds(r, "galley"));
    await bar(page).getByRole("textbox", { name: "Filter records" }).fill("galley");
    await expect(columnCount(page)).toHaveText(`${en(galleys.length)} of ${en(rows.length)} records`);
    await expect(bar(page).locator(".binnacle-row-count")).toHaveText("5 fields");
    // The port field's row, by its header's button.
    const b = await page.locator(".binnacle-layer").boundingBox();
    if (!b) throw new Error("no grid");
    const y = b.y + HEADER + 4 * ROW + ROW / 2;
    await page.mouse.move(b.x + 300, y);
    await page.mouse.move(b.x + 12, y, { steps: 4 });
    await page.mouse.click(b.x + 12, y);
    const values = menu(page, "port").getByRole("group", { name: "port values" });
    const bergen = galleys.filter((r) => r.port === "Bergen").length;
    await values.getByRole("checkbox", { name: `Bergen ${en(bergen)}` }).uncheck();
    await page.keyboard.press("Escape");
    await expect(acrossChips(page)).toHaveText(["port: not Bergen×"]);
    await expect(columnCount(page)).toHaveText(`${en(galleys.length - bergen)} of ${en(rows.length)} records`);
  });

  test("the find count follows when the records' filter box refilters the columns", async ({ page }) => {
    const port = (r: RecordRow) => (r.port === "Bergen" ? 1 : 0);
    await page.getByRole("button", { name: "Find" }).click();
    await findBar(page).getByRole("textbox", { name: "Find" }).fill("bergen");
    await expect(counted(page)).toHaveText(`${en(rows.reduce((n, r) => n + port(r), 0))} matches`);
    await page.getByRole("searchbox", { name: "Filter records" }).fill("galley");
    const galleys = rows.filter((r) => r.name.includes("galley") || (r.port?.toLowerCase().includes("galley") ?? false));
    await expect(counted(page)).toHaveText(`${en(galleys.reduce((n, r) => n + port(r), 0))} matches`);
  });
});
