// A Layout: the portable document of how a grid presents its table, by the
// Table Source's IDs so the value is plain and serialisable. Version 2 adds
// the order of items to version 1 and makes every part optional; the Rust
// `binnacle-layout` crate reads and writes the same JSON, and
// `crates/binnacle-layout/layout.schema.json` is its schema.

import { AXES, AXIS_CODE, EDGE_CODE, type AxisName, type PinEdge } from "./codes";
import type { Feed } from "./feed";
import type { Snapshot } from "./snapshot";
import type { ViewSpec } from "./source";
import type { WebGrid } from "./wasm";

/** The format `layout()` writes; `restoreLayout` restores it and version 1, nothing of another. */
export const LAYOUT_VERSION = 2;

/** Each edge's pinned items, in pin order. */
export interface LayoutPins<Id> {
  leading: Id[];
  trailing: Id[];
}

/** Per axis, the items to place first, in this order; the rest follow in the Table Source's order. */
export interface LayoutOrder<RowId, ColId> {
  rows: RowId[];
  cols: ColId[];
}

/**
 * A grid's layout, by the Table Source's IDs: the View Spec (sort, filter
 * and hidden items per axis), the order of items, the pinned items, every
 * Size Override as an `[id, size]` pair, and the thickness of the Header
 * Band leading each axis (`rows` is the column header band along the top,
 * `cols` the row-number band down the left). Each part is optional: one
 * left out is not set, and restoring leaves the grid's own. Plain data: it
 * survives `JSON.stringify` whenever the IDs do.
 */
export interface Layout<RowId, ColId> {
  version: typeof LAYOUT_VERSION;
  view?: ViewSpec<RowId, ColId>;
  order?: LayoutOrder<RowId, ColId>;
  pins?: { rows: LayoutPins<RowId>; cols: LayoutPins<ColId> };
  sizes?: { rows: [RowId, number][]; cols: [ColId, number][] };
  bands?: { rows: number; cols: number };
}

/** A version 1 layout, as saved before the order: every part but the order, each one set. */
export interface LayoutV1<RowId, ColId> {
  version: 1;
  view: ViewSpec<RowId, ColId>;
  pins: { rows: LayoutPins<RowId>; cols: LayoutPins<ColId> };
  sizes: { rows: [RowId, number][]; cols: [ColId, number][] };
  bands: { rows: number; cols: number };
}

export interface LayoutParts<RowId, ColId> {
  readonly grid: WebGrid;
  readonly feed: Feed<RowId, ColId>;
  /** Whether the widget computed `size` for `key` itself (an initial fit or fill), which a layout leaves out. */
  computed?(axis: AxisName, key: bigint, size: number): boolean;
  /** The last Snapshot drawn, if any: where an order's items sit now, and the View Spec in force. */
  readonly shown?: Snapshot | null | undefined;
}

/** Where `restoreLayout` reports the items of an order it could not place. */
export interface RestoreOptions<RowId, ColId> {
  /**
   * The items of the layout's order that were not placed: on an axis its
   * View Spec sorts or groups (a move would undo that), and those of a move
   * the Table Source refused, as it answers. Called once per such axis,
   * with the other axis empty.
   */
  unplaced?(order: LayoutOrder<RowId, ColId>): void;
}

const EDGES: PinEdge[] = ["leading", "trailing"];

/** `n`, with -0 as 0, which JSON writes it as. */
const plain = (n: number) => n + 0;

/** Whether the View Spec orders `axis` itself, by a sort or a grouping, so a layout's order does not apply to it. */
function sortedOrGrouped(view: ViewSpec<unknown, unknown>, axis: AxisName): boolean {
  const g = view.grouping;
  const pivot = !!g && (g.colLevels.length > 0 || g.rowLevels.some((l) => "values" in l));
  const grouped = axis === "rows" ? !!g && (g.rowLevels.length > 0 || pivot) : pivot;
  return grouped || view[axis].sort.length > 0;
}

/** The Keys of every item of `axis` in Position order, if the Snapshot draws them all. */
function orderOf(s: Snapshot, axis: AxisName): bigint[] | undefined {
  const byPosition = new Map(s[axis].strip.map((item) => [item.position, item.key]));
  const keys: bigint[] = [];
  for (let p = 0; p < s[axis].count; p++) {
    const key = byPosition.get(p);
    if (key === undefined || key === null) return undefined;
    keys.push(key);
  }
  return keys;
}

/**
 * The layout as of Snapshot `s`, every part set: the View Spec last asked
 * for (else the one applied), the order, pins dormant or not, and Size
 * Overrides for items shown or not, less the sizes the widget computed
 * itself. An axis's order is its items' IDs in Position order while the
 * Snapshot draws every one of them and neither a sort nor a grouping
 * orders them, and empty otherwise. Anything whose ID the grid no longer
 * knows is left out.
 */
