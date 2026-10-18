// Find end to end in the records demo: Ctrl/Cmd-F in the grid opens the find
// bar, which counts the matching cells, steps between them with Enter and
// Shift+Enter (and F3 in the grid), notes a wrap, steps by rows or by
// columns, matches case on request, copies its text to the quick filter,
// and closes with Escape, handing focus back to the grid. The slow source
// answers marks, steps and counts late.

import { expect, test, type Page } from "@playwright/test";
import { rows, type RecordRow } from "../demo/sources/records-data";

const en = (n: number) => n.toLocaleString("en");
const COLS: (keyof RecordRow)[] = ["id", "name", "weight", "crew", "port"];

/** Every cell whose text has `match` true, by rows, as [row id, column]. */
function cells(match: (text: string) => boolean): [number, keyof RecordRow][] {
  return rows.flatMap((r) =>
    COLS.flatMap((c): [number, keyof RecordRow][] => (r[c] != null && match(String(r[c])) ? [[r.id, c]] : [])),
  );
}

const findBar = (page: Page) => page.getByRole("search", { name: "Find" });
const field = (page: Page) => findBar(page).getByRole("textbox", { name: "Find" });
const counted = (page: Page) => findBar(page).locator(".binnacle-find-count");
const wrapped = (page: Page) => findBar(page).locator(".binnacle-find-wrapped");
const status = (page: Page) => page.locator("#status");

/** Cmd where the page sees macOS, Ctrl elsewhere: the user agent the browser is given decides. */
const mod = (page: Page) =>
  page.evaluate(() => {
    const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
    return /mac/i.test(nav.userAgentData?.platform ?? nav.platform) ? "Meta" : "Control";
  });

async function openFind(page: Page) {
  await page.locator(".binnacle-input").focus();
  await page.keyboard.press(`${await mod(page)}+f`);
  await expect(field(page)).toBeFocused();
}

