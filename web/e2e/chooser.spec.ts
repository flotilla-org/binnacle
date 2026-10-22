// The chooser and acting on many items (#149): Mod+Alt+O lists every column
// (Shift the rows) to tick shown or hidden, the menus' Hide and "Show only"
// act on the picks or the selection, and a hide past the rows' cap of 100
// hides nothing and says to filter. The transposed demo does the same with
// its records as columns.

import { expect, test, type Page } from "@playwright/test";
import { matches, rows } from "../demo/sources/records-data";
import { column, FIRST } from "./records-columns";

const HEADER = 28;
const ROW = 24;

const en = (n: number) => n.toLocaleString("en");
const status = (page: Page) => page.locator("#status");
const bar = (page: Page) => page.locator(".binnacle-filter-bar");
const columnCount = (page: Page) => bar(page).locator(".binnacle-column-count");
const chooser = (page: Page, nouns: string) => page.getByRole("dialog", { name: `Choose ${nouns}` });

/** Cmd where the page sees macOS, Ctrl elsewhere: the user agent the browser is given decides. */
const mod = (page: Page) =>
  page.evaluate(() => {
    const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
    return /mac/i.test(nav.userAgentData?.platform ?? nav.platform) ? "Meta" : "Control";
  });

/** Clicks `y` px into the layer, `x` px in, holding `held`. */
async function at(page: Page, x: number, y: number, held: ("Meta" | "Control" | "Shift")[] = []) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  for (const k of held) await page.keyboard.down(k);
  await page.mouse.click(b.x + x, b.y + y);
  for (const k of held) await page.keyboard.up(k);
}
const cell = (page: Page, row: number, x: number, held?: ("Meta" | "Control" | "Shift")[]) =>
  at(page, x, HEADER + row * ROW + ROW / 2, held);

