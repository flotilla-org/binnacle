// The empty and error states: an empty table, or a filter that matches
// nothing, says so in the body; a source that fails shows error cells, calls
// `onError`, and clears once a retry is answered.

import { expect, test, type Page } from "@playwright/test";

type Fixture = {
  widget: { emptyMessage: string };
  errors: { message: string; request: unknown }[];
  filter(on: boolean): void;
  fail(on: boolean): void;
  retry(): void;
};

const fixture = <T>(page: Page, run: (f: Fixture) => T) =>
  page.evaluate(`(${run.toString()})(window.fixture)`) as Promise<T>;

const grid = (page: Page) => page.getByRole("grid", { name: "People" });
const host = (page: Page) => page.locator("#grid");
const chromium = (browserName: string) => browserName === "chromium";

async function open(page: Page, query = "") {
  await page.goto(`/e2e/fixtures/states.html${query}`);
  await expect(host(page)).toHaveAttribute("data-painted", /.+/);
}

/** Waits for the frames after a change to paint. */
async function painted(page: Page) {
  const before = Number(await host(page).getAttribute("data-painted"));
  await expect.poll(async () => Number(await host(page).getAttribute("data-painted"))).toBeGreaterThan(before);
}

test("an empty table says No rows in the body", async ({ page, browserName }) => {
  await open(page, "?rows=0");
  await expect(grid(page)).toHaveAttribute("aria-rowcount", "1");
  if (chromium(browserName)) await expect(host(page)).toHaveScreenshot("empty.png");
});

test("a filter that matches nothing shows the source's message, and the app's own otherwise", async ({ page, browserName }) => {
  await open(page);
  await fixture(page, (f) => f.filter(true));
  await expect(grid(page)).toHaveAttribute("aria-rowcount", "1");
  if (chromium(browserName)) await expect(host(page)).toHaveScreenshot("filtered.png");
  // The app's message stands where the source gives none.
  await open(page, "?rows=0");
  const waiting = painted(page);
  await fixture(page, (f) => (f.widget.emptyMessage = "Nothing here yet"));
  await waiting;
  if (chromium(browserName)) await expect(host(page)).toHaveScreenshot("app-message.png");
});

test("a failing source shows error cells and calls onError; a retry that is answered clears them", async ({ page, browserName }) => {
  await open(page);
  await fixture(page, (f) => (f.fail(true), f.retry()));
  await expect(grid(page).getByRole("gridcell", { name: "Failed to load" })).toHaveCount(10);
  await expect(host(page)).toHaveAttribute("data-loading", "false");
  const errors = await fixture(page, (f) => f.errors);
  expect(errors).toEqual([{ message: "the server is down", request: { type: "window", axis: "rows", start: 0, end: 5 } }]);
  if (chromium(browserName)) await expect(host(page)).toHaveScreenshot("failed.png");

  await fixture(page, (f) => (f.fail(false), f.retry()));
  await expect(grid(page).getByRole("gridcell", { name: "Failed to load" })).toHaveCount(0);
  await expect(grid(page).getByRole("gridcell", { name: "Grace" })).toHaveCount(1);
  expect(await fixture(page, (f) => f.errors.length)).toBe(1);
});
