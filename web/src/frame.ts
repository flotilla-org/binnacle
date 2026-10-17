// One frame of the widget, in the spec's order: take in any pending scroll
// offset; begin the frame and answer its Windows; drain Effects; take the
// Snapshot and paint if anything changed; write the scroll position back.

import { isEffect, type Effect } from "./effects";
import type { Feed } from "./feed";
import type { Painter } from "./painter";
import type { ScrollMapping } from "./scroller";
import { decodeSnapshot, type Snapshot } from "./snapshot";
import type { WebGrid } from "./wasm";

export interface FrameParts {
  grid: WebGrid;
  scroll: ScrollMapping;
  feed: Feed;
  painter: Painter | null;
  /** The Grid's viewport in CSS px. */
  viewport(): { width: number; height: number };
  onEffect(effect: Effect): void;
  /** Sets sizes the Snapshot makes possible; `true` skips this frame's paint for another frame. */
  settle?(s: Snapshot): boolean;
}

export interface FrameResult {
  snapshot: Snapshot;
  painted: boolean;
  /** The painter asked for another frame. */
  again: boolean;
}

/** Drains and dispatches every queued Effect. Also run after each DOM-driven input. */
export function drainEffects(parts: FrameParts): void {
  const effects = parts.grid.poll_effects() as unknown[];
  for (const effect of effects) {
    if (!isEffect(effect)) throw new Error(`binnacle: unknown Effect ${String((effect as { type?: unknown }).type)}`);
    parts.onEffect(effect);
  }
}

/** Runs one frame. `force` paints even without Damage (a resize or DPR change). */
export function runFrame(parts: FrameParts, force: boolean): FrameResult {
  const { grid } = parts;
  parts.scroll.take(grid);
  const { width, height } = parts.viewport();
  parts.feed.fill(grid, grid.begin_frame(width, height));
  drainEffects(parts);
  const snapshot = decodeSnapshot(grid.snapshot(), grid.snapshot_keys());
  if (parts.settle?.(snapshot)) {
    parts.scroll.writeBack(snapshot);
    return { snapshot, painted: false, again: true };
  }
  const { damage } = snapshot;
  let painted = false;
  let again = false;
  if (parts.painter && (parts.feed.takeLanded() || force || damage.layout || damage.selection || damage.positions)) {
    again = parts.painter.paint(snapshot, parts.feed.content(snapshot));
    painted = true;
  }
  parts.scroll.writeBack(snapshot);
  return { snapshot, painted, again };
}
