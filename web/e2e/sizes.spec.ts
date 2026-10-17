// Initial column widths end to end, on a small grid over the page: a 40 px
// row-number band, 1 px Gaps, a 700 px wide host and six columns 100 px wide
// by default. Column 0 starts at 200 px, column 1 fits its text, column 2
// fills what is left; borders are found by the resize cursor.

import { expect, test, type Page } from "@playwright/test";
import type { FrameInfo } from "../src";
import { dblclick } from "./dblclick";

const WIDTH = 700;

async function mount(page: Page) {
  await page.goto("/?source=huge");
  await page.evaluate(async (width) => {
    const path = "/src/index.ts";
    const { GridWidget } = (await import(path)) as typeof import("../src");
    const host = document.createElement("div");
    host.id = "fixture";
    host.style.cssText = `position: fixed; left: 0; top: 0; width: ${width}px; height: 400px; z-index: 10; background: white;`;
    document.body.append(host);
    const range = (s: number, e: number) => Array.from({ length: e - s }, (_, i) => s + i);
    const w = window as unknown as { frame: FrameInfo };
    await GridWidget.create(host, {
      source: {
        rows: { identity: "position", count: () => 100, window: range, sort: (spec) => spec },
        cols: { identity: "position", count: () => 6, window: range },
        cells: (rs: number[], cs: number[]) => rs.map((r) => cs.map((c) => `r${r}c${c}`)),
      },
      rowHeight: 24,
      columnWidth: 100,
      headerHeight: 28,
      initialColumnWidth: (c) => [200, "fit", "fill"][c] as 200 | "fit" | "fill" | undefined,
      onFrame: (f) => {
        w.frame = f;
      },
    });
  }, WIDTH);
  await expect.poll(() => page.evaluate(() => (window as unknown as { frame?: FrameInfo }).frame)).toBeTruthy();
}

const frame = (page: Page) => page.evaluate(() => (window as unknown as { frame?: FrameInfo }).frame);

const settled = (page: Page) =>
  page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

/** The column a click at `x` in the body focuses. */
async function columnAt(page: Page, x: number): Promise<number | undefined> {
  await page.mouse.click(x, 28 + 24 * 3 + 12);
  await settled(page);
  return (await frame(page))?.focus?.col;
}

/** The first x from `from` to `to` where the header band shows the resize cursor. */
async function border(page: Page, from: number, to: number): Promise<number | undefined> {
  for (let x = from; x <= to; x += 2) {
    await page.mouse.move(x, 15);
    await settled(page);
    const cursor = await page.locator("#fixture .binnacle-layer").evaluate((el) => (el as HTMLElement).style.cursor);
    if (cursor === "col-resize") return x;
  }
  return undefined;
}

test.beforeEach(async ({ page }) => mount(page));

test("columns start fixed, fitted and filling", async ({ page }) => {
  // Column 0 spans 40 to 240.
  expect(await columnAt(page, 235)).toBe(0);
  expect(await columnAt(page, 246)).toBe(1);
  // "r99c1" is far narrower than the 100 px default.
  expect(await columnAt(page, 320)).toBe(2);
  // Column 2 ends where the last three, at 101 px each, leave 700 px.
  expect(await columnAt(page, WIDTH - 3 * 101 - 6)).toBe(2);
  expect(await columnAt(page, WIDTH - 3 * 101 + 6)).toBe(3);
  // Double-clicking the fitted column's border fits it again, to the same width.
  const fitted = await border(page, 250, 330);
  expect(fitted).toBeDefined();
  await dblclick(page, fitted! + 2, 15);
  await page.mouse.move(300, 300);
  expect(await border(page, 250, 330)).toBe(fitted);
});

test("a user's resize wins over the initial width and survives a sort", async ({ page }) => {
  await page.mouse.move(240, 15);
  await page.mouse.down();
  await page.mouse.move(180, 15, { steps: 4 });
  await page.mouse.up();
  expect(await columnAt(page, 175)).toBe(0);
  expect(await columnAt(page, 186)).toBe(1);
  // A click on column 1's header sorts by it.
  await page.mouse.click(200, 15);
  await expect.poll(() => page.locator("#fixture .binnacle-aria [aria-sort]").count()).toBe(1);
  expect(await columnAt(page, 175)).toBe(0);
  expect(await columnAt(page, 186)).toBe(1);
});
