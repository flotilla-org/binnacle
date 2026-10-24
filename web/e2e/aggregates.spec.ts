// The shipments demo over the slow aggregate backend (#204): the rows group
// through groupedSource, the backend answering groups after half a second
// and Leaves after one and a half. The regions and their closed ports come
// first; a port opened shows its Leaves pending ("…" in the band, loading
// cells) until they land, while the layer already counts them. The page's
// clock stands still unless a test runs it, so the backend answers at the
// test's pace.

import { expect, test, type Page } from "@playwright/test";
import { shipments } from "../demo/sources/shipments-data";

const HEADER = 28;
const ROW = 24;

const status = (page: Page) => page.locator("#status");
const queries = (page: Page) => page.locator("#tools .requests");
const en = (n: number) => n.toLocaleString("en");

const groupsBy = (field: "region") => new Set(shipments.map((s) => s[field])).size;
/** The rows as the demo opens them: each region's Group Item, its closed ports and its Subtotal, and the Grand Total. */
const closed = () => 2 * groupsBy("region") + new Set(shipments.map((s) => `${s.region}/${s.port}`)).size + 1;
const shipped = (region: string | null, port: string) => shipments.filter((s) => s.region === region && s.port === port).length;

/** Polls `read`, jumping the page's stopped clock ahead before each read. */
const clocked = <T,>(page: Page, read: () => Promise<T>, step = 250) =>
  expect.poll(
    async () => {
      await page.clock.fastForward(step);
      return read();
    },
    { intervals: [0], timeout: 20_000 },
  );

const text = async (page: Page, which: (page: Page) => ReturnType<Page["locator"]>) => (await which(page).textContent()) ?? "";

test("groups, then Leaves, arrive late from the slow backend, pending until they land", async ({ page }) => {
  await page.clock.install({ time: 0 });
  await page.clock.pauseAt(1000);
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&pace=slow&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await clocked(page, () => text(page, status)).toContain(`of ${en(closed())}`);
  await clocked(page, () => text(page, queries)).toMatch(/^0 queries in flight · \d+ held$/);
  await expect(status(page)).not.toContainText("loading");

  // Open the first port of the first region: the layer counts its Leaves at
  // once, which draw pending until the backend's late answer.
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + 24, b.y + HEADER + ROW + ROW / 2);
  const region = [...new Set(shipments.map((s) => s.region))].filter((r) => r !== null).sort()[0]!;
  const port = [...new Set(shipments.filter((s) => s.region === region).map((s) => s.port))].sort()[0]!;
  // Its Leaves and its Subtotal.
  const opened = closed() + shipped(region, port) + 1;
  await clocked(page, () => text(page, status), 100).toContain(`of ${en(opened)}`);
  await expect(status(page)).toContainText("loading");
  await expect(queries(page)).toContainText("1 queries in flight");

  await clocked(page, () => text(page, status)).not.toContain("loading");
  await expect(queries(page)).toContainText("0 queries in flight");
  await expect(status(page)).toContainText(`of ${en(opened)}`);
});

test("an edit asks the slow backend for every figure again, the rows staying where they are", async ({ page }) => {
  await page.clock.install({ time: 0 });
  await page.clock.pauseAt(1000);
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=shipments&pace=slow&theme=light");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await clocked(page, () => text(page, status)).toContain(`of ${en(closed())}`);
  await clocked(page, () => text(page, queries)).toMatch(/^0 queries in flight · \d+ held$/);

  // Open the first port and focus its first Leaf, once it lands.
  const b = await page.locator(".binnacle-layer").boundingBox();
  if (!b) throw new Error("no grid");
  await page.mouse.click(b.x + 24, b.y + HEADER + ROW + ROW / 2);
  await clocked(page, () => text(page, status)).not.toContain("loading");
  await page.mouse.click(b.x + 400, b.y + HEADER + 2 * ROW + ROW / 2);
  await clocked(page, () => text(page, status)).toContain("focus row 3");
  const before = await text(page, status);

  await page.getByRole("button", { name: "Reroute" }).click();
  await expect(page.locator("#committed")).toContainText("port of");
  // The figures are asked again at once, loading until the backend
  // answers, and nothing moves.
  await expect(queries(page)).not.toContainText(/^0 queries/);
  await clocked(page, () => text(page, status), 100).toContain("loading");
  await clocked(page, () => text(page, queries)).toMatch(/^0 queries in flight/);
  await clocked(page, () => text(page, status)).toBe(before);
});