test.describe("records", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1100, height: 600 });
    await page.goto("/?source=records&theme=light");
    await expect(status(page)).toContainText("rows 1 to");
    await expect(findBar(page)).toBeHidden();
  });

  test("Ctrl/Cmd-F opens the bar; the count, Enter and Shift+Enter step through the matches by rows and wrap", async ({
    page,
  }) => {
    const all = cells((t) => t.toLowerCase().includes("bergen"));
    expect(all.length).toBeGreaterThan(2);
    await openFind(page);
    await field(page).fill("bergen");
    await expect(counted(page)).toHaveText(`${en(all.length)} matches`);
    await page.keyboard.press("Enter");
    await expect(counted(page)).toHaveText(`1 of ${en(all.length)}`);
    await expect(status(page)).toContainText(`(${all[0]![0]} / port)`);
    await page.keyboard.press("Enter");
    await expect(counted(page)).toHaveText(`2 of ${en(all.length)}`);
    await expect(status(page)).toContainText(`(${all[1]![0]} / port)`);
    await page.keyboard.press("Shift+Enter");
    await page.keyboard.press("Shift+Enter");
    await expect(counted(page)).toHaveText(`${en(all.length)} of ${en(all.length)}`);
    await expect(wrapped(page)).toHaveText("Wrapped to the end");
    await expect(status(page)).toContainText(`(${all.at(-1)![0]} / port)`);
    await expect(status(page)).toContainText(`focus row ${en(all.at(-1)![0] + 1)}`);
    await page.keyboard.press("Enter");
    await expect(counted(page)).toHaveText(`1 of ${en(all.length)}`);
    await expect(wrapped(page)).toHaveText("Wrapped to the start");
  });

  test("by columns steps down each column in turn; match case finds nothing for the wrong case", async ({ page }) => {
    await openFind(page);
    await findBar(page).getByRole("combobox", { name: "Find order" }).selectOption("cols");
    await field(page).fill("7");
    await field(page).press("Enter");
    // The id column comes first: id 7.
    await expect(status(page)).toContainText("(7 / id)");
    await field(page).fill("BERGEN");
    await expect(counted(page)).not.toHaveText("No matches");
    await findBar(page).getByRole("button", { name: "Match case" }).click();
    await expect(counted(page)).toHaveText("No matches");
  });

  test("a step leaves the selection alone, F3 steps from the grid, and Escape closes find", async ({ page }) => {
    const all = cells((t) => t.toLowerCase().includes("cádiz"));
    // Select a 3 by 2 range from the first cell.
    await page.locator(".binnacle-input").focus();
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowRight");
    await expect(status(page)).toContainText("selected 3 × 2");
    await openFind(page);
    await field(page).fill("cádiz");
    await page.keyboard.press("Enter");
    await expect(status(page)).toContainText(`(${all[0]![0]} / port)`);
    await expect(status(page)).toContainText("selected 3 × 2");
    await page.locator(".binnacle-input").focus();
    await page.keyboard.press("F3");
    await expect(status(page)).toContainText(`(${all[1]![0]} / port)`);
    await page.keyboard.press("Shift+F3");
    await expect(status(page)).toContainText(`(${all[0]![0]} / port)`);
    await field(page).focus();
    await page.keyboard.press("Escape");
    await expect(findBar(page)).toBeHidden();
    await expect(page.locator(".binnacle-input")).toBeFocused();
    // Find is closed: F3 is the page's again, and the grid does not move.
    await page.keyboard.press("F3");
    await expect(status(page)).toContainText(`(${all[0]![0]} / port)`);
    // Reopening finds the text again.
    await page.keyboard.press(`${await mod(page)}+f`);
    await expect(field(page)).toHaveValue("cádiz");
    await expect(counted(page)).toHaveText(`${en(all.length)} matches`);
  });

  test("Show only matching rows copies the find text to the quick filter", async ({ page }) => {
    const kept = rows.filter((r) => COLS.some((c) => r[c] != null && String(r[c]).toLowerCase().includes("halifax")));
    await openFind(page);
    await field(page).fill("halifax");
    await findBar(page).getByRole("button", { name: "Show only matching rows" }).click();
    await expect(page.getByRole("textbox", { name: "Filter rows" })).toHaveValue("halifax");
    await expect(page.locator(".binnacle-row-count")).toHaveText(`${en(kept.length)} of ${en(rows.length)}`);
    await expect(counted(page)).toHaveText(`${en(kept.length)} matches`);
  });

  test.describe("screenshots", () => {
    test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

    for (const theme of ["light", "catppuccin mocha"]) {
      test(`the find bar, its marks and the current match in ${theme}`, async ({ page }) => {
        await page.goto(`/?source=records&theme=${encodeURIComponent(theme)}`);
        await expect(status(page)).toContainText("rows 1 to");
        await openFind(page);
        await field(page).fill("an");
        await page.keyboard.press("Enter");
        await page.keyboard.press("Enter");
        await expect(counted(page)).toHaveText(/^2 of /);
        await page.mouse.move(0, 0);
        await expect(page.locator(".with-filter-bar")).toHaveScreenshot(`find-${theme.replace(" ", "-")}.png`);
      });
    }
  });
});

test("the slow source's marks, steps and count arrive late", async ({ page }) => {
  const all = cells((t) => t.toLowerCase().includes("dunedin"));
  await page.setViewportSize({ width: 1100, height: 600 });
  await page.goto("/?source=slow&pace=brisk");
  await expect(status(page)).toContainText("rows 1 to");
  await openFind(page);
  await field(page).fill("dunedin");
  await expect(counted(page)).toHaveText("");
  await expect(counted(page)).toHaveText(`${en(all.length)} matches`);
  await page.keyboard.press("Enter");
  await expect(counted(page)).toHaveText(`1 of ${en(all.length)}`);
  await expect(status(page)).toContainText(`(${all[0]![0]} / port)`);
});