test.describe("records", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records&theme=light");
    await expect(status(page)).toContainText("columns 1 to 5 of 5");
  });

  test("the chooser lists every column, its ticks hide and show them, and its search narrows the list", async ({ page }) => {
    await cell(page, 0, column(1));
    await page.keyboard.press(`${await mod(page)}+Alt+O`);
    const list = chooser(page, "columns");
    await expect(list.getByRole("checkbox")).toHaveCount(6);
    await expect(list.locator(".binnacle-chooser-item")).toHaveText(["(Show all)", "id", "name", "weight", "crew", "port"]);
    await expect(list.getByRole("checkbox", { name: "(Show all)" })).toBeChecked();
    await expect(list).toContainText("5 columns shown");
    await list.getByRole("checkbox", { name: "weight" }).uncheck();
    await list.getByRole("checkbox", { name: "port" }).uncheck();
    await expect(columnCount(page)).toHaveText("3 of 5 columns");
    // The hidden ones keep their places in the list while it is open.
    await expect(list.locator(".binnacle-chooser-item")).toHaveText(["(Show all)", "id", "name", "weight", "crew", "port"]);
    await expect(list.getByRole("checkbox", { name: "(Show all)" })).not.toBeChecked();
    await expect(list).toContainText("3 of 5 columns shown");
    await list.getByRole("searchbox").fill("WE");
    await expect(list.locator(".binnacle-chooser-item")).toHaveText(['(Show all matching "WE")', "weight"]);
    await list.getByRole("checkbox", { name: '(Show all matching "WE")' }).check();
    await expect(columnCount(page)).toHaveText("4 of 5 columns");
    await list.getByRole("searchbox").fill("");
    await list.getByRole("checkbox", { name: "(Show all)" }).check();
    await expect(columnCount(page)).toBeHidden();
    await page.keyboard.press("Escape");
    await expect(list).toBeHidden();
  });

  test("Hide acts on the picked rows and Show only on the selected columns", async ({ page }) => {
    const held = await mod(page);
    await cell(page, 1, column(1), [held]);
    await cell(page, 3, column(1), [held]);
    await expect(status(page)).toContainText("2 rows picked");
    await page.keyboard.press("Alt+ArrowRight");
    await page.getByRole("dialog", { name: /^Filter columns by/ }).getByRole("button", { name: "Hide 2 rows" }).click();
    await expect(bar(page)).toContainText(`${en(rows.length - 2)} of ${en(rows.length)}`);
    await cell(page, 0, column(1));
    await cell(page, 0, column(3), ["Shift"]);
    await expect(status(page)).toContainText("selected 1 × 3");
    await page.keyboard.press("Alt+ArrowDown");
    await page.getByRole("dialog", { name: /^Filter / }).getByRole("button", { name: "Show only these 3 columns" }).click();
    await expect(columnCount(page)).toHaveText("3 of 5 columns");
    await expect(status(page)).toContainText("columns 1 to 3 of 3");
    // A lone click leaves one column chosen, hidden alone.
    await cell(page, 0, FIRST + 10);
    await page.keyboard.press("Alt+ArrowDown");
    await page.getByRole("dialog", { name: /^Filter / }).getByRole("button", { name: "Hide column" }).click();
    await expect(columnCount(page)).toHaveText("2 of 5 columns");
  });

  test("a selection from a pinned column on through the next drawn one shows only those two", async ({ page }) => {
    await cell(page, 0, column(3));
    await page.keyboard.press("Alt+Shift+ArrowLeft");
    // The pin lands after the next frame: its menu offers Unpin once it has.
    await page.keyboard.press("Alt+ArrowDown");
    await expect(page.getByRole("dialog", { name: "Filter crew" }).getByRole("button", { name: "Unpin" })).toBeVisible();
    await page.keyboard.press("Escape");
    // The pinned crew is drawn first, then id: two runs of Positions.
    await page.keyboard.press("Shift+ArrowRight");
    await expect(status(page)).toContainText("selected 1 × 2");
    await page.keyboard.press("Alt+ArrowDown");
    // The id column takes no conditions, so its menu is titled "Column id".
    await page.getByRole("dialog", { name: "Column id" }).getByRole("button", { name: "Show only these 2 columns" }).click();
    await expect(columnCount(page)).toHaveText("2 of 5 columns");
    await page.keyboard.press(`${await mod(page)}+Alt+O`);
    const list = chooser(page, "columns");
    await expect(list.getByRole("checkbox", { checked: true })).toHaveCount(2);
    await expect(list.getByRole("checkbox", { name: "id" })).toBeChecked();
    await expect(list.getByRole("checkbox", { name: "crew" })).toBeChecked();
  });

  test("the row key hides the selected rows, and Show only past the cap hides nothing and says to filter", async ({ page }) => {
    await cell(page, 1, column(1));
    await cell(page, 3, column(1), ["Shift"]);
    await expect(status(page)).toContainText("selected 3 × 1");
    await page.keyboard.press(`${await mod(page)}+Alt+9`);
    await expect(bar(page)).toContainText(`${en(rows.length - 3)} of ${en(rows.length)}`);
    await cell(page, 1, column(1));
    await page.keyboard.press("Alt+ArrowRight");
    const menu = page.getByRole("dialog", { name: /^Filter columns by/ });
    await menu.getByRole("button", { name: "Show only this row" }).click();
    await expect(menu.getByRole("status")).toHaveText("Can't hide over 100 rows: use a filter instead");
    await expect(bar(page).locator(".binnacle-hide-capped")).toHaveText("Can't hide over 100 rows: use a filter instead");
    await expect(bar(page)).toContainText(`${en(rows.length - 3)} of ${en(rows.length)}`);
    await page.keyboard.press("Escape");
    // The next press in the grid puts the hint away.
    await cell(page, 2, column(1));
    await expect(bar(page).locator(".binnacle-hide-capped")).toBeHidden();
  });

  test("within a filter, Show only this row hides the other rows it keeps", async ({ page }) => {
    const text = rows[0]!.name;
    const kept = rows.filter((r) => matches(r, text)).length;
    expect(kept).toBeGreaterThan(1);
    expect(kept).toBeLessThanOrEqual(100);
    // The toolbar's box filters in the source, so the rows it drops are not shown and not hidden.
    const box = page.getByRole("searchbox", { name: "Filter records" });
    await box.fill(text);
    await expect(bar(page)).toContainText(`${en(kept)} rows`);
    await cell(page, 0, column(1));
    await page.keyboard.press("Alt+ArrowRight");
    await page.getByRole("dialog", { name: /^Filter columns by/ }).getByRole("button", { name: "Show only this row" }).click();
    await expect(bar(page)).toContainText(`1 of ${en(rows.length)}`);
    await box.fill("");
    await expect(bar(page)).toContainText(`${en(rows.length - kept + 1)} of ${en(rows.length)}`);
  });

  test("the rows' chooser opens with Shift and refuses a hide past the cap", async ({ page }) => {
    await cell(page, 0, column(1));
    await page.keyboard.press(`${await mod(page)}+Alt+Shift+O`);
    const list = chooser(page, "rows");
    await expect(list).toContainText("First 1,000 shown; search to narrow.");
    await list.getByRole("checkbox", { name: "(Show all)" }).click();
    await expect(list.getByRole("checkbox", { name: "(Show all)" })).toBeChecked();
    await expect(list.getByRole("status")).toHaveText("Can't hide over 100 rows: use a filter instead");
    await expect(bar(page)).toContainText(`${en(rows.length)} rows`);
    await list.getByRole("checkbox", { name: "2", exact: true }).uncheck();
    await expect(bar(page)).toContainText(`${en(rows.length - 1)} of ${en(rows.length)}`);
    await expect(list.getByRole("status")).toBeHidden();
  });
});

test("in the transposed demo, the columns' chooser lists the records and Show only keeps the record", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 400 });
  await page.goto("/?source=transposed&theme=light");
  await expect(status(page)).toContainText(`of ${en(rows.length)}`);
  const text = rows[0]!.name;
  const kept = rows.filter((r) => matches(r, text)).length;
  await page.getByRole("searchbox", { name: "Filter records" }).fill(text);
  await expect(status(page)).toHaveText(new RegExp(`columns 1 to \\d+ of ${kept}\\b`));
  // The row header band fits the field names beside their marks, so the records start at 96.
  await cell(page, 1, 100);
  await page.keyboard.press("Alt+ArrowDown");
  await page.getByRole("dialog", { name: /^Filter / }).getByRole("button", { name: "Show only this column" }).click();
  await expect(status(page)).toContainText("columns 1 to 1 of 1");
  await page.keyboard.press(`${await mod(page)}+Alt+O`);
  const list = chooser(page, "columns");
  await expect(list.getByRole("checkbox", { name: "(Show all)" })).not.toBeChecked();
  await expect(list.getByRole("checkbox", { checked: true })).toHaveCount(1);
});
