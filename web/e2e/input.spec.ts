// Keys, clicks and resizing end to end. Keyboard navigation runs on the huge
// demo table; pointer tests mount a small grid with fixed geometry, so
// borders sit at known points: a 40 px row-number band, columns 100 px wide
// with a 1 px Gap (column p spans 40 + 101p to 140 + 101p), a 28 px header
// band and 24 px rows.

import { expect, test, type Page } from "@playwright/test";
import type { FrameInfo } from "../src";

const status = (page: Page) => page.locator("#status");

/** Cmd where the page sees macOS, Ctrl elsewhere: the user agent the browser is given decides. */
const mod = (page: Page) =>
  page.evaluate(() => {
    const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
    return /mac/i.test(nav.userAgentData?.platform ?? nav.platform) ? "Meta" : "Control";
  });

test.describe("keys on the huge table", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/?source=huge");
    await expect(status(page)).toContainText("rows 1 to");
    const box = (await page.locator(".binnacle-layer").boundingBox())!;
    await page.mouse.click(box.x + box.width / 2, box.y + 28 + 24 * 2 + 12);
    await expect(status(page)).toContainText("focus row 3,");
  });

  test("arrows move focus and Shift extends the selection", async ({ page }) => {
    const col = Number((await status(page).textContent())!.match(/column (\d+)/)![1]);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowRight");
    await expect(status(page)).toContainText(`focus row 4, column ${col + 1}`);
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowLeft");
    await expect(status(page)).toContainText(`focus row 6, column ${col}  ·  selected 3 × 2`);
    await page.keyboard.press("ArrowUp");
    await expect(status(page)).toHaveText(new RegExp(`focus row 5, column ${col}$`));
  });

  test("Cmd/Ctrl+arrow jumps to the last row of 10^15 and back", async ({ page }) => {
    await page.keyboard.press(`${await mod(page)}+ArrowDown`);
    await expect(status(page)).toContainText("focus row 1,000,000,000,000,000,");
    await expect(status(page)).toContainText("to 1,000,000,000,000,000 of");
    await page.keyboard.press("Shift+PageUp");
    await expect(status(page)).toContainText(/selected \d+ × 1/);
    await page.keyboard.press("Home");
    await expect(status(page)).toContainText("rows 1 to");
    await expect(status(page)).toContainText("focus row 1,");
  });

  test("Cmd/Ctrl+A selects every cell", async ({ page }) => {
    await page.keyboard.press(`${await mod(page)}+a`);
    await expect(status(page)).toContainText("selected 1,000,000,000,000,000 × 12");
  });

  test("keys the grid does not use reach the page, and only mapped keys are prevented", async ({ page }) => {
    await page.evaluate(() => {
      const w = window as unknown as { seen: string[] };
      w.seen = [];
      document.addEventListener("keydown", (e) => {
        if (!["Control", "Meta"].includes(e.key)) w.seen.push(`${e.key}:${e.defaultPrevented}`);
      });
    });
    await page.keyboard.press("x");
    await page.keyboard.press(`${await mod(page)}+k`);
    await page.keyboard.press("ArrowDown");
    const seen = await page.evaluate(() => (window as unknown as { seen: string[] }).seen);
    expect(seen).toEqual(["x:false", "k:false", "ArrowDown:true"]);
  });

  test("Tab leaves the grid, and tabbing back in keeps its focus", async ({ page }) => {
    const input = page.locator(".binnacle-input");
    await expect(input).toBeFocused();
    // The toolbar comes before the grid in tab order; the Border select is its last control.
    await page.keyboard.press("Shift+Tab");
    await expect(page.locator("#border")).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(input).toBeFocused();
    await expect(status(page)).toContainText("focus row 3,");
  });
});

/** Mounts a 100 by 6 grid with fixed geometry over the page; frames land in `window.frame`. */
async function mount(page: Page) {
  await page.goto("/?source=huge");
  await expect(status(page)).toContainText("rows 1 to");
  await page.evaluate(async () => {
    // Served by Vite; the type comes from the same module on disk.
    const path = "/src/index.ts";
    const { GridWidget } = (await import(path)) as typeof import("../src");
    const host = document.createElement("div");
    host.id = "fixture";
    host.style.cssText = "position: fixed; left: 0; top: 0; width: 700px; height: 400px; z-index: 10; background: white;";
    document.body.append(host);
    const range = (s: number, e: number) => Array.from({ length: e - s }, (_, i) => s + i);
    const w = window as unknown as { frame: FrameInfo };
    await GridWidget.create(host, {
      source: {
        rows: { identity: "position", count: () => 100, window: range },
        cols: { identity: "position", count: () => 6, window: range },
        cells: (rs: number[], cs: number[]) => rs.map((r) => cs.map((c) => `r${r}c${c}`)),
      },
      rowHeight: 24,
      columnWidth: 100,
      headerHeight: 28,
      onFrame: (f) => {
        w.frame = f;
      },
    });
  });
  await expect.poll(() => frame(page)).toBeTruthy();
}

const frame = (page: Page) => page.evaluate(() => (window as unknown as { frame?: FrameInfo }).frame);

const cursor = (page: Page) => page.locator("#fixture .binnacle-layer").evaluate((el) => (el as HTMLElement).style.cursor);

/** The middle of the cell at row `r`, column `c`, with the default sizes. */
const cell = (r: number, c: number) => ({ x: 40 + 101 * c + 50, y: 28 + 24 * r + 12 });

async function click(page: Page, at: { x: number; y: number }, modifier?: "Shift" | "ControlOrMeta") {
  if (modifier) await page.keyboard.down(modifier);
  await page.mouse.click(at.x, at.y);
  if (modifier) await page.keyboard.up(modifier);
}

