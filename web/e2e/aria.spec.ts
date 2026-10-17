// The ARIA layer: a visually hidden grid of the visible window that a screen
// reader reads in place of the canvas, the textarea pointing at the focused
// cell, and a polite live region. The fixture is a five-row table whose rows
// are labelled by name.

import { expect, test, type Page } from "@playwright/test";

type Fixture = { hold(): void; release(): void; remove(id: string): void };

const fixture = <T>(page: Page, run: (f: Fixture) => T) =>
  page.evaluate(`(${run.toString()})(window.fixture)`) as Promise<T>;

const grid = (page: Page) => page.getByRole("grid", { name: "People" });

test.beforeEach(async ({ page }) => {
  await page.goto("/e2e/fixtures/aria.html");
  await expect(page.locator("#grid")).toHaveAttribute("data-painted", "true");
});

test("the grid exposes headers, row headers and each cell's text", async ({ page }) => {
  await expect(grid(page)).toMatchAriaSnapshot(`
    - grid "People":
      - rowgroup:
        - row "Name Price Trend Load":
          - /children: equal
          - columnheader
          - columnheader "Name"
          - columnheader "Price"
          - columnheader "Trend"
          - columnheader "Load"
      - rowgroup:
        - row:
          - /children: equal
          - rowheader "Ada"
          - gridcell "Ada"
          - gridcell "12.50"
          - gridcell /^5, rising/
          - gridcell "42%"
        - row:
          - rowheader "Grace"
          - gridcell "Grace"
          - gridcell "3.00"
          - gridcell /^4, falling/
          - gridcell "90%"
        - row:
          - rowheader "Edsger"
        - row:
          - rowheader "Barbara"
        - row:
          - rowheader "Donald"
  `);
  const g = grid(page);
  await expect(g).toHaveAttribute("aria-rowcount", "6");
  await expect(g).toHaveAttribute("aria-colcount", "5");
  await expect(g).toHaveAttribute("aria-multiselectable", "true");
  await expect(g).toHaveAttribute("aria-readonly", "true");
  const grace = g.getByRole("row").nth(2);
  await expect(grace).toHaveAttribute("aria-rowindex", "3");
  await expect(grace.getByRole("rowheader")).toHaveAttribute("aria-colindex", "1");
  await expect(grace.getByRole("gridcell").nth(1)).toHaveAttribute("aria-colindex", "3");
  await expect(grace.getByRole("gridcell").nth(1)).toHaveAttribute("aria-selected", "false");
});

test("the canvas grid is hidden from sight but not from assistive technology", async ({ page }) => {
  // Nothing of the table shows or takes a pointer where its cells would be.
  const box = (await grid(page).getByRole("gridcell").first().boundingBox())!;
  const hit = await page.evaluate(([x, y]) => document.elementFromPoint(x!, y!)?.closest("table") ?? null, [
    box.x + box.width / 2,
    box.y + box.height / 2,
  ]);
  expect(hit).toBeNull();
  await expect(page.locator("canvas")).toHaveAttribute("aria-hidden", "true");
});

/** The text and role of the element the textarea names as its active descendant, `null` if none. */
const active = (page: Page) =>
  page.evaluate(() => {
    const id = document.querySelector("textarea")!.getAttribute("aria-activedescendant");
    const el = id === null ? null : document.getElementById(id);
    return el && { text: el.textContent, role: el.getAttribute("role"), selected: el.getAttribute("aria-selected") };
  });

/** Clicks the body cell at row `row`, column `col` (0 first): a 40 px row-number band or wider, 100 px columns, 24 px rows. */
async function click(page: Page, row: number, col: number) {
  const box = (await page.locator(".binnacle-layer").boundingBox())!;
  await page.mouse.click(box.x + 80 + col * 101 + 30, box.y + 28 + row * 24 + 12);
}

