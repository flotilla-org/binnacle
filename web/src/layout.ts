// A saved layout: the parts of how a grid presents its table that a host
// keeps between sessions, by the Table Source's IDs so the value is plain
// and serialisable. The order of items is not in it: that belongs to the
// source.

import { AXES, AXIS_CODE, EDGE_CODE, type AxisName, type PinEdge } from "./codes";
import type { Feed } from "./feed";
import type { Snapshot } from "./snapshot";
import type { ViewSpec } from "./source";
import type { WebGrid } from "./wasm";

/** The format `layout()` writes; `restoreLayout` restores nothing of another. */
export const LAYOUT_VERSION = 1;

/** Each edge's pinned items, in pin order. */
export interface LayoutPins<Id> {
  leading: Id[];
  trailing: Id[];
}

/**
 * A grid's layout, by the Table Source's IDs: the View Spec (sort, filter
 * and hidden items per axis), the pinned items, every Size Override as an
 * `[id, size]` pair, and the thickness of the Header Band leading each axis
 * (`rows` is the column header band along the top, `cols` the row-number
 * band down the left). Plain data: it survives `JSON.stringify` whenever the
 * IDs do.
 */
export interface Layout<RowId, ColId> {
  version: typeof LAYOUT_VERSION;
  view: ViewSpec<RowId, ColId>;
  pins: { rows: LayoutPins<RowId>; cols: LayoutPins<ColId> };
  sizes: { rows: [RowId, number][]; cols: [ColId, number][] };
  bands: { rows: number; cols: number };
}

export interface LayoutParts<RowId, ColId> {
  readonly grid: WebGrid;
  readonly feed: Feed<RowId, ColId>;
}

const EDGES: PinEdge[] = ["leading", "trailing"];

/** `n`, with -0 as 0, which JSON writes it as. */
const plain = (n: number) => n + 0;

/**
 * The layout as of Snapshot `s`: the View Spec last asked for (else the one
 * applied), pins dormant or not, and Size Overrides for items shown or not.
 * Anything whose ID the grid no longer knows is left out.
 */
export function readLayout<RowId, ColId>(s: Snapshot, { grid, feed }: LayoutParts<RowId, ColId>): Layout<RowId, ColId> {
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
      return known === undefined ? [] : [[known as Id, plain(lengths[i]!)]];
    });
  };
  return {
    version: LAYOUT_VERSION,
    view: feed.viewIds(s.requestedView ?? s.appliedView),
    pins: { rows: pins<RowId>("rows"), cols: pins<ColId>("cols") },
    sizes: { rows: sizes<RowId>("rows"), cols: sizes<ColId>("cols") },
    bands: { rows: plain(s.rows.header), cols: plain(s.cols.header) },
  };
}

/**
 * Puts `layout` back: pins, Size Overrides and band thicknesses at once
 * (overrides it does not list are cleared), and the View Spec as an
 * ordinary request, pending until the Table Source answers. It never fails
 * on what the source lacks: a missing pinned item sleeps until it appears,
 * the source leaves hidden IDs it does not know out of its answer, a
 * condition on a missing item shows as not applied, and a size waits for
 * its item. Returns `false`, restoring nothing, for a layout of another
 * version. Drain the grid's Effects afterwards.
 */
export function restoreLayout<RowId, ColId>(layout: Layout<RowId, ColId>, { grid, feed }: LayoutParts<RowId, ColId>): boolean {
  if (layout.version !== LAYOUT_VERSION) return false;
  const key = (axis: AxisName, id: unknown) => (axis === "rows" ? feed.rowKey(id as RowId) : feed.colKey(id as ColId));
  for (const axis of AXES) {
    const code = AXIS_CODE[axis];
    const pins: LayoutPins<unknown> = layout.pins[axis];
    for (const edge of EDGES) {
      grid.set_pins(code, EDGE_CODE[edge], BigUint64Array.from(pins[edge], (id) => key(axis, id)));
    }
    const sizes = new Map<bigint, number>((layout.sizes[axis] as [unknown, number][]).map(([id, size]) => [key(axis, id), size]));
    for (const k of grid.size_override_keys(code)) {
      if (!sizes.has(k)) grid.clear_size(code, k);
    }
    for (const [k, size] of sizes) grid.set_size(code, k, size);
    grid.set_header(code, layout.bands[axis]);
  }
  grid.request_view(feed.viewKeys(layout.view));
  return true;
}
