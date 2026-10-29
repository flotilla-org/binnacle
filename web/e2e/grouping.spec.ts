// Grouping end to end (#200, #201, #202, #206): the shipments demo opens grouped
// by region then port, the regions open and the ports closed, each Group
// Item drawn in outline form in the row header in every preset, each open
// group ending in its Subtotal and the Grand Total pinned at the bottom; a
// click on a disclosure mark and the tree keys of the outline column open
// and close groups; the column menu ungroups the rows and groups them by
// another column, focus staying on its shipment; and a saved layout keeps
// the grouping. A click on a Group Item's label selects its group and
// Shift-click stretches by whole groups (#205); Ctrl/Cmd-C copies the rows
// as shown, and Enter on a Summary Item's cell hands the demo its
// drill-down. Find counts the matches in closed groups and opens one to
// step to it, and a rerouted shipment stays where it was.

import { expect, test, type Page } from "@playwright/test";
import { REGIONS, shipments } from "../demo/sources/shipments-data";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");
const en = (n: number) => n.toLocaleString("en");

/** Cmd where the page sees macOS, Ctrl elsewhere: the user agent the browser is given decides. */
const mod = (page: Page) =>
  page.evaluate(() => {
    const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
    return /mac/i.test(nav.userAgentData?.platform ?? nav.platform) ? "Meta" : "Control";
  });

/** The groups a column makes of the shipments. */
const groupsBy = (field: "region" | "crates") => new Set(shipments.map((s) => s[field])).size;

/** The rows grouped by region then port: each group's Group Item and Subtotal, and the Grand Total. */
const nested = () => shipments.length + 2 * (groupsBy("region") + new Set(shipments.map((s) => `${s.region}/${s.port}`)).size) + 1;

/** The regions in the order they show, the blank one first, and the ports of `region` in theirs. */
const regions = (): (string | null)[] => [null, ...[...REGIONS].sort()];
const label = (region: string | null | undefined) => region ?? "(blank)";
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

test("a click on a Group Item's label selects its group, and Shift-click stretches by whole groups", async ({ page }) => {
  const [east, north] = regions();
  const eastRows = 1 + portsOf(east!).length + 1;
  await band(page, 0, 80);
  await expect(status(page)).toContainText(`selected ${eastRows} × 4`);
  // A closed port selects its own row.
  await band(page, 1, 80);
  await expect(status(page)).toContainText("selected 1 × 4");
  await band(page, 0, 80);
  await page.keyboard.down("Shift");
  await band(page, eastRows, 80);
  await page.keyboard.up("Shift");
  await expect(status(page)).toContainText(`selected ${eastRows + 1 + portsOf(north!).length + 1} × 4`);
});

test("Ctrl/Cmd-C copies a group as shown: labels first, closed ports' figures, the open region's in its Subtotal", async ({ page }) => {
  await page.evaluate(() => {
    const w = window as unknown as { copied: string[] };
    w.copied = [];
    window.addEventListener("copy", (e) => w.copied.push(e.clipboardData?.getData("text/plain") ?? ""));
  });
  const [east] = regions();
  const ports = portsOf(east!);
  await band(page, 0, 80);
  await page.keyboard.press(`${await mod(page)}+c`);
  await expect(page.locator("#received")).toHaveText(`copied ${ports.length + 2} rows`);
  const [text] = await page.evaluate(() => (window as unknown as { copied: string[] }).copied);
  const lines = text!.split("\n").map((l) => l.split("\t"));
  const weight = (rows: typeof shipments) => String(Math.round(rows.reduce((w, s) => w + (s.weight ?? 0), 0) * 10) / 10);
  const inEast = shipments.filter((s) => s.region === east);
  expect(lines.map((l) => l.length)).toEqual(lines.map(() => 5));
  expect(lines[0]).toEqual([`${label(east)} (${inEast.length})`, "", "", "", ""]);
  for (const [i, port] of ports.entries()) {
    expect(lines[i + 1]![0]).toBe(`${port} (${shipped(east!, port)})`);
    expect(lines[i + 1]![3]).toBe(weight(inEast.filter((s) => s.port === port)));
  }
  expect(lines.at(-1)![0]?.startsWith(`${label(east)} total`)).toBe(true);
  expect(lines.at(-1)![3]).toBe(weight(inEast));
});

test("Enter on a Summary Item's cell hands the demo its drill-down", async ({ page }) => {
  const [east] = regions();
  const [first] = portsOf(east!);
  await click(page, 1, 2);
  await page.keyboard.press("Enter");
  await expect(page.locator("#received")).toHaveText(`drill-down: region ${label(east)}, port ${first}`);
  await click(page, 0, 2);
  await page.keyboard.press("Enter");
  await expect(page.locator("#received")).toHaveText(`drill-down: region ${label(east)}`);
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
  // While grouped, it groups by its column as one level more (#212).
  await expect(menu.getByRole("button", { name: "Group rows by this column" })).toBeVisible();
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

test("find counts the matches in closed ports, and stepping to one opens its port and focuses it", async ({ page }) => {
  const found = shipments.find((s) => s.id === 1999)!;
  await page.getByRole("button", { name: "Find" }).click();
  const bar = page.getByRole("search", { name: "Find" });
  await bar.getByRole("textbox", { name: "Find" }).fill("1999");
  await expect(bar.locator(".binnacle-find-count")).toHaveText("1 match, 1 in collapsed groups");
  await page.keyboard.press("Enter");
  await expect(bar.locator(".binnacle-find-count")).toHaveText("1 of 1");
  await expect(status(page)).toContainText(`of ${en(closed() + shipped(found.region, found.port) + 1)}`);
  await expect(status(page)).toContainText("(1999 / id)");
});

test("a rerouted shipment stays where it was, the rows unchanged", async ({ page }) => {
  await openAll(page);
  await click(page, 2, 0);
  const focus = /focus row (\d+), column 1 \((\d+) \/ id\)/;
  await expect(status(page)).toContainText(focus);
  const [, row, id] = (await status(page).textContent())!.match(focus)!;
  await page.getByRole("button", { name: "Reroute" }).click();
  await expect(page.locator("#committed")).toContainText(`port of ${id} = `);
  await expect(status(page)).toContainText(`focus row ${row}, column 1 (${id} / id)`);
  await expect(status(page)).toContainText(`of ${en(nested())}`);
});

test("the corner joins the row levels' names in the outline column, and a right-click on one opens its level menu", async ({ page }) => {
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  // "region › port" from the band's padding: the port's name past the region's.
  await page.mouse.click(b.x + 80, b.y + HEADER / 2, { button: "right" });
  await expect(page.getByRole("dialog", { name: "Row level port" })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.mouse.click(b.x + 20, b.y + HEADER / 2, { button: "right" });
  await expect(page.getByRole("dialog", { name: "Row level region" })).toBeVisible();
});
