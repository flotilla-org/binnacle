// Grouping end to end (#200, #201, #202): the shipments demo opens grouped
// by region then port, the regions open and the ports closed, each Group
// Item drawn in outline form in the row header in every preset, each open
// group ending in its Subtotal and the Grand Total pinned at the bottom; a
// click on a disclosure mark and the tree keys of the outline column open
// and close groups; the column menu ungroups the rows and groups them by
// another column, focus staying on its shipment; and a saved layout keeps
// the grouping.

import { expect, test, type Page } from "@playwright/test";
import { REGIONS, shipments } from "../demo/sources/shipments-data";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");
const en = (n: number) => n.toLocaleString("en");

/** The groups a column makes of the shipments. */
const groupsBy = (field: "region" | "crates") => new Set(shipments.map((s) => s[field])).size;

/** The rows grouped by region then port: each group's Group Item and Subtotal, and the Grand Total. */
const nested = () => shipments.length + 2 * (groupsBy("region") + new Set(shipments.map((s) => `${s.region}/${s.port}`)).size) + 1;

/** The regions in the order they show, the blank one last, and the ports of `region` in theirs. */
const regions = () => [...REGIONS].sort();
const portsOf = (region: string | null) => [...new Set(shipments.filter((s) => s.region === region).map((s) => s.port))].sort();
const shipped = (region: string | null, port: string) => shipments.filter((s) => s.region === region && s.port === port).length;

/** The rows as the demo opens them: each region's Group Item, its closed ports and its Subtotal, and the Grand Total. */
const closed = () => 2 * groupsBy("region") + new Set(shipments.map((s) => `${s.region}/${s.port}`)).size + 1;

/** Clicks the row header band at screen row `row` (0 first), `x` CSS px in. */
async function band(page: Page, row: number, x: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + x, b.y + HEADER + row * ROW + ROW / 2);
}

/** Opens every group: focus into the outline column, then 9 levels deep. */
async function openAll(page: Page) {
  await band(page, 0, 80);
  await page.keyboard.press("9");
  await expect(status(page)).toContainText(`of ${en(nested())}`);
}

