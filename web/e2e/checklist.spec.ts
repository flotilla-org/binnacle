// The list-of-values checklist end to end, in the records demo's column
// menu: the port column's values with counts and "(Blanks)", narrowed by the
// other filters; unticking from all, ticking from none, ticking everything
// again, and "select all matching" each write the exact condition, shown as
// a chip and reflected in the rows; reopening shows the ticks in force; a
// capped list says so.

import { expect, test, type Page } from "@playwright/test";
import { PORTS, rows } from "../demo/sources/records-data";

const en = (n: number) => n.toLocaleString("en");

const bar = (page: Page) => page.getByRole("search");
const shown = (page: Page) => bar(page).locator(".binnacle-row-count");
const chips = (page: Page) => bar(page).getByRole("listitem");
const menu = (page: Page, label: string) => page.getByRole("dialog", { name: `Filter ${label}` });
const values = (page: Page, label: string) => menu(page, label).getByRole("group", { name: `${label} values` });
/** A checklist row's checkbox, named by its value and count. */
const box = (page: Page, label: string, value: string) => {
  const name = new RegExp("^" + value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "( [\\d,]+)?$");
  return values(page, label).getByRole("checkbox", { name });
};
const notice = (page: Page, label: string) => menu(page, label).locator(".binnacle-column-menu-notice");

/** Opens column `col`'s menu from "+ Filter". */
async function openMenu(page: Page, col: string) {
  await bar(page).getByRole("button", { name: "+ Filter" }).click();
  await page.getByRole("menu", { name: "Filter column" }).getByRole("menuitem", { name: col }).click();
  await expect(menu(page, col)).toBeVisible();
}

/** Each checklist row's text: the head, then each value and its count. */
async function listed(page: Page, label: string): Promise<string[]> {
  return values(page, label)
    .locator("label")
    .evaluateAll((labels) => labels.map((l) => (l as HTMLElement).innerText.replace(/\s+/g, " ").trim()));
}

/** The port checklist's rows for the records `keep` accepts: "(Blanks) n", then each port in order with its count. */
function ports(keep: (r: (typeof rows)[number]) => boolean = () => true): string[] {
  const kept = rows.filter(keep);
  const blanks = kept.filter((r) => r.port === null).length;
  const named = [...PORTS]
    .sort()
    .map((p) => [p, kept.filter((r) => r.port === p).length] as const)
    .filter(([, n]) => n > 0)
    .map(([p, n]) => `${p} ${en(n)}`);
  return [...(blanks ? [`(Blanks) ${en(blanks)}`] : []), ...named];
}

