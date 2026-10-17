// The slow source end to end: loading cells until the answers land, a
// re-sort while the first sort is still in flight, and what pending states
// look like with motion and with motion reduced. Canvas pixels are read
// back to see whether a pending header or cell is still moving.

import { expect, test, type Page } from "@playwright/test";
import { rows } from "../demo/sources/records-data";

const HEADER = 28;
const ROW = 24;
const COLUMN = 141;
/** Inside the first column whatever the row-number band's width (40 to 60 px). */
const FIRST = 62;

const status = (page: Page) => page.locator("#status");
const requests = (page: Page) => page.locator("#tools .requests");

async function layer(page: Page) {
  const box = await page.locator(".binnacle-layer").boundingBox();
  if (!box) throw new Error("no grid");
  return box;
}

/** Clicks column `col`'s header, then moves off it so it is not drawn hovered. */
async function header(page: Page, col: number) {
  const b = await layer(page);
  await page.mouse.click(b.x + FIRST + col * COLUMN, b.y + HEADER / 2);
  await page.mouse.move(b.x + FIRST + col * COLUMN, b.y + b.height - ROW);
}

async function cell(page: Page, row: number, col = 0) {
  const b = await layer(page);
  await page.mouse.click(b.x + FIRST + col * COLUMN, b.y + HEADER + row * ROW + ROW / 2);
}

/** The canvas's pixels along a line `width` CSS px long from (x, y) in the layer, as one string. */
function pixels(page: Page, x: number, y: number, width: number): Promise<string> {
  return page.locator(".binnacle-layer canvas").evaluate(
    (canvas: HTMLCanvasElement, [x, y, width]) => {
      const dpr = canvas.width / canvas.clientWidth;
      const ctx = canvas.getContext("2d")!;
      const data = ctx.getImageData(Math.round(x * dpr), Math.round(y * dpr), Math.round(width * dpr), 1).data;
      return Array.from(data).join(",");
    },
    [x, y, width] as const,
  );
}

/** A strip near the top of column `col`'s header, clear of its label and marker. */
const headerStrip = (page: Page, col: number) => pixels(page, FIRST + col * COLUMN, 3, 70);

/** A strip along the top of the cell at screen row `row` of the first column, clear of its text. */
const cellStrip = (page: Page, row: number) => pixels(page, FIRST, HEADER + row * ROW + 4, 70);

/** A strip through the text of the cell at screen row `row` of the first column. */
const textStrip = (page: Page, row: number) => pixels(page, FIRST - 10, HEADER + row * ROW + ROW / 2, 70);

/** Whether the strip changes over a few hundred milliseconds. */
async function moving(read: () => Promise<string>): Promise<boolean> {
  const first = await read();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 120));
    if ((await read()) !== first) return true;
  }
  return false;
}

const focus = (position: number, id: number) =>
  `focus row ${(position + 1).toLocaleString("en")}, column 1 (${id} / id)`;

/** Record indices by weight, nulls last either way, ties by index. */
function byWeight(direction: "asc" | "desc"): number[] {
  return rows
    .map((_, i) => i)
    .sort((i, j) => {
      const a = rows[i]!.weight;
      const b = rows[j]!.weight;
      if (a === b) return i - j;
      if (a === null) return 1;
      if (b === null) return -1;
      return direction === "asc" ? a - b : b - a;
    });
}

async function open(page: Page, pace: string) {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto(`/?source=slow&pace=${pace}`);
}

test("cells show as loading until the slow source answers, then settle", async ({ page }) => {
  await open(page, "slow");
  await expect(status(page)).toContainText("loading");
  const loading = await textStrip(page, 0);
  await expect(status(page)).not.toContainText("loading");
  await expect(requests(page)).toContainText("0 in flight");
  expect(await textStrip(page, 0)).not.toBe(loading);
  await cell(page, 0);
  await expect(status(page)).toContainText(focus(0, 0));
});

test("a re-sort while the first sort is in flight aborts it, and the grid settles on the latest", async ({ page }) => {
  await page.clock.install();
  await open(page, "slow");
  await cell(page, 0);
  await expect(status(page)).toContainText(focus(0, 0));
  await expect(status(page)).not.toContainText("loading");
  await expect(requests(page)).toContainText("0 in flight");
  // The page's clock stops while both sorts are asked, so the first cannot
  // land first however slowly the browser takes the second click. Clicks
  // reach the source at once; only its answers wait on the clock. Pausing
  // jumps the clock a minute ahead, so the jump is still ahead when it lands
  // on a loaded machine; nothing is in flight to fire on the way.
  await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 60_000);
  const before = Number((await requests(page).textContent())!.match(/(\d+) aborted/)![1]);
  await header(page, 2);
  await expect(requests(page)).toHaveText(`1 in flight · ${before} aborted`);
  // Ascending is still in flight: ask for descending.
  await header(page, 2);
  await expect(requests(page)).toHaveText(`1 in flight · ${before + 1} aborted`);
  await page.clock.resume();
  await expect(status(page)).not.toContainText("loading");
  await expect(requests(page)).toContainText("0 in flight", { timeout: 10_000 });
  const descending = byWeight("desc");
  // Focus followed record 0 to where descending put it.
  await expect(status(page)).toContainText(focus(descending.indexOf(0), 0));
  await cell(page, 0);
  await expect(status(page)).toContainText(focus(0, descending[0]!));
});

test("a pending sort shimmers and a pending locate pulses, then both settle", async ({ page }) => {
  await open(page, "glacial");
  await expect(requests(page)).toContainText("0 in flight", { timeout: 10_000 });
  await cell(page, 0);
  await header(page, 2);
  expect(await moving(() => headerStrip(page, 2))).toBe(true);
  // The sort lands, and focus is located: its last known cell pulses.
  await expect.poll(() => moving(() => cellStrip(page, 0)), { timeout: 10_000 }).toBe(true);
  await expect(requests(page)).toContainText("0 in flight", { timeout: 15_000 });
  expect(await moving(() => headerStrip(page, 2))).toBe(false);
  expect(await moving(() => cellStrip(page, 0))).toBe(false);
});

test("with motion reduced, pending states hold still at a static highlight", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await open(page, "glacial");
  await expect(requests(page)).toContainText("0 in flight", { timeout: 10_000 });
  const idle = await headerStrip(page, 2);
  await cell(page, 0);
  await header(page, 2);
  await expect.poll(() => headerStrip(page, 2)).not.toBe(idle);
  expect(await moving(() => headerStrip(page, 2))).toBe(false);
});
