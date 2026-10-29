// Row header labels stay whole beside what their header can show: a sort
// marker, the row menu's button while hovered, and a filtered row's mark
// (#160). The labels are canvas text, so every string the page draws is
// recorded and the probe is that a label was drawn whole, not cut to "…".

import { expect, test, type Page } from "@playwright/test";

const HEADER = 28;
const ROW = 24;

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as unknown as { drawn: string[] };
    w.drawn = [];
    const fillText = CanvasRenderingContext2D.prototype.fillText;
    CanvasRenderingContext2D.prototype.fillText = function (text, ...rest) {
      w.drawn.push(text);
      return fillText.call(this, text, ...rest);
    };
  });
});

const forget = (page: Page) =>
  page.evaluate(() => {
    (window as unknown as { drawn: string[] }).drawn = [];
  });

/** The text drawn since the last `forget`. */
const drawn = (page: Page) => page.evaluate(() => (window as unknown as { drawn: string[] }).drawn);

/** Moves the pointer onto the row header at screen row `row` (0 first), over its label at the band's right. */
async function hoverRowHeader(page: Page, row: number) {
  const b = (await page.locator(".binnacle-layer").boundingBox())!;
  const y = b.y + HEADER + row * ROW + ROW / 2;
  await page.mouse.move(b.x + 300, y);
  await page.mouse.move(b.x + 36, y, { steps: 4 });
}

test.describe("transposed", () => {
  const FIELDS = ["id", "name", "weight", "crew", "port"];
  const status = (page: Page) => page.locator(".binnacle-aria").getByRole("status");

  /** Every field label is drawn, and no text is cut short. */
  async function whole(page: Page) {
    await expect.poll(() => drawn(page)).toEqual(expect.arrayContaining(FIELDS));
    expect((await drawn(page)).filter((t) => t.endsWith("…"))).toEqual([]);
  }

  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 400 });
    await page.goto("/?source=transposed&theme=light");
    await expect(page.locator("#status")).toContainText("rows 1 to 5 of 5");
    // The records sorted by weight, then crew: both rows show a marker.
    const b = (await page.locator(".binnacle-layer").boundingBox())!;
    await page.mouse.click(b.x + 36, b.y + HEADER + 2 * ROW + ROW / 2);
    await page.keyboard.down("Shift");
    await page.mouse.click(b.x + 36, b.y + HEADER + 3 * ROW + ROW / 2);
    await page.keyboard.up("Shift");
    await expect(status(page)).toHaveText("Columns sorted by weight, ascending, then crew, ascending");
  });

  test("a sorted row's label stays whole beside its menu button while hovered", async ({ page }) => {
    for (const row of [2, 3]) {
      await forget(page);
      await hoverRowHeader(page, row);
      await whole(page);
    }
  });

  test("a sorted row filtering the records keeps its label whole beside its mark, as the next row is hovered", async ({ page }) => {
    // The weight row's menu, by its header's button: drop the records with no weight.
    await hoverRowHeader(page, 2);
    const b = (await page.locator(".binnacle-layer").boundingBox())!;
    await page.mouse.click(b.x + 12, b.y + HEADER + 2 * ROW + ROW / 2);
    const values = page.getByRole("dialog", { name: "Filter columns by weight" }).getByRole("group", { name: "weight values" });
    await values.getByRole("checkbox", { name: /^\(Blanks\)/ }).uncheck();
    await page.keyboard.press("Escape");
    await expect(page.locator(".binnacle-filter-bar").getByRole("list", { name: /^Filters on the / })).toBeVisible();
    await forget(page);
    await page.mouse.move(0, 0);
    await hoverRowHeader(page, 3);
    await whole(page);
  });
});

test.describe("records", () => {
  test("a hovered row's number stays whole beside its menu button", async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records&theme=light");
    await expect(page.locator("#status")).toContainText("rows 1 to");
    // Four-digit row numbers, the widest the records have.
    await page.locator(".binnacle-scroller").evaluate((el, top) => {
      el.scrollTop = top;
    }, 1200 * ROW);
    await expect(page.locator("#status")).toContainText("rows 1,201 to");
    await forget(page);
    await hoverRowHeader(page, 2);
    // Once a frame has drawn the rows (the ID column holds the same numbers), none is cut short.
    await expect.poll(() => drawn(page)).toContain("1203");
    // Only the row numbers: a column header may be cut to its width, which the platform's font decides.
    const numbers = Array.from({ length: 10 }, (_, i) => String(1201 + i));
    const cut = (await drawn(page)).filter((t) => t.endsWith("…") && numbers.some((n) => n.startsWith(t.slice(0, -1))));
    expect(cut).toEqual([]);
  });
});
