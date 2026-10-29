// The pivot demo end to end (#209): the shipments with ports by region down
// the rows and quarters across, each quarter spanning its total weight and
// count columns, the All columns pinned right. Cells are canvas pixels, so
// the status line's focus (Position and IDs) is the probe: a pivot's rows and
// columns both come from the data, by negative IDs. Sorting and filtering
// across the pivot (#211) read the ARIA table's row headers and the filter
// bar's chips; a second click on a header before the frame that draws the
// first one's sort still cycles it.

import { expect, test, type Page } from "@playwright/test";
import { quarterShipments } from "../demo/sources/pivot";

const ROW = 24;
/** The column band: the quarters' row, then the value columns' own. */
const BAND = 2 * 28;
/** The demo's column width and the Gap after it. */
const COLUMN = 112 + 1;
/** The row band's width as the demo restores it. */
const ROWS_BAND = 180;

const status = (page: Page) => page.locator("#status");
/** A column header by its accessible name. */
const header = (page: Page, name: string) =>
  page.locator(".binnacle-aria").getByRole("columnheader", { name, exact: true });
const bar = (page: Page) => page.getByRole("search");

/** The ARIA table's body rows: each one's level and header text, its count left off. */
const rowHeaders = (page: Page) =>
  page.evaluate(() =>
    [...document.querySelectorAll(".binnacle-aria tbody tr")].map((tr) => ({
      level: tr.getAttribute("aria-level"),
      header: (tr.querySelector('[role="rowheader"]')?.textContent ?? "").replace(/ \(\d+\)$/, ""),
    })),
  );

/** The region shown first: the blank one, as nulls sort lowest. */
const FIRST = null;

/** The ports shown under `region`, in order. */
async function portsUnder(page: Page, region: string | null): Promise<string[]> {
  const rows = await rowHeaders(page);
  const at = rows.findIndex((r) => r.level === "1" && r.header === (region ?? "(blank)"));
  const end = rows.findIndex((r, i) => i > at && r.level === "1");
  return rows.slice(at + 1, end < 0 ? undefined : end).map((r) => r.header);
}

/** `region`'s ports by their total weight in `quarter`, most first, those with none last; ties by name. */
function byQuarter(region: string | null, quarter: string): string[] {
  const totals = new Map<string, number | null>();
  for (const s of quarterShipments) {
    if (s.region !== region) continue;
    if (!totals.has(s.port)) totals.set(s.port, null);
    if (s.quarter === quarter && s.weight !== null) totals.set(s.port, (totals.get(s.port) ?? 0) + s.weight);
  }
  return [...totals]
    .sort(([a, x], [b, y]) => (x === y ? (a < b ? -1 : 1) : x === null ? 1 : y === null ? -1 : y - x))
    .map(([port]) => port);
}

/**
 * Clicks the header of screen column `col` in the value columns' row twice in
 * one task, so both clicks come before the next frame. The presses carry the
 * mouse's own pointer ID, from a real move, so the layer can capture it.
 */
async function clickHeaderTwice(page: Page, col: number) {
  const layer = page.locator(".binnacle-layer");
  const at = { x: ROWS_BAND + col * COLUMN + COLUMN / 2, y: BAND - 14 };
  const b = (await layer.boundingBox())!;
  await layer.evaluate((el: HTMLElement) =>
    el.addEventListener("pointermove", (e) => (el.dataset.pointer = String(e.pointerId)), { once: true }),
  );
  await page.mouse.move(b.x + at.x, b.y + at.y);
  await layer.evaluate((el: HTMLElement, { x, y }) => {
    const b = el.getBoundingClientRect();
    const pointerId = Number(el.dataset.pointer);
    const init = { clientX: b.left + x, clientY: b.top + y, pointerId, pointerType: "mouse", button: 0, bubbles: true, cancelable: true };
    for (let i = 0; i < 2; i++) {
      el.dispatchEvent(new PointerEvent("pointerdown", { ...init, buttons: 1 }));
      el.dispatchEvent(new PointerEvent("pointerup", { ...init, buttons: 0 }));
    }
  }, at);
}

