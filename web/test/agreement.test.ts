// Seam 2: the wasm boundary. The decoder and the Effect union are checked
// against Rust's own view of the same frame, never against constants.

import { beforeAll, describe, expect, test } from "vitest";
import { FIND, GRAND_TOTAL, NOTICE_CODE, SUPPORT } from "../src/codes";
import { decodeSnapshot, type KeyedViewSpec } from "../src/snapshot";
import { isEffect } from "../src/effects";
import type { Outline } from "../src/source";
import { bridge } from "../src/wasm";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface DebugFrame {
  numbers: number[];
  keys: bigint[];
  dump: Record<string, unknown>;
}

describe("the decoded Snapshot equals Rust's dump of the same frame", () => {
  const scenarios = [
    "plain",
    "busy",
    "resizing",
    "band",
    "moving",
    "movePending",
    "moveRefused",
    "editing",
    "locating",
    "scrolled",
    "dragging",
    "sorted",
    "filtered",
    "found",
    "rejected",
    "pinned",
    "hidden",
    "pinDrag",
    "unpinDrag",
    "clipped",
    "restored",
    "endGap",
    "grouped",
    "sliced",
  ];

  test("the scenario list matches Rust's", () => {
    expect(bridge().debug_scenarios()).toEqual(scenarios);
  });

  test.each(scenarios)("%s", (name) => {
    const grid = bridge().WebGrid.debug_scenario(name);
    try {
      const frame = grid.debug_frame() as DebugFrame;
      const decoded = decodeSnapshot(new Float64Array(frame.numbers), new BigUint64Array(frame.keys));
      expect({ ...decoded, cells: Array.from(decoded.cells) }).toEqual(frame.dump);
    } finally {
      grid.free();
    }
  });
});

