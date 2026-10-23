// Header form end to end (#207): with `&form=header` the shipments demo draws
// the region and the port each as a column of the row band, each group one
// cell spanning its rows, labelled at the top of its part in view and faded
// where it goes on out of view; a click on a span's mark opens or closes its
// group; the toolbar's select (shown when the page asks for a form)
// switches between the forms; a pinned row
// breaks the spans over it in two, each part labelled; the corner names
// each level over its column (#208), sorting by it on a click and opening
// its menu on a right-click.

import { expect, test, type Page } from "@playwright/test";
import { REGIONS, shipments } from "../demo/sources/shipments-data";

const HEADER = 28;
const ROW = 24;
/** Where the port column starts: the region column's default width. */
const PORT = 128;

const status = (page: Page) => page.locator("#status");
const en = (n: number) => n.toLocaleString("en");

const regions = () => new Set(shipments.map((s) => s.region)).size;
const ports = () => new Set(shipments.map((s) => `${s.region}/${s.port}`)).size;
const portsOf = (region: string | null) => [...new Set(shipments.filter((s) => s.region === region).map((s) => s.port))].sort();
const shipped = (region: string | null, port: string) => shipments.filter((s) => s.region === region && s.port === port).length;

/** The rows as the demo opens them in header form: each closed port's Subtotal alone, each region's Subtotal, and the Grand Total. */
const closed = () => ports() + regions() + 1;
/** Every group open: each shipment, and each group's Subtotal after it. */
const nested = () => shipments.length + ports() + regions() + 1;

/** Scrolls the grid back to its first row, a wheel turn at a time. */
async function scrollToTop(page: Page) {
  await page.mouse.move(600, 300);
  await expect(async () => {
    await page.mouse.wheel(0, -20 * ROW);
    await expect(status(page)).toContainText("rows 1 to", { timeout: 200 });
  }).toPass();
}

/** Clicks the row band at screen row `row` (0 first), `x` CSS px in. */
async function band(page: Page, row: number, x: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + x, b.y + HEADER + row * ROW + ROW / 2);
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&theme=light&form=header");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(closed())}`);
});

test("the groups draw as level columns, each closed port's Subtotal under its span", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await expect(page.locator(".grid-box")).toHaveScreenshot("header-form.png", { scale: "device" });
});

test("a click on a span's mark opens its port, and another closes it", async ({ page }) => {
  const [east] = [...REGIONS].sort();
  const [first] = portsOf(east!);
  await band(page, 0, PORT + 9);
  await expect(status(page)).toContainText(`of ${en(closed() + shipped(east!, first!))}`);
  // The view keeps the port's Subtotal where it was: back to the top for the mark.
  await scrollToTop(page);
  await band(page, 0, PORT + 9);
  await expect(status(page)).toContainText(`of ${en(closed())}`);
});

test("the toolbar's select switches the rows to outline form and back", async ({ page }) => {
  const form = page.getByRole("combobox", { name: "Group form" });
  await form.selectOption("outline");
  // Outline form adds each region's Group Item, and each closed port's stands for its Subtotal.
  await expect(status(page)).toContainText(`of ${en(closed() + regions())}`);
  await form.selectOption("header");
  await expect(status(page)).toContainText(`of ${en(closed())}`);
});

test("a scrolled span keeps its label in view and fades where it goes on", async ({ page, browserName }) => {
  // Focus into the region column, then open every level.
  await band(page, 0, 40);
  await page.keyboard.press("9");
  await expect(status(page)).toContainText(`of ${en(nested())}`);
  // The view keeps the first port's Subtotal where it was: from the top, 30 rows' worth down.
  await scrollToTop(page);
  await page.mouse.wheel(0, 30 * ROW);
  await expect(status(page)).not.toContainText("rows 1 to");
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await expect(page.locator(".grid-box")).toHaveScreenshot("header-form-scrolled.png", { scale: "device" });
});

test("a pinned shipment breaks its spans in two, each part labelled", async ({ page, browserName }) => {
  await band(page, 0, 40);
  await page.keyboard.press("9");
  await expect(status(page)).toContainText(`of ${en(nested())}`);
  await scrollToTop(page);
  // The fourth shipment, pinned to the top: its port and region go on below.
  await band(page, 3, 40);
  await page.keyboard.press("Tab");
  await expect(status(page)).toContainText("focus row 4, column 1");
  await page.keyboard.press("Alt+Shift+ArrowUp");
  test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
  await expect(page.locator(".grid-box")).toHaveScreenshot("header-form-pinned.png", { scale: "device" });
});

test("the corner names each row level over its column; a click sorts by it, a right-click opens its menu", async ({ page, browserName }) => {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  // The port's name, at the top of the port column: two clicks sort the ports descending.
  const port = { x: b.x + PORT + 16, y: b.y + HEADER / 2 };
  await page.mouse.click(port.x, port.y);
  await page.mouse.click(port.x, port.y);
  // The sort keeps the top row in place: back to the top to see each region's last port first.
  await scrollToTop(page);
  if (browserName === "chromium") {
    await expect(page.locator(".grid-box")).toHaveScreenshot("header-form-corner-sorted.png", { scale: "device" });
  }
  await page.mouse.click(port.x, port.y, { button: "right" });
  await expect(page.getByRole("dialog", { name: "Filter port" })).toBeVisible();
});
