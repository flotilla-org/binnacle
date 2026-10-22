// The ARIA layer of a grouped table (#213): rows grouped in outline form are
// a treegrid, each row with its level, a Group Item's expanded state, its
// place among its siblings and a Summary Item's kind; in header form each
// level is a column of row headers whose cells span their groups' rows; the
// corner's column headers name the levels. The fixture is seven shipments
// by region then port, Subtotals after each group and a Grand Total after
// all, the regions open and the ports closed.

import { expect, test, type Page } from "@playwright/test";

const HEADER = 28;
const ROW = 24;

const grid = (page: Page) => page.locator(".binnacle-aria table");

/** Each body row as a screen reader gets it: its header's text and the attributes that place it in the tree. */
const rows = (page: Page) =>
  page.evaluate(() =>
    [...document.querySelectorAll(".binnacle-aria tbody tr")].map((tr) => {
      const at = (name: string) => tr.getAttribute(name);
      return {
        header: tr.querySelector('[role="rowheader"]:not([aria-expanded])')?.textContent ?? null,
        index: Number(at("aria-rowindex")),
        level: at("aria-level"),
        expanded: at("aria-expanded"),
        place: at("aria-posinset") && `${at("aria-posinset")} of ${at("aria-setsize")}`,
        kind: at("aria-roledescription"),
      };
    }),
  );

/** The role, text and state of the element the textarea names as its active descendant, `null` if none. */
const active = (page: Page) =>
  page.evaluate(() => {
    const id = document.querySelector("textarea")!.getAttribute("aria-activedescendant");
    const el = id === null ? null : document.getElementById(id);
    if (!el) return null;
    const header = el.getAttribute("role") === "row" ? el.querySelector('[role="rowheader"]') : el;
    return { role: el.getAttribute("role"), text: header?.textContent, expanded: el.getAttribute("aria-expanded") };
  });

async function open(page: Page, form: "outline" | "header") {
  await page.goto(`/e2e/fixtures/aria-grouped.html${form === "header" ? "?form=header" : ""}`);
  await expect(page.locator("#grid")).toHaveAttribute("data-painted", "true");
}

/** Clicks the row band at `x` on row `row` (0 first). */
async function band(page: Page, row: number, x: number) {
  const box = (await page.locator(".binnacle-layer").boundingBox())!;
  await page.mouse.click(box.x + x, box.y + HEADER + row * ROW + ROW / 2);
}

const group = (header: string, index: number, level: string, expanded: string, place: string) =>
  ({ header, index, level, expanded, place, kind: "group row" }) as const;
const item = (header: string, index: number, level: string, place: string, kind: string | null) =>
  ({ header, index, level, expanded: null, place, kind }) as const;

test("outline form is a treegrid: each row's level, expanded state, place among its siblings and kind", async ({ page }) => {
  await open(page, "outline");
  await expect(grid(page)).toHaveAttribute("role", "treegrid");
  await expect(grid(page)).toHaveAttribute("aria-rowcount", "10");
  // The row band's column header names the levels; the grouped fields leave the body.
  await expect(grid(page).getByRole("columnheader")).toHaveText(["region, port", "id", "weight"]);
  await expect(grid(page).getByRole("columnheader", { name: "weight" })).toHaveAttribute("aria-colindex", "3");
  expect(await rows(page)).toEqual([
    group("North (4)", 2, "1", "true", "1 of 3"),
    group("Aberdeen (2)", 3, "2", "false", "1 of 3"),
    group("Bergen (2)", 4, "2", "false", "2 of 3"),
    item("North total (4)", 5, "2", "3 of 3", "subtotal row"),
    group("South (3)", 6, "1", "true", "2 of 3"),
    group("Cadiz (2)", 7, "2", "false", "1 of 3"),
    group("Dover (1)", 8, "2", "false", "2 of 3"),
    item("South total (3)", 9, "2", "3 of 3", "subtotal row"),
    item("Grand total (7)", 10, "1", "3 of 3", "grand total row"),
  ]);
  await expect(grid(page).getByRole("row").nth(4).getByRole("gridcell")).toHaveText(["", "130"]);
});