describe("Effects", () => {
  test("every Effect variant passes the union's guard", () => {
    const effects = bridge().debug_every_effect() as unknown[];
    const types = effects.map((e) => (e as { type: string }).type);
    expect(new Set(types).size).toBe(effects.length);
    for (const effect of effects) expect(isEffect(effect), JSON.stringify(effect, (_, v) => (typeof v === "bigint" ? `${v}n` : v))).toBe(true);
  });

  test("the guard rejects what is not an Effect", () => {
    expect(isEffect({ type: "Nope" })).toBe(false);
    expect(isEffect({ type: "FocusChanged", generation: 1, row: 1, col: 2n })).toBe(false);
    expect(isEffect({ type: "Locate", generation: 1, axis: "diagonal", key: 1n })).toBe(false);
    const pins = { type: "PinsChanged", generation: 1, axis: "cols", edge: "trailing", keys: [3n] };
    expect(isEffect(pins)).toBe(true);
    expect(isEffect({ ...pins, edge: "middle" })).toBe(false);
    expect(isEffect({ ...pins, keys: [3] })).toBe(false);
    expect(isEffect(null)).toBe(false);
    const edit = { type: "CycleSort", axis: "rows", key: 1n, multi: false };
    const view = (spec: object, e: unknown = edit) => ({
      type: "ViewRequested",
      generation: 1,
      spec: { grouping: null, ...spec },
      edit: e,
    });
    const unsorted = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
    expect(isEffect(view({ rows: unsorted, cols: unsorted }))).toBe(true);
    expect(isEffect(view({ rows: unsorted, cols: unsorted, grouping: undefined }))).toBe(false);
    const laidOut = { grandTotal: "off", form: "outline", expand: { depth: "all", except: [] } };
    const grouping = {
      rowLevels: [{ field: 1n, subtotal: "after" }, { values: true }],
      colLevels: [],
      values: [{ field: null, aggregate: "count" }],
      slicers: [],
      rowAxis: laidOut,
      colAxis: { ...laidOut, form: "header", expand: { depth: 2, except: [5n] } },
    };
    expect(isEffect(view({ rows: unsorted, cols: unsorted, grouping }))).toBe(true);
    const regrouped = (part: object) => view({ rows: unsorted, cols: unsorted, grouping: { ...grouping, ...part } });
    expect(isEffect(regrouped({ rowLevels: [{ field: 1, subtotal: "off" }] }))).toBe(false);
    expect(isEffect(regrouped({ rowLevels: [{ field: 1n, subtotal: "under" }] }))).toBe(false);
    expect(isEffect(regrouped({ values: [{ field: 1, aggregate: "sum" }] }))).toBe(false);
    expect(isEffect(regrouped({ rowAxis: { ...laidOut, form: "tree" } }))).toBe(false);
    expect(isEffect(regrouped({ rowAxis: { ...laidOut, expand: { depth: "some", except: [] } } }))).toBe(false);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, { type: "Group", axis: "rows", field: 1 }))).toBe(false);
    expect(isEffect(view({ rows: { filter: { quick: null }, sort: [] }, cols: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: { sort: [] }, cols: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: { ...unsorted, hidden: [1n, 4n] }, cols: unsorted }))).toBe(true);
    expect(isEffect(view({ rows: { ...unsorted, hidden: [1] }, cols: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: { ...unsorted, hidden: undefined }, cols: unsorted }))).toBe(false);
    const hide = { type: "Hide", axis: "cols", keys: [3n] };
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, hide))).toBe(true);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, { ...hide, type: "Show" }))).toBe(true);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, { ...hide, keys: [3] }))).toBe(false);
    const quick = { text: "a", caseSensitive: false, regex: false, searches: null };
    const filtered = (q: unknown) => ({ filter: { quick: q, conditions: [] }, sort: [], hidden: [] });
    expect(isEffect(view({ rows: filtered(quick), cols: unsorted }))).toBe(true);
    expect(isEffect(view({ rows: filtered({ ...quick, searches: [1n] }), cols: unsorted }))).toBe(true);
    expect(isEffect(view({ rows: filtered({ ...quick, searches: [1] }), cols: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: filtered({ ...quick, regex: "no" }), cols: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, { type: "SetQuickFilter", axis: "rows", query: 1 }))).toBe(false);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, { type: "ShownChanged", axis: "up" }))).toBe(false);
    expect(isEffect(view({ rows: unsorted, cols: { sort: [{ key: 1, direction: "ascending" }] } }))).toBe(false);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, { ...edit, multi: 1 }))).toBe(false);
    const condition = { key: 2n, not: true, kind: { type: "Text", query: { ...quick, searches: [2n] } } };
    const conditioned = (c: unknown) => ({ filter: { quick: null, conditions: [c] }, sort: [], hidden: [] });
    expect(isEffect(view({ rows: conditioned(condition), cols: unsorted }))).toBe(true);
    expect(isEffect(view({ rows: conditioned({ ...condition, key: 2 }), cols: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: conditioned({ ...condition, not: "yes" }), cols: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: conditioned({ ...condition, kind: { type: "Text" } }), cols: unsorted }))).toBe(false);
    const listed = (values: unknown) => conditioned({ ...condition, kind: { type: "OneOf", values } });
    expect(isEffect(view({ rows: listed(["Bér", null, ""]), cols: unsorted }))).toBe(true);
    expect(isEffect(view({ rows: listed([]), cols: unsorted }))).toBe(true);
    expect(isEffect(view({ rows: listed([1]), cols: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: listed(null), cols: unsorted }))).toBe(false);
    const add = (c: unknown) => ({ type: "AddCondition", axis: "rows", condition: c });
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, add(condition)))).toBe(true);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, add(null)))).toBe(false);
    const replace = { type: "ReplaceCondition", axis: "rows", from: condition, to: { ...condition, not: false } };
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, replace))).toBe(true);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, { ...replace, to: undefined }))).toBe(false);
    const find = { query: quick, by: "cols" };
    expect(isEffect({ type: "FindRequested", generation: 1, query: find })).toBe(true);
    expect(isEffect({ type: "FindRequested", generation: 1, query: null })).toBe(true);
    expect(isEffect({ type: "FindRequested", generation: 1, query: { ...find, by: "up" } })).toBe(false);
    const step = { type: "FindStepRequested", generation: 1, query: find, from: { row: 1n, col: 2n }, direction: "next" };
    expect(isEffect(step)).toBe(true);
    expect(isEffect({ ...step, from: null, direction: "previous" })).toBe(true);
    expect(isEffect({ ...step, query: null })).toBe(false);
    expect(isEffect({ ...step, from: { row: 1n, col: 2 } })).toBe(false);
    expect(isEffect({ ...step, direction: "sideways" })).toBe(false);
  });
});

