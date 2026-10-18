// The huge source end to end: the native scroller and the wheel reach every
// row of 10^15, the "Go to row" box focuses any of them, the widget draws
// nothing while idle, and find counts like a source that cannot count
// everything at once.

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

test("the Go to row box focuses a row deep in the table and hands the keyboard to the grid", async ({ page }) => {
  const box = page.getByLabel("Go to row");
  await box.fill("123,456,789,012");
  await box.press("Enter");
  await expect(status(page)).toContainText("focus row 123,456,789,012, column 1");
  await expect(status(page)).toContainText("rows 123,456,789,012 to");
  // The grid has the keyboard: arrows move on from the row gone to.
  await page.keyboard.press("ArrowDown");
  await expect(status(page)).toContainText("focus row 123,456,789,013, column 1");
});

test("the Go to row box takes e-notation and refuses what is not a row", async ({ page }) => {
  const box = page.getByLabel("Go to row");
  await box.fill("1e15");
  await box.press("Enter");
  await expect(status(page)).toContainText("focus row 1,000,000,000,000,000, column 1");
  for (const bad of ["0", "1e16", "abc"]) {
    await box.fill(bad);
    await box.press("Enter");
    await expect(box).toHaveAttribute("aria-invalid", "true");
  }
  await expect(status(page)).toContainText("focus row 1,000,000,000,000,000, column 1");
});

test("find answers with a partial count, then the counts the source pushes grow to the last", async ({ page }) => {
  await page.getByRole("button", { name: "Find" }).click();
  await page.keyboard.type("77");
  await page.keyboard.press("Enter");
  const count = page.locator(".binnacle-find-count");
  await expect(count).toHaveText(/^1 of [\d,]+\+$/);
  const first = Number((await count.textContent())!.match(/of ([\d,]+)/)![1]!.replaceAll(",", ""));
  await expect(count).toHaveText("1 of 1,459,738,939,225,188", { timeout: 10_000 });
  expect(first).toBeLessThan(1_459_738_939_225_188);
  await expect(status(page)).toContainText("focus row 78, column 1");
});
