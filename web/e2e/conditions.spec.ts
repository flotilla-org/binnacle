// Column conditions end to end: a column header's funnel, Alt+ArrowDown and
// "+ Filter" open the column menu; contains and does-not-contain conditions
// AND with each other and with the quick filter, each shows as a chip whose
// × removes it, and filtered columns' funnels are drawn solid. The id column
// takes no conditions, and the slow source, which declares no support,
// offers no regex toggle and shows conditions pending until its answer.

import { expect, test, type Page } from "@playwright/test";
import { rows, WIDTHS, type RecordRow } from "../demo/sources/records-data";

const en = (n: number) => n.toLocaleString("en");

const bar = (page: Page) => page.getByRole("search");
const shown = (page: Page) => bar(page).locator(".binnacle-row-count");
const chips = (page: Page) => bar(page).getByRole("listitem");
const menu = (page: Page, label: string) => page.getByRole("dialog", { name: `Filter ${label}` });
const menuField = (page: Page, label: string) => menu(page, label).getByRole("textbox", { name: `${label} text` });

/** How many records have column `col`'s text satisfy every test. */
function count(...tests: [keyof RecordRow, (text: string) => boolean][]): number {
  return rows.filter((r) => tests.every(([col, t]) => t(r[col] == null ? "" : String(r[col])))).length;
}
const has = (s: string) => (t: string) => t !== "" && t.toLowerCase().includes(s);
const lacks = (s: string) => (t: string) => !has(s)(t);

/** The middle of column `col`'s funnel, in page coordinates, from the starting widths. */
async function funnel(page: Page, col: Exclude<keyof RecordRow, "port">) {
  const band = await page.locator(".binnacle-band-left").boundingBox();
  const top = await page.locator(".binnacle-band-top").boundingBox();
  if (!band || !top) throw new Error("no grid");
  const order = ["id", "name", "weight", "crew"] as const;
  let right = band.x + band.width;
  for (const c of order.slice(0, order.indexOf(col) + 1)) right += WIDTHS[c] + 1;
  return { x: right - 1 - 3 - 9, y: top.y + top.height / 2 };
}