describe("View Specs", () => {
  test("every View Edit passes the edit guard", () => {
    const edits = bridge().debug_every_view_edit() as { type: string }[];
    expect(new Set(edits.map((e) => e.type))).toEqual(
      new Set([
        "CycleSort",
        "SetQuickFilter",
        "Replace",
        "ShownChanged",
        "AddCondition",
        "RemoveCondition",
        "ReplaceCondition",
        "Reapply",
        "Hide",
        "Show",
        "Group",
        "Ungroup",
        "Expand",
        "Collapse",
        "ExpandAll",
        "ExpandTo",
        "AddSlicer",
        "RemoveSlicer",
        "ReplaceSlicer",
      ]),
    );
    const unsorted = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
    for (const edit of edits) {
      const spec = { rows: unsorted, cols: unsorted, grouping: null };
      const effect = { type: "ViewRequested", generation: 1, spec, edit };
      expect(isEffect(effect), edit.type).toBe(true);
    }
  });

  test("a View Spec a host requests comes back as Rust carries it, and its totals reach the Snapshot", () => {
    const { WebAxis, WebGrid } = bridge();
    const grid = new WebGrid(new WebAxis(5, 20), new WebAxis(3, 80));
    try {
      const spec = {
        rows: {
          filter: {
            quick: { text: "Bér 🚢", caseSensitive: true, regex: false, searches: [2n, 0n] },
            conditions: [
              {
                key: 1n,
                not: true,
                kind: { type: "Text", query: { text: "Ø", caseSensitive: false, regex: true, searches: [1n] } },
              },
              { key: 2n, not: false, kind: { type: "OneOf", values: [null, "", "Bér 🚢"] } },
            ],
          },
          sort: [{ key: 1n, direction: "descending" }],
          hidden: [3n, 0n],
        },
        cols: {
          filter: { quick: { text: "x", caseSensitive: false, regex: true, searches: null }, conditions: [] },
          sort: [],
          hidden: [],
        },
        grouping: {
          rowLevels: [{ field: 2n ** 63n + 1n, subtotal: "before" }, { values: true }],
          colLevels: [{ field: 0n, subtotal: "off" }],
          values: [
            { field: null, aggregate: "count" },
            { field: 2n, aggregate: "sum" },
          ],
          slicers: [{ key: 2n, not: false, kind: { type: "OneOf", values: [null, "Ø"] } }],
          rowAxis: { grandTotal: "after", form: "outline", expand: { depth: "all", except: [2n ** 63n] } },
          colAxis: { grandTotal: "off", form: "header", expand: { depth: 0, except: [] } },
        },
      };
      grid.request_view(spec);
      const requested = (grid.poll_effects() as { type: string; spec?: unknown; edit?: unknown }[]).filter(
        (e) => e.type === "ViewRequested",
      );
      expect(requested.map((e) => [e.spec, e.edit])).toEqual([[spec, { type: "Replace" }]]);
      grid.view_applied(spec, 40, NaN);
      grid.set_quick_filter_support(0, 3);
      grid.begin_frame(400, 200);
      const s = decodeSnapshot(grid.snapshot(), grid.snapshot_keys());
      expect(s.appliedView).toEqual(spec);
      expect([s.rows.total, s.cols.total]).toEqual([40, null]);
      expect([s.rows.quickFilter, s.cols.quickFilter]).toEqual([{ regex: true }, null]);
      grid.reapply_view();
      const again = (grid.poll_effects() as { type: string; spec?: unknown; edit?: unknown }[]).filter(
        (e) => e.type === "ViewRequested",
      );
      expect(again.map((e) => [e.spec, e.edit])).toEqual([[spec, { type: "Reapply" }]]);
    } finally {
      grid.free();
    }
  });
});

