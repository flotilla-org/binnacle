// The React page: the wrapper mounts one grid (under StrictMode), its status
// line is React state fed by onFrame, and props reach the widget without
// React re-rendering it. A widget element tagged before a change and still
// tagged after it is the same widget, not a recreated one.

import { expect, test, type Page } from "@playwright/test";
import { matches, rows } from "../demo/sources/records-data";
import { dblclick } from "./dblclick";
import { column, FIRST } from "./records-columns";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");
const grids = (page: Page) => page.locator(".binnacle");

async function cell(page: Page, row: number) {
  const box = await page.locator(".binnacle-layer").boundingBox();
  if (!box) throw new Error("no grid");
  await page.mouse.click(box.x + FIRST, box.y + HEADER + row * ROW + ROW / 2);
}

/** The status line's "rows a to b" part. */
const shownRows = async (page: Page) => /rows [\d,]+ to [\d,]+/.exec((await status(page).textContent()) ?? "")?.[0];
const tag = (page: Page) => grids(page).evaluate((el) => (el.dataset.tagged = "yes"));
const tagged = (page: Page) => grids(page).evaluate((el) => el.dataset.tagged === "yes");

/** Waits until the widget has run the frame for the input so far. */
const settled = (page: Page) =>
  page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/react.html");
  await expect(page).toHaveTitle("binnacle React demo");
  await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
});

test("mounts one grid, and the status line follows its frames", async ({ page }) => {
  await expect(grids(page)).toHaveCount(1);
  // The `label` prop names the grid for assistive technology.
  await expect(page.getByRole("grid", { name: "Records" })).toHaveCount(1);
  await cell(page, 0);
  await expect(status(page)).toContainText("focus row 1 (0 / id)");
  await page.keyboard.press("ArrowDown");
  await expect(status(page)).toContainText("focus row 2 (1 / id)");
});

test("Theme and Border Style props restyle the same widget", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await tag(page);
  await page.getByLabel("Theme").selectOption("catppuccin mocha");
  await expect(page.locator("html")).toHaveAttribute("data-scheme", "dark");
  // onFrame is a new closure each render: the widget calls the latest one.
  await expect(status(page)).toContainText("theme catppuccin mocha");
  const before = await shownRows(page);
  // Grid lines give rows Gaps too, so fewer rows fit.
  await page.getByLabel("Border").selectOption("grid heavy");
  await expect(status(page)).toContainText("border grid heavy");
  await expect.poll(() => shownRows(page)).not.toBe(before);
  expect(await tagged(page)).toBe(true);
});

test("the filter notifies the same widget", async ({ page }) => {
  await tag(page);
  await page.getByRole("searchbox", { name: "Filter records" }).fill("bergen");
  const kept = rows.filter((r) => matches(r, "bergen")).length;
  await expect(status(page)).toContainText(`of ${kept.toLocaleString("en")}`);
  expect(await tagged(page)).toBe(true);
});

test("a new creation option makes a new widget", async ({ page }) => {
  await tag(page);
  await page.getByLabel("Compact rows").check();
  await expect.poll(() => tagged(page)).toBe(false);
  await expect(grids(page)).toHaveCount(1);
  await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
  await cell(page, 0);
  await expect(status(page)).toContainText("focus row 1 (0 / id)");
});

test("unmounting destroys the widget, and mounting again works", async ({ page }) => {
  await page.getByLabel("Show the grid").uncheck();
  await expect(grids(page)).toHaveCount(0);
  await page.getByLabel("Show the grid").check();
  await expect(grids(page)).toHaveCount(1);
  await cell(page, 1);
  await expect(status(page)).toContainText("focus row 2 (1 / id)");
});

test("onCommit and editor props reach the widget: an edit commits through the latest closure", async ({ page }) => {
  await tag(page);
  const box = await page.locator(".binnacle-layer").boundingBox();
  // The name column of the first row.
  await page.mouse.click(box!.x + column(1), box!.y + HEADER + ROW / 2);
  await page.keyboard.type("Zed");
  await page.keyboard.press("Enter");
  await expect(page.locator("#committed")).toHaveText('name of 0 = "Zed" (edit 1)');
  await page.keyboard.press("ArrowUp");
  await page.keyboard.type("Ahoy");
  await page.keyboard.press("Enter");
  await expect(page.locator("#committed")).toHaveText('name of 0 = "Ahoy" (edit 2)');
  // The port column picks from a list: the custom editor. The commit dropped
  // the grid's Keys for the edited row until its next frame asks again, and a
  // double-click before then has no cell to activate.
  await settled(page);
  await dblclick(page, box!.x + column(4), box!.y + HEADER + ROW / 2);
  await page.getByRole("combobox", { name: "Port" }).selectOption("Bergen");
  await page.keyboard.press("Enter");
  await expect(page.locator("#committed")).toHaveText('port of 0 = "Bergen" (edit 3)');
  expect(await tagged(page)).toBe(true);
});

test("typing straight after a commit and a move, before the next frame, keeps every character", async ({ page }) => {
  const box = await page.locator(".binnacle-layer").boundingBox();
  await page.mouse.click(box!.x + column(1), box!.y + HEADER + ROW / 2);
  await page.keyboard.type("Zed");
  // The commit drops the Keys in view until a frame fetches them again; the
  // commit, the move and the typing all come in one task, before that frame.
  await page.locator(".binnacle-input").evaluate((textarea: HTMLTextAreaElement) => {
    const key = (key: string) => textarea.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    key("Enter");
    key("ArrowUp");
    textarea.value = "Ahoy";
    textarea.dispatchEvent(new InputEvent("input", { data: "Ahoy", inputType: "insertText", bubbles: true }));
    key("Enter");
  });
  // React has not rendered between the two commits, so the demo counts edit 1 twice.
  await expect(page.locator("#committed")).toHaveText(/^name of 0 = "Ahoy"/);
});
