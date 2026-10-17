// What a table that cannot show cells paints instead: the empty message when
// an axis has no items, and the error marker on cells whose request failed.

import { beforeAll, expect, test } from "vitest";
import { light, type TableSource } from "../src";
import { createCanvasPainter } from "../src/canvas/painter";
import { fakeDocument, type DrawCall } from "./canvas";
import { BAND, HEADER, Harness } from "./harness";
import { loadForTests } from "./wasm";

const calls: DrawCall[] = [];

beforeAll(async () => {
  await loadForTests();
  fakeDocument(calls);
});

type Source = TableSource<number, number, string>;

const table = (rows: number, cols: number, cells: Source["cells"] = (rs, cs) => rs.map((r) => cs.map((c) => `${r}${c}`))): Source => ({
  rows: { identity: "position", count: () => rows, window: (s, e) => Array.from({ length: e - s }, (_, i) => s + i) },
  cols: { identity: "position", count: () => cols, window: (s, e) => Array.from({ length: e - s }, (_, i) => s + i) },
  cells,
});

/** Paints `h`'s latest frame and returns the text drawn, with where and in what colour. */
function paint(h: Harness<number, number, string>, empty?: string) {
  const s = h.frame();
  const painter = createCanvasPainter(light);
  if (empty !== undefined) painter.empty = empty;
  painter.resize(h.width, h.height, 1);
  calls.length = 0;
  painter.paint(s, h.feed.content(s));
  return calls
    .filter((c) => c.name === "fillText")
    .map((c) => ({ text: c.args[0], x: c.args[1], y: c.args[2], fill: c.fill }));
}

test("an axis with no items paints the empty message centred in the body, in the loading colour", () => {
  for (const [rows, cols] of [
    [0, 3],
    [4, 0],
    [0, 0],
  ] as const) {
    const h = new Harness(table(rows, cols));
    try {
      const drawn = paint(h).filter((t) => t.text === "No rows");
      // The fake measures 7 px a character.
      const x = BAND + (h.width - BAND - 7 * "No rows".length) / 2;
      expect(drawn, `${rows} by ${cols}`).toEqual([{ text: "No rows", x, y: HEADER + (h.height - HEADER) / 2, fill: light.loading }]);
    } finally {
      h.destroy();
    }
  }
});

test("the empty message is the painter's, and a table with items paints none", () => {
  const empty = new Harness(table(0, 2));
  const full = new Harness(table(2, 2));
  try {
    expect(paint(empty, "No rows match the filter").map((t) => t.text)).toContain("No rows match the filter");
    expect(paint(full).map((t) => t.text)).not.toContain("No rows");
  } finally {
    empty.destroy();
    full.destroy();
  }
});

test("a cell whose request failed paints the error marker in the Theme's error colour", async () => {
  const h = new Harness(table(2, 2, () => Promise.reject(new Error("down"))));
  try {
    h.frame();
    await new Promise((r) => setTimeout(r, 0));
    const drawn = paint(h).filter((t) => t.fill === light.error);
    expect(drawn.map((t) => t.text)).toEqual(["⚠", "⚠", "⚠", "⚠"]);
  } finally {
    h.destroy();
  }
});
