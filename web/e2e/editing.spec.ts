// Editing the records source in place: typing, F2, the commit and cancel
// keys, clicking away, IME composition (driven over CDP in Chromium), the
// port column's custom editor, and a read-only grid ignoring it all. The
// demo's `#committed` line shows what `onCommit` received, and the textarea
// is the editor, so its value is what the user sees in the cell.

import { expect, test, type Page } from "@playwright/test";
import { rows } from "../demo/sources/records-data";
import { column } from "./records-columns";

const HEADER = 28;
const ROW = 24;
const COLS = ["id", "name", "weight", "crew", "port"] as const;

const status = (page: Page) => page.locator("#status");
const committed = (page: Page) => page.locator("#committed");
const editor = (page: Page) => page.locator(".binnacle-input");

/** Clicks the visible cell at screen row `row` (0 first) of column `col`. */
async function cell(page: Page, row: number, col: number) {
  const box = await page.locator(".binnacle-layer").boundingBox();
  if (!box) throw new Error("no grid");
  await page.mouse.click(box.x + column(col), box.y + HEADER + row * ROW + ROW / 2);
}

const focus = (row: number, col: number) =>
  `focus row ${row + 1}, column ${col + 1} (${row} / ${COLS[col]})`;

async function expectEditing(page: Page, on: boolean) {
  if (on) await expect(status(page)).toContainText("editing");
  else await expect(status(page)).not.toContainText("editing");
}

