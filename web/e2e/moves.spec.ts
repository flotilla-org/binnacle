// Moves end to end: a row header drags its record as a column header drags
// its column, a move on sorted rows keeps the order shown and drops the
// sort, `?sortedMoves=refuse` withdraws row moves while sorted, the slow
// source shows a move pending and announces a move it refused, the
// transposed source moves records and fields, and the huge source offers no
// move at all. Cells are canvas pixels, so the status line's focus is the
// probe, as in records.spec.ts.

import { expect, test, type Locator, type Page } from "@playwright/test";
import { rows, type RecordRow } from "../demo/sources/records-data";
import { column, FIRST } from "./records-columns";

const HEADER = 28;
const ROW = 24;
/** Inside the row header band whatever its width, clear of the filter button at its start. */
const BAND = 30;

const status = (page: Page) => page.locator("#status");
const announced = (page: Page) => page.locator(".binnacle-aria").getByRole("status");
const requests = (page: Page) => page.locator("#tools .requests");

async function layer(page: Page) {
  const box = await page.locator(".binnacle-layer").boundingBox();
  if (!box) throw new Error("no grid");
  return box;
}

/** Drags from (x0, y0) to (x1, y1) in the layer, calling `during` before letting go. */
async function drag(page: Page, x0: number, y0: number, x1: number, y1: number, during?: () => Promise<void>) {
  const b = await layer(page);
  await page.mouse.move(b.x + x0, b.y + y0);
  await page.mouse.down();
  await page.mouse.move(b.x + (x0 + x1) / 2, b.y + (y0 + y1) / 2, { steps: 5 });
  await page.mouse.move(b.x + x1, b.y + y1, { steps: 5 });
  await during?.();
  await page.mouse.up();
}

/**
 * Drags the row header at screen row `from` into the top half of screen row
 * `to`, just past the top pin zone's 4 px.
 */
const dragRow = (page: Page, from: number, to: number) =>
  drag(page, BAND, HEADER + from * ROW + ROW / 2, BAND, HEADER + to * ROW + 8);

/** Clicks the visible cell at screen row `row` of the records' column `col`. */
async function cell(page: Page, row: number, col = 0) {
  const b = await layer(page);
  await page.mouse.click(b.x + column(col), b.y + HEADER + row * ROW + ROW / 2);
}

/** Clicks the records' column `col`'s header, then moves off it. */
async function header(page: Page, col: number) {
  const b = await layer(page);
  await page.mouse.click(b.x + column(col), b.y + HEADER / 2);
  await page.mouse.move(b.x + column(col), b.y + b.height - ROW);
}

const focus = (position: number, id: number, col = "id") =>
  `focus row ${(position + 1).toLocaleString("en")}, column 1 (${id} / ${col})`;

/** The records' indices by weight ascending, nulls last, ties by index. */
function byWeight(): number[] {
  const weight = (i: number) => rows[i]!.weight;
  return rows
    .map((_, i) => i)
    .sort((i, j) => {
      const a = weight(i);
      const b = weight(j);
      if (a === b) return i - j;
      if (a === null) return 1;
      if (b === null) return -1;
      return a - b;
    });
}

/** The canvas's pixels along a line `width` CSS px long from (x, y) in the layer, as one string. */
function pixels(page: Page, x: number, y: number, width: number): Promise<string> {
  return page.locator(".binnacle-layer canvas").evaluate(
    (canvas: HTMLCanvasElement, [x, y, width]) => {
      const dpr = canvas.width / canvas.clientWidth;
      const data = canvas
        .getContext("2d")!
        .getImageData(Math.round(x * dpr), Math.round(y * dpr), Math.round(width * dpr), 1).data;
      return Array.from(data).join(",");
    },
    [x, y, width] as const,
  );
}

/** Whether the strip changes over a few hundred milliseconds of the page's stopped clock. */
async function moving(page: Page, read: () => Promise<string>): Promise<boolean> {
  const first = await read();
  for (let i = 0; i < 4; i++) {
    await page.clock.runFor(120);
    if ((await read()) !== first) return true;
  }
  return false;
}

/**
 * Polls `read`, jumping the page's stopped clock ahead before each read.
 * A jump runs a frame once rather than every 16 ms, so a loaded browser
 * keeps up.
 */
