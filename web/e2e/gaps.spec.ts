// The gaps source end to end: the feature-gap research, first by demand
// times fit, with a detail panel that follows `onSelectionChange` by row ID,
// so it stays on its feature through a sort and a filter.

import { expect, test, type Page } from "@playwright/test";
import { features, keeps } from "../demo/sources/gaps-data";

const HEADER = 28;
const ROW = 24;
/** Inside the first column whatever the row-number band's width. */
const FIRST = 62;

/** The grid's column count: Feature, Kind, Category, the statuses, Demand, Fit, Products. */
const COLUMNS = 7;

const panel = (page: Page) => page.locator(".gap-detail");

async function layer(page: Page) {
  const box = await page.locator(".gaps-grid .binnacle-layer").boundingBox();
  if (!box) throw new Error("no grid");
  return box;
}

async function cell(page: Page, row: number) {
  const b = await layer(page);
  await page.mouse.click(b.x + FIRST, b.y + HEADER + row * ROW + ROW / 2);
}

async function header(page: Page, col: number) {
  const b = await layer(page);
  const width = (b.width - 48) / COLUMNS;
  await page.mouse.click(b.x + 48 + col * width + width / 2, b.y + HEADER / 2);
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 800 });
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/?source=gaps");
  await expect(page.locator("#status")).toContainText(`of ${features.length}`);
});

test("the panel follows focus by ID through a sort and a filter", async ({ page }) => {
  await expect(panel(page)).toContainText("Pick a feature");
  // The first app feature: its kind's filter keeps few enough rows to show them all.
  const chosen = features.find((f) => f.kind === "app")!;
  await cell(page, features.indexOf(chosen));
  await expect(panel(page)).toHaveAttribute("data-feature", chosen.id);
  await expect(panel(page).locator("h2")).toHaveText(chosen.name);

  // Sorting by name moves the row; the panel stays on its feature.
  await header(page, 0);
  const byName = [...features].sort((a, b) => (a.name < b.name ? -1 : 1));
  await expect(page.locator("#status")).toContainText(`focus row ${byName.indexOf(chosen) + 1},`);
  await expect(panel(page)).toHaveAttribute("data-feature", chosen.id);

  // Filtering to its kind keeps it too, and the panel then follows a new click.
  await page.getByLabel("Filter by kind").selectOption(chosen.kind);
  const kept = byName.filter((f) => keeps(f, { status: null, kind: chosen.kind }));
  await expect(page.locator("#status")).toContainText(`rows 1 to ${kept.length} of ${kept.length}`);
  await expect(panel(page)).toHaveAttribute("data-feature", chosen.id);
  const other = kept.find((f) => f !== chosen)!;
  await cell(page, kept.indexOf(other));
  await expect(panel(page)).toHaveAttribute("data-feature", other.id);
});

test("a click selects the feature's whole row, as in a list", async ({ page }) => {
  await cell(page, 2);
  await expect(page.locator("#status")).toContainText(`focus row 3, column 1  ·  selected 1 × ${COLUMNS}`);
  await page.keyboard.press("ArrowDown");
  await expect(page.locator("#status")).toContainText(`focus row 4, column 1  ·  selected 1 × ${COLUMNS}`);
});

test("the status filter keeps the features with that status in that adapter", async ({ page }) => {
  await page.getByLabel("Filter by status").selectOption("web:missing");
  const kept = features.filter((f) => f.web === "missing");
  await expect(page.locator("#status")).toContainText(`of ${kept.length}  ·`);
  await cell(page, 0);
  await expect(panel(page)).toHaveAttribute("data-feature", kept[0]!.id);
});

test("the gap and kind filters combine", async ({ page }) => {
  await page.getByLabel("Filter by status").selectOption("gap");
  await page.getByLabel("Filter by kind").selectOption("app");
  const kept = features.filter((f) => keeps(f, { status: { type: "gap" }, kind: "app" }));
  expect(kept.length).toBeGreaterThan(0);
  await expect(page.locator("#status")).toContainText(`rows 1 to ${kept.length} of ${kept.length}`);
});

test("evidence links open in a new tab", async ({ page }) => {
  await cell(page, 0);
  const links = panel(page).locator(".gap-evidence a");
  await expect(links).toHaveCount(features[0]!.evidence.length);
  for (const link of await links.all()) {
    await expect(link).toHaveAttribute("target", "_blank");
    await expect(link).toHaveAttribute("rel", /noopener/);
  }
});

test.describe("screenshots", () => {
  test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

  for (const scheme of ["light", "dark"] as const) {
    test(`the gaps page with the panel open, ${scheme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await cell(page, 1);
      await expect(panel(page)).toHaveAttribute("data-feature", features[1]!.id);
      // Moves the pointer off the grid, so no header shows hovered.
      await page.mouse.move(0, 0);
      await expect(page.locator(".gaps")).toHaveScreenshot(`gaps-${scheme}.png`, { scale: "device" });
    });
  }
});
