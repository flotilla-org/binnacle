// Row Menu Items end to end: on the shipments demo, a row's menu lists the
// host's "Track" item below the built-in ones; choosing it closes the menu
// and notes the row's id. A column's menu has no "Track".

import { expect, test, type Page } from "@playwright/test";
import { shipments } from "../demo/sources/shipments-data";

const status = (page: Page) => page.locator("#status");

/** The middle of the first body cell, past the row band and the header band. */
async function firstCell(page: Page) {
  const band = await page.locator(".binnacle-band-left").boundingBox();
  const top = await page.locator(".binnacle-band-top").boundingBox();
  if (!band || !top) throw new Error("no grid");
  return { x: band.x + band.width + 10, y: top.y + top.height + 10 };
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 600 });
  await page.goto("/?source=shipments&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(status(page)).toContainText("columns 1 to 4 of 4");
  // Ungrouped, the rows are shipments.
  const at = await firstCell(page);
  await page.mouse.click(at.x, at.y);
  await page.keyboard.press("Alt+ArrowDown");
  await page.getByRole("dialog").getByRole("button", { name: "Ungroup rows" }).click();
  await expect(status(page)).toContainText("columns 1 to 6 of 6");
  await page.mouse.click(at.x, at.y);
});

test("choosing Track on a row's menu closes the menu and notes the row's id", async ({ page }) => {
  await page.keyboard.press("Alt+ArrowRight");
  const menu = page.getByRole("dialog");
  const track = menu.getByRole("button", { name: "Track" });
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
