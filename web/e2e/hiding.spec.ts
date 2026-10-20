// Hiding end to end (#148): the column menu's Hide and "Show hidden ▸" in
// the records demo, the keys (Mod+Alt+0 and Mod+Alt+9, Shift to list), the
// count in the filter bar, hidden items staying hidden when a filter clears,
// the transposed demo, where hiding a column hides a record, and the slow
// demo, whose rows do not hide.

import { expect, test, type Page } from "@playwright/test";
import { rows } from "../demo/sources/records-data";
import { column, FIRST } from "./records-columns";

const HEADER = 28;
const ROW = 24;

const en = (n: number) => n.toLocaleString("en");
const status = (page: Page) => page.locator("#status");
const bar = (page: Page) => page.locator(".binnacle-filter-bar");
const columnCount = (page: Page) => bar(page).locator(".binnacle-column-count");
const flyout = (page: Page, noun: string) => page.getByRole("menu", { name: `Hidden ${noun}s` });

/** Cmd where the page sees macOS, Ctrl elsewhere: the user agent the browser is given decides. */
const mod = (page: Page) =>
  page.evaluate(() => {
    const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
    return /mac/i.test(nav.userAgentData?.platform ?? nav.platform) ? "Meta" : "Control";
  });

/** Clicks the cell at screen row `row` (0 first), `x` px into the layer. */
async function click(page: Page, row: number, x: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + x, b.y + HEADER + row * ROW + ROW / 2);
}

test.describe("records", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records&theme=light");
    await expect(status(page)).toContainText("columns 1 to 5 of 5");
  });

  test("the column menu hides its column and lists it to show again, in its place, as wide as before", async ({ page }) => {
    await click(page, 1, column(2));
    await expect(status(page)).toContainText("focus row 2, column 3 (1 / weight)");
    await page.keyboard.press("Alt+ArrowDown");
    const menu = page.getByRole("dialog", { name: "Filter weight" });
    await expect(menu.getByRole("button", { name: "Show hidden columns ▸" })).toBeDisabled();
    await menu.getByRole("button", { name: "Hide column" }).click();
    await expect(menu).toBeHidden();
    await expect(columnCount(page)).toHaveText("4 of 5 columns");
    // Focus goes to the next column, which takes the hidden one's place.
    await expect(status(page)).toContainText("columns 1 to 4 of 4");
    await expect(status(page)).toContainText("focus row 2, column 3 (1 / crew)");
    await page.keyboard.press("Alt+ArrowDown");
    await page.getByRole("dialog", { name: "Filter crew" }).getByRole("button", { name: "Show hidden columns (1) ▸" }).click();
    await flyout(page, "column").getByRole("menuitem", { name: "weight" }).click();
    await expect(columnCount(page)).toBeHidden();
    await expect(status(page)).toContainText("columns 1 to 5 of 5");
    // The last one shown, the list closes and focus goes back to the menu.
    await expect(flyout(page, "column")).toBeHidden();
    await expect(page.getByRole("button", { name: "Hide column" })).toBeFocused();
    await page.keyboard.press("Escape");
    await click(page, 1, column(3));
    await expect(status(page)).toContainText("focus row 2, column 4 (1 / crew)");
  });

  test("keys hide the focused column and row, and with Shift list them", async ({ page }) => {
    await click(page, 2, column(1));
    await page.keyboard.press(`${await mod(page)}+Alt+0`);
    await expect(columnCount(page)).toHaveText("4 of 5 columns");
    await expect(status(page)).toContainText("focus row 3, column 2 (2 / weight)");
    await page.keyboard.press(`${await mod(page)}+Alt+9`);
    await expect(bar(page)).toContainText(`${en(rows.length - 1)} of ${en(rows.length)}`);
    await expect(status(page)).toContainText("focus row 3, column 2 (3 / weight)");
    await page.keyboard.press(`${await mod(page)}+Alt+Shift+9`);
    const hiddenRows = flyout(page, "row");
    await expect(hiddenRows.getByRole("menuitem")).toHaveText(["2", "Show all"]);
    await expect(hiddenRows.getByRole("menuitem", { name: "2" })).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(bar(page)).toContainText(`${en(rows.length)} rows`);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await page.locator(".binnacle-layer").click({ position: { x: FIRST, y: HEADER + ROW / 2 } });
    await page.keyboard.press(`${await mod(page)}+Alt+Shift+0`);
    await expect(flyout(page, "column").getByRole("menuitem")).toHaveText(["name", "Show all"]);
  });

  test("clearing the filter leaves hidden rows hidden", async ({ page }) => {
    const quick = bar(page).getByLabel("Filter rows", { exact: true });
    await quick.fill("anchor");
    const anchors = rows.filter((r) => Object.values(r).some((v) => String(v ?? "").toLowerCase().includes("anchor")));
    await expect(bar(page)).toContainText(`${en(anchors.length)} of ${en(rows.length)}`);
    await click(page, 0, column(1));
    await page.keyboard.press(`${await mod(page)}+Alt+9`);
    await expect(bar(page)).toContainText(`${en(anchors.length - 1)} of ${en(rows.length)}`);
    await quick.fill("");
    await expect(bar(page)).toContainText(`${en(rows.length - 1)} of ${en(rows.length)}`);
  });
});

test("in the transposed demo, hiding a column hides a record and the bar counts the records", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 400 });
  await page.goto("/?source=transposed&theme=light");
  await expect(status(page)).toContainText(`of ${en(rows.length)}`);
  await click(page, 1, FIRST + 113);
  await expect(status(page)).toContainText("focus row 2, column 2 (name / 1)");
  await page.keyboard.press(`${await mod(page)}+Alt+0`);
  await expect(bar(page)).toContainText(`${en(rows.length - 1)} of ${en(rows.length)} records`);
  await expect(status(page)).toContainText("focus row 2, column 2 (name / 2)");
  await page.keyboard.press(`${await mod(page)}+Alt+9`);
  await expect(status(page)).toContainText("rows 1 to 4 of 4");
  await expect(status(page)).toContainText("focus row 2, column 2 (weight / 2)");
});

test("in the slow demo, whose rows do not hide, the row menu has no Hide and the key does nothing", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=slow&theme=light&pace=brisk");
  await expect(status(page)).toContainText("columns 1 to 5 of 5");
  // The demo's columns are 140 wide.
  await click(page, 1, FIRST + 141 + 20);
  await expect(status(page)).toContainText("focus row 2, column 2 (1 / name)");
  await page.keyboard.press("Alt+ArrowRight");
  const menu = page.getByRole("dialog", { name: /^Filter columns by/ });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("button", { name: "Hide row" })).toBeHidden();
  await page.keyboard.press("Escape");
  await page.keyboard.press(`${await mod(page)}+Alt+9`);
  await expect(status(page)).toContainText(`of ${en(rows.length)}`);
  await page.keyboard.press("Alt+ArrowDown");
  await expect(page.getByRole("dialog", { name: /^Filter / }).getByRole("button", { name: "Hide column" })).toBeVisible();
});
