// A pivot through `fromRows` (#209): rows by region and columns by quarter
// come from the data, each cell the figure where they cross, checked against
// a naive cross-tabulation; the columns are told every change as a
// `Refiltered`, keep their IDs, and don't move.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { bridge } from "../src/wasm";
import { VALUES_FIELD } from "../src/codes";
import { fromRows, type RowsColId } from "../src/rows";
import type {
  AggregateValue,
  AppliedView,
  ChangeNotice,
  Grouping,
  Level,
  OutlinedWindow,
  Placement,
  TableSink,
  ViewSpec,
} from "../src/source";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Ship {
  region: string | null;
  quarter: string;
  weight: number | null;
}

type Col = RowsColId<Ship>;

const COLUMNS = [{ key: "region" }, { key: "quarter" }, { key: "weight" }] as const;
const REGIONS = ["North", "South", "East", null];
const QUARTERS = ["Q1", "Q2", "Q3", "Q4"];

const ships = gs.arrays(
  gs.record({
    region: gs.sampledFrom(REGIONS),
    quarter: gs.sampledFrom(QUARTERS),
    // Whole numbers, so a sum is the same in any order.
    weight: gs.oneOf(gs.just(null), gs.integers({ minValue: 0, maxValue: 20 })),
  }),
  { maxSize: 25 },
);

const SUM: AggregateValue<keyof Ship> = { field: "weight", aggregate: "sum" };
const COUNT: AggregateValue<keyof Ship> = { field: null, aggregate: "count" };
const NAMES = ["sum(weight)", "count"];

const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
const signal = new AbortController().signal;

/** Rows by region (or none), columns by quarter then the values if more than one, Grand Totals placed so. */
function pivot(byRegion: boolean, values: AggregateValue<keyof Ship>[], rows: Placement, cols: Placement): Grouping<keyof Ship, number, Col> {
  const colLevels: Level<keyof Ship>[] = [{ field: "quarter", subtotal: "off" }];
  if (values.length > 1) colLevels.push({ values: true });
  return {
    rowLevels: byRegion ? [{ field: "region", subtotal: "off" }] : [],
    colLevels,
    values,
    slicers: [],
    rowAxis: { grandTotal: rows, form: "outline", expand: { depth: "all", except: [] } },
    colAxis: { grandTotal: cols, form: "header", expand: { depth: "all", except: [] } },
  };
}

const viewOf = (grouping: Grouping<keyof Ship, number, Col> | null): ViewSpec<number, Col> => ({ rows: plain, cols: plain, grouping });

/** Sinks that keep the columns' Change Notices in `cols`. */
function sinksTo(cols: ChangeNotice[]): TableSink<number, Col> {
  const ignore = () => {};
  return {
    rows: { notify: ignore, supportChanged: ignore, movableChanged: ignore },
    cols: { notify: (n) => void cols.push(n), supportChanged: ignore, movableChanged: ignore },
    viewChanged: ignore,
    findCounted: ignore,
  };
}

/** A row of the pivot: a region (`undefined` for the Grand Total). */
type RowOf = string | null | undefined;
/** A column of the pivot: a quarter (`undefined` for an All column) and the index of its value. */
type ColOf = { quarter: string | undefined; value: number };

/** Text order by code point, blanks last, as the groups order. */
const ascending = (a: string | null, b: string | null) =>
  a === b ? 0 : a === null ? 1 : b === null ? -1 : a < b ? -1 : 1;

/** Where a Grand Total placed `at` goes among `items`. */
function placed<T>(items: T[], totals: T[], at: Placement): T[] {
  return at === "before" ? [...totals, ...items] : at === "after" ? [...items, ...totals] : items;
}

/** The model's rows and columns: groups in ascending order, Grand Totals placed, and one All column per value. */
function model(rows: Ship[], g: Grouping<keyof Ship, number, Col>) {
  const regions = [...new Set(rows.map((r) => r.region))].sort(ascending);
  const quarters = [...new Set(rows.map((r) => r.quarter))].sort(ascending);
  const n = g.values.length;
  // An axis with no levels shows only its Grand Total.
  const shownRows: RowOf[] = g.rowLevels.length === 0 ? [undefined] : placed<RowOf>(regions, [undefined], g.rowAxis.grandTotal);
  const values = Array.from({ length: n }, (_, v) => v);
  const byQuarter = quarters.flatMap((quarter) => values.map((value) => ({ quarter, value })));
  const shownCols = placed<ColOf>(byQuarter, values.map((value) => ({ quarter: undefined, value })), g.colAxis.grandTotal);
  return { rows: shownRows, cols: shownCols };
}