const clocked = <T,>(page: Page, read: () => Promise<T>) =>
  expect.poll(
    async () => {
      await page.clock.fastForward(250);
      return read();
    },
    { intervals: [0], timeout: 20_000 },
  );

const text = async (locator: Locator) => (await locator.textContent()) ?? "";

/**
 * Waits until the widget has run the frame for the input so far. A click
 * hits what the last frame drew, and a pixel read sees what it painted.
 */
const settled = (page: Page) =>
  page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

/** The ARIA table's cell for screen row 0's first column: its record's id. */
const topId = (page: Page) =>
  page.locator('.binnacle-aria [role="row"][aria-rowindex="2"] [role="gridcell"]').first();

/**
 * Whether dragging a column header from x `from` to x `to` draws anything
 * in the header band within 100 px of `to` (away from the lit pin zones and
 * the dragged header's mark) that the pointer merely resting over `to` does
 * not: a drop line where moves are offered.
 */
async function dragDraws(page: Page, from: number, to: number): Promise<boolean> {
  const b = await layer(page);
  const band = () => pixels(page, to - 100, HEADER / 2, 200);
  await page.mouse.move(b.x + to, b.y + HEADER / 2);
  await page.mouse.move(b.x + to + 1, b.y + HEADER / 2);
  const resting = await band();
  let dragging = resting;
  await drag(page, from, HEADER / 2, to + 1, HEADER / 2, async () => {
    await settled(page);
    dragging = await band();
  });
  return dragging !== resting;
}

test.describe("records", () => {
  async function open(page: Page, query = "") {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto(`/?source=records${query}`);
    await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
  }

  test("dragging a row header moves its record", async ({ page }) => {
    await open(page);
    await dragRow(page, 4, 0);
    await expect(topId(page)).toHaveText("4");
    await cell(page, 0);
    await expect(status(page)).toContainText(focus(0, 4));
    await cell(page, 1);
    await expect(status(page)).toContainText(focus(1, 0));
  });

  test("a row move on sorted rows keeps the order shown and drops the sort marker", async ({ page }) => {
    await open(page);
    const weight = page.locator(".binnacle-aria").getByRole("columnheader", { name: "weight" });
    await header(page, 2);
    await expect(weight).toHaveAttribute("aria-sort", "ascending");
    const sorted = byWeight();
    await dragRow(page, 3, 0);
    await expect(topId(page)).toHaveText(String(sorted[3]));
    await cell(page, 0);
    await expect(status(page)).toContainText(focus(0, sorted[3]!));
    await expect(weight).not.toHaveAttribute("aria-sort", "ascending");
    // The rest keep the order the sort showed.
    for (const [at, i] of [0, 1, 2, 4].entries()) {
      await cell(page, at + 1);
      await expect(status(page)).toContainText(focus(at + 1, sorted[i]!));
    }
  });

  test("with sortedMoves=refuse, sorted rows offer no move while columns still move", async ({ page }) => {
    await open(page, "&sortedMoves=refuse");
    await header(page, 2);
    await expect(page.locator(".binnacle-aria").getByRole("columnheader", { name: "weight" })).toHaveAttribute(
      "aria-sort",
      "ascending",
    );
    const sorted = byWeight();
    await cell(page, 0);
    await expect(status(page)).toContainText(focus(0, sorted[0]!));
    await dragRow(page, 3, 0);
    await cell(page, 0);
    await expect(status(page)).toContainText(focus(0, sorted[0]!));
    await expect(announced(page)).not.toContainText("sorted");
    // The columns are not sorted, so the port column still moves first.
    await drag(page, column(4), HEADER / 2, FIRST, HEADER / 2);
    await expect(page.locator('.binnacle-aria [role="columnheader"][aria-colindex="2"]')).toHaveText("port");
    await cell(page, 0, 0);
    await expect(status(page)).toContainText(focus(0, sorted[0]!, "port"));
  });

  test("a column drag draws a drop line where the huge source draws none", async ({ page }) => {
    await open(page);
    expect(await dragDraws(page, column(4), column(1))).toBe(true);
  });
});