/** Clicks the id cell at screen row `row` (0 first), then steps right `right` columns. */
async function click(page: Page, row: number, right: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + 230, b.y + HEADER + row * ROW + ROW / 2);
  await expect(status(page)).toContainText("/ id)");
  for (let i = 0; i < right; i++) await page.keyboard.press("ArrowRight");
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(closed())}`);
});

test("the rows open grouped by region then port, both columns gone from the body", async ({ page }) => {
  expect(groupsBy("region")).toBe(REGIONS.length + 1);
  await expect(status(page)).toContainText("columns 1 to 4 of 4");
});

for (const theme of ["light", "catppuccin mocha", "mono"]) {
  test(`Group Items draw in outline form, the Grand Total pinned below, in ${theme}`, async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "screenshots are taken in Chromium only");
    await page.goto(`/?source=shipments&theme=${encodeURIComponent(theme)}`);
    await expect(status(page)).toContainText(`of ${en(closed())}`);
    await expect(page.locator(".grid-box")).toHaveScreenshot(`grouped-${theme.replace(" ", "-")}.png`, { scale: "device" });
  });
}

test("a click on a port's disclosure mark opens it, its figures moving to its Subtotal, and another closes it", async ({ page }) => {
  const [east] = regions();
  const [first] = portsOf(east!);
  // The first port's mark, a level in.
  await band(page, 1, 24);
  await expect(status(page)).toContainText(`of ${en(closed() + shipped(east!, first!) + 1)}`);
  await band(page, 1, 24);
  await expect(status(page)).toContainText(`of ${en(closed())}`);
  // A click on the label beside the mark opens nothing.
  await band(page, 1, 80);
  await expect(status(page)).toContainText(`of ${en(closed())}`);
});

test("the outline column's tree keys open and close groups, and Tab leaves it", async ({ page }) => {
  const [east] = regions();
  const ports = portsOf(east!);
  await click(page, 0, 0);
  await expect(status(page)).toContainText("focus row 1, column 1");
  // Left from the first column enters the outline column; Left again closes East.
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await expect(status(page)).toContainText(`of ${en(closed() - ports.length - 1)}`);
  await page.keyboard.press("Enter");
  await expect(status(page)).toContainText(`of ${en(closed())}`);
  // Right on an open group steps to its first child; on a closed one opens it.
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText("focus row 2, column 1");
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText(`of ${en(closed() + shipped(east!, ports[0]!) + 1)}`);
  // Left on the open port closes it, Left again steps out to East.
  await page.keyboard.press("ArrowLeft");
  await expect(status(page)).toContainText(`of ${en(closed())}`);
  await page.keyboard.press("ArrowLeft");
  await expect(status(page)).toContainText("focus row 1, column 1");
  // * opens everything under East; 0 shows the regions alone; 2 opens all.
  await page.keyboard.press("*");
  const eastShipments = shipments.filter((s) => s.region === east).length;
  await expect(status(page)).toContainText(`of ${en(closed() + eastShipments + ports.length)}`);
  await page.keyboard.press("0");
  await expect(status(page)).toContainText(`of ${en(groupsBy("region") + 1)}`);
  await page.keyboard.press("2");
  await expect(status(page)).toContainText(`of ${en(nested())}`);
  // Tab leaves for the body, where Right moves along the columns again.
  await page.keyboard.press("Tab");
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText("focus row 1, column 2");
});

test("the column menu ungroups the rows and groups them by its column, focus staying on its shipment", async ({ page }) => {
  await openAll(page);
  await click(page, 2, 1);
  const focus = /focus row \d+, column 2 \((\d+) \/ shipped\)/;
  await expect(status(page)).toContainText(focus);
  const id = (await status(page).textContent())!.match(focus)![1];
  await page.keyboard.press("Alt+ArrowDown");
  const menu = page.getByRole("dialog", { name: "Filter shipped" });
  await expect(menu.getByRole("button", { name: "Group rows by this column" })).toBeHidden();
  await menu.getByRole("button", { name: "Ungroup rows" }).click();
  await expect(menu).toBeHidden();
  await expect(status(page)).toContainText(`of ${en(shipments.length)}  ·  columns 1 to 6 of 6`);
  await expect(status(page)).toContainText(new RegExp(`\\(${id} / shipped\\)`));

  // The crates column, two along.
  for (let i = 0; i < 2; i++) await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText(`(${id} / crates)`);
  await page.keyboard.press("Alt+ArrowDown");
  const crates = page.getByRole("dialog", { name: "Filter crates" });
  await expect(crates.getByRole("button", { name: "Ungroup rows" })).toBeHidden();
  await crates.getByRole("button", { name: "Group rows by this column" }).click();
  await expect(status(page)).toContainText(`of ${en(shipments.length + groupsBy("crates"))}  ·  columns 1 to 5 of 5`);
});

test("a saved layout keeps the grouping", async ({ page }) => {
  await openAll(page);
  await click(page, 2, 1);
  await page.keyboard.press("Alt+ArrowDown");
  await page.getByRole("dialog", { name: "Filter shipped" }).getByRole("button", { name: "Ungroup rows" }).click();
  await expect(status(page)).toContainText(`of ${en(shipments.length)}  ·`);
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(shipments.length)}  ·  columns 1 to 6 of 6`);

  await click(page, 2, 5);
  await page.keyboard.press("Alt+ArrowDown");
  await page.getByRole("dialog", { name: "Filter crates" }).getByRole("button", { name: "Group rows by this column" }).click();
  await expect(status(page)).toContainText(`of ${en(shipments.length + groupsBy("crates"))}  ·`);
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(shipments.length + groupsBy("crates"))}  ·  columns 1 to 5 of 5`);
});