test.describe("the records source", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 600 });
    await page.goto("/?source=records");
    await expect(status(page)).toContainText(`of ${rows.length.toLocaleString("en")}`);
  });

  test("typing on a focused cell starts an edit with the typed text, and Enter commits it", async ({ page }) => {
    await cell(page, 0, 1);
    await expect(status(page)).toContainText(focus(0, 1));
    await page.keyboard.type("Zed");
    await expectEditing(page, true);
    await expect(editor(page)).toHaveValue("Zed");
    await page.keyboard.press("Enter");
    await expect(committed(page)).toHaveText('name of 0 = "Zed"');
    await expectEditing(page, false);
    await expect(status(page)).toContainText(focus(0, 1));
    // The committed value is the cell's value now.
    await page.keyboard.press("F2");
    await expect(editor(page)).toHaveValue("Zed");
  });

  test("F2 then fast typing loses no keystroke", async ({ page }) => {
    await cell(page, 1, 1);
    await page.keyboard.press("F2");
    await page.keyboard.type(" and fast typing");
    await page.keyboard.press("Enter");
    await expect(committed(page)).toHaveText(`name of 1 = "${rows[1]!.name} and fast typing"`);
  });

  test("Escape cancels: nothing is committed and the cell keeps its value", async ({ page }) => {
    await cell(page, 2, 1);
    await page.keyboard.type("nope");
    await expect(editor(page)).toHaveValue("nope");
    await page.keyboard.press("Escape");
    await expectEditing(page, false);
    await expect(committed(page)).toHaveText("");
    await page.keyboard.press("F2");
    await expect(editor(page)).toHaveValue(rows[2]!.name);
  });

  test("Tab commits and moves right, Shift+Tab commits and moves left", async ({ page }) => {
    await cell(page, 0, 2);
    await page.keyboard.type("12.5");
    await page.keyboard.press("Tab");
    await expect(committed(page)).toHaveText("weight of 0 = 12.5");
    await expect(status(page)).toContainText(focus(0, 3));
    await page.keyboard.type("7");
    await page.keyboard.press("Shift+Tab");
    await expect(committed(page)).toHaveText("crew of 0 = 7");
    await expect(status(page)).toContainText(focus(0, 2));
    // Not editing, Shift+Tab leaves the grid for the filter bar above it.
    await page.keyboard.press("Shift+Tab");
    await expect(page.getByRole("search").locator(":focus")).toHaveCount(1);
  });

  test("clicking another cell commits, and so does clicking outside the grid", async ({ page }) => {
    await cell(page, 3, 1);
    await page.keyboard.type("Ahoy");
    await cell(page, 4, 1);
    await expect(committed(page)).toHaveText('name of 3 = "Ahoy"');
    await expect(status(page)).toContainText(focus(4, 1));
    await page.keyboard.type("Avast");
    await page.getByRole("searchbox", { name: "Filter records" }).click();
    await expect(committed(page)).toHaveText('name of 4 = "Avast"');
    await expectEditing(page, false);
  });

  test("a value that does not fit its column is not kept, not even for a moment", async ({ page }) => {
    await cell(page, 1, 3);
    await page.keyboard.type("lots");
    await expect(editor(page)).toHaveValue("lots");
    // No frame runs from here, so the source answers nothing until F2 has opened the editor.
    await page.clock.install();
    await page.clock.pauseAt(Date.now() + 60_000);
    await page.keyboard.press("Enter");
    await expect(committed(page)).toHaveText("crew of 1 kept");
    await page.keyboard.press("F2");
    await expect(editor(page)).toHaveValue(String(rows[1]!.crew));
  });

  test("the port column edits with a list, its own editor over the cell", async ({ page }) => {
    await cell(page, 1, 4);
    await page.keyboard.press("F2");
    const picker = page.getByRole("combobox", { name: "Port" });
    await expect(picker).toBeFocused();
    await expect(picker).toHaveValue(rows[1]!.port ?? "");
    await picker.selectOption("Bergen");
    await page.keyboard.press("Enter");
    await expect(committed(page)).toHaveText('port of 1 = "Bergen"');
    await expect(picker).toHaveCount(0);
    // DOM focus is back with the grid, so keys move focus again.
    await page.keyboard.press("ArrowDown");
    await expect(status(page)).toContainText(focus(2, 4));
    // Typing opens the list too, and Escape closes it without committing.
    await page.keyboard.type("x");
    await expect(picker).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(picker).toHaveCount(0);
    await expect(committed(page)).toHaveText('port of 1 = "Bergen"');
  });

  test("an edit that changes the sorted column re-sorts, and focus follows the record", async ({ page }) => {
    const box = (await page.locator(".binnacle-layer").boundingBox())!;
    // Sort by weight, then give the top record the heaviest weight.
    await page.mouse.click(box.x + column(2), box.y + HEADER / 2);
    await cell(page, 0, 2);
    await expect(status(page)).toContainText("focus row 1, column 3");
    const id = Number((await status(page).textContent())!.match(/\((\d+) \//)![1]);
    await page.keyboard.type("100000");
    await page.keyboard.press("Enter");
    await expect(committed(page)).toHaveText(`weight of ${id} = 100000`);
    const last = rows.filter((r) => r.weight !== null).length;
    await expect(status(page)).toContainText(`focus row ${last.toLocaleString("en")}, column 3 (${id} / weight)`);
  });

  test("composition starts an edit in place and its text is committed", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "composition is driven over CDP");
    await cell(page, 0, 1);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Input.imeSetComposition", { text: "に", selectionStart: 1, selectionEnd: 1 });
    await expectEditing(page, true);
    await expect(editor(page)).toHaveValue("に");
    await cdp.send("Input.imeSetComposition", { text: "にほ", selectionStart: 2, selectionEnd: 2 });
    await cdp.send("Input.insertText", { text: "日本" });
    await expect(editor(page)).toHaveValue("日本");
    await page.keyboard.press("Enter");
    await expect(committed(page)).toHaveText('name of 0 = "日本"');
  });
});

test("a read-only grid ignores typing, F2, Enter and composition", async ({ page, browserName }) => {
  await page.goto("/?source=huge");
  await expect(status(page)).toContainText("rows 1 to");
  const box = (await page.locator(".binnacle-layer").boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + HEADER + ROW * 2 + ROW / 2);
  await expect(status(page)).toContainText("focus row 3,");
  await page.keyboard.type("abc");
  await page.keyboard.press("F2");
  await page.keyboard.press("Enter");
  if (browserName === "chromium") {
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Input.imeSetComposition", { text: "に", selectionStart: 1, selectionEnd: 1 });
    await cdp.send("Input.insertText", { text: "日本" });
  }
  await page.keyboard.press("ArrowDown");
  await expect(status(page)).toContainText("focus row 4,");
  await expectEditing(page, false);
  await expect(editor(page)).toHaveValue("");
});
