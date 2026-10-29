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

/** Per axis, the items to place first, in this order; the rest follow in the Table Source's order. An axis left out is not set. */
export interface LayoutOrder<RowId, ColId> {
  rows?: RowId[];
  cols?: ColId[];
}

/**
 * A grid's layout, by the Table Source's IDs: the View Spec (sort, filter
 * and hidden items per axis), the order of items, the pinned items, every
 * Size Override as an `[id, size]` pair, and the thickness of the Header
 * Band leading each axis (`rows` is the column header band along the top,
 * `cols` the row-number band down the left). Each part is optional: one
 * left out is not set, and restoring leaves the grid's own. Plain data: it
 * survives `JSON.stringify` whenever the IDs do. The IDs name every item
 * of their axis as the source does, Group Items included (the View Spec's
 * open groups name them).
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

/** A layout as `readLayout` reads it: every part set but the order, which is set where the grid has one. */
export type ReadLayout<RowId, ColId> = Required<Omit<Layout<RowId, ColId>, "order">> & Pick<Layout<RowId, ColId>, "order">;

export interface LayoutParts<RowId, ColId> {
  readonly grid: WebGrid;
  readonly feed: Feed<RowId, ColId>;
  /** Whether the widget computed `size` for `key` itself (an initial fit or fill), which a layout leaves out. */
  computed?(axis: AxisName, key: bigint, size: number): boolean;
  /** The last Snapshot drawn, if any: the View Spec in force. */
  readonly shown?: Snapshot | null | undefined;
  /** The layout last restored, if any: an order axis the grid has none on (sorted, or its move refused) keeps its. */
  readonly restored?: Layout<RowId, ColId> | undefined;
}

/** Where `restoreLayout` reports the items of an order it could not place. */
export interface RestoreOptions<RowId, ColId> {
  /**
   * The items of the layout's order that were not placed: on an axis its
   * View Spec sorts or groups (a move would undo that), those of a move
   * the Table Source refused, as it answers, and those the source lacks,
   * once it has made the move. Called once per such axis, with the other
   * axis left out.
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

/**
 * The layout as of Snapshot `s`: the View Spec last asked for (else the
 * one applied), pins dormant or not, and Size Overrides for items shown or
 * not, less the sizes the widget computed itself, each set; and an axis's
 * order where the grid has arranged one, the items a restored order and
 * the header moves since placed first, unless a sort or grouping orders
 * the axis. An axis with none keeps the restored layout's order. Anything
 * whose ID the grid no longer knows is left out.
 */
export function readLayout<RowId, ColId>(s: Snapshot, { grid, feed, computed, restored }: LayoutParts<RowId, ColId>): ReadLayout<RowId, ColId> {
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
  const order: LayoutOrder<RowId, ColId> = {};
  for (const axis of AXES) {
    const arranged = sortedOrGrouped(view, axis) ? [] : ids(axis, grid.arranged(AXIS_CODE[axis]));
    const list = arranged.length > 0 ? arranged : restored?.order?.[axis];
    if (list) (order as Record<AxisName, unknown[]>)[axis] = list;
  }
  return {
    version: LAYOUT_VERSION,
    view,
    ...(order.rows || order.cols ? { order } : {}),
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
 * pending until the Table Source answers, and each axis of the order as a
 * move of the listed items to the front, through the source's `move` as a
 * header drop asks for one, unless the grid shows them in place already;
 * they become the axis's arrangement, which `readLayout` reads. It never
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
      const listed: unknown[] | undefined = order[axis];
      if (!listed) continue;
      const report = (ids: unknown[]) => unplaced?.(axis === "rows" ? { rows: ids as RowId[] } : { cols: ids as ColId[] });
      if (inForce && sortedOrGrouped(inForce, axis)) {
        if (listed.length > 0) report(listed);
        continue;
      }
      const byKey = new Map(listed.map((id) => [key(axis, id), id]));
      const keys = [...byKey.keys()];
      if (!grid.arrange(AXIS_CODE[axis], BigUint64Array.from(keys))) continue;
      const hidden: unknown[] = inForce?.[axis].hidden ?? [];
      const shown = keys.filter((k) => !hidden.some((id) => key(axis, id) === k));
      feed.onMoved(axis, keys, (refusal) => {
        if (refusal !== null) return report(listed);
        // The source moves what it has: an item it shows that is missing from the front, it lacks.
        feed.leading(axis, shown.length, (front) => {
          const placed = new Set(front);
          const missing = shown.filter((k) => !placed.has(k)).map((k) => byKey.get(k));
          if (missing.length > 0) report(missing);
        });
      });
    }
  }
  return true;
}
