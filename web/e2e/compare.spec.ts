// Comparison conditions end to end, in the shipments demo with its rows
// ungrouped: the weight column's menu keeps the shipments over a weight, the
// shipped column's the shipments between two dates (both days included),
// each a chip; a column of text offers no comparison, and Clear removes one.

import { expect, test, type Page } from "@playwright/test";
import { day, shipments } from "../demo/sources/shipments-data";

const en = (n: number) => n.toLocaleString("en");

const bar = (page: Page) => page.getByRole("search");
const shown = (page: Page) => bar(page).locator(".binnacle-row-count");
const chips = (page: Page) => bar(page).getByRole("listitem");
const menu = (page: Page, label: string) => page.getByRole("dialog", { name: `Filter ${label}` });

async function openMenu(page: Page, column: string) {
  await bar(page).getByRole("button", { name: "+ Filter" }).click();
  await page.getByRole("menuitem", { name: column }).click();
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&theme=light");
  // The demo opens grouped by region then port; these tests filter the plain rows.
  await openMenu(page, "crates");
  await menu(page, "crates").getByRole("button", { name: "Ungroup rows" }).click();
  await expect(shown(page)).toHaveText(`${en(shipments.length)} rows`);
});

test("a weight over 100 and a quarter's dates filter the shipments as chips, and Clear removes one", async ({ page }) => {
  await openMenu(page, "weight");
  const weight = menu(page, "weight");
  await weight.getByRole("combobox", { name: "Comparison" }).selectOption("greater");
  await weight.getByRole("spinbutton", { name: "weight value" }).fill("100");
  const heavy = shipments.filter((s) => s.weight !== null && s.weight > 100);
  await expect(shown(page)).toHaveText(`${en(heavy.length)} of ${en(shipments.length)}`);
  await expect(chips(page)).toHaveText(["weight: > 100×"]);
  await page.keyboard.press("Escape");

  await openMenu(page, "shipped");
  const shipped = menu(page, "shipped");
  await shipped.getByRole("combobox", { name: "Comparison" }).selectOption("between");
  await shipped.getByLabel("shipped from").fill("2024-04-01");
  await shipped.getByLabel("shipped up to").fill("2024-06-30");
  const quarter = heavy.filter((s) => day(s.shipped) >= "2024-04-01" && day(s.shipped) <= "2024-06-30");
  expect(quarter.some((s) => day(s.shipped) === "2024-06-30")).toBe(true);
  await expect(shown(page)).toHaveText(`${en(quarter.length)} of ${en(shipments.length)}`);
  await expect(chips(page)).toHaveText(["weight: > 100×", "shipped: between 2024-04-01 and 2024-06-30×"]);

  await shipped.getByRole("button", { name: "Clear" }).click();
  await expect(chips(page)).toHaveText(["weight: > 100×"]);
  await expect(shown(page)).toHaveText(`${en(heavy.length)} of ${en(shipments.length)}`);
  await page.keyboard.press("Escape");
  await openMenu(page, "crates");
  await expect(menu(page, "port").getByRole("combobox", { name: "Comparison" })).toBeHidden();
});

test("a comparison reopens in its column's menu as it was written", async ({ page }) => {
  await openMenu(page, "crates");
  const crates = menu(page, "crates");
  await crates.getByRole("combobox", { name: "Comparison" }).selectOption("atMost");
  await crates.getByRole("spinbutton", { name: "crates value" }).fill("3");
  await expect(chips(page)).toHaveText(["crates: ≤ 3×"]);
  await expect(shown(page)).toHaveText(`${en(shipments.filter((s) => s.crates <= 3).length)} of ${en(shipments.length)}`);
  await page.keyboard.press("Escape");
  await openMenu(page, "crates");
  await expect(crates.getByRole("combobox", { name: "Comparison" })).toHaveValue("atMost");
  await expect(crates.getByRole("spinbutton", { name: "crates value" })).toHaveValue("3");
});

test.describe("screenshots", () => {
  test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

  test("a between of dates in the column menu, over its chip", async ({ page }) => {
    await openMenu(page, "shipped");
    const shipped = menu(page, "shipped");
    await shipped.getByRole("combobox", { name: "Comparison" }).selectOption("between");
    await shipped.getByLabel("shipped from").fill("2024-04-01");
    await shipped.getByLabel("shipped up to").fill("2024-06-30");
    await expect(chips(page)).toHaveText(["shipped: between 2024-04-01 and 2024-06-30×"]);
    await expect(shipped.locator(".binnacle-column-menu-notice")).not.toHaveText(/Loading/);
    await page.mouse.move(0, 0);
    await expect(page).toHaveScreenshot("compare-menu.png");
  });
});
