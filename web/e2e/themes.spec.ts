// Themes on the demo page: by default the grid follows the system colour
// scheme (light, or Catppuccin Mocha when dark) and switches when it flips;
// a Theme picked by hand stays put.

import { expect, test, type Page } from "@playwright/test";

const status = (page: Page) => page.locator("#status");

/** The canvas colour a little inside the first body cell, as `#rrggbb`. */
async function bodyColour(page: Page): Promise<string> {
  return page.locator(".binnacle canvas").evaluate((canvas: HTMLCanvasElement) => {
    const ctx = canvas.getContext("2d")!;
    const dpr = canvas.width / canvas.clientWidth;
    // Past the row-number band and under the header band, clear of the text.
    const [r, g, b] = ctx.getImageData(Math.round(canvas.width - 4 * dpr), Math.round(30 * dpr), 1, 1).data;
    return `#${[r, g, b].map((v) => v!.toString(16).padStart(2, "0")).join("")}`;
  });
}

test("following the system theme switches light and dark as the colour scheme flips", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/?source=huge");
  await expect(status(page)).toContainText("theme light");
  await expect(page.getByLabel("Follow the system theme")).toBeChecked();
  expect(await bodyColour(page)).toBe("#ffffff");

  await page.emulateMedia({ colorScheme: "dark" });
  await expect(status(page)).toContainText("theme catppuccin mocha");
  await expect.poll(() => bodyColour(page)).toBe("#1e1e2e");

  await page.emulateMedia({ colorScheme: "light" });
  await expect(status(page)).toContainText("theme light");
  await expect.poll(() => bodyColour(page)).toBe("#ffffff");
});

test("a Theme picked by hand stays when the colour scheme flips", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/?source=huge");
  await page.getByLabel("Follow the system theme").uncheck();
  await page.getByLabel("Theme", { exact: true }).selectOption("mono");
  await expect(status(page)).toContainText("theme mono");

  await page.emulateMedia({ colorScheme: "dark" });
  await page.waitForTimeout(200);
  await expect(status(page)).toContainText("theme mono");
  expect(await bodyColour(page)).toBe("#ffffff");
});

test("the Border Style switcher names the style in the status line", async ({ page }) => {
  await page.goto("/?source=huge");
  await page.getByLabel("Border", { exact: true }).selectOption("grid double");
  await expect(status(page)).toContainText("border grid double");
});

test("the header under the pointer is drawn in the headerHovered role", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/?source=huge");
  await expect(status(page)).toContainText("theme light");
  const canvas = page.locator(".binnacle canvas");
  const box = (await canvas.boundingBox())!;
  /** The canvas colour at the right end of the first column header, clear of its label. */
  const headerColour = () =>
    canvas.evaluate((c: HTMLCanvasElement, x: number) => {
      const dpr = c.width / c.clientWidth;
      const [r, g, b] = c.getContext("2d")!.getImageData(Math.round(x * dpr), Math.round(6 * dpr), 1, 1).data;
      return `#${[r, g, b].map((v) => v!.toString(16).padStart(2, "0")).join("")}`;
    }, 280);
  expect(await headerColour()).toBe("#f1f5f9");
  await page.mouse.move(box.x + 280, box.y + 14);
  await expect.poll(headerColour).toBe("#e2e8f0");
  await page.mouse.move(box.x + 280, box.y + 200);
  await expect.poll(headerColour).toBe("#f1f5f9");
});