test("the textarea points at the focused cell as focus moves", async ({ page }) => {
  expect(await active(page)).toBeNull();
  await click(page, 1, 1);
  await expect(page.locator("textarea")).toBeFocused();
  // The focused element is named for the grid it stands in for.
  await expect(page.locator("textarea")).toHaveAccessibleName("People");
  await expect.poll(() => active(page)).toEqual({ text: "3.00", role: "gridcell", selected: "false" });
  await page.keyboard.press("ArrowDown");
  await expect.poll(() => active(page)).toEqual({ text: "7.25", role: "gridcell", selected: "false" });
  await page.keyboard.press("ArrowLeft");
  await expect.poll(() => active(page)).toEqual({ text: "Edsger", role: "gridcell", selected: "false" });
});

test("cells in the selected range are selected", async ({ page }) => {
  await click(page, 0, 0);
  await page.keyboard.press("Shift+ArrowDown");
  await page.keyboard.press("Shift+ArrowRight");
  const selected = grid(page).locator('[role="gridcell"][aria-selected="true"]');
  await expect(selected).toHaveText(["Ada", "12.50", "Grace", "3.00"]);
  await page.keyboard.press("ArrowUp");
  await expect(selected).toHaveCount(0);
});

/** Clicks column `col`'s header. */
async function header(page: Page, col: number, shift = false) {
  const box = (await page.locator(".binnacle-layer").boundingBox())!;
  if (shift) await page.keyboard.down("Shift");
  await page.mouse.click(box.x + 80 + col * 101 + 30, box.y + 14);
  if (shift) await page.keyboard.up("Shift");
}

test("a sort shows on its header, reorders the rows and is announced", async ({ page }) => {
  const status = page.getByRole("status");
  const price = grid(page).getByRole("columnheader", { name: "Price" });
  await expect(price).not.toHaveAttribute("aria-sort");
  await header(page, 1);
  await expect(price).toHaveAttribute("aria-sort", "ascending");
  await expect(status).toHaveText("Sorted by Price, ascending");
  await expect(grid(page).getByRole("rowheader")).toHaveText(["Donald", "Grace", "Edsger", "Ada", "Barbara"]);
  await expect(grid(page).getByRole("row").nth(1)).toHaveAttribute("aria-rowindex", "2");
  await header(page, 1);
  await expect(price).toHaveAttribute("aria-sort", "descending");
  await expect(status).toHaveText("Sorted by Price, descending");
  await expect(grid(page).getByRole("rowheader")).toHaveText(["Barbara", "Ada", "Edsger", "Grace", "Donald"]);
  // A second column sorts within the first; only the first carries aria-sort.
  await header(page, 0, true);
  await expect(status).toHaveText("Sorted by Price, descending, then Name, ascending");
  await expect(grid(page).getByRole("columnheader", { name: "Name" })).not.toHaveAttribute("aria-sort");
});

test("cells are busy while their values load", async ({ page }) => {
  await fixture(page, (f) => f.hold());
  // A Reset drops every cached value, so every cell asks again.
  await fixture(page, (f) => f.remove("edsger"));
  const busy = grid(page).locator('[role="gridcell"][aria-busy="true"]');
  await expect(busy).toHaveCount(16);
  await expect(busy.first()).toHaveText("Loading");
  await expect(grid(page).getByRole("rowheader")).toHaveText(["Ada", "Grace", "Barbara", "Donald"]);
  await fixture(page, (f) => f.release());
  await expect(busy).toHaveCount(0);
  await expect(grid(page).getByRole("row").nth(3).getByRole("gridcell")).toHaveText(["Barbara", "40.00", /^2, /, "100%"]);
});

test("a focused row that leaves the table is announced", async ({ page }) => {
  await click(page, 1, 0);
  await expect.poll(() => active(page)).toMatchObject({ text: "Grace" });
  await fixture(page, (f) => f.remove("grace"));
  await expect(page.getByRole("status")).toHaveText("The focused row is no longer in the table");
  // Focus falls back to the row now at its Position.
  await expect.poll(() => active(page)).toMatchObject({ text: "Edsger" });
});

