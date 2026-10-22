// Copy: the Grid's CopyRequested over a grouped fromRows table, gathered as
// shown (labels in the row band, collapsed groups left out, an open Group
// Item's figures left to its Subtotal), at once from a source that answers
// at once and later from one that answers with Promises.

import { beforeAll, describe, expect, test } from "vitest";
import { gatherCopy, type CopyParts } from "../src/copy";
import type { Effect } from "../src/effects";
import { fromRows } from "../src/rows";
import { plainText } from "../src/renderer";
import { rowsBy, summaryLabel, type AppliedView, type MaybePromise, type Outline, type TableSource, type ViewSpec } from "../src/source";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Ship {
  region: string;
  port: string;
  weight: number;
}

const SHIPS: Ship[] = [
  { region: "North", port: "Leith", weight: 10 },
  { region: "South", port: "Cadiz", weight: 7 },
  { region: "North", port: "Bergen", weight: 3 },
  { region: "North", port: "Leith", weight: 5 },
];

const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };

/** By region then port, a Subtotal after each region, every group open. */
function spec(): ViewSpec<number, keyof Ship> {
  const by = rowsBy<keyof Ship>("region");
  return {
    rows: plain,
    cols: plain,
    grouping: {
      ...by,
      rowLevels: [
        { field: "region", subtotal: "after" },
        { field: "port", subtotal: "off" },
      ],
      values: [{ field: "weight", aggregate: "sum" }],
    },
  };
}

function parts<V>(source: TableSource<number, keyof Ship, V>, levels = spec().grouping!.rowLevels): CopyParts<number, keyof Ship, V> {
  return {
    source,
    renderer: () => plainText as never,
    signal: new AbortController().signal,
    label: ({ id, outline }) => (outline && outline.kind !== "leaf" ? (source.rows.groupLabel?.(id, outline) ?? summaryLabel(outline)) : ""),
    figureless: (o: Outline<number, keyof Ship> | null) => {
      if (o?.kind !== "group" || o.open !== "open") return false;
      const level = levels[o.depth];
      return level !== undefined && "field" in level && level.subtotal !== "off";
    },
  };
}

/** The source answering every request with a Promise. */
function slow<V>(source: TableSource<number, keyof Ship, V>): TableSource<number, keyof Ship, V> {
  const later = <T>(v: MaybePromise<T>) => Promise.resolve(v);
  return {
    ...source,
    rows: { ...source.rows, window: (a, b, s) => later(source.rows.window(a, b, s)) },
    cols: { ...source.cols, window: (a, b, s) => later(source.cols.window(a, b, s)) },
    cells: (r, c, s) => later(source.cells(r, c, s)),
  } as TableSource<number, keyof Ship, V>;
}

describe("copy", () => {
  test("copying every cell copies what is shown, as shown", async () => {
    const source = fromRows(SHIPS, [{ key: "region" }, { key: "port" }, { key: "weight" }]);
    const h = new Harness(source);
    try {
      h.grid.request_view(h.feed.viewKeys(spec() as AppliedView<number, keyof Ship>));
      h.drain();
      h.settled();
      const copies: Effect[] = [];
      h.grid.set_focus(h.snapshot.rows.strip[0]!.key!, h.snapshot.cols.strip[0]!.key!);
      h.grid.select_all();
      h.grid.copy();
      for (const e of h.grid.poll_effects() as Effect[]) (e.type === "CopyRequested" ? copies.push(e) : h.feed.answer(e));
      const [copy] = copies;
      if (copy?.type !== "CopyRequested") throw new Error("no copy");
      expect(copy.rowLabels).toBe(true);
      const copied = gatherCopy(copy.rows, copy.cols, copy.rowLabels, parts(source));
      if (copied instanceof Promise || copied === null) throw new Error("fromRows answers at once");
      // The region and port columns leave the body while the rows group by them.
      expect(copied.cols).toEqual(["weight"]);
      expect(copied.text.split("\n")).toEqual([
        "North\t",
        "Bergen\t3",
        "\t3",
        "Leith\t15",
        "\t10",
        "\t5",
        "North total\t18",
        "South\t",
        "Cadiz\t7",
        "\t7",
        "South total\t7",
      ]);
      const later = await gatherCopy(copy.rows, copy.cols, copy.rowLabels, parts(slow(source)));
      expect(later?.text).toBe(copied.text);
    } finally {
      h.destroy();
    }
  });

  test("a collapsed group's items are left out, and its Group Item carries its figures", () => {
    const source = fromRows(SHIPS, [{ key: "region" }, { key: "port" }, { key: "weight" }]);
    const closed = spec();
    closed.grouping!.rowAxis = { ...closed.grouping!.rowAxis, expand: { depth: 1, except: [] } };
    source.view!(closed, new AbortController().signal);
    const copied = gatherCopy([{ start: 0, end: 6 }], [{ start: 0, end: 1 }], true, parts(source));
    if (copied instanceof Promise || copied === null) throw new Error("fromRows answers at once");
    expect(copied.text.split("\n")).toEqual([
      "North\t",
      "Bergen\t3",
      "Leith\t15",
      "North total\t18",
      "South\t",
      "Cadiz\t7",
    ]);
  });

  test("a plain table copies cells alone, quoting text that holds a tab, a line break or a quote", () => {
    const source = fromRows(
      [
        { a: "x\ty", b: 'say "hi"' },
        { a: "two\nlines", b: "plain" },
      ],
      [{ key: "a" }, { key: "b" }],
    );
    const copied = gatherCopy([{ start: 0, end: 2 }], [{ start: 0, end: 2 }], false, parts(source as never));
    if (copied instanceof Promise || copied === null) throw new Error("fromRows answers at once");
    expect(copied.text).toBe('"x\ty"\t"say ""hi"""\n"two\nlines"\tplain');
  });

  test("a selection past the limit copies nothing", () => {
    const source = fromRows(SHIPS, [{ key: "region" }]);
    expect(gatherCopy([{ start: 0, end: 1_000_001 }], [{ start: 0, end: 1 }], false, parts(source))).toBeNull();
  });
});
