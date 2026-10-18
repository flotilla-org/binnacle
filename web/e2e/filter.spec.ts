// The records demo's filter bar end to end: typing narrows the rows to those
// with the text in any column, the case and regex toggles and the clear
// button change the quick filter, the bar counts the rows shown of the total,
// and the grid's live region announces the count once the filter applies.

import { expect, test, type Page } from "@playwright/test";
import { matches, rows, type RecordRow } from "../demo/sources/records-data";

const en = (n: number) => n.toLocaleString("en");

/** How many records have `match` true for some column's text. */
function count(match: (text: string) => boolean): number {
  const texts = (r: RecordRow) => Object.values(r).flatMap((v) => (v == null ? [] : [String(v)]));
  return rows.filter((r) => texts(r).some(match)).length;
}

const bar = (page: Page) => page.getByRole("search");
const field = (page: Page) => bar(page).getByRole("textbox", { name: "Filter rows" });
const shown = (page: Page) => bar(page).locator(".binnacle-row-count");
const announced = (page: Page) => page.locator(".binnacle-aria").getByRole("status");

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=records");
  await expect(shown(page)).toHaveText(`${en(rows.length)} rows`);
});

test("typing keeps the rows with the text in any column, ignoring case, and the count is announced", async ({ page }) => {
  const n = count((t) => t.toLowerCase().includes("bergen"));
  expect(n).toBeGreaterThan(0);
  await field(page).fill("BERGEN");
  await expect(shown(page)).toHaveText(`${en(n)} of ${en(rows.length)}`);
  await expect(announced(page)).toHaveText(new RegExp(`^${en(n)} of ${en(rows.length)} rows`));
  await expect(page.locator("#status")).toContainText(`of ${en(n)}`);
  // Enter applies what is typed, already applied: nothing changes.
  await field(page).press("Enter");
  await expect(shown(page)).toHaveText(`${en(n)} of ${en(rows.length)}`);
});

test("match case and regex change the quick filter, and the clear button and Escape clear it", async ({ page }) => {
  const caseToggle = bar(page).getByRole("button", { name: "Match case" });
  const regexToggle = bar(page).getByRole("button", { name: "Use regular expression" });
  await field(page).fill("bergen");
  await caseToggle.click();
  await expect(caseToggle).toHaveAttribute("aria-pressed", "true");
  await expect(shown(page)).toHaveText(`0 of ${en(rows.length)}`);
  await expect(announced(page)).toHaveText(new RegExp(`^0 of ${en(rows.length)} rows`));
  await caseToggle.click();
  await regexToggle.click();
  await expect(regexToggle).toHaveAttribute("aria-pressed", "true");
  await field(page).fill("^an.*1$");
  const n = count((t) => /^an.*1$/iu.test(t));
  await expect(shown(page)).toHaveText(`${en(n)} of ${en(rows.length)}`);
  await bar(page).getByRole("button", { name: "Clear filter" }).click();
  await expect(field(page)).toHaveValue("");
  await expect(field(page)).toBeFocused();
  await expect(shown(page)).toHaveText(`${en(rows.length)} rows`);
  await expect(announced(page)).toHaveText(new RegExp(`^${en(rows.length)} rows`));
  await field(page).fill("hawser");
  await expect(shown(page)).not.toHaveText(`${en(rows.length)} rows`);
  await field(page).press("Escape");
  await expect(field(page)).toHaveValue("");
  await expect(shown(page)).toHaveText(`${en(rows.length)} rows`);
});

test("the quick filter and the host's filter box both apply", async ({ page }) => {
  await field(page).fill("4");
  await page.getByRole("searchbox", { name: "Filter records" }).fill("bergen");
  const n = rows.filter(
    (r) => matches(r, "bergen") && Object.values(r).some((v) => v != null && String(v).includes("4")),
  ).length;
  await expect(shown(page)).toHaveText(`${en(n)} of ${en(rows.length)}`);
});

test("a source whose rows take no quick filter shows no bar", async ({ page }) => {
  await page.goto("/?source=huge");
  await expect(page.locator("#status")).toContainText("rows 1 to");
  await expect(page.getByRole("search")).toHaveCount(0);
});
