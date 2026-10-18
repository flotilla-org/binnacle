// The records source end to end: header clicks sort (Shift adds a column),
// the filter box filters (keeping the view's place), header drags move columns, and focus follows its
// record through a re-sort. Cells are canvas pixels, so the status line's
// focus (Position, row ID, column ID) is the probe: the row ID is the
// record's index, which is also its `id`.

import { expect, test, type Page } from "@playwright/test";
import { matches, rows, type RecordRow } from "../demo/sources/records-data";
import { column, FIRST } from "./records-columns";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");

async function layer(page: Page) {
  const box = await page.locator(".binnacle-layer").boundingBox();
  if (!box) throw new Error("no grid");
  return box;
}

/** Clicks column `col`'s header (Positions as shown). */
async function header(page: Page, col: number, modifiers: ("Shift" | "Alt")[] = []) {
  const b = await layer(page);
  for (const m of modifiers) await page.keyboard.down(m);
  await page.mouse.click(b.x + column(col), b.y + HEADER / 2);
  for (const m of modifiers) await page.keyboard.up(m);
}

/** Clicks the visible cell at screen row `row` (0 first) of column `col`. */
async function cell(page: Page, row: number, col = 0) {
  const b = await layer(page);
  await page.mouse.click(b.x + column(col), b.y + HEADER + row * ROW + ROW / 2);
}

const focus = (position: number, id: number, col: string, colPosition = 0) =>
  `focus row ${(position + 1).toLocaleString("en")}, column ${colPosition + 1} (${id} / ${col})`;

type Key = keyof RecordRow;

/** Record indices in the order a stable sort by `spec` gives, nulls last either way. */
function sorted(spec: [Key, "asc" | "desc"][], keep: (r: RecordRow) => boolean = () => true): number[] {
  return rows
    .map((_, i) => i)
    .filter((i) => keep(rows[i]!))
    .sort((i, j) => {
      for (const [key, direction] of spec) {
        const a = rows[i]![key];
        const b = rows[j]![key];
        if (a === b) continue;
        if (a === null) return 1;
        if (b === null) return -1;
        const c = a < b ? -1 : 1;
        return direction === "asc" ? c : -c;
      }
      return i - j;
    });
}

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=records");
  await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
});

test("onSelectionChange fires once per change, and its IDs survive a re-sort", async ({ page }) => {
  const changes = () => status(page).getAttribute("data-selection-changes");
  await cell(page, 2, 1);
  await expect(status(page)).toContainText(focus(2, 2, "name", 1));
  expect(await changes()).toBe("1");
  // Two re-sorts move the record but not the focus by ID, so nothing fires.
  await header(page, 2);
  const at = sorted([["weight", "asc"]]).indexOf(2);
  await expect(status(page)).toContainText(focus(at, 2, "name", 1));
  await header(page, 2);
  await expect(status(page)).toContainText(focus(sorted([["weight", "desc"]]).indexOf(2), 2, "name", 1));
  expect(await changes()).toBe("1");
});

test("a header click sorts, and focus follows its record through a re-sort", async ({ page }) => {
  await header(page, 2);
  const ascending = sorted([["weight", "asc"]]);
  await cell(page, 0);
  await expect(status(page)).toContainText(focus(0, ascending[0]!, "id"));
  // Descending: the focused record is located at its new Position.
  await header(page, 2);
  const descending = sorted([["weight", "desc"]]);
  const at = descending.indexOf(ascending[0]!);
  await expect(status(page)).toContainText(focus(at, ascending[0]!, "id"));
  // Nulls stay last when descending.
  expect(rows[descending.at(-1)!]!.weight).toBeNull();
  await page.locator(".binnacle-scroller").evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(status(page)).toContainText(`to ${rows.length.toLocaleString("en")} of`);
  const b = await layer(page);
  await page.mouse.click(b.x + FIRST, b.y + b.height - ROW / 2);
  await expect(status(page)).toContainText(focus(rows.length - 1, descending.at(-1)!, "id"));
});

test("Shift-click sorts by a second column", async ({ page }) => {
  await header(page, 3);
  await header(page, 2, ["Shift"]);
  await cell(page, 0);
  await expect(status(page)).toContainText(focus(0, sorted([["crew", "asc"], ["weight", "asc"]])[0]!, "id"));
  await cell(page, 5);
  await expect(status(page)).toContainText(focus(5, sorted([["crew", "asc"], ["weight", "asc"]])[5]!, "id"));
});

test("the filter box keeps the matching records, in the sort in force", async ({ page }) => {
  await header(page, 1);
  await page.getByRole("searchbox", { name: "Filter records" }).fill("bergen");
  const kept = sorted([["name", "asc"]], (r) => matches(r, "bergen"));
  await expect(status(page)).toContainText(`of ${kept.length.toLocaleString("en")}`);
  await cell(page, 2);
  await expect(status(page)).toContainText(focus(2, kept[2]!, "id"));
  await page.getByRole("searchbox", { name: "Filter records" }).fill("");
  await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
});

test("a filter keeps the topmost record still shown at the top, and clearing it keeps it there", async ({ page }) => {
  const b = await layer(page);
  await page.mouse.move(b.x + column(1), b.y + HEADER + ROW * 3);
  await page.mouse.wheel(0, ROW * 25);
  await expect(status(page)).not.toContainText("rows 1 to");
  const first = Number(/rows ([\d,]+) to/.exec((await status(page).textContent()) ?? "")![1]!.replace(/,/g, ""));
  // The topmost record shown that matches, and its Position once filtered.
  const kept = sorted([], (r) => matches(r, "e"));
  const survivor = kept.find((i) => i >= first - 1)!;
  const top = kept.indexOf(survivor) + 1;
  await page.getByRole("searchbox", { name: "Filter records" }).fill("e");
  await expect(status(page)).toContainText(`rows ${top} to`);
  await expect(status(page)).not.toContainText("loading");
  await page.getByRole("searchbox", { name: "Filter records" }).fill("");
  await expect(status(page)).toContainText(`rows ${survivor + 1} to`);
  await expect(status(page)).not.toContainText("loading");
});

test("dragging a header moves its column", async ({ page }) => {
  const b = await layer(page);
  await page.mouse.move(b.x + column(4), b.y + HEADER / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + column(2), b.y + HEADER / 2, { steps: 5 });
  await page.mouse.move(b.x + FIRST - 10, b.y + HEADER / 2, { steps: 5 });
  await page.mouse.up();
  await cell(page, 0, 0);
  await expect(status(page)).toContainText(focus(0, 0, "port"));
  // The port column now starts wider than its old place: step right by key.
  await page.keyboard.press("ArrowRight");
  await expect(status(page)).toContainText(focus(0, 0, "id", 1));
});
