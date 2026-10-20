// Pinning through the wasm Grid and the real frame: a pinned column draws
// after the row band whatever the scroll, the scrolling Strip skips it, and
// the painter draws the edge in the Theme's `pinnedEdge` with a shadow once
// the body has scrolled under it.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, expect, test } from "vitest";
import { light, type TableSource, type Theme } from "../src";
import { createCanvasPainter } from "../src/canvas/painter";
import { AXIS_CODE, EDGE_CODE } from "../src/codes";
import { pinFocused, pinnedKeys } from "../src/pin";
import { shownItems, type Snapshot } from "../src/snapshot";
import { fakeDocument, type DrawCall } from "./canvas";
import { BAND, COL_WIDTH, Harness } from "./harness";
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

test("across pins, unpins and scrolls, the pinned columns lead in pin order and the scrolling ones follow in Position order, none twice", () =>
  hegel.test((tc) => {
    const cols = 20;
    const h = new Harness(table(30, cols));
    try {
      h.grid.set_gap(AXIS_CODE.cols, GAP);
      h.frame();
      const model: number[] = [];
      type Op = { pin: number } | { unpin: number } | { dx: number };
      const col = gs.integers({ minValue: 0, maxValue: cols - 1 });
      const ops = gs.arrays(
        gs.oneOf<Op>(
          col.map((c) => ({ pin: c })),
          col.map((c) => ({ unpin: c })),
          gs.integers({ minValue: -600, maxValue: 600 }).map((dx) => ({ dx })),
        ),
        { maxSize: 12 },
      );
      for (const op of tc.draw(ops)) {
        if ("pin" in op) {
          h.grid.pin(AXIS_CODE.cols, EDGE_CODE.leading, h.feed.colKey(op.pin));
          model.splice(0, model.length, ...model.filter((c) => c !== op.pin), op.pin);
        } else if ("unpin" in op) {
          h.grid.unpin(AXIS_CODE.cols, h.feed.colKey(op.unpin));
          model.splice(0, model.length, ...model.filter((c) => c !== op.unpin));
        } else {
          h.grid.scroll(op.dx, 0);
        }
        h.drain();
        const s = h.frame();
        tc.note(`${JSON.stringify(op)}: pinned ${model}, shown ${positions(s)}`);
        const lead = s.cols.strip.slice(0, s.cols.pinned);
        expect(lead.map((i) => i.position)).toEqual(model);
        let at = BAND;
        for (const item of lead) {
          expect(item.offset).toBe(at);
          at += item.size + GAP;
        }
        const scrolling = positions(s).slice(model.length);
        const between = (a: number, b: number) =>
          Array.from({ length: b - a + 1 }, (_, i) => a + i).filter((p) => !model.includes(p));
        if (scrolling.length > 0) {
          // The first shown is at the Scroll Anchor, or past it when the
          // anchor's Position is pinned: what scrolled under the pinned Strip is hidden.
          expect(scrolling[0]).toBe(between(s.cols.anchor.position, cols - 1)[0]);
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
    expect(pinnedKeys(s, "cols")).toEqual([h.feed.colKey(3)]);
    pinFocused(h.grid, s, "cols", "leading");
    s = h.frame();
    expect(pinnedKeys(s, "cols")).toEqual([]);
    pinFocused(h.grid, s, "cols", "leading");
    s = h.frame();
    pinFocused(h.grid, s, "cols", null);
    expect(pinnedKeys(h.frame(), "cols")).toEqual([]);
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
