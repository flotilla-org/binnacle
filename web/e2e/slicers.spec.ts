// Slicers and grouping support end to end (#203), in the shipments demo
// grouped by region then port, its ports closed: "+ Slice" in the filter bar
// slices the rows by a column, hidden too, through the column menu in slicer
// mode, and the groups' figures change with it; a comparison on the summed
// weight leaves out the ports whose total fails it, the figures above them
// staying; a layout asking for what the source can't do restores, its left
// out parts red chips, not applied; and a source that groups nothing offers
// no grouping or slicer controls.

import { expect, test, type Page } from "@playwright/test";
import { shipments, type Shipment } from "../demo/sources/shipments-data";

const en = (n: number) => n.toLocaleString("en");

const status = (page: Page) => page.locator("#status");
const bar = (page: Page) => page.getByRole("search");
const slicers = (page: Page) => bar(page).getByRole("list", { name: "Slicers" }).getByRole("listitem");

/** The rows the demo shows of `rows`, its ports closed: each region's Group Item, its ports and its Subtotal, and the Grand Total. */
const closed = (rows: Shipment[]) =>
  2 * new Set(rows.map((s) => s.region)).size + new Set(rows.map((s) => `${s.region}/${s.port}`)).size + 1;

/** The total weight of `rows`, as the weight column writes it. */
const weight = (rows: Shipment[]) => String(Math.round(rows.reduce((sum, s) => sum + (s.weight ?? 0), 0) * 10) / 10);

/** The Grand Total's row in the grid's accessible table. */
const grandTotal = (page: Page) => page.getByRole("row").filter({ has: page.getByRole("rowheader", { name: /^Grand total/ }) });

async function openMenu(page: Page, column: string) {
  await bar(page).getByRole("button", { name: "+ Filter" }).click();
  await page.getByRole("menuitem", { name: column, exact: true }).click();
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(closed(shipments))}`);
});

test("a slicer on a hidden column changes the groups' figures, and × brings them back", async ({ page }) => {
  await openMenu(page, "crates");
  await page.getByRole("dialog", { name: "Filter crates" }).getByRole("button", { name: "Hide column" }).click();
  await expect(status(page)).toContainText("columns 1 to 3 of 3");

  await bar(page).getByRole("button", { name: "+ Slice" }).click();
  await page.getByRole("menuitem", { name: "crates", exact: true }).click();
  const menu = page.getByRole("dialog", { name: "Slice by crates" });
  await expect(menu.getByRole("button", { name: "Hide column" })).toBeHidden();
  await menu.getByRole("combobox", { name: "Comparison" }).selectOption("greater");
  await menu.getByRole("spinbutton", { name: "crates value" }).fill("35");
  await page.keyboard.press("Escape");

  const many = shipments.filter((s) => s.crates > 35);
  expect(closed(many)).not.toBe(closed(shipments));
  await expect(slicers(page)).toHaveText(["crates: > 35×"]);
  await expect(status(page)).toContainText(`of ${en(closed(many))}`);
  await expect(grandTotal(page)).toContainText(weight(many));
  // The slicer is no condition on the rows: no filter chip, and the crates column stays hidden.
  await expect(bar(page).getByRole("list", { name: "Column filters" }).getByRole("listitem")).toHaveCount(0);
  await expect(status(page)).toContainText("columns 1 to 3 of 3");

  await slicers(page).getByRole("button", { name: "Remove slicer crates: > 35" }).click();
  await expect(slicers(page)).toHaveCount(0);
  await expect(status(page)).toContainText(`of ${en(closed(shipments))}`);
  await expect(grandTotal(page)).toContainText(weight(shipments));
});

test("a comparison on the summed weight leaves out the ports whose total fails it, the figures above them staying", async ({ page }) => {
  const total = (rows: Shipment[]) => rows.reduce((sum, s) => sum + (s.weight ?? 0), 0);
  const ports = [...new Set(shipments.map((s) => `${s.region}/${s.port}`))].map((key) =>
    total(shipments.filter((s) => `${s.region}/${s.port}` === key)),
  );
  // A bar half the ports pass.
  const over = Math.round([...ports].sort((a, b) => a - b)[Math.floor(ports.length / 2)]!);
  await openMenu(page, "weight");
  const menu = page.getByRole("dialog", { name: "Filter weight" });
  await menu.getByRole("combobox", { name: "Comparison" }).selectOption("greater");
  await menu.getByRole("spinbutton", { name: "weight value" }).fill(String(over));
  await page.keyboard.press("Escape");

  // Each region passing with a port passing shows those ports, then its Subtotal; one with none drops out.
  const regions = [...new Set(shipments.map((s) => s.region))].filter((r) => total(shipments.filter((s) => s.region === r)) > over);
  const portsOf = (r: string | null) =>
    new Set(shipments.filter((s) => s.region === r).map((s) => s.port)).size -
    [...new Set(shipments.filter((s) => s.region === r).map((s) => s.port))].filter(
      (p) => total(shipments.filter((s) => s.region === r && s.port === p)) <= over,
    ).length;
  const shown = regions.reduce((n, r) => n + (portsOf(r) > 0 ? 2 + portsOf(r) : 0), 0) + 1;
  expect(ports.filter((t) => t > over).length).toBeGreaterThan(0);
  expect(shown).toBeLessThan(closed(shipments));
  await expect(status(page)).toContainText(`of ${en(shown)}`);
  await expect(grandTotal(page)).toContainText(weight(shipments));
});

test("a layout asking for what the source can't do restores, the parts left out red chips, not applied", async ({ page }) => {
  await page.getByRole("button", { name: "Save layout" }).click();
  await page.evaluate(() => {
    const key = "binnacle-demo-layout:shipments";
    const layout = JSON.parse(localStorage.getItem(key)!);
    const grouping = layout.view.grouping;
    grouping.values.push({ field: "weight", aggregate: "median" });
    grouping.slicers = [{ id: "port", kind: { type: "Native", text: "port LIKE 'A%'" }, not: false }];
    localStorage.setItem(key, JSON.stringify(layout));
  });
  await page.reload();
  await expect(status(page)).toContainText(`of ${en(closed(shipments))}`);
  await expect(slicers(page)).toHaveText(["port: ‹port LIKE 'A%'›×", "median(weight): not applied"]);
  await expect(slicers(page).first()).toHaveAttribute("title", /^Not applied/);
});

test("a source that groups nothing offers no grouping or slicer controls", async ({ page }) => {
  await page.goto("/?source=slow&pace=slow");
  await expect(bar(page).getByRole("button", { name: "+ Filter" })).toBeVisible();
  await expect(bar(page).getByRole("button", { name: "+ Slice" })).toBeHidden();
  await openMenu(page, "port");
  const menu = page.getByRole("dialog", { name: "Filter port" });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("button", { name: "Group rows by this column" })).toBeHidden();
  await expect(menu.getByRole("button", { name: "Slice by this column" })).toBeHidden();
});
