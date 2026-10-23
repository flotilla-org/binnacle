// Header form (#207): `fromRows` answers grouped rows in header form with no
// Group Items, each group known by its Key all the same; the Grid's tiers
// and spans reach the Snapshot; the painter draws each span in its level
// column, labelled at the top of its part in view, with its disclosure mark,
// and each Summary Item's label from where its header starts.

import { beforeAll, expect, test } from "vitest";
import { light } from "../src";
import { createCanvasPainter } from "../src/canvas/painter";
import { createPendingMarks, MARK_DELAY } from "../src/canvas/pending";
import { AXIS_CODE, EDGE_CODE } from "../src/codes";
import { fromRows } from "../src/rows";
import { rowsBy, type Grouping, type OutlinedWindow } from "../src/source";
import { fakeDocument, type DrawCall } from "./canvas";
import { Harness, HEADER, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

const calls: DrawCall[] = [];

beforeAll(async () => {
  await loadForTests();
  fakeDocument(calls);
});

interface Ship {
  region: string;
  port: string;
  weight: number;
}

/** North holds Aberdeen (two shipments) and Bergen (one); South holds Cadiz (three). */
const SHIPS: Ship[] = [
  { region: "North", port: "Aberdeen", weight: 1 },
  { region: "South", port: "Cadiz", weight: 2 },
  { region: "South", port: "Cadiz", weight: 3 },
  { region: "North", port: "Aberdeen", weight: 4 },
  { region: "North", port: "Bergen", weight: 5 },
  { region: "South", port: "Cadiz", weight: 6 },
];

const COLUMNS = [{ key: "region" }, { key: "port" }, { key: "weight" }] as const;

/** Grouped by region then port, each level with a Subtotal after it, in `form`. */
const grouping = (form: "outline" | "header"): Grouping<keyof Ship, number, keyof Ship> => ({
  ...rowsBy<keyof Ship>("region"),
  rowLevels: [
    { field: "region", subtotal: "after" },
    { field: "port", subtotal: "after" },
  ],
  rowAxis: { ...rowsBy("region").rowAxis, form },
});

const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
const signal = new AbortController().signal;

test("fromRows in header form lists no Group Items, yet labels each group by its ID", () => {
  const source = fromRows(SHIPS, COLUMNS);
  try {
    source.view!({ rows: plain, cols: plain, grouping: grouping("header") }, signal);
    const n = source.rows.count() as number;
    const window = source.rows.window(0, n, signal) as OutlinedWindow<number, keyof Ship>;
    expect(window.outlines.map((o) => o.kind)).toEqual([
      ...["leaf", "leaf", "subtotal", "leaf", "subtotal", "subtotal"],
      ...["leaf", "leaf", "leaf", "subtotal", "subtotal"],
    ]);
    const [north, aberdeen] = window.outlines[0]!.ancestors;
    expect(source.rows.spanLabel!(north!, 0)).toBe("North (3)");
    expect(source.rows.spanLabel!(aberdeen!, 1)).toBe("Aberdeen (2)");
    expect(source.group(aberdeen!)).toMatchObject({ kind: "group", field: "port", value: "Aberdeen", count: 2 });
  } finally {
    source.destroy();
  }
});

/** The shipments in a Harness grouped in `form`, the level columns 100 and 110 wide in a band of 300. */
function grouped(form: "outline" | "header", height = 300) {
  const source = fromRows(SHIPS, COLUMNS);
  const h = new Harness(source, 500, height);
  h.grid.set_header(AXIS_CODE.cols, 300);
  h.grid.set_tiers(AXIS_CODE.rows, Float32Array.from([100, 110]));
  h.frame();
  h.grid.request_view(h.feed.viewKeys({ rows: plain, cols: plain, grouping: grouping("outline") }));
  h.drain();
  h.settled();
  if (form === "header") {
    h.grid.set_form(AXIS_CODE.rows, 1);
    h.drain();
  }
  h.settled();
  return { h, source };
}

/** Paints `h`'s latest frame and returns the text drawn, with where. */
function paint(h: Harness<number, keyof Ship, unknown>) {
  const s = h.frame();
  const painter = createCanvasPainter(light);
  painter.resize(h.width, h.height, 1);
  calls.length = 0;
  painter.paint(s, h.feed.content(s));
  return calls.filter((c) => c.name === "fillText").map((c) => ({ text: c.args[0], x: c.args[1], y: c.args[2] }));
}

test("switching to header form is a View Spec request the source answers, and the Snapshot carries the tiers and spans", () => {
  const { h, source } = grouped("outline");
  try {
    expect(h.snapshot.rows.tiers).toEqual([]);
    h.grid.set_form(AXIS_CODE.rows, 1);
    const effects = h.grid.poll_effects() as { type: string; edit?: unknown }[];
    expect(effects.find((e) => e.type === "ViewRequested")?.edit).toEqual({ type: "SetForm", axis: "rows", form: "header" });
    for (const e of effects) h.feed.answer(e as never);
    const s = h.settled();
    expect(s.appliedView.grouping?.rowAxis.form).toBe("header");
    expect(s.rows.tiers).toEqual([
      { start: 0, end: 100 },
      { start: 100, end: 210 },
    ]);
    const label = (key: bigint) => h.feed.content(s).spanLabel("rows", s.rows.spans.find((span) => span.key === key)!);
    expect(s.rows.spans.map((span) => [span.tier, label(span.key), span.first, span.last, span.open])).toEqual([
      [0, "North (3)", 0, 4, "open"],
      [0, "South (3)", 6, 9, "open"],
      [1, "Aberdeen (2)", 0, 1, "open"],
      [1, "Bergen (1)", 3, 3, "open"],
      [1, "Cadiz (3)", 6, 8, "open"],
    ]);
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("each span's mark and label draw at the top of its level column, and a Subtotal's label where its header starts", () => {
  const { h, source } = grouped("header");
  try {
    const drawn = paint(h);
    const at = (text: string) => drawn.find((t) => t.text === text);
    const top = (row: number) => HEADER + row * ROW_HEIGHT + ROW_HEIGHT / 2;
    // A mark at the column's padding, the label past the mark's room.
    expect(at("North (3)")).toEqual({ text: "North (3)", x: 6 + 14, y: top(0) });
    expect(at("Aberdeen (2)")).toEqual({ text: "Aberdeen (2)", x: 100 + 6 + 14, y: top(0) });
    expect(drawn.filter((t) => t.text === "▾" && t.y === top(0)).map((t) => t.x)).toEqual([6, 106]);
    // Aberdeen's Subtotal, outside Aberdeen's span, labelled from the port
    // column past the 13 px a filter mark takes; North's from the region column.
    expect(at("Aberdeen total (2)")).toEqual({ text: "Aberdeen total (2)", x: 100 + 6 + 13, y: top(2) });
    expect(at("North total (3)")).toEqual({ text: "North total (3)", x: 6 + 13, y: top(5) });
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("a span scrolled part out of view keeps its label at the top of what shows, and is cut there", () => {
  const { h, source } = grouped("header", HEADER + 3 * ROW_HEIGHT);
  try {
    h.grid.scroll_to_item(AXIS_CODE.rows, 1);
    const s = h.settled();
    const north = s.rows.spans.find((span) => span.tier === 0 && span.first <= 1)!;
    expect(north.cutBefore && north.cutAfter).toBe(true);
    expect(north.visible.y).toBe(HEADER);
    const drawn = paint(h);
    expect(drawn.find((t) => t.text === "North (3)")?.y).toBe(HEADER + ROW_HEIGHT / 2);
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("a pinned row breaks the spans over it in two, each part labelled", () => {
  const { h, source } = grouped("header");
  try {
    const first = h.snapshot.rows.strip[0]!.key!;
    h.grid.pin(AXIS_CODE.rows, EDGE_CODE.leading, first);
    h.drain();
    const s = h.settled();
    const aberdeen = s.rows.spans.filter((span) => span.tier === 1 && span.first <= 1);
    expect(aberdeen.map((span) => [span.first, span.last, span.cutBefore, span.cutAfter])).toEqual([
      [0, 0, false, true],
      [1, 1, true, false],
    ]);
    expect(paint(h).filter((t) => t.text === "Aberdeen (2)")).toHaveLength(2);
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("a toggled span's mark waits for its answer, and draws pending once it has waited", () => {
  const { h, source } = grouped("header");
  try {
    const north = h.snapshot.rows.spans[0]!.key;
    // The source would answer at once: the request is held back.
    h.parts.onEffect = () => {};
    h.grid.toggle(AXIS_CODE.rows, north);
    const s = h.frame();
    expect(s.rows.spans.find((span) => span.key === north)?.togglePending).toBe(true);
    const marks = createPendingMarks();
    marks.seen(s, 0);
    expect(marks.pending(north, MARK_DELAY)).toBe(true);
  } finally {
    h.destroy();
    source.destroy();
  }
});