test("a focused row that is found again is not announced", async ({ page }) => {
  await click(page, 1, 0);
  await expect.poll(() => active(page)).toMatchObject({ text: "Grace" });
  await fixture(page, (f) => f.remove("ada"));
  await expect(grid(page).getByRole("rowheader")).toHaveText(["Grace", "Edsger", "Barbara", "Donald"]);
  await expect.poll(() => active(page)).toMatchObject({ text: "Grace" });
  await expect(page.getByRole("status")).toHaveText("");
});

test("an open editor is read as itself, named for its cell, and the grid says it is editable", async ({ page }) => {
  await page.goto("/e2e/fixtures/aria.html?editable");
  await expect(page.locator("#grid")).toHaveAttribute("data-painted", "true");
  await expect(grid(page)).not.toHaveAttribute("aria-readonly");
  const textarea = page.locator("textarea");
  await click(page, 1, 1);
  await expect.poll(() => active(page)).toMatchObject({ text: "3.00" });
  await page.keyboard.press("F2");
  await expect(textarea).toHaveValue("3.00");
  // While editing, the textarea is the editor: pointing away from it would hide the text being edited.
  await expect(textarea).not.toHaveAttribute("aria-activedescendant");
  await expect(textarea).toHaveAccessibleName("Edit Price, Grace");
  await page.keyboard.press("Escape");
  await expect.poll(() => active(page)).toMatchObject({ text: "3.00" });
  await expect(textarea).toHaveAccessibleName("People");
});

test.describe("on the huge table", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/?source=huge");
    await expect(page.locator("#status")).toContainText("rows 1 to");
  });

  const scroller = (page: Page) => page.locator(".binnacle-scroller");
  const rowIndices = (page: Page) =>
    page.locator('table[role="grid"] tbody [role="row"]').evaluateAll((rows) => rows.map((r) => r.getAttribute("aria-rowindex")));

  test("its counts are the whole table's, and a scroll keeps the elements of the rows still in view", async ({
    page,
  }) => {
    const table = page.getByRole("grid");
    await expect(table).toHaveAttribute("aria-rowcount", "1000000000000001");
    await expect.poll(() => rowIndices(page).then((r) => r[0])).toBe("2");
    const third = table.getByRole("row").nth(3).getByRole("gridcell").first();
    const id = await third.getAttribute("id");
    await third.evaluate((el) => el.setAttribute("data-mark", "kept"));
    await scroller(page).evaluate((el) => (el.scrollTop += 48));
    await expect.poll(() => rowIndices(page).then((r) => r[0])).toBe("4");
    // The same element, with the same id, is now the first row's.
    const first = table.getByRole("row").nth(1).getByRole("gridcell").first();
    await expect(first).toHaveAttribute("data-mark", "kept");
    await expect(first).toHaveAttribute("id", id!);
  });

  test("a fast scroll updates the table only once it slows, then shows where it ended", async ({ page }) => {
    const updates = await page.evaluate(async () => {
      const el = document.querySelector<HTMLElement>(".binnacle-scroller")!;
      const body = document.querySelector('table[role="grid"] tbody')!;
      let changed = 0;
      let frames = 0;
      const seen = new MutationObserver(() => changed++);
      seen.observe(body, { childList: true, subtree: true, attributes: true, characterData: true });
      // Fast flinging: a big step every frame for 40 frames.
      for (; frames < 40; frames++) {
        el.scrollTop += 300;
        await new Promise(requestAnimationFrame);
        await new Promise((r) => setTimeout(r));
      }
      seen.takeRecords();
      const result = changed;
      seen.disconnect();
      return result;
    });
    expect(updates).toBeLessThan(10);
    const first = Number((await page.locator("#status").textContent())!.match(/rows ([\d,]+)/)![1]!.replaceAll(",", ""));
    await expect.poll(() => rowIndices(page).then((r) => Number(r[0]))).toBe(first + 1);
  });
});
