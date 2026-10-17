// The focus and scroll API: `focus`, `reveal` and `scrollTo` for IDs inside
// and outside the Window, after a reorder, with a late Locate, and for IDs
// the Table Source no longer has; and the row selection unit. The fixture is
// 10,000 rows (`row-0` on) by 8 columns (`a` to `h`), taller and wider than
// the grid.

import { expect, test, type Page } from "@playwright/test";
import type { FrameInfo, SelectionChange } from "../src";

/** Runs `body` in the page with the fixture as `f`; returns what it returns. */
const run = <T = unknown>(page: Page, body: string) =>
  page.evaluate(`(async (f) => { ${body} })(window.fixture)`) as Promise<T>;

const frame = (page: Page) => run<FrameInfo<string, string>>(page, "return f.frame");

/** Whether Position `p` is among the visible rows (or columns). */
async function shows(page: Page, axis: "rows" | "cols", p: number) {
  const span = (await frame(page))[axis];
  return span.first <= p && p <= span.last;
}

/** How `promise` settles in the page: "resolved", or the rejection's message. */
const outcome = (page: Page, call: string) =>
  run<string>(page, `try { await f.widget.${call}; return "resolved"; } catch (e) { return String(e.message); }`);

async function open(page: Page, query = "") {
  await page.goto(`/e2e/fixtures/focus.html${query}`);
  await expect(page.locator("#grid")).toHaveAttribute("data-painted", "true");
}

test.describe("focus", () => {
  test("focuses a visible cell by its IDs", async ({ page }) => {
    await open(page);
    expect(await outcome(page, `focus("row-2", "c")`)).toBe("resolved");
    expect((await frame(page)).focusIds).toEqual({ row: "row-2", col: "c" });
    expect((await frame(page)).focus).toEqual({ row: 2, col: 2 });
  });

  test("focuses a row outside the Window and scrolls it into view", async ({ page }) => {
    await open(page);
    expect(await outcome(page, `focus("row-5000", "h")`)).toBe("resolved");
    expect((await frame(page)).focusIds).toEqual({ row: "row-5000", col: "h" });
    expect(await shows(page, "rows", 5000)).toBe(true);
    expect(await shows(page, "cols", 7)).toBe(true);
  });

  test("without a column, keeps the focused one", async ({ page }) => {
    await open(page);
    await outcome(page, `focus("row-1", "e")`);
    expect(await outcome(page, `focus("row-800")`)).toBe("resolved");
    expect((await frame(page)).focusIds).toEqual({ row: "row-800", col: "e" });
  });

  test("without a column or a focused cell, takes the first visible column", async ({ page }) => {
    await open(page);
    await run(page, "f.widget.scrollTo(0, 3)");
    expect(await outcome(page, `focus("row-9")`)).toBe("resolved");
    expect((await frame(page)).focusIds).toEqual({ row: "row-9", col: "d" });
  });

  test("finds a row by its ID after a reorder", async ({ page }) => {
    await open(page);
    await run(page, "f.reverse()");
    expect(await outcome(page, `focus("row-10", "a")`)).toBe("resolved");
    expect((await frame(page)).focus?.row).toBe(9989);
    expect(await shows(page, "rows", 9989)).toBe(true);
  });

  test("settles once a late Locate lands", async ({ page }) => {
    await open(page, "?async");
    await run(page, "f.reverse()");
    expect(await outcome(page, `focus("row-7000", "b")`)).toBe("resolved");
    expect((await frame(page)).focus).toEqual({ row: 2999, col: 1 });
    expect(await shows(page, "rows", 2999)).toBe(true);
  });

  test("is rejected for a row the Table Source no longer has", async ({ page }) => {
    await open(page, "?async");
    await run(page, `f.remove("row-6000")`);
    expect(await outcome(page, `focus("row-6000", "a")`)).toMatch(/row-6000.*gone/);
    expect(await outcome(page, `focus("nobody", "a")`)).toMatch(/nobody.*gone/);
  });

  test("an earlier call that has not landed is rejected when a later one supersedes it", async ({ page }) => {
    await open(page, "?async");
    await run(page, "f.reverse()");
    const outcomes = await run<string[]>(
      page,
      `const settle = (p) => p.then(() => "resolved", (e) => String(e.message));
       return Promise.all([settle(f.widget.focus("row-7000", "a")), settle(f.widget.focus("row-4000", "a"))]);`,
    );
    expect(outcomes[0]).toMatch(/row-7000/);
    expect(outcomes[1]).toBe("resolved");
    expect((await frame(page)).focusIds).toEqual({ row: "row-4000", col: "a" });
  });
});

test.describe("reveal and scrollTo", () => {
  test("reveal scrolls a row and column into view without moving focus", async ({ page }) => {
    await open(page);
    await outcome(page, `focus("row-1", "a")`);
    expect(await outcome(page, `reveal("row-3000", "g")`)).toBe("resolved");
    expect(await shows(page, "rows", 3000)).toBe(true);
    expect(await shows(page, "cols", 6)).toBe(true);
    expect((await frame(page)).focusIds).toEqual({ row: "row-1", col: "a" });
  });

  test("reveal finds a row after a reorder, through a late Locate", async ({ page }) => {
    await open(page, "?async");
    await run(page, "f.reverse()");
    expect(await outcome(page, `reveal("row-100")`)).toBe("resolved");
    expect(await shows(page, "rows", 9899)).toBe(true);
  });

  test("reveal is rejected for a row the Table Source no longer has", async ({ page }) => {
    await open(page);
    await run(page, `f.remove("row-20")`);
    expect(await outcome(page, `reveal("row-20")`)).toMatch(/row-20.*gone/);
  });

  test("scrollTo puts a Position at the top, and a column at the left", async ({ page }) => {
    await open(page);
    await run(page, "f.widget.scrollTo(4321, 2)");
    await expect.poll(async () => (await frame(page)).rows.first).toBe(4321);
    expect((await frame(page)).cols.first).toBe(2);
    expect((await frame(page)).focus).toBeNull();
  });

  test("scrollTo refuses what is not a Position", async ({ page }) => {
    await open(page);
    for (const bad of ["-1", "1.5", "NaN"]) {
      expect(await run(page, `try { f.widget.scrollTo(${bad}); return "ok"; } catch (e) { return e.name; }`)).toBe("RangeError");
    }
  });
});

test.describe("the row selection unit", () => {
  test("focus selects its whole row, and onSelectionChange reports it", async ({ page }) => {
    await open(page, "?unit=rows");
    await outcome(page, `focus("row-4", "b")`);
    expect((await frame(page)).selection).toEqual({ rows: { start: 4, end: 5 }, cols: { start: 0, end: 8 } });
    await expect
      .poll(() => run<SelectionChange<string, string>>(page, "return f.change"))
      .toMatchObject({
        focus: { rowId: "row-4", colId: "b" },
        selection: { rows: { count: 1, ids: ["row-4"] }, cols: { count: 8, first: "a" } },
      });
  });

  test("the cell unit selects nothing beyond focus", async ({ page }) => {
    await open(page);
    await outcome(page, `focus("row-4", "b")`);
    expect((await frame(page)).selection).toBeNull();
  });
});
