// Pinning through the wasm Grid and the real frame: a pinned column draws
// after the row band whatever the scroll, the scrolling Strip skips it, and
// the painter draws the edge in the Theme's `pinnedEdge` with a shadow once
// the body has scrolled under it.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, expect, test } from "vitest";
import { light, type TableSource, type Theme } from "../src";
import { createCanvasPainter } from "../src/canvas/painter";
import { AXIS_CODE, EDGE_CODE, type PinEdge } from "../src/codes";
import { freezeFocused, pinFocused, pinnedKeys } from "../src/pin";
import { shownItems, type Snapshot } from "../src/snapshot";
import { fakeDocument, type DrawCall } from "./canvas";
import { BAND, COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

const calls: DrawCall[] = [];

beforeAll(async () => {
  await loadForTests();
  fakeDocument(calls);
});

type Source = TableSource<number, number, string>;

const table = (rows: number, cols: number): Source => ({
  rows: { identity: "position", count: () => rows, window: (s, e) => Array.from({ length: e - s }, (_, i) => s + i) },
  cols: { identity: "position", count: () => cols, window: (s, e) => Array.from({ length: e - s }, (_, i) => s + i) },
  cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}:${c}`)),
});

const GAP = 4;

/** A 30 by 20 table, columns 4 px apart, with column 6 pinned. */
function pinned(): Harness<number, number, string> {
  const h = new Harness(table(30, 20));
  h.grid.set_gap(AXIS_CODE.cols, GAP);
  h.frame();
  h.grid.pin(AXIS_CODE.cols, EDGE_CODE.leading, h.feed.colKey(6));
  h.drain();
  h.frame();
  return h;
}

const positions = (s: Snapshot) => shownItems(s.cols).map((i) => i.position);

test("a pinned column draws after the row band, and the scrolling columns skip it", () => {
  const h = pinned();
  try {
    const s = h.frame();
    expect(s.cols.pinned).toBe(1);
    expect(s.cols.strip[0]).toMatchObject({ position: 6, offset: BAND, size: COL_WIDTH });
    expect(s.cols.scrolling.start).toBe(BAND + COL_WIDTH + GAP);
    expect(positions(s)).toEqual([6, 0, 1, 2, 3, 4]);
  } finally {
    h.destroy();
  }
});

/** A 30 by 20 table, columns 4 px apart, with column 6 pinned right. */
function pinnedRight(): Harness<number, number, string> {
  const h = new Harness(table(30, 20));
  h.grid.set_gap(AXIS_CODE.cols, GAP);
  h.frame();
  h.grid.pin(AXIS_CODE.cols, EDGE_CODE.trailing, h.feed.colKey(6));
  h.drain();
  h.frame();
  return h;
}

test("a column pinned right draws flush with the far edge, and the scrolling columns stop short of it", () => {
  const h = pinnedRight();
  try {
    const s = h.frame();
    expect(s.cols.trailing).toBe(1);
    expect(s.cols.strip.at(-1)).toMatchObject({ position: 6, offset: h.width - COL_WIDTH, size: COL_WIDTH });
    expect(s.cols.scrolling.end).toBe(h.width - COL_WIDTH - GAP);
    // Column 4 starts at 376, under the scrolling end at 416; column 5 at 460 is under the pin.
    expect(positions(s)).toEqual([0, 1, 2, 3, 4, 6]);
  } finally {
    h.destroy();
  }
});

test("an End Gap keeps room past the last column scrolled to the end, and past a column pinned right", () => {
  const h = new Harness(table(30, 20));
  try {
    h.grid.set_end_gap(AXIS_CODE.cols, 6);
    h.grid.scroll_to(AXIS_CODE.cols, 1);
    let s = h.frame();
    expect(s.cols.strip.at(-1)).toMatchObject({ position: 19, size: COL_WIDTH });
    expect(s.cols.strip.at(-1)!.offset + COL_WIDTH).toBe(h.width - 6);
    h.grid.pin(AXIS_CODE.cols, EDGE_CODE.trailing, h.feed.colKey(6));
    h.drain();
    s = h.frame();
    expect(s.cols.strip.at(-1)).toMatchObject({ position: 6, offset: h.width - 6 - COL_WIDTH });
    expect(s.cols.regions.trailing.end).toBe(h.width - 6);
  } finally {
    h.destroy();
  }
});

test("across pins at either edge, unpins and scrolls, the pinned columns lead and trail in pin order and the scrolling ones lie between in Position order, none twice", () =>
  hegel.test((tc) => {
    const cols = 20;
    const h = new Harness(table(30, cols));
    try {
      h.grid.set_gap(AXIS_CODE.cols, GAP);
      h.frame();
      const model: Record<PinEdge, number[]> = { leading: [], trailing: [] };
      const drop = (c: number) => {
        for (const edge of ["leading", "trailing"] as const) model[edge] = model[edge].filter((x) => x !== c);
      };
      type Op = { pin: number; edge: PinEdge } | { unpin: number } | { dx: number };
      const col = gs.integers({ minValue: 0, maxValue: cols - 1 });
      const ops = gs.arrays(
        gs.oneOf<Op>(
          gs.tuples(col, gs.sampledFrom<PinEdge>(["leading", "trailing"])).map(([c, edge]) => ({ pin: c, edge })),
          col.map((c) => ({ unpin: c })),
          gs.integers({ minValue: -600, maxValue: 600 }).map((dx) => ({ dx })),
        ),
        { maxSize: 12 },
      );
      for (const op of tc.draw(ops)) {
        if ("pin" in op) {
          h.grid.pin(AXIS_CODE.cols, EDGE_CODE[op.edge], h.feed.colKey(op.pin));
          drop(op.pin);
          model[op.edge].push(op.pin);
        } else if ("unpin" in op) {
          h.grid.unpin(AXIS_CODE.cols, h.feed.colKey(op.unpin));
          drop(op.unpin);
        } else {
          h.grid.scroll(op.dx, 0);
        }
        h.drain();
        const s = h.frame();
        tc.note(`${JSON.stringify(op)}: pinned ${JSON.stringify(model)}, shown ${positions(s)}`);
        const lead = s.cols.strip.slice(0, s.cols.pinned);
        const trail = s.cols.strip.slice(s.cols.strip.length - s.cols.trailing);
        expect(lead.map((i) => i.position)).toEqual(model.leading);
        expect(trail.map((i) => i.position)).toEqual(model.trailing);
        let at = BAND;
        for (const item of lead) {
          expect(item.offset).toBe(at);
          at += item.size + GAP;
        }
        at = h.width;
        for (const item of [...trail].reverse()) {
          expect(item.offset + item.size).toBe(at);
          at = item.offset - GAP;
        }
        const pinnedAll = [...model.leading, ...model.trailing];
        const shown = positions(s);
        const scrolling = shown.slice(model.leading.length, shown.length - model.trailing.length);
        const between = (a: number, b: number) =>
          Array.from({ length: b - a + 1 }, (_, i) => a + i).filter((p) => !pinnedAll.includes(p));
        if (scrolling.length > 0) {
          // The first shown is the first unpinned item from the Scroll
          // Anchor, or the next once the anchor's item has scrolled into its
          // Gap: what scrolled under the pinned Strip is hidden.
          const { position, offset } = s.cols.anchor;
          const from = between(position, cols - 1);
          const gone = from[0] === position && offset >= COL_WIDTH;
          expect(scrolling[0]).toBe(from[gone ? 1 : 0]);
          expect(scrolling).toEqual(between(scrolling[0]!, scrolling.at(-1)!));
        }
      }
    } finally {
      h.destroy();
    }
  }));

test("Pin toggles the focused column and Unpin unpins it", () => {
  const h = new Harness(table(30, 20));
  try {
    let s = h.frame();
    h.grid.set_focus(h.feed.rowKey(0), h.feed.colKey(3));
    s = h.frame();
    pinFocused(h.grid, s, "cols", "leading");
    s = h.frame();
    expect(pinnedKeys(s, "cols", "leading")).toEqual([h.feed.colKey(3)]);
    pinFocused(h.grid, s, "cols", "leading");
    s = h.frame();
    expect(pinnedKeys(s, "cols", "leading")).toEqual([]);
    pinFocused(h.grid, s, "cols", "leading");
    s = h.frame();
    pinFocused(h.grid, s, "cols", null);
    expect(pinnedKeys(h.frame(), "cols", "leading")).toEqual([]);
  } finally {
    h.destroy();
  }
});

test("Freeze pins the columns from the start up to the focused one, as many as fit, and on a pinned one unpins those after it", () => {
  const h = new Harness(table(30, 20));
  try {
    h.grid.set_gap(AXIS_CODE.cols, GAP);
    h.frame();
    const freeze = (col: number) => {
      h.grid.set_focus(h.feed.rowKey(0), h.feed.colKey(col));
      freezeFocused(h.grid, h.frame(), "cols");
      h.drain();
      return pinnedKeys(h.frame(), "cols", "leading").map((k) => h.feed.colId(k));
    };
    expect(freeze(2)).toEqual([0, 1, 2]);
    // Four columns end at 372, leaving 84 px for one more to scroll in;
    // a fifth would leave 44.
    expect(freeze(6)).toEqual([0, 1, 2, 3]);
    expect(freeze(1)).toEqual([0, 1]);
  } finally {
    h.destroy();
  }
});

const theme: Theme = { ...light, pinnedEdge: "#ff00ff", pinnedShadow: "#00ff00" };

/** The rects painted in `colour` over the latest frame of `h`. */
function painted(h: Harness<number, number, string>, colour: string) {
  const s = h.frame();
  const painter = createCanvasPainter(theme);
  painter.resize(h.width, h.height, 1);
  calls.length = 0;
  painter.paint(s, h.feed.content(s));
  return calls.filter((c) => c.name === "fillRect" && c.fill === colour).map((c) => c.args);
}

test("the edge is a line in the pinnedEdge colour, in the Gap after the pinned column, down the whole grid", () => {
  const h = pinned();
  try {
    // Centred in the Gap from 120 to 124, snapped to a device pixel.
    expect(painted(h, theme.pinnedEdge)).toEqual([[122, 0, 1, h.height]]);
  } finally {
    h.destroy();
  }
});

test("the shadow falls on the scrolling side only once the body has scrolled", () => {
  const h = pinned();
  try {
    const gradients = () => calls.filter((c) => c.name === "createLinearGradient");
    painted(h, theme.pinnedEdge);
    expect(gradients()).toEqual([]);
    h.grid.scroll(30, 0);
    h.drain();
    painted(h, theme.pinnedEdge);
    const start = BAND + COL_WIDTH + GAP;
    expect(gradients().map((c) => c.args)).toEqual([[start, 0, start + 8, 0]]);
  } finally {
    h.destroy();
  }
});

test("a right edge is a line in the Gap before the column pinned right, with a shadow on the scrolling side until the body has scrolled to its end", () => {
  const h = pinnedRight();
  try {
    const gradients = () => calls.filter((c) => c.name === "createLinearGradient").map((c) => c.args);
    // Centred in the Gap from 416 to 420.
    expect(painted(h, theme.pinnedEdge)).toEqual([[418, 0, 1, h.height]]);
    expect(gradients()).toEqual([[416, 0, 408, 0]]);
    h.grid.scroll(1_000_000, 0);
    h.drain();
    painted(h, theme.pinnedEdge);
    expect(gradients()).toEqual([]);
  } finally {
    h.destroy();
  }
});

test("rows pinned top and bottom draw edges across the grid in the Gaps beside them", () => {
  const h = new Harness(table(30, 20));
  try {
    h.grid.set_gap(AXIS_CODE.rows, GAP);
    h.frame();
    h.grid.pin(AXIS_CODE.rows, EDGE_CODE.leading, h.feed.rowKey(9));
    h.grid.pin(AXIS_CODE.rows, EDGE_CODE.trailing, h.feed.rowKey(3));
    h.drain();
    const s = h.frame();
    expect(s.rows.strip[0]).toMatchObject({ position: 9, offset: HEADER });
    expect(s.rows.strip.at(-1)).toMatchObject({ position: 3, offset: h.height - ROW_HEIGHT });
    // After row 9 (24 to 44) the Gap runs to 48; before row 3 (280 to 300) it starts at 276.
    expect(painted(h, theme.pinnedEdge)).toEqual([
      [0, 46, h.width, 1],
      [0, 278, h.width, 1],
    ]);
  } finally {
    h.destroy();
  }
});

/** Columns 0 to 4 pinned left and 9 right, 50 px kept to scroll: too wide for the 500 px grid. */
function overfull(): Harness<number, number, string> {
  const h = new Harness(table(30, 20));
  h.grid.set_gap(AXIS_CODE.cols, GAP);
  h.grid.set_min_scrolling(AXIS_CODE.cols, 50);
  h.frame();
  for (const c of [0, 1, 2, 3, 4]) h.grid.pin(AXIS_CODE.cols, EDGE_CODE.leading, h.feed.colKey(c));
  h.grid.pin(AXIS_CODE.cols, EDGE_CODE.trailing, h.feed.colKey(9));
  h.drain();
  h.frame();
  return h;
}

test("too wide, the pinned Strips clip at their inside ends, the trailing one first, and nothing unpins", () => {
  const h = overfull();
  try {
    const s = h.frame();
    expect(s.cols.pinned).toBe(5);
    expect(s.cols.trailing).toBe(1);
    // 410 px for the pins and their Gaps: column 4 (376 to 456) clips at 442, column 9 wholly.
    expect(s.cols.regions).toEqual({ leading: { start: BAND, end: 442 }, trailing: { start: 500, end: 500 } });
    expect(s.cols.scrolling).toEqual({ start: 446, end: 496 });
    expect(positions(s)).toEqual([0, 1, 2, 3, 4, 5]);
    expect(pinnedKeys(s, "cols", "trailing")).toEqual([h.feed.colKey(9)]);
    // The edges sit where the Strips clip.
    expect(painted(h, theme.pinnedEdge)).toEqual([
      [444, 0, 1, h.height],
      [498, 0, 1, h.height],
    ]);
  } finally {
    h.destroy();
  }
});

test("focus on a column the trailing Strip clips makes room for it", () => {
  const h = overfull();
  try {
    h.grid.set_focus(h.feed.rowKey(2), h.feed.colKey(9));
    h.drain();
    const s = h.frame();
    expect(s.cols.regions.trailing).toEqual({ start: 420, end: 500 });
    expect(s.cols.regions.leading).toEqual({ start: BAND, end: 362 });
    // Column 4 (376 to 456) is now clipped whole, and column 3 in part.
    expect(positions(s)).toEqual([0, 1, 2, 3, 5, 9]);
    expect(s.focusRect).toMatchObject({ x: 420, width: COL_WIDTH });
  } finally {
    h.destroy();
  }
});
