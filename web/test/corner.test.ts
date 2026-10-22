// The corner grid (#208): the Grid's corner tiers reach the Snapshot, and the
// corner renderer names the levels in the Tiers layout: the caption and the
// values pseudo-level's name on the top row, column levels right-aligned in
// their tiers with an arrow, row levels along the bottom row.

import { beforeAll, expect, test } from "vitest";
import { light } from "../src";
import { layOutCorner, type CornerText } from "../src/canvas/corner";
import { createCanvasPainter } from "../src/canvas/painter";
import { AXIS_CODE } from "../src/codes";
import type { Content } from "../src/content";
import { fromRows } from "../src/rows";
import { decodeSnapshot } from "../src/snapshot";
import { rowsBy, type Grouping, type SortSpec } from "../src/source";
import { bridge } from "../src/wasm";
import { fakeDocument } from "./canvas";
import { Harness, HEADER } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(async () => {
  await loadForTests();
  fakeDocument([]);
});

interface Ship {
  region: string;
  port: string;
  weight: number;
}

const SHIPS: Ship[] = [
  { region: "North", port: "Aberdeen", weight: 1 },
  { region: "South", port: "Cadiz", weight: 2 },
  { region: "North", port: "Bergen", weight: 5 },
];

const COLUMNS = [{ key: "region" }, { key: "port" }, { key: "weight" }] as const;

const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };

const grouping = (form: "outline" | "header"): Grouping<keyof Ship, number, keyof Ship> => ({
  ...rowsBy<keyof Ship>("region"),
  rowLevels: [
    { field: "region", subtotal: "off" },
    { field: "port", subtotal: "off" },
  ],
  rowAxis: { ...rowsBy("region").rowAxis, form },
});

/** The shipments grouped by region then port in `form`, a row band 300 wide with level columns 100 and 110. */
function grouped(form: "outline" | "header", sort: SortSpec<keyof Ship> = []) {
  const source = fromRows(SHIPS, COLUMNS);
  const h = new Harness(source, 500, 300);
  h.grid.set_header(AXIS_CODE.cols, 300);
  h.grid.set_tiers(AXIS_CODE.rows, Float32Array.from([100, 110]));
  h.frame();
  h.grid.request_view(h.feed.viewKeys({ rows: { ...plain, sort }, cols: plain, grouping: grouping(form) }));
  h.drain();
  const s = h.settled();
  return { h, source, s };
}

const measure = () => createCanvasPainter(light).measure;

/** Each text the corner lays out: its kind, text, and where it starts. */
const laid = (texts: CornerText[]) => texts.map((t) => [t.kind, t.text, t.rect.x, t.rect.y]);

test("in outline form the row levels join in the one band column, the caption to their right", () => {
  const { h, source, s } = grouped("outline");
  try {
    expect(s.rows.corner).toEqual([{ across: { start: 0, end: 300 }, levels: [expect.objectContaining({ index: 0 }), expect.objectContaining({ index: 1 })] }]);
    expect(s.cols.corner).toEqual([{ across: { start: 0, end: HEADER }, levels: [] }]);
    // Text measures 7 px a character; names start at the 6 px padding.
    expect(laid(layOutCorner(s, h.feed.content(s), measure(), light))).toEqual([
      ["level", "region", 6, 0],
      ["join", " › ", 6 + 42, 0],
      ["level", "port", 6 + 42 + 21, 0],
      ["caption", "count", 300 - 6 - 35, 0],
    ]);
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("in header form each row level is named in its own level column, with its sort mark", () => {
  const { h, source, s } = grouped("header", [{ id: "port", direction: "descending" }]);
  try {
    expect(s.rows.corner.map((t) => [t.across.start, t.across.end, t.levels.length])).toEqual([
      [0, 100, 1],
      [100, 210, 1],
      [210, 300, 0],
    ]);
    expect(laid(layOutCorner(s, h.feed.content(s), measure(), light))).toEqual([
      ["level", "region", 6, 0],
      ["level", "port ▼", 106, 0],
      ["caption", "count", 300 - 6 - 35, 0],
    ]);
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("with column tiers, the caption and the values level top the corner, column levels sit right in their tiers, row levels along the bottom", () => {
  // Rows by two fields in header form (level columns 16 and 12 of a 40 px
  // band); columns by a field then the values pseudo-level, tiers 8 high
  // in a 24 px band.
  const grid = bridge().WebGrid.debug_scenario("corner");
  try {
    const frame = grid.debug_frame() as { numbers: number[]; keys: bigint[] };
    const s = decodeSnapshot(new Float64Array(frame.numbers), new BigUint64Array(frame.keys));
    expect(s.cols.corner.map((t) => [t.across.start, t.across.end, t.levels.map((l) => l.field === null)])).toEqual([
      [0, 8, [false]],
      [8, 16, [true]],
      [16, 24, []],
    ]);
    const names = new Map<bigint, string>(s.rows.corner.flatMap((t) => t.levels).map((l, i) => [l.field!, ["a", "b"][i]!]));
    names.set(s.cols.corner[0]!.levels[0]!.field!, "q");
    const content = { caption: () => "n", levelName: (_axis: string, field: bigint | null) => (field === null ? "V" : names.get(field)!) } as unknown as Content;
    expect(laid(layOutCorner(s, content, measure(), light))).toEqual([
      ["level", "q", 40 - 6 - 7 - 14, 0],
      ["join", " →", 40 - 6 - 14, 0],
      ["level", "a", 6, 16],
      ["level", "b", 16 + 6, 16],
      ["caption", "n", 6, 0],
      ["join", "  ", 6 + 7, 0],
      ["level", "V", 6 + 7 + 14, 0],
    ]);
  } finally {
    grid.free();
  }
});