describe("Hiding", () => {
  test("hide and show come back as Hide and Show requests, and hide support reaches the Snapshot", () => {
    const { WebAxis, WebGrid } = bridge();
    const grid = new WebGrid(new WebAxis(5, 20), new WebAxis(3, 80));
    try {
      grid.begin_frame(400, 200);
      grid.hide(1, BigUint64Array.from([2n]));
      expect((grid.poll_effects() as { type: string }[]).filter((e) => e.type === "ViewRequested")).toEqual([]);
      grid.set_hide_support(1, true);
      grid.hide(1, BigUint64Array.from([2n, 0n, 2n]));
      grid.show(1, BigUint64Array.from([0n, 1n]));
      const requested = (grid.poll_effects() as { type: string; spec?: KeyedViewSpec; edit?: unknown }[]).filter(
        (e) => e.type === "ViewRequested",
      );
      expect(requested.map((e) => [e.spec?.cols.hidden, e.edit])).toEqual([
        [[2n, 0n], { type: "Hide", axis: "cols", keys: [2n, 0n] }],
        [[2n], { type: "Show", axis: "cols", keys: [0n] }],
      ]);
      grid.begin_frame(400, 200);
      const s = decodeSnapshot(grid.snapshot(), grid.snapshot_keys());
      expect([s.rows.hideSupport, s.cols.hideSupport]).toEqual([false, true]);
      expect(s.requestedView?.cols.hidden).toEqual([2n]);
    } finally {
      grid.free();
    }
  });

  test("a hide past the axis's cap returns false and asks for nothing", () => {
    const { WebAxis, WebGrid } = bridge();
    const grid = new WebGrid(new WebAxis(5, 20), new WebAxis(3, 80));
    try {
      grid.set_hide_support(0, true);
      grid.set_hide_cap(0, 2);
      expect(grid.hide(0, BigUint64Array.from([1n, 2n, 3n]))).toBe(false);
      expect(grid.hide(0, BigUint64Array.from([1n, 2n]))).toBe(true);
      const requested = (grid.poll_effects() as { type: string; spec?: KeyedViewSpec }[]).filter(
        (e) => e.type === "ViewRequested",
      );
      expect(requested.map((e) => e.spec?.rows.hidden)).toEqual([[1n, 2n]]);
      grid.begin_frame(400, 200);
      const s = decodeSnapshot(grid.snapshot(), grid.snapshot_keys());
      expect([s.rows.hideCap, s.cols.hideCap]).toEqual([2, 1000]);
    } finally {
      grid.free();
    }
  });
});

