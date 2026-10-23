// The grouping menu's edits (#212) through the wasm Grid, the feed and
// `fromRows`: a level moved to the other axis turns the grouped table into a
// pivot and back, values come and go with the values pseudo-level, and the
// source's fields group though none of them is in view.

import { beforeAll, expect, test } from "vitest";
import { AXIS_CODE } from "../src/codes";
import { fromRows, type RowsColId } from "../src/rows";
import { rowsBy, type Grouping } from "../src/source";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Ship {
  region: string;
  port: string;
  quarter: string;
  weight: number;
}

const SHIPS: Ship[] = [
  { region: "North", port: "Aberdeen", quarter: "Q1", weight: 1 },
  { region: "South", port: "Cadiz", quarter: "Q2", weight: 2 },
  { region: "North", port: "Bergen", quarter: "Q1", weight: 5 },
];

const COLUMNS = [{ key: "region" }, { key: "port" }, { key: "quarter" }, { key: "weight" }] as const;

type Col = RowsColId<Ship>;

const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };

/** The shipments in a harness, grouped as `grouping` asks once it settles. */
function table(grouping: Grouping<keyof Ship, number, Col> | null, width = 600) {
  const source = fromRows(SHIPS, COLUMNS, { pivots: true });
  const h = new Harness(source, width, 300);
  h.frame();
  h.grid.request_view(h.feed.viewKeys({ rows: plain, cols: plain, grouping }));
  h.drain();
  h.settled();
  return { h, source };
}

/** Runs `edit` on the Grid, answers its request and settles. */
function edit(h: Harness<number, Col, unknown>, run: () => void) {
  run();
  h.drain();
  return h.settled();
}

/** The grouping the source applied, by IDs: the levels' fields (`values` for the pseudo-level) and the values. */
function applied(h: Harness<number, Col, unknown>) {
  const grouping = h.feed.viewIds(h.snapshot.appliedView).grouping;
  const fields = (levels: Grouping<Col>["rowLevels"]) => levels.map((l) => ("field" in l ? l.field : "values"));
  return grouping && {
    rows: fields(grouping.rowLevels),
    cols: fields(grouping.colLevels),
    values: grouping.values.map((v) => `${v.aggregate}(${v.field ?? ""})`),
  };
}

/** The columns in view, by ID. */
const columns = (h: Harness<number, Col, unknown>) => h.snapshot.cols.strip.map((i) => i.key !== null && h.feed.colId(i.key));

const ROWS = AXIS_CODE.rows;
const COLS = AXIS_CODE.cols;

const byRegionThenPort: Grouping<keyof Ship, number, Col> = {
  ...rowsBy<keyof Ship>("region"),
  rowLevels: [
    { field: "region", subtotal: "off" },
    { field: "port", subtotal: "off" },
  ],
};

test("moving the rows' inner level to the columns makes a pivot by it, and moving it back the grouped table again", () => {
  const { h, source } = table(byRegionThenPort);
  try {
    expect(applied(h)).toEqual({ rows: ["region", "port"], cols: [], values: ["count()"] });
    expect(columns(h)).toEqual(["quarter", "weight"]);
    edit(h, () => h.grid.move_level(ROWS, h.feed.colKey("port")));
    expect(applied(h)).toEqual({ rows: ["region"], cols: ["port"], values: ["count()"] });
    // A pivot column for each port (`rowsBy` places no column Grand Total).
    expect(columns(h).map((id) => (typeof id === "number" ? source.pivotColumn(id)?.value : id))).toEqual(["Aberdeen", "Bergen", "Cadiz"]);
    edit(h, () => h.grid.move_level(COLS, h.feed.colKey("port")));
    expect(applied(h)).toEqual({ rows: ["region", "port"], cols: [], values: ["count()"] });
    expect(columns(h)).toEqual(["quarter", "weight"]);
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("removing a level keeps the rest, and removing the last brings back the plain table", () => {
  const { h, source } = table(byRegionThenPort);
  try {
    edit(h, () => h.grid.remove_level(ROWS, h.feed.colKey("region")));
    expect(applied(h)).toEqual({ rows: ["port"], cols: [], values: ["count()"] });
    edit(h, () => h.grid.remove_level(ROWS, h.feed.colKey("port")));
    expect(applied(h)).toBeNull();
    expect(columns(h)).toEqual(["region", "port", "quarter", "weight"]);
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("a second value on a pivot brings the values level into the columns, and removing one takes it away", () => {
  const { h, source } = table({ ...rowsBy<keyof Ship>("region"), colLevels: [{ field: "quarter", subtotal: "off" }] });
  try {
    expect(applied(h)).toEqual({ rows: ["region"], cols: ["quarter"], values: ["count()"] });
    const sum = { field: h.feed.colKey("weight"), aggregate: "sum" };
    edit(h, () => h.grid.add_value(sum));
    expect(applied(h)).toEqual({ rows: ["region"], cols: ["quarter", "values"], values: ["count()", "sum(weight)"] });
    edit(h, () => h.grid.remove_value({ field: null, aggregate: "count" }));
    expect(applied(h)).toEqual({ rows: ["region"], cols: ["quarter"], values: ["sum(weight)"] });
    // The last value stays.
    edit(h, () => h.grid.remove_value(sum));
    expect(applied(h)).toEqual({ rows: ["region"], cols: ["quarter"], values: ["sum(weight)"] });
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("an aggregate the source doesn't offer over a column is not asked for", () => {
  const { h, source } = table(byRegionThenPort);
  try {
    // A column of text takes counts, not sums.
    edit(h, () => h.grid.add_value({ field: h.feed.colKey("port"), aggregate: "sum" }));
    expect(applied(h)).toEqual({ rows: ["region", "port"], cols: [], values: ["count()"] });
    edit(h, () => h.grid.add_value({ field: h.feed.colKey("port"), aggregate: "distinct count" }));
    expect(applied(h)).toEqual({ rows: ["region", "port"], cols: [], values: ["count()", "distinct count(port)"] });
  } finally {
    h.destroy();
    source.destroy();
  }
});

test("on a pivot, whose columns are its groups, the source's fields still group either axis", () => {
  // Narrow, so no base column was ever in view: the feed declares the
  // fields `cols.fields` lists.
  const source = fromRows(SHIPS, COLUMNS, { pivots: true });
  const h = new Harness(source, 100, 300);
  try {
    h.grid.request_view(
      h.feed.viewKeys({ rows: plain, cols: plain, grouping: { ...rowsBy<keyof Ship>("region"), rowLevels: [], colLevels: [{ field: "quarter", subtotal: "off" }] } }),
    );
    h.drain();
    h.settled();
    expect(columns(h).every((id) => typeof id === "number")).toBe(true);
    edit(h, () => h.grid.group_by(ROWS, h.feed.colKey("port")));
    edit(h, () => h.grid.group_by(COLS, h.feed.colKey("region")));
    expect(applied(h)).toEqual({ rows: ["port"], cols: ["quarter", "region"], values: ["count()"] });
  } finally {
    h.destroy();
    source.destroy();
  }
});
