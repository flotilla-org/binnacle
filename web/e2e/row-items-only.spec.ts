// Row Menu Items on rows with nothing else in their menu: a read-only status
// table whose columns take no conditions on a row and whose rows do not hide.
// A row with the host's items opens its menu by Alt+ArrowRight, `openRow` and
// its header's button; "row-1", given none, has no menu.

import { expect, test, type Page } from "@playwright/test";

const HEADER = 28;
const ROW = 24;

/** Runs `body` in the page with the fixture as `f`; returns what it returns. */
const run = <T = unknown>(page: Page, body: string) =>
  page.evaluate(`(async (f) => { ${body} })(window.fixture)`) as Promise<T>;

const dialog = (page: Page) => page.getByRole("dialog");

/** Focuses the first cell of `rowId`, the grid holding the keyboard. */
async function focusRow(page: Page, rowId: string) {
  await run(page, `await f.widget.focus(${JSON.stringify(rowId)}, "state")`);
  await page.locator("#grid textarea").focus();
}

/** Presses the row header's menu button of the row at Position `row`, hovering it first. */
async function pressButton(page: Page, row: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  const y = b.y + HEADER + row * ROW + ROW / 2;
  await page.mouse.move(b.x + 300, y);
  await page.mouse.move(b.x + 12, y, { steps: 4 });
  await page.mouse.click(b.x + 12, y);
}

test.beforeEach(async ({ page }) => {
  await page.goto("/e2e/fixtures/row-items.html");
  await expect(page.locator("#grid")).toHaveAttribute("data-painted", "true");
});

test("Alt+ArrowRight opens a row's menu of the host's items alone, and choosing one reports its row", async ({ page }) => {
  await focusRow(page, "row-3");
  await page.keyboard.press("Alt+ArrowRight");
  const open = dialog(page).getByRole("button", { name: "Open" });
  await expect(open).toBeVisible();
  await expect(dialog(page).getByRole("button", { name: "Hide row" })).toHaveCount(0);
  await open.click();
  await expect(dialog(page)).toHaveCount(0);
  expect(await run(page, "return f.chosen")).toEqual({ name: "open", rowId: "row-3" });
});

test("openRow opens on a row with items and not on one without", async ({ page }) => {
  expect(await run(page, `return f.menu.openRow("row-4")`)).toBe(true);
  await expect(dialog(page).getByRole("button", { name: "Open" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  expect(await run(page, `return f.menu.openRow("row-1")`)).toBe(false);
  await focusRow(page, "row-1");
  await page.keyboard.press("Alt+ArrowRight");
  await expect(dialog(page)).toHaveCount(0);
});

test("a row header's button opens the menu of a row with items, and a row without has none", async ({ page }) => {
  await pressButton(page, 2);
  await expect(dialog(page).getByRole("button", { name: "Open" })).toBeVisible();
  await dialog(page).getByRole("button", { name: "Open" }).click();
  expect(await run(page, "return f.chosen")).toEqual({ name: "open", rowId: "row-2" });
  await pressButton(page, 1);
  await expect(dialog(page)).toHaveCount(0);
});
