// The slow source end to end: loading cells until the answers land, a
// re-sort while the first sort is still in flight, and what pending states
// look like with motion and with motion reduced. Canvas pixels are read
// back to see whether a pending header or cell is still moving. A quick
// filter keeps the rows until its last answer, and an invalid regex turns
// the field (or find's count) red with its message. The page's clock stands
// still unless a test runs it, so the source answers, and pending states
// move, at the test's pace however loaded the machine is.

import { expect, test, type Locator, type Page } from "@playwright/test";
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

/** Whether the strip changes over a few hundred milliseconds of the page's clock. */
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

/** Opens the slow source with the page's clock stopped. */
async function open(page: Page, pace: string) {
  await page.clock.install({ time: 0 });
  await page.clock.pauseAt(1000);
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto(`/?source=slow&pace=${pace}`);
}

/** Runs the clock until the first answers have landed. */
async function loaded(page: Page) {
  await clocked(page, () => text(status(page))).toContain("rows 1 to");
  await clocked(page, () => text(status(page))).not.toContain("loading");
  await clocked(page, () => text(requests(page))).toContain("0 in flight");
}

test("cells show as loading until the slow source answers, then settle", async ({ page }) => {
  await open(page, "slow");
  await clocked(page, () => text(status(page))).toContain("loading");
  const loading = await textStrip(page, 0);
  await loaded(page);
  expect(await textStrip(page, 0)).not.toBe(loading);
  await cell(page, 0);
  await clocked(page, () => text(status(page))).toContain(focus(0, 0));
});

test("a re-sort while the first sort is in flight aborts it, and the grid settles on the latest", async ({ page }) => {
  await open(page, "slow");
  await loaded(page);
  await cell(page, 0);
  await clocked(page, () => text(status(page))).toContain(focus(0, 0));
  // The clock stands still while both sorts are asked, so the first cannot
  // land first however slowly the browser takes the second click. Clicks
  // reach the source at once; only its answers wait on the clock.
  const before = Number((await requests(page).textContent())!.match(/(\d+) aborted/)![1]);
  await header(page, 2);
  await expect(requests(page)).toHaveText(`1 in flight · ${before} aborted`);
  // Ascending is still in flight: ask for descending.
  await header(page, 2);
  await expect(requests(page)).toHaveText(`1 in flight · ${before + 1} aborted`);
  await loaded(page);
  const descending = byWeight("desc");
  // Focus followed record 0 to where descending put it.
  await clocked(page, () => text(status(page))).toContain(focus(descending.indexOf(0), 0));
  await cell(page, 0);
  await clocked(page, () => text(status(page))).toContain(focus(0, descending[0]!));
});

test("a pending sort shimmers and a pending locate pulses, then both settle", async ({ page }) => {
  await open(page, "glacial");
  await loaded(page);
  await cell(page, 0);
  await header(page, 2);
  expect(await moving(page, () => headerStrip(page, 2))).toBe(true);
  // The sort lands, and focus is located: its last known cell pulses.
  await clocked(page, () => moving(page, () => cellStrip(page, 0))).toBe(true);
  await clocked(page, () => text(requests(page))).toContain("0 in flight");
  expect(await moving(page, () => headerStrip(page, 2))).toBe(false);
  expect(await moving(page, () => cellStrip(page, 0))).toBe(false);
});

test("with motion reduced, pending states hold still at a static highlight", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await open(page, "glacial");
  await loaded(page);
  const idle = await headerStrip(page, 2);
  await cell(page, 0);
  await header(page, 2);
  await clocked(page, () => headerStrip(page, 2)).not.toBe(idle);
  expect(await moving(page, () => headerStrip(page, 2))).toBe(false);
});

/** How many records have some cell whose text contains `text`, ignoring case. */
const quickCount = (text: string) =>
  rows.filter((r) => Object.values(r).some((v) => v != null && String(v).toLowerCase().includes(text))).length;

const filterField = (page: Page) => page.getByRole("search").first().getByRole("textbox").first();
const rowCount = (page: Page) => page.locator(".binnacle-row-count");

test("typing a quick filter keeps the rows, reads filtering…, and paints only the last answer", async ({ page }) => {
  await open(page, "slow");
  await loaded(page);
  // Every status line the page shows from here on.
  await page.evaluate(() => {
    const seen: string[] = [];
    (window as unknown as { seen: string[] }).seen = seen;
    const line = document.querySelector("#status")!;
    new MutationObserver(() => seen.push(line.textContent ?? "")).observe(line, {
      childList: true,
      subtree: true,
      characterData: true,
    });
  });
  const before = Number((await requests(page).textContent())!.match(/(\d+) aborted/)![1]);
  await filterField(page).click();
  await page.keyboard.type("b");
  await clocked(page, () => text(rowCount(page))).toBe("filtering…");
  // The answer for "b" is still on its way when the rest is typed.
  await page.keyboard.type("erg");
  await clocked(page, () => text(rowCount(page))).toBe("filtering…");
  await clocked(page, () => text(status(page))).toContain("of 5,000");
  const n = quickCount("berg");
  await clocked(page, () => text(rowCount(page))).toBe(`${n.toLocaleString("en")} of 5,000`);
  // The request for "b" was aborted (and maybe cells besides).
  await expect
    .poll(async () => Number((await requests(page).textContent())!.match(/(\d+) aborted/)![1]))
    .toBeGreaterThan(before);
  const seen = await page.evaluate(() => (window as unknown as { seen: string[] }).seen);
  expect(seen.filter((line) => line.includes(`of ${quickCount("b").toLocaleString("en")} `))).toEqual([]);
});

test("an invalid regex turns the quick filter red with its message, the text kept and the rows in force", async ({
  page,
}) => {
  await open(page, "brisk");
  await loaded(page);
  const all = "5,000 rows";
  await clocked(page, () => text(rowCount(page))).toBe(all);
  await page.getByRole("search").first().getByRole("button", { name: "Use regular expression" }).click();
  await filterField(page).fill("berg(");
  const error = page.locator(".binnacle-filter-error");
  // RegExp's own words, which differ between browsers.
  await clocked(page, () => text(error)).toMatch(/\w/);
  await expect(error).not.toHaveText("Not applied");
  await expect(filterField(page)).toHaveValue("berg(");
  await expect(filterField(page)).toHaveAttribute("aria-invalid", "");
  await expect(rowCount(page)).toHaveText(all);
  await filterField(page).fill("berg");
  await clocked(page, () => error.isHidden()).toBe(true);
  await clocked(page, () => text(rowCount(page))).toBe(`${quickCount("berg").toLocaleString("en")} of 5,000`);
});

test("find reads finding… until the slow count lands, and an invalid regex reads No matches with its message", async ({
  page,
}) => {
  await open(page, "brisk");
  await loaded(page);
  await page.getByRole("button", { name: "Find" }).click();
  const find = page.getByRole("search").nth(1);
  const count = page.locator(".binnacle-find-count");
  await find.getByRole("textbox").fill("bergen");
  await clocked(page, () => text(count)).toBe("finding…");
  await clocked(page, () => text(count)).toMatch(/\d matches/);
  await find.getByRole("button", { name: "Use regular expression" }).click();
  await find.getByRole("textbox").fill("berg(");
  await clocked(page, () => text(count)).toBe("No matches");
  await expect(page.locator(".binnacle-find-wrapped")).toHaveText(/\w/);
  await expect(find.getByRole("textbox")).toHaveValue("berg(");
});
