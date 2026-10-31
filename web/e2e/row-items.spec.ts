// Row Menu Items end to end: on the shipments demo, a row's menu lists the
// host's "Track" item below the built-in ones; choosing it closes the menu
// and notes the row's id, which after a filter is neither the row's Position
// nor the first row's. A column's menu has no "Track".

import { expect, test, type Page } from "@playwright/test";
import { shipments } from "../demo/sources/shipments-data";

const HEADER = 28;
const ROW = 24;
const en = (n: number) => n.toLocaleString("en");

const status = (page: Page) => page.locator("#status");
const bar = (page: Page) => page.getByRole("search");

/** Clicks the id cell of the row shown at Position `row`. */
async function clickRow(page: Page, row: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + 230, b.y + HEADER + row * ROW + ROW / 2);
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&theme=light");
  // The demo opens grouped by region then port; ungroup through a column's menu.
  await bar(page).getByRole("button", { name: "+ Filter" }).click();
  await page.getByRole("menuitem", { name: "crates" }).click();
  await page.getByRole("dialog", { name: "Filter crates" }).getByRole("button", { name: "Ungroup rows" }).click();
  await expect(bar(page).locator(".binnacle-row-count")).toHaveText(`${en(shipments.length)} rows`);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await clickRow(page, 0);
  await expect(status(page)).toContainText("/ id)");
});

test("choosing Track on a filtered row's menu closes the menu and notes that row's id", async ({ page }) => {
  // Shipments over 100: the third shown is neither the third nor the first in the data.
  await bar(page).getByRole("button", { name: "+ Filter" }).click();
  await page.getByRole("menuitem", { name: "weight" }).click();
  const weight = page.getByRole("dialog", { name: "Filter weight" });
  await weight.getByRole("combobox", { name: "Comparison" }).selectOption("greater");
  await weight.getByRole("spinbutton", { name: "weight value" }).fill("100");
  const third = shipments.filter((s) => s.weight !== null && s.weight > 100)[2]!;
  expect(third.id).not.toBe(2);
  await expect(bar(page).locator(".binnacle-row-count")).toContainText(" of ");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await clickRow(page, 2);
  await expect(status(page)).toContainText(`(${third.id} / id)`);
  await page.keyboard.press("Alt+ArrowRight");
  const track = page.getByRole("dialog").getByRole("button", { name: "Track" });
  await expect(track).toBeVisible();
  await track.click();
  await expect(page.locator("#received")).toHaveText(`track: ${third.id}`);
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("a column's menu has no Track", async ({ page }) => {
  await page.keyboard.press("Alt+ArrowDown");
  await expect(page.getByRole("dialog").getByRole("button", { name: "Pin left" })).toBeVisible();
  await expect(page.getByRole("dialog").getByRole("button", { name: "Track" })).toHaveCount(0);
});