describe("Support", () => {
  test("each support code names the same support in Rust's Snapshot", () => {
    const { WebAxis, WebGrid } = bridge();
    const grid = new WebGrid(new WebAxis(5, 20), new WebAxis(7, 50));
    try {
      const declared = [
        0,
        SUPPORT.sort,
        SUPPORT.text,
        SUPPORT.sort | SUPPORT.text | SUPPORT.textRegex,
        SUPPORT.oneOf | SUPPORT.group | SUPPORT.subtotalAfter | SUPPORT.native,
        SUPPORT.compareNumbers | SUPPORT.pivot | SUPPORT.subtotalBefore,
        SUPPORT.sort | SUPPORT.compareDates,
      ];
      grid.begin_frame(400, 200);
      grid.set_window(1, 0, BigUint64Array.from([10n, 11n, 12n, 13n, 14n, 15n, 16n]));
      declared.forEach((bits, i) => grid.set_support(0, BigInt(10 + i), bits));
      const offered = [{ name: "sum", params: [] }, { name: "percentile", params: ["p"] }];
      grid.set_aggregates(0, 15n, offered);
      grid.begin_frame(400, 200);
      const s = decodeSnapshot(grid.snapshot(), grid.snapshot_keys());
      const dump = (grid.debug_frame() as DebugFrame).dump as { cols: { headers: { support: unknown; aggregates: unknown }[] } };
      expect(s.cols.headers.map((h) => h.support)).toEqual(dump.cols.headers.map((h) => h.support));
      expect(s.cols.headers.map((h) => h.aggregates)).toEqual(dump.cols.headers.map((h) => h.aggregates));
      const none = { group: false, native: false, pivot: false, subtotals: { before: false, after: false } };
      expect(s.cols.headers.map((h) => h.support)).toEqual([
        { ...none, sort: false, text: null, oneOf: false, compare: null },
        { ...none, sort: true, text: null, oneOf: false, compare: null },
        { ...none, sort: false, text: { regex: false }, oneOf: false, compare: null },
        { ...none, sort: true, text: { regex: true }, oneOf: false, compare: null },
        { ...none, sort: false, text: null, oneOf: true, compare: null, group: true, native: true, subtotals: { before: false, after: true } },
        { ...none, sort: false, text: null, oneOf: false, compare: "numbers", pivot: true, subtotals: { before: true, after: false } },
        { ...none, sort: true, text: null, oneOf: false, compare: "dates" },
      ]);
      expect(s.cols.headers.map((h) => h.aggregates.length)).toEqual([0, 0, 0, 0, 0, 2, 0]);
      expect(s.cols.headers[5]!.aggregates).toEqual(offered);
    } finally {
      grid.free();
    }
  });
});

describe("Find", () => {
  test("a Find Query a host sets comes back as Rust carries it, and a step's answer becomes the current match", () => {
    const { WebAxis, WebGrid } = bridge();
    const grid = new WebGrid(new WebAxis(5, 20), new WebAxis(3, 80));
    try {
      grid.begin_frame(400, 200);
      grid.set_window(0, 0, BigUint64Array.from([10n, 11n, 12n, 13n, 14n]));
      grid.set_window(1, 0, BigUint64Array.from([20n, 21n, 22n]));
      grid.set_find_support(FIND.on | FIND.regex);
      const query = { query: { text: "Bé🚢", caseSensitive: true, regex: false, searches: [21n] }, by: "cols" };
      grid.set_find(query);
      grid.find_previous();
      const effects = (grid.poll_effects() as { type: string }[]).filter((e) => e.type.startsWith("Find"));
      expect(effects.map(({ generation: _, ...e }: { type: string; generation?: number }) => e)).toEqual([
        { type: "FindRequested", query },
        { type: "FindStepRequested", query, from: null, direction: "previous" },
      ]);
      grid.find_stepped({ row: 13n, col: 21n });
      grid.begin_frame(400, 200);
      const s = decodeSnapshot(grid.snapshot(), grid.snapshot_keys());
      expect([s.find, s.findMatch, s.findSupport]).toEqual([query, { row: 13n, col: 21n }, { regex: true }]);
      expect(s.rows.focus?.key).toBe(13n);
      grid.set_find(null);
      grid.set_find_support(FIND.regex);
      grid.begin_frame(400, 200);
      const closed = decodeSnapshot(grid.snapshot(), grid.snapshot_keys());
      expect([closed.find, closed.findMatch, closed.findSupport]).toEqual([null, null, null]);
    } finally {
      grid.free();
    }
  });
});

