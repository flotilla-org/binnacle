// Pinning end to end: Alt+Shift with an arrow and the menus pin the focused
// column left or right and its row top or bottom, where they stay while the
// body scrolls under them, on the records and on their column-major twin;
// arrow keys follow drawn order; Freeze up to here pins from the start. ARIA
// reads and numbers the cells in drawn order, pins included. Screenshots show the
// edge in each Border Style and Theme, with its shadow once the body has
// scrolled, and all four edges at once.

import { expect, test, type Page } from "@playwright/test";

/** The middle of the first body cell, past the row band and the header band. */
async function firstCell(page: Page) {
  const band = await page.locator(".binnacle-band-left").boundingBox();
  const top = await page.locator(".binnacle-band-top").boundingBox();
  if (!band || !top) throw new Error("no grid");
  return { x: band.x + band.width + 10, y: top.y + top.height + 10 };
}

const status = (page: Page) => page.locator("#status");
/** The key the page's keymap takes as Mod: it reads the platform as the browser reports it. */
const mod = (page: Page) =>
  page.evaluate(() => {
    const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
    return /mac/i.test(nav.userAgentData?.platform ?? nav.platform) ? "Meta" : "Control";
  });

const headers = (page: Page) => page.getByRole("grid").getByRole("columnheader");

/** The ARIA column indices of the column headers, in the order they are read. */
const indices = (page: Page) =>
  headers(page).evaluateAll((els) => els.map((el) => Number(el.getAttribute("aria-colindex"))));

/** Whether the headers are numbered in the order they are read. */
async function inReadOrder(page: Page) {
  const read = await indices(page);
  return read.every((index, i) => i === 0 || index > read[i - 1]!);
}

/** Opens the focused column's menu and reads its pin button. */
async function pinButton(page: Page) {
  await page.keyboard.press("Alt+ArrowDown");
  return page.getByRole("dialog").getByRole("button", { name: /^(Pin left|Unpin)$/ });
}

