// Row Menu Items end to end: on the shipments demo, a row's menu lists the
// host's "Track" item below the built-in ones; choosing it closes the menu
// and notes the row's id. A column's menu has no "Track".

import { expect, test, type Page } from "@playwright/test";
import { shipments } from "../demo/sources/shipments-data";

const HEADER = 28;
const ROW = 24;
const en = (n: number) => n.toLocaleString("en");

const status = (page: Page) => page.locator("#status");
const bar = (page: Page) => page.getByRole("search");

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&theme=light");
  // The demo opens grouped by region then port; ungroup through a column's menu.
  await bar(page).getByRole("button", { name: "+ Filter" }).click();
  await page.getByRole("menuitem", { name: "crates" }).click();
  await page.getByRole("dialog", { name: "Filter crates" }).getByRole("button", { name: "Ungroup rows" }).click();
  await expect(bar(page).locator(".binnacle-row-count")).toHaveText(`${en(shipments.length)} rows`);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  // The id cell of the first shipment.
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + 230, b.y + HEADER + ROW / 2);
  await expect(status(page)).toContainText("/ id)");
});

test("choosing Track on a row's menu closes the menu and notes the row's id", async ({ page }) => {
  await page.keyboard.press("Alt+ArrowRight");
  const track = page.getByRole("dialog").getByRole("button", { name: "Track" });
  await expect(track).toBeVisible();
  await track.click();
  await expect(page.locator("#received")).toHaveText(`track: ${shipments[0]!.id}`);
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("a column's menu has no Track", async ({ page }) => {
  await page.keyboard.press("Alt+ArrowDown");
  await expect(page.getByRole("dialog").getByRole("button", { name: "Pin left" })).toBeVisible();
  await expect(page.getByRole("dialog").getByRole("button", { name: "Track" })).toHaveCount(0);
});