test.describe("slow", () => {
  async function open(page: Page, query: string) {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto(`/?source=slow${query}`);
    await expect(status(page)).toContainText("rows 1 to");
    await expect(status(page)).not.toContainText("loading", { timeout: 10_000 });
    await expect(requests(page)).toContainText("0 in flight", { timeout: 10_000 });
  }

  test("a moved row's header shimmers until the move lands, and the order holds meanwhile", async ({ page }) => {
    await page.clock.install();
    await open(page, "&pace=glacial");
    // The clock stands still unless the test runs it, so the move cannot
    // land while the order is checked, however loaded the machine is.
    await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 60_000);
    const rowHeader = () => pixels(page, 0, HEADER + 3 * ROW + 3, 40);
    await dragRow(page, 3, 0);
    await expect(requests(page)).toContainText("1 in flight");
    expect(await moving(page, rowHeader)).toBe(true);
    await cell(page, 0);
    await clocked(page, () => text(status(page))).toContain(focus(0, 0));
    await clocked(page, () => text(requests(page))).toContain("0 in flight");
    expect(await moving(page, rowHeader)).toBe(false);
    await cell(page, 0);
    await clocked(page, () => text(status(page))).toContain(focus(0, 3));
  });

  test("a move that reaches the source after a sort it refuses moves nothing and is announced", async ({ page }) => {
    await page.clock.install();
    await open(page, "&sortedMoves=refuse");
    // The clock stops while the sort and the move are both asked, so the
    // sort lands first and the move finds the rows sorted.
    await page.clock.pauseAt((await page.evaluate(() => Date.now())) + 60_000);
    // The slow demo's columns are all one width: the weight header.
    const b = await layer(page);
    await page.mouse.click(b.x + FIRST + 2 * 141, b.y + HEADER / 2);
    await dragRow(page, 3, 0);
    await expect(requests(page)).toContainText("2 in flight");
    await page.clock.resume();
    await expect(requests(page)).toContainText("0 in flight", { timeout: 10_000 });
    await expect(announced(page)).toContainText("Can't move while sorted");
    await cell(page, 0);
    await expect(status(page)).toContainText(focus(0, byWeight()[0]!));
  });
});

test("the transposed source moves a field by its row header and a record by its column header", async ({ page }) => {
  const FIRST_RECORD = 90;
  const COLUMN = 112 + 1;
  const fields: (keyof RecordRow)[] = ["id", "name", "weight", "crew", "port"];
  await page.setViewportSize({ width: 1000, height: 400 });
  await page.goto("/?source=transposed&theme=light");
  await expect(status(page)).toContainText("rows 1 to 5 of 5");
  // While the port field is dragged its row header is marked, and only it:
  // record 4's column header, whose Key is the same number, looks like
  // record 3's past its label (the column header band is lit as the top pin
  // zone's reach, both alike).
  const portHeader = () => pixels(page, 2, HEADER + 4 * ROW + ROW / 2, 4);
  const recordHeader = (record: number) => pixels(page, FIRST_RECORD + record * COLUMN + 30, 3, 50);
  const before = await portHeader();
  await drag(page, BAND, HEADER + 4 * ROW + ROW / 2, BAND, HEADER + 8, async () => {
    await settled(page);
    expect(await portHeader()).not.toBe(before);
    expect(await recordHeader(4)).toBe(await recordHeader(3));
  });
  await drag(page, FIRST_RECORD + 3 * COLUMN, HEADER / 2, FIRST_RECORD + 30, HEADER / 2);
  await settled(page);
  const b = await layer(page);
  await page.mouse.click(b.x + FIRST_RECORD, b.y + HEADER + ROW / 2);
  await expect(status(page)).toContainText(`focus row 1, column 1 (${fields[4]} / 3)`);
  await page.mouse.click(b.x + FIRST_RECORD + COLUMN, b.y + HEADER + ROW + ROW / 2);
  await expect(status(page)).toContainText(`focus row 2, column 2 (${fields[0]} / 0)`);
});

test("the huge source offers no move: a header drag draws no drop line", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=huge");
  await expect(status(page)).toContainText("rows 1 to");
  const b = await layer(page);
  expect(await dragDraws(page, b.width * 0.75, b.width * 0.4)).toBe(false);
});
