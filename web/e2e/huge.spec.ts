// The huge source end to end: the native scroller and the wheel reach every
// row of 10^15, and the widget draws nothing while idle.

import { expect, test, type Page } from "@playwright/test";

const status = (page: Page) => page.locator("#status");

/** Counts animation frames the page asks for, from before any script runs. */
async function countFrames(page: Page) {
  await page.addInitScript(() => {
    const w = window as unknown as { frames_requested: number };
    w.frames_requested = 0;
    const raf = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (cb) => {
      w.frames_requested++;
      return raf(cb);
    };
  });
}

const framesRequested = (page: Page) =>
  page.evaluate(() => (window as unknown as { frames_requested: number }).frames_requested);

/** The first visible row's number, from the status line. */
async function first(page: Page): Promise<number> {
  const text = (await status(page).textContent()) ?? "";
  return Number(text.match(/rows ([\d,]+)/)?.[1]?.replaceAll(",", "") ?? NaN);
}

async function scrollTo(page: Page, top: number | "end") {
  await page.locator(".binnacle-scroller").evaluate((el, to) => {
    el.scrollTop = to === "end" ? el.scrollHeight : to;
  }, top);
}

test.beforeEach(async ({ page }) => {
  await countFrames(page);
  await page.goto("/?source=huge");
  await expect(status(page)).toContainText("rows 1 to");
});

test("the native scroller reaches the last row and comes back to the first", async ({ page }) => {
  await scrollTo(page, "end");
  await expect(status(page)).toContainText("to 1,000,000,000,000,000 of 1,000,000,000,000,000");
  await scrollTo(page, 0);
  await expect(status(page)).toContainText("rows 1 to");
});

test("the wheel scrolls by the native distance, deep in the table too", async ({ page }) => {
  const grid = page.locator(".binnacle-scroller");
  const box = (await grid.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, 24 * 10);
  await expect(status(page)).toContainText("rows 11 to");
  await scrollTo(page, 5_000_000);
  await expect.poll(() => first(page)).toBeGreaterThan(1e14);
  const before = await first(page);
  await page.mouse.wheel(0, 24 * 10);
  await expect.poll(() => first(page)).toBe(before + 10);
  await page.mouse.wheel(0, -24 * 10);
  await expect.poll(() => first(page)).toBe(before);
});

test("the wheel reaches the end of the table", async ({ page }) => {
  await scrollTo(page, "end");
  await expect(status(page)).toContainText("to 1,000,000,000,000,000 of");
  const grid = page.locator(".binnacle-scroller");
  const box = (await grid.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, -24 * 5);
  await expect(status(page)).toContainText("to 999,999,999,999,995 of");
  await page.mouse.wheel(0, 24 * 50);
  await expect(status(page)).toContainText("to 1,000,000,000,000,000 of");
});

test("draws no frames while idle", async ({ page }) => {
  await scrollTo(page, 1000);
  await expect(status(page)).not.toContainText("rows 1 to");
  await page.waitForTimeout(200);
  const settled = await framesRequested(page);
  await page.waitForTimeout(1000);
  expect(await framesRequested(page)).toBe(settled);
});

test("destroy removes the grid and stops its frames", async ({ page }) => {
  const result = await page.evaluate(async () => {
    // Served by Vite; the type comes from the same module on disk.
    const path = "/src/index.ts";
    const { GridWidget } = (await import(path)) as typeof import("../src");
    const host = document.createElement("div");
    host.style.cssText = "position: fixed; inset: 0; width: 300px; height: 200px;";
    document.body.append(host);
    const range = (s: number, e: number) => Array.from({ length: e - s }, (_, i) => s + i);
    const widget = await GridWidget.create(host, {
      source: {
        rows: { identity: "position", count: () => 100, window: range },
        cols: { identity: "position", count: () => 3, window: range },
        cells: (rs: number[], cs: number[]) => rs.map((r) => cs.map((c) => r * c)),
      },
    });
    await new Promise((r) => setTimeout(r, 100));
    const mounted = host.querySelector("canvas") !== null;
    widget.destroy();
    widget.destroy();
    const w = window as unknown as { frames_requested: number };
    const frames = w.frames_requested;
    host.style.width = "400px";
    await new Promise((r) => setTimeout(r, 300));
    return { mounted, left: host.childElementCount, framesAfter: w.frames_requested - frames };
  });
  expect(result).toEqual({ mounted: true, left: 0, framesAfter: 0 });
});
