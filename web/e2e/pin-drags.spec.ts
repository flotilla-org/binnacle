// Dragging headers across the pinned edge end to end: a header dropped over
// the row-number band (or the column header band) pins its column (or row),
// dragged out again it unpins, and moves where moves are offered; pinned
// headers reorder among themselves. On the records, on their column-major
// twin, and on a fixture whose axes can't move, where a pin dragged out
// returns to its own place. Screenshots show the lit pin zones, the drop
// line's caption and the dashed line of an unpin that returns.

import { expect, test, type Page } from "@playwright/test";

const HEADER = 28;
const ROW = 24;
/** Inside the row-number band, clear of the filter marker at its left. */
const ROW_HEADER = 30;

const status = (page: Page) => page.locator("#status");

/** The layer's box and the right edge of the row-number band, from the layer's left. */
async function geometry(page: Page) {
  const layer = await page.locator(".binnacle-layer").boundingBox();
  const band = await page.locator(".binnacle-band-left").boundingBox();
  if (!layer || !band) throw new Error("no grid");
  return { ...layer, band: band.x + band.width - layer.x };
}

/** Presses at (x0, y0) in the layer and drags to (x1, y1), running `during` before the release. */
async function drag(page: Page, x0: number, y0: number, x1: number, y1: number, during?: () => Promise<void>) {
  const b = await geometry(page);
  await page.mouse.move(b.x + x0, b.y + y0);
  await page.mouse.down();
  await page.mouse.move(b.x + (x0 + x1) / 2, b.y + (y0 + y1) / 2, { steps: 5 });
  await page.mouse.move(b.x + x1, b.y + y1, { steps: 5 });
  await during?.();
  await page.mouse.up();
}

/** Clicks a body cell at layer x `x`, screen row `row`. */
async function click(page: Page, x: number, row = 0) {
  const b = await geometry(page);
  await page.mouse.click(b.x + x, b.y + HEADER + row * ROW + ROW / 2);
}

test.describe("records", () => {
  /** Inside the drawn column after `band`: the demo's id, name, weight, crew are 64, 128, 96, 64 wide. */
  const lefts = [0, 65, 194, 291, 356];
  const at = (band: number, col: number) => band + 1 + lefts[col]! + 10;

  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records&theme=light");
    await expect(status(page)).toContainText("rows 1 to");
  });

  test("a column header dropped over the row-number band pins the column, and dragged out it unpins and moves", async ({
    page,
  }) => {
    const { band } = await geometry(page);
    await drag(page, at(band, 3), HEADER / 2, 10, HEADER / 2);
    await click(page, at(band, 0));
    await expect(status(page)).toContainText("column 4 (0 / crew)");
    // Out again into the first half of the id column, drawn just after the
    // pinned crew: unpinned and moved to the front.
    await drag(page, band + 20, HEADER / 2, band + 1 + 65 + 10, HEADER / 2);
    await click(page, at(band, 0));
    await expect(status(page)).toContainText("column 1 (0 / crew)");
    await click(page, at(band, 1));
    await expect(status(page)).toContainText("column 2 (0 / id)");
  });

  test("pinned column headers reorder among themselves", async ({ page }) => {
    const { band } = await geometry(page);
    await drag(page, at(band, 0), HEADER / 2, 10, HEADER / 2);
    // Over the band, ahead of the pinned id column: crew pins before it.
    await drag(page, at(band, 3), HEADER / 2, 10, HEADER / 2);
    await click(page, at(band, 0));
    await expect(status(page)).toContainText("column 4 (0 / crew)");
    // id, pinned second, dragged ahead of crew.
    await drag(page, at(band, 1), HEADER / 2, 10, HEADER / 2);
    await click(page, at(band, 0));
    await expect(status(page)).toContainText("column 1 (0 / id)");
    await click(page, at(band, 1));
    await expect(status(page)).toContainText("column 4 (0 / crew)");
  });

  test("a row header dragged past the last row pins it bottom", async ({ page }) => {
    const b = await geometry(page);
    await drag(page, ROW_HEADER, HEADER + ROW / 2, ROW_HEADER, b.height - 2);
    await page.mouse.click(b.x + at(b.band, 0), b.y + b.height - ROW / 2);
    await expect(status(page)).toContainText("(0 / id)");
  });
});

test("on the column-major source a record's column header pins left and a field's row header pins bottom", async ({
  page,
}) => {
  // The row header band fits the field names beside their marks, so the records start at 96.
  const FIRST = 100;
  const COLUMN = 112 + 1;
  await page.setViewportSize({ width: 1000, height: 400 });
  await page.goto("/?source=transposed&theme=light");
  await expect(status(page)).toContainText("rows 1 to 5 of 5");
  await drag(page, FIRST + 3 * COLUMN, HEADER / 2, 10, HEADER / 2);
  await click(page, FIRST, 1);
  await expect(status(page)).toContainText("(name / 3)");
  // The id field, dragged below the last field, pinned to the bottom.
  const b = await geometry(page);
  await drag(page, ROW_HEADER, HEADER + ROW / 2, ROW_HEADER, b.height - 2);
  await click(page, FIRST, 0);
  await expect(status(page)).toContainText("(name / 3)");
  // Drawn at the bottom edge, past the empty space under the other fields.
  await page.mouse.click(b.x + FIRST + COLUMN, b.y + b.height - ROW / 2);
  await expect(status(page)).toContainText("(id / 0)");
});

test.describe("screenshots on a fixture whose axes can't move", () => {
  test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

  async function open(page: Page, query: Record<string, string> = {}) {
    await page.goto(`/e2e/fixtures/presentation.html?${new URLSearchParams({ theme: "light", ...query })}`);
    const grid = page.locator("#grid");
    await expect(grid).toHaveAttribute("data-painted", "true");
    return grid;
  }

  test("a column header over the row-number band lights both empty pin zones, the left one hot, captioned pin left", async ({
    page,
  }) => {
    const grid = await open(page);
    const { band } = await geometry(page);
    await drag(page, band + 2 * 97 + 40, HEADER / 2, 10, HEADER / 2, async () => {
      await expect(grid).toHaveScreenshot("drag-pin-left.png", { scale: "device" });
    });
    await expect(grid).toHaveScreenshot("dragged-pinned-left.png", { scale: "device" });
  });

  test("a pin dragged out returns to its own place, its drop line dashed there and captioned", async ({ page }) => {
    const grid = await open(page, { pin: "3" });
    const { band } = await geometry(page);
    await drag(page, band + 40, HEADER / 2, 400, HEADER / 2, async () => {
      await expect(grid).toHaveScreenshot("drag-unpin-returns.png", { scale: "device" });
    });
    await expect(grid).toHaveScreenshot("dragged-unpinned.png", { scale: "device" });
  });

  test("a row header below the last row lights the bottom zone hot, captioned pin bottom", async ({ page }) => {
    const grid = await open(page);
    const b = await geometry(page);
    await drag(page, ROW_HEADER, HEADER + ROW + ROW / 2, ROW_HEADER, b.height - 4, async () => {
      await expect(grid).toHaveScreenshot("drag-pin-bottom.png", { scale: "device" });
    });
    await expect(grid).toHaveScreenshot("dragged-pinned-bottom.png", { scale: "device" });
  });
});