test.describe("records", () => {
  test.beforeEach(async ({ page }) => {
    // Narrow enough that the columns overflow and scroll.
    await page.setViewportSize({ width: 360, height: 500 });
    await page.goto("/?source=records&theme=light");
    await expect(headers(page).filter({ hasText: "crew" })).toHaveCount(1);
    const at = await firstCell(page);
    await page.mouse.click(at.x, at.y);
  });

  test("Alt+Shift+ArrowLeft pins the focused column, which stays while the body scrolls, read where it is drawn", async ({
    page,
  }) => {
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Alt+Shift+ArrowLeft");
    // Drawn first, read first and numbered so; id follows it.
    await expect(headers(page).nth(1)).toHaveText("crew");
    await expect(headers(page).nth(1)).toHaveAttribute("aria-colindex", "2");
    await expect(headers(page).nth(2)).toHaveText("id");
    await expect(headers(page).nth(2)).toHaveAttribute("aria-colindex", "3");
    expect(await inReadOrder(page)).toBe(true);
    const at = await firstCell(page);
    await page.mouse.move(at.x + 150, at.y + 100);
    await page.mouse.wheel(600, 0);
    // id and name scrolled under the pinned crew column; crew stays.
    await expect(headers(page).filter({ hasText: /^id$/ })).toHaveCount(0);
    await expect(headers(page).filter({ hasText: "crew" })).toHaveCount(1);
    expect(await inReadOrder(page)).toBe(true);
    await expect(await pinButton(page)).toHaveText("Unpin");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Alt+Shift+ArrowLeft");
    await expect(await pinButton(page)).toHaveText("Pin left");
  });

  /** Pins the focused column left and waits for it, as the pin lands after the next frame. */
  async function pinFocused(page: Page) {
    await page.keyboard.press("Alt+Shift+ArrowLeft");
    await expect(await pinButton(page)).toHaveText("Unpin");
    await page.keyboard.press("Escape");
  }

  test("pinned columns too wide clip at the inside end and stay pinned; focusing a clipped one shows it", async ({ page }) => {
    for (const name of ["id", "name", "weight", "crew"]) {
      await expect(status(page)).toContainText(`/ ${name})`);
      await pinFocused(page);
      await page.keyboard.press("ArrowRight");
    }
    // Four pinned columns of 120 px leave 50 px of the 360 to scroll: back
    // on id, crew is clipped whole and weight in part.
    await expect(status(page)).toContainText("/ port)");
    for (let i = 0; i < 4; i++) await page.keyboard.press("ArrowLeft");
    await expect(status(page)).toContainText("/ id)");
    await expect(headers(page).filter({ hasText: "crew" })).toHaveCount(0);
    await expect(headers(page).filter({ hasText: "weight" })).toHaveCount(1);
    // Focus on crew slides the pinned Strip to show it; it is still pinned.
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
    await expect(status(page)).toContainText("/ crew)");
    await expect(headers(page).filter({ hasText: "crew" })).toHaveCount(1);
    await expect(headers(page).filter({ hasText: /^id$/ })).toHaveCount(0);
    await expect(await pinButton(page)).toHaveText("Unpin");
  });

  test("a hidden pinned column comes back to its place among the pins when shown", async ({ page }) => {
    await pinFocused(page);
    await page.keyboard.press("ArrowRight");
    await expect(status(page)).toContainText("/ name)");
    await pinFocused(page);
    await page.keyboard.press("ArrowLeft");
    await expect(status(page)).toContainText("/ id)");
    await page.keyboard.press(`${await mod(page)}+Alt+0`);
    await expect(status(page)).toContainText("/ name)");
    await expect(headers(page).filter({ hasText: /^id$/ })).toHaveCount(0);
    await page.keyboard.press("Alt+ArrowDown");
    await page.getByRole("dialog", { name: "Filter name" }).getByRole("button", { name: "Show hidden columns (1) ▸" }).click();
    await page.getByRole("menu", { name: "Hidden columns" }).getByRole("menuitem", { name: "id" }).click();
    await page.getByRole("dialog", { name: "Filter name" }).getByRole("button", { name: "Close" }).click();
    await expect(page.getByRole("dialog")).toBeHidden();
    // Drawn order is pin order again: id first, then name, then the
    // scrolling ones.
    await expect(headers(page).filter({ hasText: /^id$/ })).toHaveCount(1);
    const at = await firstCell(page);
    await page.mouse.click(at.x, at.y);
    await expect(status(page)).toContainText("/ id)");
    // Showing id focused it, so the click opened the editor.
    await page.keyboard.press("Escape");
    await page.keyboard.press("ArrowRight");
    await expect(status(page)).toContainText("/ name)");
    await page.keyboard.press("ArrowRight");
    await expect(status(page)).toContainText("/ weight)");
  });

  test("the column menu pins and unpins any column, the id column (which takes no conditions) included", async ({ page }) => {
    const button = await pinButton(page);
    await expect(page.getByRole("dialog", { name: "Column id" })).toBeVisible();
    await expect(button).toHaveText("Pin left");
    await button.click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
    const unpin = await pinButton(page);
    await expect(unpin).toHaveText("Unpin");
    await unpin.click();
    await expect(await pinButton(page)).toHaveText("Pin left");
  });

  test("a row's menu filters the columns and pins the row top or bottom", async ({ page }) => {
    await page.keyboard.press("Alt+ArrowRight");
    const menu = page.getByRole("dialog", { name: "Filter columns by 1" });
    await expect(menu.getByRole("textbox")).toBeVisible();
    await expect(menu.getByRole("button", { name: /^(Pin left|Pin right)$/ })).toHaveCount(0);
    await menu.getByRole("button", { name: "Pin bottom" }).click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
    // Drawn last, flush with the bottom: down stays on it, up goes to the
    // last scrolling row, and the first drawn row is now row 2.
    await expect(status(page)).toContainText("focus row 1,");
    await expect(status(page)).toContainText("rows 2 to");
    await page.keyboard.press("ArrowDown");
    await expect(status(page)).toContainText("focus row 1,");
    await page.keyboard.press("ArrowUp");
    await expect(status(page)).toContainText("focus row 5,000,");
    await page.keyboard.press(`${await mod(page)}+ArrowUp`);
    await expect(status(page)).toContainText("focus row 2,");
    await page.keyboard.press("Alt+ArrowRight");
    await page.getByRole("dialog").getByRole("button", { name: "Pin top" }).click();
    await page.keyboard.press("Alt+ArrowRight");
    const unpin = page.getByRole("dialog").getByRole("button", { name: "Unpin" });
    await expect(page.getByRole("dialog").getByRole("button", { name: "Pin bottom" })).toBeHidden();
    await unpin.click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
  });

  test("Alt+Shift+ArrowRight pins the focused column right, and arrow keys reach it last", async ({ page }) => {
    await page.keyboard.press("Alt+Shift+ArrowRight");
    // The pin lands after the next frame: wait for it before stepping.
    await expect(await pinButton(page)).toHaveText("Unpin");
    await page.keyboard.press("Escape");
    expect(await inReadOrder(page)).toBe(true);
    // Right from it stays; left is the last scrolling column; from the start, the first scrolling one.
    await page.keyboard.press("ArrowRight");
    await expect(status(page)).toContainText("column 1 ");
    await page.keyboard.press("ArrowLeft");
    await expect(status(page)).toContainText("column 5 ");
    await page.keyboard.press(`${await mod(page)}+ArrowLeft`);
    await expect(status(page)).toContainText("column 2 ");
    await page.keyboard.press(`${await mod(page)}+ArrowRight`);
    await expect(status(page)).toContainText("column 1 ");
    // It stays as the body scrolls, while the column after it scrolls away.
    const at = await firstCell(page);
    await page.mouse.move(at.x + 100, at.y + 100);
    await page.mouse.wheel(600, 0);
    await expect(headers(page).filter({ hasText: /^name$/ })).toHaveCount(0);
    await expect(headers(page).filter({ hasText: /^id$/ })).toHaveCount(1);
  });

  test("Freeze up to here pins every column from the start to the one chosen", async ({ page }) => {
    // Wide enough that id and name fit pinned beside a scrolling column; at
    // 360 the freeze stops short at id.
    await page.setViewportSize({ width: 440, height: 500 });
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Alt+ArrowDown");
    await page.getByRole("dialog").getByRole("button", { name: "Freeze up to here" }).click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
    const at = await firstCell(page);
    await page.mouse.move(at.x + 150, at.y + 100);
    await page.mouse.wheel(600, 0);
    // The scroll lands in a later frame: wait for weight, first after the pins, to scroll away.
    await expect(headers(page).filter({ hasText: /^weight$/ })).toHaveCount(0);
    await expect(headers(page).nth(1)).toHaveText("id");
    await expect(headers(page).nth(2)).toHaveText("name");
    expect(await inReadOrder(page)).toBe(true);
  });
});