test.describe("records", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records&theme=light");
    await expect(shown(page)).toHaveText(`${en(rows.length)} rows`);
  });

  test("a funnel opens the column menu, and contains and not-contains AND with the quick filter as chips", async ({
    page,
  }) => {
    const at = await funnel(page, "name");
    await page.mouse.click(at.x, at.y);
    await expect(menuField(page, "name")).toBeFocused();
    await page.keyboard.type("an");
    const named = count(["name", has("an")]);
    await expect(shown(page)).toHaveText(`${en(named)} of ${en(rows.length)}`);
    await expect(chips(page)).toHaveText(["name: an×"]);

    await page.keyboard.press("Escape");
    await expect(menu(page, "name")).toHaveCount(0);
    await bar(page).getByRole("button", { name: "+ Filter" }).click();
    const picker = page.getByRole("menu", { name: "Filter column" });
    await expect(picker.getByRole("menuitem")).toHaveText(["name", "weight", "crew", "port"]);
    await picker.getByRole("menuitem", { name: "port" }).click();
    await menu(page, "port").getByRole("combobox", { name: "Condition" }).selectOption("not");
    await menuField(page, "port").fill("bergen");
    const both = count(["name", has("an")], ["port", lacks("bergen")]);
    await expect(shown(page)).toHaveText(`${en(both)} of ${en(rows.length)}`);
    await expect(chips(page)).toHaveText(["name: an×", "port: not bergen×"]);
    await page.mouse.click(5, 300);
    await expect(menu(page, "port")).toHaveCount(0);

    await bar(page).getByRole("textbox", { name: "Filter rows" }).fill("4");
    const quick = rows.filter(
      (r) =>
        has("an")(r.name) &&
        lacks("bergen")(r.port ?? "") &&
        Object.values(r).some((v) => v != null && String(v).includes("4")),
    ).length;
    await expect(shown(page)).toHaveText(`${en(quick)} of ${en(rows.length)}`);

    await bar(page).getByRole("button", { name: "Remove filter name: an" }).click();
    await expect(chips(page)).toHaveText(["port: not bergen×"]);
    await bar(page).getByRole("textbox", { name: "Filter rows" }).fill("");
    await expect(shown(page)).toHaveText(`${en(count(["port", lacks("bergen")]))} of ${en(rows.length)}`);
  });

  test("Alt+ArrowDown opens the focused column's menu; regex and case toggles edit the condition; Clear removes it", async ({
    page,
  }) => {
    const band = await page.locator(".binnacle-band-left").boundingBox();
    const top = await page.locator(".binnacle-band-top").boundingBox();
    await page.mouse.click(band!.x + band!.width + WIDTHS.id + 1 + 20, top!.y + top!.height + 10);
    await page.keyboard.press("Alt+ArrowDown");
    await expect(menuField(page, "name")).toBeFocused();
    await menu(page, "name").getByRole("button", { name: "Use regular expression" }).click();
    await menuField(page, "name").fill("^an.*1$");
    await expect(shown(page)).toHaveText(`${en(count(["name", (t) => /^an.*1$/i.test(t)]))} of ${en(rows.length)}`);
    await expect(chips(page)).toHaveText(["name: /^an.*1$/×"]);
    await menu(page, "name").getByRole("button", { name: "Match case" }).click();
    await expect(chips(page)).toHaveText(["name: /^an.*1$/ (Aa)×"]);
    await expect(shown(page)).toHaveText(`${en(count(["name", (t) => /^an.*1$/.test(t)]))} of ${en(rows.length)}`);
    await menu(page, "name").getByRole("button", { name: "Clear" }).click();
    await expect(chips(page)).toHaveCount(0);
    await expect(shown(page)).toHaveText(`${en(rows.length)} rows`);
    await page.keyboard.press("Escape");
    await expect(menu(page, "name")).toHaveCount(0);
    // Focus goes back to the grid, where Alt+ArrowDown opens the menu again.
    await page.keyboard.press("Alt+ArrowDown");
    await expect(menuField(page, "name")).toBeFocused();
  });

  test("the id column draws no funnel and opens no menu", async ({ page }) => {
    const band = await page.locator(".binnacle-band-left").boundingBox();
    const top = await page.locator(".binnacle-band-top").boundingBox();
    const at = await funnel(page, "id");
    await page.mouse.click(at.x, at.y);
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await page.mouse.click(band!.x + band!.width + 20, top!.y + top!.height + 10);
    await page.keyboard.press("Alt+ArrowDown");
    await expect(page.getByRole("dialog")).toHaveCount(0);
  });

  test.describe("screenshots", () => {
    test.skip(({ browserName }) => browserName !== "chromium", "screenshots are taken in Chromium only");

    test("the column menu over filtered headers", async ({ page }) => {
      await bar(page).getByRole("button", { name: "+ Filter" }).click();
      await page.getByRole("menuitem", { name: "crew" }).click();
      await menuField(page, "crew").fill("1");
      await page.keyboard.press("Escape");
      const at = await funnel(page, "name");
      await page.mouse.click(at.x, at.y);
      await menu(page, "name").getByRole("combobox", { name: "Condition" }).selectOption("not");
      await menuField(page, "name").fill("an");
      await expect(chips(page)).toHaveText(["crew: 1×", "name: not an×"]);
      await expect(menu(page, "name").locator(".binnacle-column-menu-notice")).not.toHaveText(/Loading/);
      await expect(page).toHaveScreenshot("column-menu.png");
    });

    test("chips in the filter bar", async ({ page }) => {
      await bar(page).getByRole("button", { name: "+ Filter" }).click();
      await page.getByRole("menuitem", { name: "port" }).click();
      await menuField(page, "port").fill("bergen");
      await page.keyboard.press("Escape");
      await bar(page).getByRole("button", { name: "+ Filter" }).click();
      await page.getByRole("menuitem", { name: "name" }).click();
      await menu(page, "name").getByRole("combobox", { name: "Condition" }).selectOption("not");
      await menuField(page, "name").fill("an");
      await page.keyboard.press("Escape");
      await expect(chips(page)).toHaveText(["port: bergen×", "name: not an×"]);
      await page.mouse.move(0, 0);
      await expect(page.locator(".with-filter-bar")).toHaveScreenshot("chips.png");
    });
  });
});

test("a source that declares no support takes contains without regexes, and its chips wait for the answer", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.goto("/?source=slow&pace=brisk");
  await expect(page.locator("#status")).toContainText("rows 1 to");
  await bar(page).getByRole("button", { name: "+ Filter" }).click();
  await page.getByRole("menuitem", { name: "port" }).click();
  await expect(menu(page, "port").getByRole("button", { name: "Match case" })).toBeVisible();
  await expect(menu(page, "port").getByRole("button", { name: "Use regular expression" })).toBeHidden();
  await menuField(page, "port").fill("bergen");
  const chip = chips(page).first();
  await expect(chip).toHaveAttribute("aria-busy", "true");
  await expect(chip).not.toHaveAttribute("aria-busy");
  await expect(page.locator("#status")).toContainText(`of ${en(count(["port", has("bergen")]))}`);
});