/** Waits until the widget has run the frame for the input so far. */
const settled = (page: Page) =>
  page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

/** Whether the pointer at `x` in the column header band is on a column border. */
async function onBorder(page: Page, x: number): Promise<boolean> {
  await page.mouse.move(x, 15);
  await settled(page);
  return (await cursor(page)) === "col-resize";
}

test.describe("pointer", () => {
  test.beforeEach(async ({ page }) => mount(page));

  test("click focuses, Shift-click extends, Cmd/Ctrl-click picks rows", async ({ page }) => {
    await click(page, cell(2, 1));
    await expect.poll(async () => (await frame(page))?.focus).toEqual({ row: 2, col: 1 });
    await click(page, cell(5, 3), "Shift");
    await expect.poll(async () => (await frame(page))?.selection).toEqual({
      rows: { start: 2, end: 6 },
      cols: { start: 1, end: 4 },
    });
    await click(page, cell(8, 0), "ControlOrMeta");
    await click(page, cell(10, 0), "ControlOrMeta");
    await expect.poll(async () => (await frame(page))?.picked).toBe(2);
    await click(page, cell(10, 0), "ControlOrMeta");
    await expect.poll(async () => (await frame(page))?.picked).toBe(1);
    await click(page, cell(1, 1));
    await expect.poll(async () => frame(page)).toMatchObject({ focus: { row: 1, col: 1 }, picked: 0 });
    await expect(page.locator("#fixture .binnacle-input")).toBeFocused();
  });

  test("dragging across cells selects a range", async ({ page }) => {
    const from = cell(1, 0);
    const to = cell(4, 2);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 5 });
    await page.mouse.up();
    await expect.poll(async () => (await frame(page))?.selection).toEqual({
      rows: { start: 1, end: 5 },
      cols: { start: 0, end: 3 },
    });
  });

  test("dragging a column border resizes the column", async ({ page }) => {
    expect(await onBorder(page, 140)).toBe(true);
    await page.mouse.down();
    await page.mouse.move(200, 15, { steps: 4 });
    // Past the grid's edge too: the drag keeps tracking under pointer capture.
    await page.mouse.move(200, 600, { steps: 2 });
    await page.mouse.up();
    expect(await onBorder(page, 140)).toBe(false);
    expect(await onBorder(page, 200)).toBe(true);
    // The cell right of the old border is still column 0.
    await click(page, { x: 170, y: cell(3, 0).y });
    await expect.poll(async () => (await frame(page))?.focus).toEqual({ row: 3, col: 0 });
  });

  test("double-clicking a column border fits the column to its visible text", async ({ page }) => {
    await page.mouse.dblclick(140, 15);
    // "r99c0" in 13 px type is far narrower than 100 px.
    await page.mouse.move(300, 300);
    expect(await onBorder(page, 140)).toBe(false);
    await click(page, { x: 90, y: cell(0, 0).y });
    await expect.poll(async () => (await frame(page))?.focus?.col).toBe(1);
  });

  test("dragging the row-number band's edge resizes the band", async ({ page }) => {
    await page.mouse.move(39, 14);
    await settled(page);
    expect(await cursor(page)).toBe("col-resize");
    await page.mouse.down();
    await page.mouse.move(90, 14, { steps: 4 });
    await page.mouse.up();
    // The band now ends past x = 80: a click there is on row 2's header, selecting the row.
    await click(page, { x: 80, y: cell(2, 0).y });
    await expect.poll(async () => (await frame(page))?.selection).toEqual({
      rows: { start: 2, end: 3 },
      cols: { start: 0, end: 6 },
    });
    await click(page, { x: 100, y: cell(4, 0).y });
    await expect.poll(async () => frame(page)).toMatchObject({ focus: { row: 4, col: 0 }, selection: null });
  });

  test("a lost pointer capture cancels the resize", async ({ page }) => {
    expect(await onBorder(page, 140)).toBe(true);
    await page.mouse.down();
    await page.mouse.move(220, 15, { steps: 4 });
    await page.locator("#fixture .binnacle-layer").evaluate((el) => {
      for (let id = 0; id < 32; id++) if (el.hasPointerCapture(id)) el.releasePointerCapture(id);
    });
    await page.mouse.up();
    expect(await onBorder(page, 220)).toBe(false);
    expect(await onBorder(page, 140)).toBe(true);
  });
});

test.describe("touch", () => {
  test.use({ hasTouch: true });
  test.skip(({ browserName }) => browserName !== "chromium", "touch emulation is Chromium's");
  test.beforeEach(async ({ page }) => mount(page));

  test("a tap focuses the cell", async ({ page }) => {
    const at = cell(3, 2);
    await page.touchscreen.tap(at.x, at.y);
    await expect.poll(async () => (await frame(page))?.focus).toEqual({ row: 3, col: 2 });
    await expect(page.locator("#fixture .binnacle-input")).toBeFocused();
  });

  test("a touch drag pans the grid instead of selecting", async ({ page }) => {
    const cdp = await page.context().newCDPSession(page);
    const touch = (type: "touchStart" | "touchMove" | "touchEnd", y: number) =>
      cdp.send("Input.dispatchTouchEvent", {
        type,
        touchPoints: type === "touchEnd" ? [] : [{ x: cell(0, 1).x, y }],
      });
    await touch("touchStart", 300);
    for (let y = 290; y >= 100; y -= 10) await touch("touchMove", y);
    await touch("touchEnd", 100);
    await expect.poll(async () => (await frame(page))?.rows.first).toBeGreaterThan(3);
    expect((await frame(page))?.focus).toBeNull();
  });
});