/** Clicks the header of screen column `col` in the value columns' row. */
async function clickHeader(page: Page, col: number) {
  const b = (await page.locator(".binnacle-layer").boundingBox())!;
  await page.mouse.click(b.x + ROWS_BAND + col * COLUMN + COLUMN / 2, b.y + BAND - 14);
}

const regions = new Set(quarterShipments.map((s) => s.region)).size;
const ports = new Set(quarterShipments.map((s) => `${s.region}/${s.port}`)).size;
const quarters = new Set(quarterShipments.map((s) => s.quarter)).size;

/** Clicks the cell at screen row `row` (0 first) of screen column `col`. */
async function cell(page: Page, row: number, col: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + ROWS_BAND + col * COLUMN + COLUMN / 2, b.y + BAND + row * ROW + ROW / 2);
}

/** The focus as the status line gives it: its Positions and its row and column IDs. */
async function focus(page: Page): Promise<string> {
  const text = (await status(page).textContent()) ?? "";
  return /focus (.*)$/.exec(text)?.[1] ?? "";
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 600 });
  await page.goto("/?source=pivot&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  // Each region heads its ports, and the Grand Total ends the rows; each quarter has two value columns, and each value an All column.
  await expect(status(page)).toContainText(`of ${regions + ports + 1}`);
  await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
  // The filter bar shows from a frame and shrinks the grid, and the layer
  // follows in a frame after: wait for it before measuring the layer.
  await expect(bar(page)).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const height = (selector: string) => document.querySelector(selector)!.getBoundingClientRect().height;
        return height(".binnacle-layer") === height(".grid-box");
      }),
    )
    .toBe(true);
});

test("ports by region down and quarters across, each quarter over its values, the All columns pinned right", async ({
  page,
  browserName,
}) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await expect(page.locator("#grid")).toHaveScreenshot("pivot.png", { scale: "device" });
});

test("scrolled to the end, the last quarter meets the All columns pinned at the right", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await page.setViewportSize({ width: 800, height: 400 });
  await page.mouse.move(400, 300);
  await expect(async () => {
    await page.mouse.wheel(20 * COLUMN, 0);
    await expect(status(page)).toContainText("to 8 of 10", { timeout: 200 });
  }).toPass();
  await expect(page.locator("#grid")).toHaveScreenshot("pivot-scrolled.png", { scale: "device" });
});

test("a pivot column's header drags nothing: the columns follow their sort", async ({ page }) => {
  await cell(page, 1, 0);
  await expect(status(page)).toContainText("focus row 2, column 1");
  const before = await focus(page);
  expect(before).toMatch(/^row 2, column 1 \(-\d+ \/ -\d+\)$/);
  const b = (await page.locator(".binnacle-layer").boundingBox())!;
  const y = b.y + BAND - 14;
  await page.mouse.move(b.x + ROWS_BAND + COLUMN / 2, y);
  await page.mouse.down();
  await page.mouse.move(b.x + ROWS_BAND + 3 * COLUMN, y, { steps: 8 });
  await page.mouse.up();
  await cell(page, 1, 0);
  await expect(status(page)).toContainText(`focus ${before}`);
});

test("focus moves along a row's quarters, each pivot column by its own ID", async ({ page }) => {
  await cell(page, 1, 0);
  await expect(status(page)).toContainText("focus row 2, column 1");
  const first = await focus(page);
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText("focus row 2, column 2");
  expect(await focus(page)).not.toBe(first);
  await page.keyboard.press("ArrowLeft");
  await expect(status(page)).toContainText(`focus ${first}`);
});

test("Up from the first row enters the quarters, where Left steps out to Q1 and closes it to its total columns", async ({
  page,
  browserName,
}) => {
  await cell(page, 0, 0);
  await expect(status(page)).toContainText("focus row 1, column 1");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  // Q1's two value columns give way to its total of each value.
  await expect(header(page, "Q1 count")).toBeVisible();
  await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
  if (browserName === "chromium") {
    await expect(page.locator("#grid")).toHaveScreenshot("pivot-q1-closed.png", { scale: "device" });
  }
  await page.keyboard.press("Enter");
  await expect(header(page, "Q1 count")).toHaveCount(0);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText("focus row 1, column 2");
});

