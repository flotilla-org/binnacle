// Dragging headers across the pinned edge through the wasm Grid and the real
// painter: an empty pinned Strip's pin zone lights up, the drop line carries
// a caption saying what a drop does, and is dashed where an unpinned item
// returns to its own place.

import { beforeAll, expect, test } from "vitest";
import { light, type TableSource, type Theme } from "../src";
import { createCanvasPainter } from "../src/canvas/painter";
import { dropCaption } from "../src/canvas/drop";
import { AXIS_CODE, EDGE_CODE, POINTER } from "../src/codes";
import { pinnedKeys } from "../src/pin";
import { fakeDocument, type DrawCall } from "./canvas";
import { BAND, COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

const calls: DrawCall[] = [];

beforeAll(async () => {
  await loadForTests();
  fakeDocument(calls);
});

const table = (rows: number, cols: number): TableSource<number, number, string> => ({
  rows: { identity: "position", count: () => rows, window: (s, e) => Array.from({ length: e - s }, (_, i) => s + i) },
  cols: { identity: "position", count: () => cols, window: (s, e) => Array.from({ length: e - s }, (_, i) => s + i) },
  cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}:${c}`)),
});

const ZONE = 24;
const theme: Theme = { ...light, pinZone: "#ff00ff", dropIndicator: { color: "#00ff00", width: 2 } };

/** A 30 by 20 table whose columns can't move, with pin zones of 24 px. */
function grid(): Harness<number, number, string> {
  const h = new Harness(table(30, 20));
  h.grid.set_pin_zone(AXIS_CODE.cols, ZONE);
  h.grid.set_pin_zone(AXIS_CODE.rows, ZONE);
  h.frame();
  return h;
}

/** Presses the column header at `from` and holds it dragged to `to`. */
function drag(h: Harness<number, number, string>, from: number, to: number): void {
  h.grid.pointer(POINTER.down, from, HEADER / 2, 0, 0, 1);
  h.grid.pointer(POINTER.move, to, HEADER / 2, 0, 0, 1);
}

/** Paints the latest frame of `h` and returns what was drawn. */
function paint(h: Harness<number, number, string>): DrawCall[] {
  const s = h.frame();
  const painter = createCanvasPainter(theme);
  painter.resize(h.width, h.height, 1);
  calls.length = 0;
  painter.paint(s, h.feed.content(s));
  return [...calls];
}

const fills = (drawn: DrawCall[], colour: string) =>
  drawn.filter((c) => c.name === "fillRect" && c.fill === colour).map((c) => c.args);
const texts = (drawn: DrawCall[]) => drawn.filter((c) => c.name === "fillText").map((c) => c.args[0]);

test("dragging a column header lights both empty pin zones, the hot one twice, and the caption says pin left", () => {
  const h = grid();
  try {
    drag(h, BAND + 2 * COL_WIDTH + 40, BAND + 10);
    const drawn = paint(h);
    expect(h.frame().dropIndicator?.target).toEqual({ type: "Pin", edge: "leading", before: null, reorder: false });
    expect(fills(drawn, theme.pinZone)).toEqual([
      [0, 0, BAND + ZONE, h.height],
      [0, 0, BAND + ZONE, h.height],
      [h.width - ZONE, 0, ZONE, h.height],
    ]);
    expect(texts(drawn)).toContain("Pin left");
    h.grid.pointer(POINTER.up, BAND + 10, HEADER / 2, 0, 0, 1);
    h.drain();
    expect(pinnedKeys(h.frame(), "cols", "leading").map((k) => h.feed.colId(k))).toEqual([2]);
    expect(fills(paint(h), theme.pinZone)).toEqual([]);
  } finally {
    h.destroy();
  }
});

test("a pin dragged out where the columns can't move unpins with a dashed line at its own place", () => {
  const h = grid();
  try {
    h.grid.pin(AXIS_CODE.cols, EDGE_CODE.leading, h.feed.colKey(3));
    h.drain();
    h.frame();
    // Column 3 pinned at 40 to 120; columns 0 to 2 scroll from 120 on.
    drag(h, BAND + 40, 400);
    const drawn = paint(h);
    expect(h.frame().dropIndicator).toMatchObject({ target: { type: "Unpin", at: 3 }, offset: 360 });
    const line = fills(drawn, theme.dropIndicator.color);
    // Dashes 6 long every 10 px down the grid, centred on its own place.
    expect(line.length).toBe(Math.ceil(h.height / 10));
    expect(line[0]).toEqual([359, 0, 2, 6]);
    expect(texts(drawn)).toContain("Unpin: returns here");
    h.grid.pointer(POINTER.up, 400, HEADER / 2, 0, 0, 1);
    h.drain();
    expect(h.frame().cols.pinned).toBe(0);
  } finally {
    h.destroy();
  }
});

test("over the body of an unmovable axis a drag drops nothing and draws no line or caption", () => {
  const h = grid();
  try {
    drag(h, BAND + 40, BAND + 3 * COL_WIDTH + 40);
    const drawn = paint(h);
    expect(h.frame().mode).toMatchObject({ type: "MovingHeaders", target: { type: "Stay" } });
    expect(h.frame().dropIndicator).toBeNull();
    expect(fills(drawn, theme.dropIndicator.color)).toEqual([]);
    expect(fills(drawn, theme.pinZone)).toHaveLength(2);
  } finally {
    h.destroy();
  }
});

test("a row header dragged above the first row pins it top", () => {
  const h = grid();
  try {
    const y = HEADER + 4 * ROW_HEIGHT + 10;
    h.grid.pointer(POINTER.down, 10, y, 0, 0, 1);
    h.grid.pointer(POINTER.move, 10, HEADER + 5, 0, 0, 1);
    expect(texts(paint(h))).toContain("Pin top");
    h.grid.pointer(POINTER.up, 10, HEADER + 5, 0, 0, 1);
    h.drain();
    expect(pinnedKeys(h.frame(), "rows", "leading").map((k) => h.feed.rowId(k))).toEqual([4]);
  } finally {
    h.destroy();
  }
});

test("each drop target has its caption on either axis", () => {
  expect(dropCaption("cols", { type: "Move", to: 3, unpin: false })).toBe("Move here");
  expect(dropCaption("rows", { type: "Move", to: 3, unpin: true })).toBe("Unpin and move here");
  expect(dropCaption("cols", { type: "Pin", edge: "trailing", before: null, reorder: false })).toBe("Pin right");
  expect(dropCaption("rows", { type: "Pin", edge: "trailing", before: 4n, reorder: false })).toBe("Pin bottom");
  expect(dropCaption("rows", { type: "Pin", edge: "leading", before: 4n, reorder: true })).toBe("Reorder the pins");
  expect(dropCaption("cols", { type: "Stay" })).toBeNull();
});
