// The built-in Cell Renderers on screen: screenshots of a fixed table per
// Theme (Chromium only, accepted after looking at them), and host-driven
// flashes, which light a cell, fade, and then let the widget idle.

import { expect, test, type Page } from "@playwright/test";

type Fixture = { widget: { flash(row: number, col?: number): void } };

async function open(page: Page, theme: string) {
  await page.goto(`/e2e/fixtures/renderers.html?${new URLSearchParams({ theme })}`);
  await expect(page.locator("#grid")).toHaveAttribute("data-painted", "true");
}

const flash = (page: Page, row: number, col?: number) =>
  page.evaluate(([r, c]) => (window as unknown as { fixture: Fixture }).fixture.widget.flash(r!, c), [row, col] as const);

test.describe("screenshots", () => {
  test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

  for (const theme of ["light", "catppuccin mocha", "mono"]) {
    test(`the built-in renderers in ${theme}`, async ({ page }) => {
      await open(page, theme);
      await expect(page.locator("#grid")).toHaveScreenshot(`renderers-${theme.replaceAll(" ", "-")}.png`, { scale: "device" });
    });
  }

  test("a flashed row and cell, held at full brightness", async ({ page }) => {
    await page.clock.install();
    await open(page, "light");
    await page.clock.pauseAt(Date.now() + 1000);
    await flash(page, 1);
    await flash(page, 3, 3);
    await page.clock.runFor(20);
    await expect(page.locator("#grid")).toHaveScreenshot("renderers-flash.png", { scale: "device" });
  });
});

test("a flash keeps the widget painting until it fades, then it idles", async ({ page }) => {
  // The page's clock runs only when told to, so frames come at its pace,
  // not the machine's.
  await page.clock.install();
  await open(page, "light");
  await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 1000);
  const paints = async () => Number(await page.locator("#grid").getAttribute("data-paints"));
  await flash(page, 2, 1);
  // The flash lasts 800 ms: every stretch of it paints.
  for (let t = 0; t < 800; t += 100) {
    const before = await paints();
    await page.clock.runFor(100);
    expect(await paints()).toBeGreaterThan(before);
  }
  // Once it has faded, painting stops.
  await page.clock.runFor(100);
  const settled = await paints();
  await page.clock.runFor(1000);
  expect(await paints()).toBe(settled);
});
