// The Values page: the shared fixture of Typed Values drawn by
// `valueRenderer`. A screenshot in light (Chromium only, accepted after
// looking at it), the formatted text as assistive technology reads it, and
// relative times that move on as the page's clock does.

import { expect, test, type Page } from "@playwright/test";

const grid = (page: Page) => page.getByRole("grid", { name: "values table" });

async function open(page: Page) {
  await page.clock.install();
  await page.setViewportSize({ width: 1600, height: 500 });
  await page.goto(`/?${new URLSearchParams({ source: "values", theme: "light" })}`);
  await expect(grid(page).getByRole("row").nth(1)).toContainText("Aberdeen");
}

test("the values page draws every type through the shared formatter", async ({ page, browserName }) => {
  await open(page);
  const row = (n: number) => grid(page).getByRole("row").nth(n);
  await expect(row(1)).toContainText("Aberdeen");
  // A null, an empty text and an invalid value read differently.
  await expect(row(3).getByRole("gridcell").first()).toHaveText("∅");
  await expect(row(2).getByRole("gridcell").nth(3)).toHaveText("");
  await expect(row(4).getByRole("gridcell").first()).toHaveText("12x");
  await expect(row(1).getByRole("gridcell").nth(2)).toHaveText("7.000");
  await expect(row(2).getByRole("gridcell").nth(9)).toHaveText("1.5 KiB");
  if (browserName === "chromium") {
    await expect(page.locator("#grid")).toHaveScreenshot("values-light.png", { scale: "device" });
  }
});

test("a relative time redraws as the clock moves on, and reads as the absolute time", async ({ page }) => {
  await open(page);
  // Text for assistive technology is kept, so it is absolute.
  const seen = grid(page).getByRole("row").nth(1).getByRole("gridcell").last();
  await expect(seen).toHaveText("2026-10-29 13:59");
  const canvas = page.locator("#grid canvas").first();
  const before = await canvas.screenshot();
  await page.clock.runFor(10_000);
  // Still "just now": the first row was seen 30 s before the page's clock starts.
  expect((await canvas.screenshot()).equals(before)).toBe(true);
  await page.clock.runFor(21_000);
  // "1m ago" now.
  expect((await canvas.screenshot()).equals(before)).toBe(false);
});
