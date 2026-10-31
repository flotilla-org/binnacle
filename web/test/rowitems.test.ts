// Row Menu Items: which items a menu lists and what a choice reports. The
// logic is pure, so no DOM is needed.

import { expect, test } from "vitest";
import { hasRowItems, rowChoice, rowItemsFor, type ColumnMenuOptions } from "../src/rowitems";

const track = { name: "track", label: "Track" };
const options: ColumnMenuOptions<number> = {
  rowItems: (rowId) => (rowId === 7 ? [track, { name: "notes", label: "Notes" }] : []),
};

test("a row's menu lists the items the host gives for that row", () => {
  expect(rowItemsFor(options, "cols", 7, false).map((i) => i.name)).toEqual(["track", "notes"]);
  expect(rowItemsFor(options, "cols", 8, false)).toEqual([]);
});

test("a column's menu and slicer mode list none", () => {
  expect(rowItemsFor(options, "rows", 7, false)).toEqual([]);
  expect(rowItemsFor(options, "cols", 7, true)).toEqual([]);
  expect(rowItemsFor(undefined, "cols", 7, false)).toEqual([]);
});

test("a choice reports the item's name and the row's ID", () => {
  expect(rowChoice(track, 7)).toEqual({ name: "track", rowId: 7 });
});

test("a row with items opens a menu when nothing else would", () => {
  expect(hasRowItems(options, 7)).toBe(true);
  expect(hasRowItems(options, 8)).toBe(false);
  expect(hasRowItems(undefined, 7)).toBe(false);
});
