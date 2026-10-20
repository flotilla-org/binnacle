// Pinning the focused item, as the Pin and Unpin Actions ask.

import { AXIS_CODE, EDGE_CODE, type AxisName, type PinEdge } from "./codes";
import type { Snapshot } from "./snapshot";
import type { WebGrid } from "./wasm";

/** The Keys drawn in the leading pinned Strip of `axis`, in pin order. */
export function pinnedKeys(s: Snapshot, axis: AxisName): bigint[] {
  return s[axis].strip.slice(0, s[axis].pinned).flatMap((i) => (i.key === null ? [] : [i.key]));
}

/**
 * Pins the focused item of `axis` to `edge`, or unpins it if it is pinned
 * already; with no `edge`, only unpins it. Does nothing while focus is
 * unplaced or its Key unknown.
 */
export function pinFocused(grid: WebGrid, s: Snapshot, axis: AxisName, edge: PinEdge | null): void {
  const key = s[axis].focus?.key;
  if (key == null) return;
  if (edge !== null && !pinnedKeys(s, axis).includes(key)) grid.pin(AXIS_CODE[axis], EDGE_CODE[edge], key);
  else grid.unpin(AXIS_CODE[axis], key);
}
