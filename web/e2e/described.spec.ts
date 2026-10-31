// The described source: a Described Table from a fake table server through
// the dev server's `/tables` proxy. Most tests read the shared fake the
// webServer entry starts; those that change the table as they go start
// their own (`fake`) and name it with `?server=`, so that path runs too.
// Cells are read through the ARIA grid, which holds each value's Stable
// Text, and the watch's state through `#described-status`'s data
// attributes, never through font metrics.

import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures/fake-tables";

const status = (page: Page) => page.locator("#described-status");
const grid = (page: Page, table = "crews") => page.getByRole("grid", { name: `${table} table` });
const count = (page: Page) => page.locator(".binnacle-row-count");

/** The gridcells of the row whose Name is `name`, in drawn order: Name and Phase pinned first. */
const row = (page: Page, name: string) =>
  grid(page)
    .getByRole("row")
    .filter({ has: page.getByRole("gridcell", { name, exact: true }) })
    .getByRole("gridcell");

/** Opens the crews table and waits until it is synced. */
async function crews(page: Page, query = "") {
  await page.goto(`/?source=described&table=crews${query}`);
  await expect(status(page)).toHaveAttribute("data-state", "synced");
  await expect(row(page, "alpha").first()).toBeVisible();
}

/** Geometry: a row-number band up to 80 px, 120 px columns with 1 px gaps, a 28 px header band and 24 px rows. */
/** Clicks the header of the `col`th column drawn (0 first). */
async function header(page: Page, col: number) {
  const box = (await page.locator(".binnacle-layer").boundingBox())!;
  await page.mouse.click(box.x + 80 + col * 121 + 30, box.y + 14);
}

/** Clicks the body cell in the `row`th row drawn, first column. */
async function cell(page: Page, at: number) {
  const box = (await page.locator(".binnacle-layer").boundingBox())!;
  await page.mouse.click(box.x + 80 + 30, box.y + 28 + at * 24 + 12);
}

/** The Name of each row drawn, in order. */
const names = (page: Page) => grid(page).getByRole("row").locator('[role="gridcell"][aria-colindex="2"]');

test("the picker lists the server's tables and remembers the one chosen", async ({ page }) => {
  await page.goto("/?source=described");
  const picker = page.getByRole("combobox", { name: "Table" });
  await expect(picker.locator("option")).toHaveText(["crews", "hosts", "big"]);
  await expect(status(page)).toHaveAttribute("data-state", "synced");
  await picker.selectOption("hosts");
  await expect(grid(page, "hosts")).toBeVisible();
  await expect(status(page)).toHaveAttribute("data-state", "synced");
  expect(new URL(page.url()).searchParams.get("table")).toBe("hosts");
  await page.reload();
  await expect(picker).toHaveValue("hosts");
  await expect(grid(page, "hosts")).toBeVisible();
  await expect(status(page)).toHaveAttribute("data-state", "synced");
});

test("tables picked one after another while mounting leave one grid, the last", async ({ page }) => {
  await crews(page);
  // Two picks in one task: the first is still mounting when the second comes.
  await page.evaluate(() => {
    const picker = document.querySelector<HTMLSelectElement>('select[aria-label="Table"]')!;
    for (const name of ["hosts", "big", "crews", "hosts"]) {
      picker.value = name;
      picker.dispatchEvent(new Event("change"));
    }
  });
  await expect(grid(page, "hosts")).toBeVisible();
  await expect(status(page)).toHaveAttribute("data-state", "synced");
  await expect(page.getByRole("grid")).toHaveCount(1);
  await expect(page.locator(".grid-box")).toHaveCount(1);
  await expect(page.locator(".binnacle-row-count")).toHaveCount(1);
});

for (const encoding of ["ndjson", "cbor"]) {
  test(`the crews table draws each Typed Value's Stable Text, read as ${encoding}`, async ({ page }) => {
    await crews(page, `&encoding=${encoding}`);
    await expect(status(page)).toHaveAttribute("data-rows", "6");
    await expect(status(page)).toContainText("crews · synced · 6 rows · 3 mismatches");
    const headers = grid(page).getByRole("columnheader");
    await expect(headers.nth(1)).toHaveText("Name");
    await expect(headers.nth(2)).toHaveText("Phase");
    await expect(headers).toContainText(["CPUs"]);
    await expect(headers.filter({ hasText: "Spec" })).toHaveCount(0);
    await expect(row(page, "bravo").nth(1)).toHaveText("∅");
    await expect(row(page, "alpha").nth(5)).toHaveText("");
    await expect(row(page, "charlie").nth(3)).toHaveText("many");
    await expect(row(page, "echo").nth(3)).toHaveText("18446744073709551615");
  });
}

