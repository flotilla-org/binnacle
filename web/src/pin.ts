// Pinning the focused item, as the Pin, Unpin and Freeze Actions ask.

import { AXIS_CODE, EDGE_CODE, type AxisName, type PinEdge } from "./codes";
import type { Snapshot } from "./snapshot";
import type { WebGrid } from "./wasm";

/** The Keys drawn in the pinned Strip at `edge` of `axis`, in pin order. */
export function pinnedKeys(s: Snapshot, axis: AxisName, edge: PinEdge): bigint[] {
  const { strip, pinned, trailing } = s[axis];
  const items = edge === "leading" ? strip.slice(0, pinned) : strip.slice(strip.length - trailing);
  return items.flatMap((i) => (i.key === null ? [] : [i.key]));
}

/**
 * Pins the focused item of `axis` to `edge`, or unpins it if it is pinned
 * there already (one pinned at the other edge moves); with no `edge`, only
 * unpins it. Does nothing while focus is unplaced or its Key unknown.
 */
export function pinFocused(grid: WebGrid, s: Snapshot, axis: AxisName, edge: PinEdge | null): void {
  const key = s[axis].focus?.key;
  if (key == null) return;
  if (edge !== null && !pinnedKeys(s, axis, edge).includes(key)) grid.pin(AXIS_CODE[axis], EDGE_CODE[edge], key);
  else grid.unpin(AXIS_CODE[axis], key);
}

/** Freezes `axis` up to its focused item (see `GridWidget.freeze`). */
export function freezeFocused(grid: WebGrid, s: Snapshot, axis: AxisName): void {
  const key = s[axis].focus?.key;
  if (key != null) grid.freeze(AXIS_CODE[axis], key);
}
