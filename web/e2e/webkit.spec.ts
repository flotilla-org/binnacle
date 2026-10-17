// What only WebKit shows: a native text selection over the grid. Safari tints
// a selected canvas, so a right-click (which selects) painted the whole grid
// blue, and since the widget prevents the default of a press to keep DOM
// focus on its textarea, a click in the grid never collapsed it again. The
// `webkit` project runs only this file.

import { expect, test, type Page } from "@playwright/test";
import { rows } from "../demo/sources/records-data";
import { column } from "./records-columns";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");
const editor = (page: Page) => page.locator(".binnacle-input");

async function at(page: Page, row: number, col: number) {
  const box = await page.locator(".binnacle-layer").boundingBox();
  if (!box) throw new Error("no grid");
  return { x: box.x + column(col), y: box.y + HEADER + row * ROW + ROW / 2 };
}

/** The page's selection: whether it is collapsed, and its text. */
const selection = (page: Page) =>
  page.evaluate(() => {
    const s = getSelection();
    return { collapsed: !s || s.isCollapsed, text: s?.toString() ?? "" };
  });

test.describe("selection in WebKit", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records");
    await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
  });

  test("a right-click leaves no text selection over the grid", async ({ page }) => {
    const { x, y } = await at(page, 2, 1);
    await page.mouse.click(x, y, { button: "right" });
    expect(await selection(page)).toMatchObject({ collapsed: true, text: "" });
    await expect(page.locator(".binnacle")).toHaveScreenshot("right-click.png");
  });

  test("a click in the grid clears a selection made elsewhere on the page", async ({ page }) => {
    // With the textarea focused already, the click moves no DOM focus, which would collapse it anyway.
    const first = await at(page, 0, 1);
    await page.mouse.click(first.x, first.y);
    await page.evaluate(() => {
      const range = document.createRange();
      range.selectNodeContents(document.querySelector("h1")!);
      getSelection()!.removeAllRanges();
      getSelection()!.addRange(range);
    });
    expect((await selection(page)).text).toBe("binnacle");
    const { x, y } = await at(page, 1, 1);
    await page.mouse.click(x, y);
    expect((await selection(page)).collapsed).toBe(true);
    // The textarea keeps its caret: typing still starts an edit.
    await page.keyboard.type("Zed");
    await expect(editor(page)).toHaveValue("Zed");
  });

  test("clicking cells with the textarea focused keeps its caret", async ({ page }) => {
    for (const row of [0, 1]) {
      const { x, y } = await at(page, row, 1);
      await page.mouse.click(x, y);
    }
    await page.keyboard.type("Zed");
    await expect(editor(page)).toHaveValue("Zed");
  });

  test("text in the edit textarea can still be selected", async ({ page }) => {
    const { x, y } = await at(page, 0, 1);
    await page.mouse.click(x, y);
    await page.keyboard.type("hello world");
    await expect(editor(page)).toHaveValue("hello world");
    await page.keyboard.press("Shift+Home");
    const range = await editor(page).evaluate((el: HTMLTextAreaElement) => [el.selectionStart, el.selectionEnd]);
    expect(range).toEqual([0, 11]);
    // A double-click in the open editor selects a word there, the editor's own.
    const box = await editor(page).boundingBox();
    if (!box) throw new Error("no editor");
    await page.mouse.dblclick(box.x + 6, box.y + box.height / 2);
    const word = await editor(page).evaluate((el: HTMLTextAreaElement) =>
      el.value.slice(el.selectionStart, el.selectionEnd),
    );
    expect(word).toBe("hello");
  });
});