describe("Grouping", () => {
  test("outlines a host sends with a Window reach the Snapshot, and Group rows by comes back as a Group request", () => {
    const { WebAxis, WebGrid } = bridge();
    const grid = new WebGrid(new WebAxis(4, 20), new WebAxis(2, 80));
    try {
      grid.begin_frame(400, 200);
      const north = 2n ** 63n;
      grid.set_window(0, 0, BigUint64Array.from([north, 7n, 2n ** 60n, 9n]));
      const outlines = [
        { kind: "group", depth: 0, open: "open", ancestors: [], group: { field: 1n, value: "North" } },
        { kind: "leaf", depth: 1, open: "cantOpen", ancestors: [north], group: null },
        { kind: "group", depth: 0, open: "closed", ancestors: [], group: { field: 1n, value: null } },
        { kind: "grandTotal", depth: 0, open: "cantOpen", ancestors: [], group: null },
      ];
      grid.set_outlines(0, 0, outlines);
      grid.begin_frame(400, 200);
      const s = decodeSnapshot(grid.snapshot(), grid.snapshot_keys());
      expect(s.rows.strip.map((i) => i.outline)).toEqual(outlines);
      expect(s.cols.strip.map((i) => i.outline)).toEqual([null, null]);
      expect(() => grid.set_outlines(0, 0, [{ kind: "branch" }])).toThrow(/outlines/);

      grid.set_window(1, 0, BigUint64Array.from([0n, 1n]));
      grid.group_by(0, 1n);
      expect(grid.poll_effects()).toEqual([]);
      grid.set_support(0, 1n, SUPPORT.group | SUPPORT.sort);
      grid.group_by(0, 1n);
      const [effect] = grid.poll_effects() as { type: string; spec: { grouping: unknown }; edit: unknown }[];
      expect(effect?.edit).toEqual({ type: "Group", axis: "rows", field: 1n });
      expect(effect?.spec.grouping).toMatchObject({ rowLevels: [{ field: 1n, subtotal: "off" }], values: [{ field: null, aggregate: "count" }] });
      grid.begin_frame(400, 200);
      expect(decodeSnapshot(grid.snapshot(), grid.snapshot_keys()).cols.headers[1]?.support.group).toBe(true);
    } finally {
      grid.free();
    }
  });

  test("binnacle-memory groups the rows in view, with Subtotals, a Grand Total and figures, ordered by the one sort rule", () => {
    const memory = new (bridge().WebMemory)(5, 2);
    try {
      const encode = (values: (string | null)[]) => {
        const bytes: number[] = [];
        const ends: number[] = [];
        for (const v of values) {
          bytes.push(...new TextEncoder().encode(v ?? ""));
          ends.push(bytes.length);
        }
        return [Uint8Array.from(bytes), Uint32Array.from(ends), Uint8Array.from(values, (v) => (v === null ? 1 : 0))] as const;
      };
      // Column 1 is the region, column 0 the weight.
      const region = encode(["b", "a", "b", null, "a"]);
      const weights = [5, 1, 2, 10, 4];
      const stage = () => {
        memory.stage_field(1, ...region, new Float64Array(5), Uint8Array.from([1, 1, 1, 1, 1]));
        memory.stage_field(0, ...encode(weights.map(String)), Float64Array.from(weights), new Uint8Array(5));
      };
      const grouping = {
        rowLevels: [{ field: 1n, subtotal: "after" }],
        colLevels: [],
        values: [{ field: 0n, aggregate: "sum" }, { field: null, aggregate: "count" }],
        slicers: [],
        rowAxis: { grandTotal: "after", form: "outline", expand: { depth: "all", except: [] } },
        colAxis: { grandTotal: "off", form: "header", expand: { depth: "all", except: [] } },
      };
      memory.stage_strings(...region);
      memory.sort(0, BigUint64Array.from([1n]), Uint8Array.from([0]));
      stage();
      expect(memory.group(grouping, new Uint32Array(), new Uint8Array(), [])).toBe(12);
      type Item = { key: bigint; item: number | null; outline: Outline<bigint, bigint>; aggregates: (number | null)[] | null };
      const items = memory.grouped_window(0, 12) as Item[];
      const [a, b, blank] = [items[0]!.key, items[4]!.key, items[8]!.key];
      const group = (value: string | null) => ({ kind: "group", depth: 0, open: "open", ancestors: [], group: { field: 1n, value } });
      const leaf = (parent: bigint) => ({ kind: "leaf", depth: 1, open: "cantOpen", ancestors: [parent], group: null });
      const subtotal = (parent: bigint, value: string | null) => ({ ...leaf(parent), kind: "subtotal", group: { field: 1n, value } });
      expect(items.map((i) => [i.item, i.outline])).toEqual([
        [null, group("a")],
        [1, leaf(a)],
        [4, leaf(a)],
        [null, subtotal(a, "a")],
        [null, group("b")],
        [0, leaf(b)],
        [2, leaf(b)],
        [null, subtotal(b, "b")],
        [null, group(null)],
        [3, leaf(blank)],
        [null, subtotal(blank, null)],
        [null, { kind: "grandTotal", depth: 0, open: "cantOpen", ancestors: [], group: null }],
      ]);
      expect(items.map((i) => i.aggregates)).toEqual([
        [5, 2], null, null, [5, 2], [7, 2], null, null, [7, 2], [10, 1], null, [10, 1], [22, 5],
      ]);
      expect(items[11]!.key).toBe(bridge().grand_total_key());
      expect(GRAND_TOTAL).toBe(bridge().grand_total_key());
      expect(Array.from(memory.locate_summary(items[4]!.key))).toEqual([0, 4]);
      expect(Array.from(memory.locate_summary(GRAND_TOTAL))).toEqual([0, 11]);
      expect(Array.from(memory.locate(0, 3))).toEqual([0, 9]);

      // Sorted by the weight, heaviest first: the groups by their sum.
      memory.stage_strings(...region);
      memory.stage_numbers(Float64Array.from(weights), new Uint8Array(5));
      memory.sort(0, BigUint64Array.from([1n, 0n]), Uint8Array.from([0, 1]));
      stage();
      memory.group(grouping, Uint32Array.from([0]), Uint8Array.from([1]), []);
      const sorted = memory.grouped_window(0, 12) as Item[];
      expect(sorted.map((i) => i.item ?? `${i.outline.kind} ${i.outline.group?.value}`)).toEqual([
        "group null", 3, "subtotal null", "group b", 0, 2, "subtotal b", "group a", 4, 1, "subtotal a", "grandTotal undefined",
      ]);
      expect(memory.ungroup()).toBe(5);
    } finally {
      memory.free();
    }
  });
});

