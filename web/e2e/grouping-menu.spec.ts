// The grouping menu end to end (#212): a right-click on a level's name in the
// corner, or a pivot column's menu, removes or moves the level, collapses to
// it or expands through it, removes or adds a value, or groups by a column no
// level uses. Moving the port level to the columns turns the grouped
// shipments into a pivot of ports across, and moving it back the grouped
// table again. The column chooser lists the columns the rows group by as
// "grouped".

import { expect, test, type Page } from "@playwright/test";
import { quarterShipments } from "../demo/sources/pivot";
import { shipments } from "../demo/sources/shipments-data";

const HEADER = 28;

const status = (page: Page) => page.locator("#status");
/** A column header by its accessible name. */
const header = (page: Page, name: string) =>
  page.locator(".binnacle-aria").getByRole("columnheader", { name, exact: true });

const regions = new Set(shipments.map((s) => s.region)).size;
const ports = new Set(shipments.map((s) => s.port)).size;
const quarters = new Set(quarterShipments.map((s) => s.quarter)).size;

/** A right-click at (x, y) of the grid's layer. */
async function rightClick(page: Page, x: number, y: number) {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + x, b.y + y, { button: "right" });
}

test.describe("the grouped shipments", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1200, height: 600 });
    await page.goto("/?source=shipments&theme=light");
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await expect(status(page)).toContainText("columns 1 to 4 of 4");
  });

  test("the port level moved to the columns makes a pivot of ports across, and moved back the grouped table", async ({ page, browserName }) => {
    // "region › port" in the corner: the port's name past the region's.
    await rightClick(page, 68, HEADER / 2);
    const menu = page.getByRole("dialog", { name: "Row level port" });
    await expect(menu.getByRole("button")).toHaveText([
      "Remove level",
      "Move to columns",
      "Collapse to port",
      "Expand through port",
      "Remove value sum(weight)",
      "Remove value count",
      "Add a value ▸",
      "Group by another column ▸",
      "Close",
    ]);
    if (browserName === "chromium") await expect(page).toHaveScreenshot("level-menu.png", { scale: "device" });
    await menu.getByRole("button", { name: "Move to columns" }).click();
    await expect(menu).toBeHidden();
    // A row for each region and the Grand Total; for each port its two values and a Subtotal of each.
    await expect(status(page)).toContainText(`of ${regions + 1}  ·  columns 1 to`);
    await expect(status(page)).toContainText(`of ${ports * 4}`);

    // The port's name now sits at the right of the corner's top row.
    await rightClick(page, 165, HEADER / 2);
    await page.getByRole("dialog", { name: "Column level port" }).getByRole("button", { name: "Move to rows" }).click();
    await expect(status(page)).toContainText("of 56  ·  columns 1 to 4 of 4");
  });

  test("the chooser lists the columns the rows group by as grouped, with no tick", async ({ page, browserName }) => {
    await page.locator(".binnacle-layer").click({ position: { x: 300, y: HEADER + 12 } });
    const mod = await page.evaluate(() => {
      const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
      return /mac/i.test(nav.userAgentData?.platform ?? nav.platform) ? "Meta" : "Control";
    });
    await page.keyboard.press(`${mod}+Alt+O`);
    const chooser = page.getByRole("dialog", { name: "Choose columns" });
    await expect(chooser.locator(".binnacle-chooser-item")).toHaveText(["(Show all)", "id", "shipped", "weight", "crates", "regiongrouped", "portgrouped"]);
    await expect(chooser.locator(".binnacle-chooser-grouped input")).toHaveCount(2);
    for (const box of await chooser.locator(".binnacle-chooser-grouped input").all()) await expect(box).toBeDisabled();
    if (browserName === "chromium") await expect(page).toHaveScreenshot("chooser-grouped.png", { scale: "device" });
  });

  test("a column's menu adds it as a column level, which makes a pivot", async ({ page }) => {
    await page.locator(".binnacle-layer").click({ position: { x: 300, y: HEADER + 12 } });
    // The shipped column, the second in the body.
    await page.keyboard.press("Alt+ArrowDown");
    const menu = page.getByRole("dialog", { name: /^Filter / });
    await menu.getByRole("button", { name: "Add a value ▸" }).click();
    await expect(menu.getByRole("group", { name: "Add a value" }).getByRole("button").first()).toBeFocused();
    await menu.getByRole("button", { name: "Add as a column level" }).click();
    await expect(status(page)).not.toContainText("columns 1 to 4 of 4");
    await rightClick(page, 165, HEADER / 2);
    await expect(page.getByRole("dialog", { name: /^Column level / })).toBeVisible();
  });
});

test.describe("the pivot", () => {
  test.beforeEach(async ({ page }) => {
    // Tall enough for a column menu with its grouping lists open.
    await page.setViewportSize({ width: 1200, height: 1000 });
    await page.goto("/?source=pivot&theme=light");
    await page.evaluate(() => localStorage.clear());
    await page.reload();
    await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
  });

  /** Opens the menu of the first pivot column, from a cell of it. */
  async function columnMenu(page: Page) {
    // The row band is 180 wide and the columns 112, under a band of two rows.
    await page.locator(".binnacle-layer").click({ position: { x: 180 + 56, y: 2 * HEADER + 12 } });
    await page.keyboard.press("Alt+ArrowDown");
    const menu = page.getByRole("dialog", { name: /^Filter/ });
    await expect(menu).toBeVisible();
    return menu;
  }

  test("a pivot column's menu collapses to its quarter level and expands through it again", async ({ page }) => {
    const menu = await columnMenu(page);
    await expect(menu.getByRole("button", { name: "Remove level" })).toBeVisible();
    await menu.getByRole("button", { name: "Collapse to quarter" }).click();
    // Each quarter closed to its total of each value, and the All columns.
    await expect(header(page, "Q1 count")).toBeVisible();
    await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
    await (await columnMenu(page)).getByRole("button", { name: "Expand through quarter" }).click();
    await expect(header(page, "Q1 count")).toHaveCount(0);
    await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
  });

  test("a pivot column's menu removes a value and adds one", async ({ page }) => {
    await (await columnMenu(page)).getByRole("button", { name: "Remove value count" }).click();
    // One value: no values level, a column per quarter and one All column.
    await expect(status(page)).toContainText(`of ${quarters + 1}`);
    const menu = await columnMenu(page);
    await expect(menu.getByRole("button", { name: /^Remove value/ })).toHaveCount(0);
    await menu.getByRole("button", { name: "Add a value ▸" }).click();
    await menu.getByRole("button", { name: "maximum(weight)" }).click();
    await expect(status(page)).toContainText(`of ${quarters * 2 + 2}`);
  });

  test("the values level's name in the corner moves the values to the rows", async ({ page }) => {
    await rightClick(page, 30, HEADER / 2);
    const menu = page.getByRole("dialog", { name: "Values" });
    await expect(menu.getByRole("button", { name: "Remove level" })).toHaveCount(0);
    await menu.getByRole("button", { name: "Move to rows" }).click();
    // A column per quarter and one All column; each port a group of its two values.
    await expect(status(page)).toContainText(`of ${quarters + 1}`);
  });
});
