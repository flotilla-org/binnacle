// Pinning a column end to end: Alt+Shift+ArrowLeft and the column menu pin
// the focused column to the left, where it stays while the body scrolls
// under it, on the records and on their column-major twin. ARIA reads the
// cells in Position order whatever is pinned. Screenshots show the edge in
// each Border Style and Theme, with its shadow once the body has scrolled.

import { expect, test, type Page } from "@playwright/test";

/** The middle of the first body cell, past the row band and the header band. */
async function firstCell(page: Page) {
  const band = await page.locator(".binnacle-band-left").boundingBox();
  const top = await page.locator(".binnacle-band-top").boundingBox();
  if (!band || !top) throw new Error("no grid");
  return { x: band.x + band.width + 10, y: top.y + top.height + 10 };
}

const headers = (page: Page) => page.getByRole("grid").getByRole("columnheader");

/** The ARIA column indices of the column headers, in the order they are read. */
const indices = (page: Page) =>
  headers(page).evaluateAll((els) => els.map((el) => Number(el.getAttribute("aria-colindex"))));

/** Whether the headers are read in Position order. */
async function inPositionOrder(page: Page) {
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

  test("Alt+Shift+ArrowLeft pins the focused column, which stays while the body scrolls, read in Position order", async ({
    page,
  }) => {
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Alt+Shift+ArrowLeft");
    // Drawn first, read where it belongs: after weight, at its own index.
    await expect(headers(page).nth(1)).toHaveText("id");
    await expect(headers(page).filter({ hasText: "crew" })).toHaveAttribute("aria-colindex", "5");
    expect(await inPositionOrder(page)).toBe(true);
    const at = await firstCell(page);
    await page.mouse.move(at.x + 150, at.y + 100);
    await page.mouse.wheel(600, 0);
    // id and name scrolled under the pinned crew column; crew stays.
    await expect(headers(page).filter({ hasText: /^id$/ })).toHaveCount(0);
    await expect(headers(page).filter({ hasText: "crew" })).toHaveCount(1);
    expect(await inPositionOrder(page)).toBe(true);
    await expect(await pinButton(page)).toHaveText("Unpin");
    await page.keyboard.press("Escape");
    await page.keyboard.press("Alt+Shift+ArrowLeft");
    await expect(await pinButton(page)).toHaveText("Pin left");
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

  test("a row's menu filters the columns and offers no pin", async ({ page }) => {
    await page.keyboard.press("Alt+ArrowRight");
    const menu = page.getByRole("dialog", { name: "Filter columns by 1" });
    await expect(menu.getByRole("textbox")).toBeVisible();
    await expect(menu.getByRole("button", { name: /^(Pin left|Unpin)$/ })).toBeHidden();
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
  await expect(headers(page).nth(1)).toHaveAttribute("aria-colindex", "3");
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