test("on the column-major records, a record's column pins and stays while the others scroll", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 420 });
  await page.goto("/?source=transposed&theme=light");
  await expect(headers(page).filter({ hasText: /^3$/ })).toHaveCount(1);
  const at = await firstCell(page);
  await page.mouse.click(at.x, at.y);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Alt+Shift+ArrowLeft");
  await page.mouse.move(at.x + 200, at.y + 60);
  await page.mouse.wheel(2000, 0);
  await expect(headers(page).filter({ hasText: /^3$/ })).toHaveCount(0);
  await expect(headers(page).first()).toHaveText("");
  await expect(headers(page).nth(1)).toHaveText("1");
  await expect(headers(page).nth(1)).toHaveAttribute("aria-colindex", "2");
});

/** The ARIA row headers' text and their rows' indices, in the order they are read. */
const rowHeaders = (page: Page) =>
  page
    .getByRole("grid")
    .getByRole("rowheader")
    .evaluateAll((els) => els.map((el) => `${el.textContent} ${el.closest("tr")!.getAttribute("aria-rowindex")}`));

/** The ARIA column headers' text and indices, in the order they are read. */
const columnHeaders = (page: Page) =>
  headers(page).evaluateAll((els) => els.map((el) => `${el.textContent} ${el.getAttribute("aria-colindex")}`));

test("on the records, a row pinned top is read first and numbered where it is drawn", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 300 });
  await page.goto("/?source=records&theme=light");
  await expect(headers(page).filter({ hasText: "crew" })).toHaveCount(1);
  const at = await firstCell(page);
  await page.mouse.click(at.x, at.y);
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Alt+Shift+ArrowUp");
  await expect.poll(async () => (await rowHeaders(page)).slice(0, 4)).toEqual(["3 2", "1 3", "2 4", "4 5"]);
});