/** The model's figure where `row` and `col` cross: none where two groups share no rows. */
function figure(rows: Ship[], g: Grouping<keyof Ship, number, Col>, row: RowOf, col: ColOf): number | null {
  const under = rows.filter((r) => (row === undefined || r.region === row) && (col.quarter === undefined || r.quarter === col.quarter));
  if (under.length === 0 && row !== undefined && col.quarter !== undefined) return null;
  if (g.values[col.value]!.aggregate === "count") return under.length;
  const weights = under.flatMap((r) => (r.weight === null ? [] : [r.weight]));
  return weights.length === 0 ? null : weights.reduce((a, b) => a + b, 0);
}

const pivotSource = (rows: Ship[]) => fromRows(rows, COLUMNS, { pivots: true });
type Source = ReturnType<typeof pivotSource>;

/** What the source shows: its rows and columns as the model names them, each checked against its outline and label. */
function shown(source: Source, g: Grouping<keyof Ship, number, Col>) {
  const rowWindow = source.rows.window(0, source.rows.count() as number, signal) as OutlinedWindow<number, Col>;
  const rows = rowWindow.ids.map((id, i): RowOf => {
    const outline = rowWindow.outlines[i]!;
    expect(id).toBeLessThan(0);
    expect(outline.open).toBe("cantOpen");
    if (outline.kind === "grandTotal") return undefined;
    expect(outline.kind).toBe("group");
    return source.group(id)!.value;
  });
  const colWindow = source.cols.window(0, source.cols.count() as number, signal) as OutlinedWindow<Col, Col>;
  const cols = colWindow.ids.map((id, i): ColOf => {
    const outline = colWindow.outlines[i]!;
    expect(typeof id === "number" && id < 0).toBe(true);
    const label = source.cols.groupLabel!(id, outline);
    const named = (name: string | null | undefined) => (name == null ? 0 : NAMES.indexOf(name));
    if (outline.kind === "grandTotal") {
      // One All column per value, each named by its value.
      const value = named(outline.group?.value);
      expect(label).toBe(g.values.length > 1 ? `All: ${NAMES[value]}` : "All");
      return { quarter: undefined, value };
    }
    expect(outline.kind).toBe("group");
    expect(outline.open).toBe("cantOpen");
    if (outline.group?.field === null) {
      // A value's column under its quarter's.
      const quarter = source.pivotColumn(outline.ancestors.at(-1) as number)!.value!;
      expect(source.cols.spanLabel!(outline.ancestors.at(-1)!, 0)).toBe(quarter);
      expect(label).toBe(outline.group.value);
      return { quarter, value: named(outline.group.value) };
    }
    expect(outline.group?.field).toBe("quarter");
    expect(label).toBe(outline.group?.value);
    return { quarter: outline.group!.value!, value: 0 };
  });
  const cells = source.cells(rowWindow.ids, colWindow.ids, signal) as (number | null)[][];
  return { rows, cols, ids: colWindow.ids, cells };
}