describe("Unmatched items", () => {
  test.each([
    ["rows", 0],
    ["columns", 1],
  ])("binnacle-memory keeps edited %s in view, flagged, until the axis is filtered again", (_, a) => {
    const memory = a === 0 ? new (bridge().WebMemory)(4, 1) : new (bridge().WebMemory)(1, 4);
    const order = () => Array.from(memory.window(a, 0, memory.count(a)));
    try {
      expect(memory.filter(a, Uint8Array.from([1, 1, 0, 1]))).toBe(3);
      // The user's edit: item 1 no longer matches but stays; item 2 now matches and comes in.
      expect(memory.defer(a, 1, 0)).toBe(false);
      expect(memory.defer(a, 2, 1)).toBe(true);
      expect(order()).toEqual([0, 1, 2, 3]);
      expect(Array.from(memory.unmatched(a, Uint32Array.from([0, 1, 2, 3, 9])))).toEqual([0, 1, 0, 0, 0]);
      expect(memory.has_unmatched(a)).toBe(true);
      expect(memory.has_unmatched(1 - a)).toBe(false);
      // A change from the data takes item 3 out at once.
      expect(memory.refilter(a, 3, 0)).toBe(true);
      expect(order()).toEqual([0, 1, 2]);
      // Only flags that changed move: item 1 stays unmatched, item 0 goes.
      expect(memory.refilter_all(a, Uint8Array.from([0, 0, 1, 0]))).toBe(true);
      expect(order()).toEqual([1, 2]);
      expect(memory.filter(a, Uint8Array.from([1, 0, 1, 0]))).toBe(2);
      expect(memory.has_unmatched(a)).toBe(false);
    } finally {
      memory.free();
    }
  });
});

describe("Change Notices", () => {
  test("each notice code names the same notice in Rust", () => {
    for (const [type, code] of Object.entries(NOTICE_CODE)) {
      expect(bridge().debug_notice(code, 1, 2).split(/[ ({]/)[0]).toBe(type);
    }
  });
});