test.describe("screenshot", () => {
  test.use({ timezoneId: "UTC" });
  test.skip(
    ({ browserName }) => browserName !== "chromium" || process.platform !== "darwin",
    "the baseline is taken in Chromium on macOS",
  );

  test("the crews table in the light theme", async ({ page }) => {
    await page.clock.install({ time: Date.parse("2026-10-29T13:00:00Z") });
    await crews(page, "&theme=light");
    await page.mouse.move(0, 0);
    await expect(page.locator(".with-filter-bar")).toHaveScreenshot("described-crews-light.png");
  });
});

test("a table over the watch's cap shows partial", async ({ page }) => {
  await page.goto("/?source=described&table=big");
  await expect(status(page)).toHaveAttribute("data-state", "synced", { timeout: 30_000 });
  await expect(status(page)).toHaveAttribute("data-partial", "true");
  await expect(status(page)).toContainText("10,000 of more");
  await expect(count(page)).toHaveText("10,000 rows");
});

test("upserts arrive live", async ({ page, fake }) => {
  await crews(page, `&server=${fake.url}`);
  fake.push({ step: "upsert", table: "crews", row: { name: "alpha", phase: "Running", cpus: 16, spec: {} } });
  await expect(row(page, "alpha").nth(3)).toHaveText("16");
  fake.push({ step: "upsert", table: "crews", row: { name: "golf", phase: "Running", cpus: 1, spec: {} } });
  await expect(count(page)).toHaveText("7 rows");
});

test("a resync keeps focus and the sort", async ({ page, fake }) => {
  await crews(page, `&server=${fake.url}`);
  await header(page, 0);
  await header(page, 0);
  const name = grid(page).getByRole("columnheader", { name: "Name" });
  await expect(name).toHaveAttribute("aria-sort", "descending");
  await expect(names(page)).toHaveText(["foxtrot", "echo", "delta", "charlie", "bravo", "alpha"]);
  await cell(page, 2);
  await expect(status(page)).toHaveAttribute("data-focus", '["delta"]');
  fake.push({ step: "resync", table: "crews" });
  await expect(status(page)).toHaveAttribute("data-resyncs", "1");
  await expect(status(page)).toHaveAttribute("data-state", "synced");
  await expect(status(page)).toHaveAttribute("data-focus", '["delta"]');
  await expect(name).toHaveAttribute("aria-sort", "descending");
  await page.keyboard.press("ArrowDown");
  await expect(status(page)).toHaveAttribute("data-focus", '["charlie"]');
});

test("the stream's end reconnects and resyncs, keeping focus", async ({ page, fake }) => {
  await crews(page, `&server=${fake.url}`);
  await cell(page, 3);
  await expect(status(page)).toHaveAttribute("data-focus", '["delta"]');
  // The delete lands while no watch is open, so only the resync removes alpha.
  fake.push({ step: "end" });
  fake.push({ step: "delete", table: "crews", key: ["alpha"] });
  await expect(status(page)).toHaveAttribute("data-reconnects", "1");
  await expect(status(page)).toHaveAttribute("data-state", "synced");
  await expect(count(page)).toHaveText("5 rows");
  await expect(names(page)).toHaveText(["bravo", "charlie", "delta", "echo", "foxtrot"]);
  await expect(status(page)).toHaveAttribute("data-focus", '["delta"]');
  // Focus is on delta, now row 3: a step down lands on echo.
  await page.keyboard.press("ArrowDown");
  await expect(status(page)).toHaveAttribute("data-focus", '["echo"]');
});

test("an added field comes last, and the sort and focus hold", async ({ page, fake }) => {
  await crews(page, `&server=${fake.url}`);
  await header(page, 3);
  const cpus = grid(page).getByRole("columnheader", { name: "CPUs" });
  await expect(cpus).toHaveAttribute("aria-sort", "ascending");
  await cell(page, 0);
  await expect(status(page)).toHaveAttribute("data-focus", '["delta"]');
  fake.push({ step: "add-field", table: "crews", field: { name: "zone", type: "string", title: "Zone" }, value: "eu" });
  await expect(grid(page).getByRole("columnheader").last()).toHaveText("Zone");
  await expect(cpus).toHaveAttribute("aria-sort", "ascending");
  await expect(row(page, "alpha").last()).toHaveText("eu");
  await expect(status(page)).toHaveAttribute("data-focus", '["delta"]');
});

test("the proxy refuses a server that is not loopback", async ({ page }) => {
  await page.goto("/?source=described&table=crews&server=http://example.com");
  await expect(status(page)).toHaveAttribute("data-state", "error");
  await expect(status(page)).toContainText("400");
  await expect(status(page)).toContainText("server must be loopback or unix:");
});
