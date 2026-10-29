// Animated scrolls end to end: a smooth scrollBy, as a browser that animates
// the wheel runs one, reaches its full distance on the records, with a pinned
// column too, and on the huge source. A write-back landing mid-animation
// would cancel it short.

import { expect, test, type Page } from "@playwright/test";

const scroller = (page: Page) => page.locator(".binnacle-scroller");
const headers = (page: Page) => page.getByRole("grid").getByRole("columnheader");

const offsets = (page: Page) => scroller(page).evaluate((el) => ({ left: el.scrollLeft, top: el.scrollTop }));

/** Starts a smooth scroll by the given distance and waits for the offsets to stop moving. */
async function smoothScrollBy(page: Page, left: number, top: number) {
  await scroller(page).evaluate((el, { left, top }) => el.scrollBy({ left, top, behavior: "smooth" }), { left, top });
  let last = { left: NaN, top: NaN };
  await expect
    .poll(
      async () => {
        const now = await offsets(page);
        const still = now.left === last.left && now.top === last.top;
        last = now;
        return still;
      },
      { intervals: [200] },
    )
    .toBe(true);
  return last;
}

test.describe("records", () => {
  test.beforeEach(async ({ page }) => {
    // Narrow and short enough that both axes scroll.
    await page.setViewportSize({ width: 360, height: 500 });
    await page.goto("/?source=records&theme=light");
    await expect(headers(page).filter({ hasText: "crew" })).toHaveCount(1);
  });

  /** Scrolls smoothly across to 10 px short of the far end, and 600 px down. */
  async function nearlyAcross(page: Page) {
    const start = await smoothScrollBy(page, 0, 0);
    const left = (await scroller(page).evaluate((el) => el.scrollWidth - el.clientWidth)) - start.left - 10;
    expect(left).toBeGreaterThan(50);
    expect(await smoothScrollBy(page, left, 600)).toEqual({ left: start.left + left, top: start.top + 600 });
  }

  test("a smooth scroll reaches its full distance", async ({ page }) => {
    await nearlyAcross(page);
  });

  test("a smooth scroll reaches its full distance past a pinned column", async ({ page }) => {
    const band = (await page.locator(".binnacle-band-left").boundingBox())!;
    const top = (await page.locator(".binnacle-band-top").boundingBox())!;
    await page.mouse.click(band.x + band.width + 10, top.y + top.height + 10);
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Alt+Shift+ArrowLeft");
    await expect(headers(page).nth(1)).toHaveText("crew");
    // Pinning scrolls the focus into view; the scroll starts from there.
    await nearlyAcross(page);
  });
});

test("on the huge source, a smooth scroll reaches its full distance", async ({ page }) => {
  await page.goto("/?source=huge");
  const status = page.locator("#status");
  await expect(status).toContainText("rows 1 to");
  // Ten and a half rows of 24 px: Linux Firefox's animation can end a px short of
  // its target, and once still the capped sizer may place the offset a few px off.
  await smoothScrollBy(page, 0, 252);
  await expect(status).toContainText("rows 11 to");
});

test("on the huge source, a smooth scroll back from the end reaches its full distance", async ({ page }) => {
  await page.goto("/?source=huge");
  const status = page.locator("#status");
  await expect(status).toContainText("rows 1 to");
  await scroller(page).evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(status).toContainText("to 1,000,000,000,000,000 of");
  // Five and a half rows of 24 px. The animation's first step may be a single px, inside the end.
  await smoothScrollBy(page, 0, -132);
  await expect(status).toContainText("to 999,999,999,999,995 of");
});