test("a click on a quarter's mark closes it and another opens it", async ({ page }) => {
  const b = (await page.locator(".binnacle-layer").boundingBox())!;
  const mark = { x: b.x + ROWS_BAND + 10, y: b.y + 14 };
  await page.mouse.click(mark.x, mark.y);
  await expect(header(page, "Q1 count")).toBeVisible();
  await page.mouse.click(mark.x, mark.y);
  await expect(header(page, "Q1 count")).toHaveCount(0);
});

test("a click on Q4's weight header sorts each region's ports by it; without the Q4 shipments the sort stays, not applied, and applies again with them", async ({
  page,
}) => {
  const ascending = await portsUnder(page, FIRST);
  expect(ascending).toEqual([...ascending].sort((a, b) => (a < b ? -1 : 1)));
  // Q4's weight is the seventh column: two per quarter before it. Ascending, then descending.
  await clickHeader(page, 6);
  await clickHeader(page, 6);
  const sorted = byQuarter(FIRST, "Q4");
  expect(sorted).not.toEqual(ascending);
  await expect.poll(() => portsUnder(page, FIRST)).toEqual(sorted);
  await expect(bar(page).getByText("not applied")).toHaveCount(0);

  await page.getByLabel("Q4 shipments").uncheck();
  await expect(status(page)).toContainText(`of ${(quarters - 1) * 2 + 2}`);
  await expect(bar(page).getByText("Sorted by Q4 sum(weight) descending: not applied")).toBeVisible();
  await expect.poll(() => portsUnder(page, FIRST)).toEqual(ascending);

  await page.getByLabel("Q4 shipments").check();
  await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
  await expect(bar(page).getByText("not applied")).toHaveCount(0);
  await expect.poll(() => portsUnder(page, FIRST)).toEqual(sorted);
});

test("a second click on Q4's weight header before the frame that draws the first one's sort sorts it descending", async ({
  page,
}) => {
  // The first click's sort is answered at once, which leaves the Grid
  // without the columns' Keys until the next frame fetches them again.
  await clickHeaderTwice(page, 6);
  await expect.poll(() => portsUnder(page, FIRST)).toEqual(byQuarter(FIRST, "Q4"));
});

test("the Grand Total row's menu keeps the quarters whose total weight passes its comparison", async ({ page }) => {
  const b = (await page.locator(".binnacle-layer").boundingBox())!;
  // The Grand Total row, pinned at the bottom.
  await page.mouse.click(b.x + ROWS_BAND + COLUMN / 2, b.y + b.height - ROW / 2);
  await expect(status(page)).toContainText(`focus row ${regions + ports + 1},`);
  await page.keyboard.press("Alt+ArrowRight");
  const menu = page.getByRole("dialog", { name: "Filter columns by Grand total" });
  await menu.getByRole("combobox", { name: "Comparison" }).selectOption("greater");
  await menu.getByRole("spinbutton").fill("110000");
  await page.keyboard.press("Escape");
  const totals = new Map<string, number>();
  for (const s of quarterShipments) totals.set(s.quarter, (totals.get(s.quarter) ?? 0) + (s.weight ?? 0));
  const kept = [...totals.values()].filter((t) => t > 110_000).length;
  expect(kept).toBeGreaterThan(0);
  expect(kept).toBeLessThan(quarters);
  await expect(status(page)).toContainText(`of ${kept * 2 + 2}`);
  await expect(bar(page).getByRole("listitem")).toHaveText(["Grand total: > 110000×"]);
});

test("a sort on a quarter the data lacks shows as a red chip", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await clickHeader(page, 6);
  await page.getByLabel("Q4 shipments").uncheck();
  await expect(bar(page).getByText("not applied")).toBeVisible();
  await expect(page.locator("#grid")).toHaveScreenshot("pivot-sort-not-applied.png", { scale: "device" });
});