test("on the column-major records, a record pinned left or right is read where it is drawn", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 300 });
  await page.goto("/?source=transposed&theme=light");
  await expect(headers(page).filter({ hasText: /^3$/ })).toHaveCount(1);
  const at = await firstCell(page);
  await page.mouse.click(at.x, at.y);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Alt+Shift+ArrowLeft");
  await expect.poll(async () => (await columnHeaders(page)).slice(0, 4)).toEqual([" 1", "2 2", "0 3", "1 4"]);
  expect(await inReadOrder(page)).toBe(true);
  await page.keyboard.press("Alt+Shift+ArrowRight");
  const count = Number(await page.getByRole("grid").getAttribute("aria-colcount"));
  await expect.poll(async () => (await columnHeaders(page)).at(-1)).toBe(`2 ${count}`);
  expect(await inReadOrder(page)).toBe(true);
});

test("on the column-major records, a field's row pins to the bottom and a record's column to the right", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 300 });
  await page.goto("/?source=transposed&theme=light");
  await expect(headers(page).filter({ hasText: /^3$/ })).toHaveCount(1);
  const at = await firstCell(page);
  await page.mouse.click(at.x, at.y);
  await page.keyboard.press("Alt+Shift+ArrowDown");
  await page.keyboard.press("Alt+Shift+ArrowRight");
  await page.mouse.move(at.x + 200, at.y + 60);
  await page.mouse.wheel(2000, 2000);
  // Record 0's column, pinned right, stays while record 1's scrolls away.
  await expect(headers(page).filter({ hasText: /^1$/ })).toHaveCount(0);
  await expect(headers(page).filter({ hasText: /^0$/ })).toHaveCount(1);
  // The id field's row, pinned to the bottom, is still on screen.
  await expect(page.getByRole("grid").getByRole("rowheader", { name: "id", exact: true })).toHaveCount(1);
});

test.describe("screenshots", () => {
  test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

  async function shot(page: Page, theme: string, border: string, scrolled: boolean, name: string) {
    const query = new URLSearchParams({ theme, border, pin: "3" });
    await page.goto(`/e2e/fixtures/presentation.html?${query}`);
    const grid = page.locator("#grid");
    await expect(grid).toHaveAttribute("data-painted", "true");
    if (scrolled) {
      const box = (await grid.boundingBox())!;
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      await page.mouse.wheel(60, 0);
    }
    await expect(grid).toHaveScreenshot(`${name}.png`, { scale: "device" });
  }

  test("the pinned edge before any scroll casts no shadow", async ({ page }) => {
    await shot(page, "light", "columns light", false, "pinned-unscrolled");
  });

  test("columns 0 to 2 pinned left and 3 right, too wide: the right one clips whole, then the left ones at their inside end, each with its edge", async ({
    page,
  }) => {
    const query = new URLSearchParams({ theme: "light", border: "grid light", pin: "0,1,2", right: "3" });
    await page.goto(`/e2e/fixtures/presentation.html?${query}`);
    const grid = page.locator("#grid");
    await expect(grid).toHaveAttribute("data-painted", "true");
    await expect(grid).toHaveScreenshot("pinned-clipped.png", { scale: "device" });
  });

  test("all four edges pinned, scrolled so each edge's shadow falls on the body", async ({ page }) => {
    const query = new URLSearchParams({ theme: "light", border: "grid light", pin: "0", right: "3", top: "0", bottom: "11", rows: "12" });
    await page.goto(`/e2e/fixtures/presentation.html?${query}`);
    const grid = page.locator("#grid");
    await expect(grid).toHaveAttribute("data-painted", "true");
    const box = (await grid.boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(4, 30);
    await expect(grid).toHaveScreenshot("pinned-four-edges.png", { scale: "device" });
  });

  for (const theme of ["light", "catppuccin mocha", "mono"]) {
    test(`the pinned edge in theme ${theme}`, async ({ page }) => {
      await shot(page, theme, "columns light", true, `pinned-theme-${theme.replaceAll(" ", "-")}`);
    });
  }

  for (const border of ["none light", "headers light", "headers light + band rules", "columns double", "grid light", "grid heavy"]) {
    test(`the pinned edge in border ${border}`, async ({ page }) => {
      await shot(page, "light", border, true, `pinned-border-${border.replaceAll(" + ", " ").replaceAll(" ", "-")}`);
    });
  }
});