test("opening and closing a group updates its expanded state, its rows taking their places as their row indexes say", async ({
  page,
}) => {
  await open(page, "outline");
  // A click on a row header enters the outline column, where focus is on the row.
  await band(page, 1, 100);
  await expect.poll(() => active(page)).toEqual({ role: "row", text: "Aberdeen (2)", expanded: "false" });
  await page.keyboard.press("ArrowRight");
  await expect.poll(() => active(page)).toEqual({ role: "row", text: "Aberdeen (2)", expanded: "true" });
  await expect(grid(page)).toHaveAttribute("aria-rowcount", "13");
  const opened = await rows(page);
  expect(opened.slice(1, 6)).toEqual([
    group("Aberdeen (2)", 3, "2", "true", "1 of 3"),
    // A Leaf's header is its row number, as drawn.
    item("3", 4, "3", "1 of 3", null),
    item("4", 5, "3", "2 of 3", null),
    item("Aberdeen total (2)", 6, "3", "3 of 3", "subtotal row"),
    group("Bergen (2)", 7, "2", "false", "2 of 3"),
  ]);
  // Row indexes run on with no gap, so a reader counting rows and one reading the tree agree.
  expect(opened.map((r) => r.index)).toEqual(opened.map((_, i) => i + 2));

  await page.keyboard.press("ArrowLeft");
  await expect.poll(() => active(page)).toEqual({ role: "row", text: "Aberdeen (2)", expanded: "false" });
  await expect(grid(page)).toHaveAttribute("aria-rowcount", "10");
  expect((await rows(page))[2]).toEqual(group("Bergen (2)", 4, "2", "false", "2 of 3"));

  // Tab leaves the outline column for the body, where focus is on a cell again.
  await page.keyboard.press("Tab");
  await expect.poll(() => active(page).then((a) => a?.role)).toBe("gridcell");
});

test("header form exposes each level as a column of row headers, each group's cell spanning its rows", async ({ page }) => {
  await open(page, "header");
  await expect(grid(page)).toHaveAttribute("role", "grid");
  await expect(grid(page)).toHaveAttribute("aria-colcount", "5");
  await expect(grid(page).getByRole("columnheader")).toHaveText(["region", "port", "", "id", "weight"]);
  const north = grid(page).getByRole("rowheader", { name: "North (4)" });
  await expect(north).toHaveAttribute("rowspan", "2");
  await expect(north).toHaveAttribute("aria-colindex", "1");
  await expect(north).toHaveAttribute("aria-expanded", "true");
  const aberdeen = grid(page).getByRole("rowheader", { name: "Aberdeen (2)" });
  await expect(aberdeen).not.toHaveAttribute("rowspan");
  await expect(aberdeen).toHaveAttribute("aria-colindex", "2");
  await expect(aberdeen).toHaveAttribute("aria-expanded", "false");
  // A closed port's Subtotal stands under its span; a region's own sits outside it, across the band.
  const first = grid(page).getByRole("row").nth(1);
  await expect(first.getByRole("rowheader")).toHaveText(["North (4)", "Aberdeen (2)", "Aberdeen total (2)"]);
  await expect(first.getByRole("rowheader").nth(2)).toHaveAttribute("aria-colindex", "3");
  await expect(first.getByRole("gridcell")).toHaveText(["", "40"]);
  await expect(first.getByRole("gridcell").nth(1)).toHaveAttribute("aria-colindex", "5");
  await expect(first).toHaveAttribute("aria-roledescription", "subtotal row");
  await expect(first).not.toHaveAttribute("aria-level");
  const total = grid(page).getByRole("rowheader", { name: "North total (4)" });
  await expect(total).toHaveAttribute("colspan", "3");
  await expect(total).toHaveAttribute("aria-colindex", "1");
  await expect(grid(page).getByRole("row").nth(2).getByRole("rowheader")).toHaveText(["Bergen (2)", "Bergen total (2)"]);

  // A click on a level column focuses the span there; closing it updates its expanded state.
  await band(page, 0, 60);
  await expect.poll(() => active(page)).toEqual({ role: "rowheader", text: "North (4)", expanded: "true" });
  await page.keyboard.press("ArrowLeft");
  await expect.poll(() => active(page)).toEqual({ role: "rowheader", text: "North (4)", expanded: "false" });
  await expect(grid(page).getByRole("row").nth(1).getByRole("rowheader")).toHaveText(["North (4)", "North total (4)"]);
  await expect(grid(page).getByRole("rowheader", { name: "North total (4)" })).toHaveAttribute("colspan", "2");
});

test("a level name the rows are sorted by says so", async ({ page }) => {
  await open(page, "header");
  const region = grid(page).getByRole("columnheader", { name: "region" });
  await expect(region).not.toHaveAttribute("aria-sort");
  const box = (await page.locator(".binnacle-layer").boundingBox())!;
  await page.mouse.click(box.x + 40, box.y + HEADER / 2);
  await expect(region).toHaveAttribute("aria-sort", "ascending");
});
