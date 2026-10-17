// Screenshot snapshots of a small fixed table in each Theme preset and each
// Border Style, in Chromium with a bundled font: the web counterpart of the
// ratatui adapter's insta snapshots. Accept a new image only after looking at it.

import { expect, test, type Page } from "@playwright/test";

test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

async function shot(page: Page, theme: string, border: string, name: string) {
  const query = new URLSearchParams({ theme, border });
  await page.goto(`/e2e/fixtures/presentation.html?${query}`);
  const grid = page.locator("#grid");
  await expect(grid).toHaveAttribute("data-painted", "true");
  // Device scale keeps every device pixel, so snapping shows at a DPR of 2.
  await expect(grid).toHaveScreenshot(`${name}.png`, { scale: "device" });
}

for (const theme of ["light", "catppuccin mocha", "mono"]) {
  test(`theme ${theme}`, async ({ page }) => {
    await shot(page, theme, "grid light", `theme-${theme.replaceAll(" ", "-")}`);
  });
}

const BORDERS = [
  "none light",
  "headers light",
  "headers light + band rules",
  "columns light",
  "columns heavy",
  "columns double",
  "grid light",
  "grid heavy",
  "grid double",
];

for (const border of BORDERS) {
  test(`border ${border}`, async ({ page }) => {
    await shot(page, "light", border, `border-${border.replaceAll(" + ", " ").replaceAll(" ", "-")}`);
  });
}

test("double lines over a dark Theme with zebra stripes", async ({ page }) => {
  await shot(page, "catppuccin mocha", "grid double", "mocha-grid-double");
});

test.describe("at a device pixel ratio of 2", () => {
  test.use({ deviceScaleFactor: 2 });

  for (const border of ["grid light", "grid heavy", "grid double"]) {
    test(`border ${border}`, async ({ page }) => {
      await shot(page, "light", border, `dpr2-border-${border.replaceAll(" ", "-")}`);
    });
  }
});