export function readLayout<RowId, ColId>(s: Snapshot, { grid, feed, computed }: LayoutParts<RowId, ColId>): Required<Layout<RowId, ColId>> {
  const id = (axis: AxisName, key: bigint) => (axis === "rows" ? feed.rowId(key) : feed.colId(key));
  const ids = <Id>(axis: AxisName, keys: Iterable<bigint>): Id[] =>
    [...keys].flatMap((key) => {
      const known = id(axis, key);
      return known === undefined ? [] : [known as Id];
    });
  const pins = <Id>(axis: AxisName): LayoutPins<Id> => ({
    leading: ids(axis, s[axis].pins.leading),
    trailing: ids(axis, s[axis].pins.trailing),
  });
  const sizes = <Id>(axis: AxisName): [Id, number][] => {
    const keys = grid.size_override_keys(AXIS_CODE[axis]);
    const lengths = grid.size_override_sizes(AXIS_CODE[axis]);
    return [...keys].flatMap((key, i) => {
      const known = id(axis, key);
      const size = lengths[i]!;
      return known === undefined || computed?.(axis, key, size) ? [] : [[known as Id, plain(size)]];
    });
  };
  const view = feed.viewIds(s.requestedView ?? s.appliedView);
  const order = <Id>(axis: AxisName): Id[] => (sortedOrGrouped(view, axis) ? [] : ids(axis, orderOf(s, axis) ?? []));
  return {
    version: LAYOUT_VERSION,
    view,
    order: { rows: order<RowId>("rows"), cols: order<ColId>("cols") },
    pins: { rows: pins<RowId>("rows"), cols: pins<ColId>("cols") },
    sizes: { rows: sizes<RowId>("rows"), cols: sizes<ColId>("cols") },
    bands: { rows: plain(s.rows.header), cols: plain(s.cols.header) },
  };
}

/**
 * `layout` as version 2: itself, or a version 1 layout, which has the same
 * parts less the order; `undefined` for a layout of another version.
 */
export function migrateLayout<RowId, ColId>(layout: Layout<RowId, ColId> | LayoutV1<RowId, ColId>): Layout<RowId, ColId> | undefined {
  if (layout.version === LAYOUT_VERSION) return layout;
  if (layout.version === 1) return { ...layout, version: LAYOUT_VERSION };
  return undefined;
}

/**
 * Puts `layout` back, each part it sets: pins, Size Overrides and band
 * thicknesses at once (overrides it does not list are cleared, but for
 * sizes the widget computed itself), the View Spec as an ordinary request,
 * pending until the Table Source answers, and the order as a move of the
 * listed items to the front, through the source's `move` as a header drop
 * asks for one, unless the Snapshot shows them in place already. It never
 * fails on what the source lacks: a missing pinned item sleeps until it
 * appears, the source leaves hidden IDs it does not know out of its answer,
 * a condition on a missing item shows as not applied, and a size waits for
 * its item. An order it can't place goes to `unplaced`. Returns `false`,
 * restoring nothing, for a layout of another version than 1 or 2. Drain the
 * grid's Effects afterwards.
 */
export function restoreLayout<RowId, ColId>(
  layout: Layout<RowId, ColId> | LayoutV1<RowId, ColId>,
  { grid, feed, computed, shown }: LayoutParts<RowId, ColId>,
  { unplaced }: RestoreOptions<RowId, ColId> = {},
): boolean {
  const migrated = migrateLayout(layout);
  if (!migrated) return false;
  const { view, order, pins, sizes, bands } = migrated;
  const key = (axis: AxisName, id: unknown) => (axis === "rows" ? feed.rowKey(id as RowId) : feed.colKey(id as ColId));
  for (const axis of AXES) {
    const code = AXIS_CODE[axis];
    if (pins) {
      const edges: LayoutPins<unknown> = pins[axis];
      for (const edge of EDGES) {
        grid.set_pins(code, EDGE_CODE[edge], BigUint64Array.from(edges[edge], (id) => key(axis, id)));
      }
    }
    if (sizes) {
      const listed = new Map<bigint, number>((sizes[axis] as [unknown, number][]).map(([id, size]) => [key(axis, id), size]));
      const keys = grid.size_override_keys(code);
      const lengths = grid.size_override_sizes(code);
      keys.forEach((k, i) => {
        if (!listed.has(k) && !computed?.(axis, k, lengths[i]!)) grid.clear_size(code, k);
      });
      for (const [k, size] of listed) grid.set_size(code, k, size);
    }
    if (bands) grid.set_header(code, bands[axis]);
  }
  if (view) grid.request_view(feed.viewKeys(view));
  if (order) {
    const spec = shown && (shown.requestedView ?? shown.appliedView);
    const inForce = view ?? (spec ? feed.viewIds(spec) : undefined);
    for (const axis of AXES) {
      const ids: unknown[] = order[axis];
      if (ids.length === 0) continue;
      const report = () => unplaced?.(axis === "rows" ? { rows: ids as RowId[], cols: [] } : { rows: [], cols: ids as ColId[] });
      if (inForce && sortedOrGrouped(inForce, axis)) {
        report();
        continue;
      }
      const keys = ids.map((id) => key(axis, id));
      const at = new Map(shown?.[axis].strip.map((item) => [item.position, item.key]));
      if (keys.every((k, p) => at.get(p) === k)) continue;
      feed.onMoved(axis, keys, (refusal) => {
        if (refusal !== null) report();
      });
      grid.request_move(AXIS_CODE[axis], BigUint64Array.from(keys), 0);
    }
  }
  return true;
}