test.describe("records", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 640 });
    await page.goto("/?source=records&theme=light");
    await expect(shown(page)).toHaveText(`${en(rows.length)} rows`);
  });

  test("the port checklist lists values with counts and (Blanks), narrowed by the other filters", async ({ page }) => {
    await openMenu(page, "port");
    await expect.poll(() => listed(page, "port")).toEqual(["(Select all)", ...ports()]);
    await expect(notice(page, "port")).toHaveText("Narrowed by the other filters");
    await page.keyboard.press("Escape");

    await openMenu(page, "crew");
    await menu(page, "crew").getByRole("textbox", { name: "crew text" }).fill("12");
    await page.keyboard.press("Escape");
    await openMenu(page, "port");
    await expect.poll(() => listed(page, "port")).toEqual(["(Select all)", ...ports((r) => r.crew === 12)]);
  });

  test("unticking from all excludes those values, and ticking them again removes the condition", async ({ page }) => {
    await openMenu(page, "port");
    await box(page, "port", "Bergen").uncheck();
    await box(page, "port", "(Blanks)").uncheck();
    await expect(chips(page)).toHaveText(["port: not Bergen, (Blanks)×"]);
    const kept = rows.filter((r) => r.port !== null && r.port !== "Bergen").length;
    await expect(shown(page)).toHaveText(`${en(kept)} of ${en(rows.length)}`);
    await expect(box(page, "port", "(Select all)")).not.toBeChecked();

    await box(page, "port", "Bergen").check();
    await box(page, "port", "(Blanks)").check();
    await expect(chips(page)).toHaveCount(0);
    await expect(shown(page)).toHaveText(`${en(rows.length)} rows`);
    await expect(box(page, "port", "(Select all)")).toBeChecked();
  });

  test("ticking from none includes just those values, and reopening shows the ticks in force", async ({ page }) => {
    await openMenu(page, "port");
    await box(page, "port", "(Select all)").uncheck();
    await expect(shown(page)).toHaveText(`0 of ${en(rows.length)}`);
    await expect(chips(page)).toHaveText(["port: nothing×"]);
    await box(page, "port", "Cádiz").check();
    await box(page, "port", "Halifax").check();
    await expect(chips(page)).toHaveText(["port: Cádiz, Halifax×"]);
    const kept = rows.filter((r) => r.port === "Cádiz" || r.port === "Halifax").length;
    await expect(shown(page)).toHaveText(`${en(kept)} of ${en(rows.length)}`);
    await page.keyboard.press("Escape");

    await openMenu(page, "port");
    await expect(box(page, "port", "Cádiz")).toBeChecked();
    await expect(box(page, "port", "Halifax")).toBeChecked();
    await expect(box(page, "port", "Bergen")).not.toBeChecked();
    await expect(box(page, "port", "(Blanks)")).not.toBeChecked();
  });

  test("select all matching a search writes a Text condition, and later unticks AND a not OneOf", async ({ page }) => {
    await openMenu(page, "port");
    await menu(page, "port").getByRole("searchbox", { name: "Search port values" }).fill("en");
    const matching = PORTS.filter((p) => p.toLowerCase().includes("en")).sort();
    await expect.poll(async () => (await listed(page, "port")).map((t) => t.replace(/ [\d,]+$/, ""))).toEqual([
      '(Select all matching "en")',
      ...matching,
    ]);
    await box(page, "port", '(Select all matching "en")').check();
    await expect(chips(page)).toHaveText(["port: en×"]);
    await box(page, "port", "Bergen").uncheck();
    await expect(chips(page)).toHaveText(["port: en×", "port: not Bergen×"]);
    const kept = rows.filter((r) => r.port !== null && r.port.toLowerCase().includes("en") && r.port !== "Bergen");
    await expect(shown(page)).toHaveText(`${en(kept.length)} of ${en(rows.length)}`);
    await expect(menu(page, "port").getByRole("textbox", { name: "port text" })).toHaveValue("en");

    // Without the search, the values it rejects are unticked and can't be ticked.
    await menu(page, "port").getByRole("searchbox", { name: "Search port values" }).fill("");
    await expect(box(page, "port", "Cádiz")).toBeDisabled();
    await expect(box(page, "port", "Cádiz")).not.toBeChecked();
    await expect(box(page, "port", "Aberdeen")).toBeChecked();
    await expect(box(page, "port", "Bergen")).not.toBeChecked();
  });

  test("a column with more values than the checklist holds says so", async ({ page }) => {
    await openMenu(page, "weight");
    await expect(notice(page, "weight")).toHaveText("First 1,000 values; type to search. Narrowed by the other filters");
    await menu(page, "weight").getByRole("searchbox", { name: "Search weight values" }).fill("999.");
    await expect(notice(page, "weight")).toHaveText("Narrowed by the other filters");
  });

  test("keys reach the checklist: Tab to a value, Space unticks it", async ({ page }) => {
    await openMenu(page, "port");
    await expect(box(page, "port", "Aberdeen")).toBeVisible();
    await box(page, "port", "Aberdeen").focus();
    await page.keyboard.press("Space");
    await expect(chips(page)).toHaveText(["port: not Aberdeen×"]);
    await expect(box(page, "port", "Aberdeen")).toBeFocused();
  });

  test.describe("screenshots", () => {
    test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

    test("the port checklist with values unticked", async ({ page }) => {
      await openMenu(page, "port");
      await box(page, "port", "Bergen").uncheck();
      await box(page, "port", "(Blanks)").uncheck();
      await expect(chips(page)).toHaveText(["port: not Bergen, (Blanks)×"]);
      await page.mouse.move(0, 0);
      await expect(page).toHaveScreenshot("checklist.png");
    });
  });
});