describe("fromRows's pivot", () => {
  test("whatever the data, values and Grand Totals, the pivot's rows, columns and cells are a cross-tabulation's", () =>
    hegel.test((tc) => {
      const rows = tc.draw(ships);
      const source = pivotSource(rows);
      try {
        const values = tc.draw(gs.sampledFrom([[SUM], [COUNT], [SUM, COUNT]]));
        const placement = gs.sampledFrom<Placement>(["before", "after", "off"]);
        const g = pivot(tc.draw(gs.booleans()), values, tc.draw(placement), tc.draw(placement));
        const answer = source.view!(viewOf(g), signal) as AppliedView<number, Col>;
        expect(answer.grouping).toEqual(g);
        const want = model(rows, g);
        const got = shown(source, g);
        expect(got.rows).toEqual(want.rows);
        expect(got.cols).toEqual(want.cols);
        expect(got.cells).toEqual(want.rows.map((r) => want.cols.map((c) => figure(rows, g, r, c))));
      } finally {
        source.destroy();
      }
    }));

  test("the columns hear every change as a Refiltered, keep their IDs as rows come and go, and never move", () => {
    const rows: Ship[] = [
      { region: "North", quarter: "Q1", weight: 1 },
      { region: "South", quarter: "Q2", weight: 2 },
      { region: "North", quarter: "Q3", weight: 3 },
    ];
    const source = pivotSource(rows);
    const notices: ChangeNotice[] = [];
    source.attach!(sinksTo(notices));
    try {
      const g = pivot(true, [SUM, COUNT], "after", "after");
      source.view!(viewOf(g), signal);
      expect(notices).toEqual([{ type: "Refiltered" }]);
      expect(source.cols.movable!()).toBe(false);
      const before = shown(source, g);
      expect(source.cols.move!([before.ids[0]!], 3)).toEqual({ refused: "Can't move pivot columns: they follow the column sort" });
      // Q2 leaves with South's row and comes back with it.
      source.filter((r) => r.region !== "South");
      const without = shown(source, g);
      expect(without.cols.map((c) => c.quarter)).toEqual(["Q1", "Q1", "Q3", "Q3", undefined, undefined]);
      source.filter(null);
      const again = shown(source, g);
      expect(again.ids).toEqual(before.ids);
      expect(without.ids).toEqual([0, 1, 4, 5, 6, 7].map((i) => before.ids[i]));
      source.view!(viewOf(null), signal);
      expect(source.cols.window(0, 3, signal)).toEqual(["region", "quarter", "weight"]);
      expect(source.cols.movable!()).toBe(true);
      expect(notices).toEqual(Array.from({ length: 4 }, () => ({ type: "Refiltered" })));
    } finally {
      source.destroy();
    }
  });

  test("sorted by the Grand Total row, most first, the quarters follow their totals", () => {
    const rows: Ship[] = [
      { region: "North", quarter: "Q1", weight: 1 },
      { region: "North", quarter: "Q2", weight: 5 },
      { region: "South", quarter: "Q3", weight: 3 },
    ];
    const source = pivotSource(rows);
    try {
      const g = pivot(true, [SUM], "after", "after");
      const answer = source.view!({ ...viewOf(g), cols: { ...plain, sort: [{ id: -1, direction: "descending" }] } }, signal);
      expect((answer as AppliedView<number, Col>).cols.sort).toEqual([{ id: -1, direction: "descending" }]);
      expect(shown(source, g).cols.map((c) => c.quarter)).toEqual(["Q2", "Q3", "Q1", undefined]);
    } finally {
      source.destroy();
    }
  });

  test("with the values on the rows, each region heads a row per value, and each cell is that value's figure", () => {
    const rows: Ship[] = [
      { region: "North", quarter: "Q1", weight: 1 },
      { region: "North", quarter: "Q1", weight: 4 },
      { region: "South", quarter: "Q2", weight: 2 },
    ];
    const source = pivotSource(rows);
    try {
      const g = pivot(true, [SUM, COUNT], "off", "off");
      g.rowLevels = [{ field: "region", subtotal: "off" }, { values: true }];
      g.colLevels = [{ field: "quarter", subtotal: "off" }];
      source.view!(viewOf(g), signal);
      const window = source.rows.window(0, source.rows.count() as number, signal) as OutlinedWindow<number, Col>;
      const labels = window.ids.map((id, i) => source.rows.groupLabel!(id, window.outlines[i]!));
      expect(labels).toEqual(["North (2)", "sum(weight) (2)", "count (2)", "South (1)", "sum(weight) (1)", "count (1)"]);
      expect(window.outlines[1]!.group).toEqual({ field: null, value: "sum(weight)" });
      const cols = source.cols.window(0, source.cols.count() as number, signal) as OutlinedWindow<Col, Col>;
      expect(cols.outlines.map((o) => o.group?.value)).toEqual(["Q1", "Q2"]);
      // A region's own row shows the first value's figure, as a Group Item over no values does.
      expect(source.cells(window.ids, cols.ids, signal)).toEqual([
        [5, null],
        [5, null],
        [2, null],
        [null, 2],
        [null, 2],
        [null, 1],
      ]);
    } finally {
      source.destroy();
    }
  });

  test("a source that does not pivot leaves column levels out", () => {
    const source = fromRows<Ship>([{ region: "North", quarter: "Q1", weight: 1 }], COLUMNS);
    try {
      const g = pivot(true, [SUM], "off", "after");
      const answer = source.view!({ rows: plain, cols: plain, grouping: g as Grouping<keyof Ship, number, keyof Ship> }, signal);
      expect(answer).toMatchObject({ grouping: { colLevels: [] } });
      expect(source.cols.window(0, 3, signal)).toEqual(["quarter", "weight"]);
    } finally {
      source.destroy();
    }
  });

  test("the values pseudo-level's field is the wasm bridge's", () => {
    expect(VALUES_FIELD).toBe(bridge().values_field());
  });
});
