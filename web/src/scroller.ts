// The capped native scroller and the hybrid mapping between its offsets and
// the Grid: small moves go in as `Scroll` (pixel exact at any depth), jumps
// and both ends as `ScrollTo` by fraction, and after each frame the Thumb is
// written back. The sizer is capped, so on a huge table the native thumb
// barely moves while the content scrolls.
//
// Any write to an offset cancels a scroll the browser is animating, so an
// offset the scroller moved itself is left alone while the Grid followed it
// pixel for pixel. On a capped sizer it is placed once the scroller is still.

import { AXES, AXIS_CODE, type AxisName } from "./codes";
import type { AxisSnapshot, Snapshot } from "./snapshot";

/** The scroller element as the mapping uses it; `setSize` sizes its sizer. */
export interface ScrollPort {
  scrollTop: number;
  scrollLeft: number;
  readonly clientWidth: number;
  readonly clientHeight: number;
  setSize(width: number, height: number): void;
}

/** The Grid inputs the mapping drives (a `WebGrid`). */
export interface ScrollTarget {
  scroll(dx: number, dy: number): void;
  scroll_to(axis: number, fraction: number): void;
}

export interface ScrollMapping {
  /** Feeds offsets that changed since the last take or write-back into the Grid. Returns whether any had. */
  take(grid: ScrollTarget): boolean;
  /** Sizes the sizer from the Thumbs and moves the offsets to match the Grid, but for ones the Grid followed. */
  writeBack(s: Snapshot): void;
  /** The scroller has stopped moving. Returns whether the next write-back has an offset to place. */
  settle(): boolean;
}

export interface ScrollOptions {
  /** The largest sizer in CSS px. Chromium manages about 33M, Firefox less. */
  cap?: number;
  /** A move of more than this many viewports is a jump. */
  jump?: number;
}

export const DEFAULT_SIZER_CAP = 10_000_000;

export function createScrollMapping(port: ScrollPort, options: ScrollOptions = {}): ScrollMapping {
  const cap = options.cap ?? DEFAULT_SIZER_CAP;
  const jump = options.jump ?? 1.5;
  // Per axis (rows: vertical, cols: horizontal): the sizer length, and the
  // offset the Grid was last told about or last wrote.
  const size = { rows: 0, cols: 0 };
  const known = { rows: 0, cols: 0 };
  // The distance taken in as `Scroll` since the last write-back, or null after a `ScrollTo`.
  const fed: Record<AxisName, number | null> = { rows: 0, cols: 0 };
  // Whether the offset is the scroller's own, which the Grid followed.
  const following = { rows: false, cols: false };
  const capped = { rows: false, cols: false };
  // Whether a capped sizer's offset was left off the Thumb, to be placed once the scroller is still.
  const owed = { rows: false, cols: false };
  // The scrolling items' places at the last write-back, to see how far the Grid moved since.
  const placed: Record<AxisName, Map<number, number>> = { rows: new Map(), cols: new Map() };
  let still = false;

  const offset = (axis: "rows" | "cols") => (axis === "rows" ? port.scrollTop : port.scrollLeft);
  const view = (axis: "rows" | "cols") => (axis === "rows" ? port.clientHeight : port.clientWidth);

  return {
    take(grid) {
      let changed = false;
      const delta = { rows: 0, cols: 0 };
      for (const axis of AXES) {
        const now = offset(axis);
        if (now === known[axis]) continue;
        changed = true;
        const max = size[axis] - view(axis);
        const moved = now - known[axis];
        // Reaching an end goes to it; a step away from one, maybe of 1 px, is a move.
        const start = now <= 0 && moved < 0;
        const finish = max > 0 && now >= max - 1 && moved > 0;
        if (max <= 0 || start || finish || Math.abs(moved) > view(axis) * jump) {
          // Within a pixel of the last end is the end itself: on a capped sizer a pixel is many rows.
          const fraction = finish ? 1 : max > 0 ? Math.min(1, Math.max(0, now / max)) : 0;
          grid.scroll_to(AXIS_CODE[axis], fraction);
          fed[axis] = null;
        } else {
          delta[axis] = moved;
          if (fed[axis] !== null) fed[axis] += moved;
        }
        known[axis] = now;
        still = false;
      }
      if (delta.rows || delta.cols) grid.scroll(delta.cols, delta.rows);
      return changed;
    },
    writeBack(s) {
      const content = { rows: 0, cols: 0 };
      for (const axis of AXES) {
        const { thumb, scrolling } = s[axis];
        const v = view(axis);
        // The Thumb's size is the scrolling Strip's share of the content it scrolls through; pinned Strips stay put.
        const body = scrolling.end - scrolling.start;
        content[axis] = thumb.size >= 1 ? v : v - body + body / thumb.size;
        size[axis] = Math.round(Math.min(cap, content[axis]));
        capped[axis] = content[axis] > cap;
        following[axis] = followed(axis, s[axis], following[axis]);
        fed[axis] = 0;
      }
      port.setSize(size.cols, size.rows);
      for (const axis of AXES) {
        const v = view(axis);
        const max = Math.max(0, size[axis] - v);
        const { fraction } = s[axis].thumb;
        let target = Math.round(fraction * max);
        // On a capped sizer each pixel covers many rows, so rounding can pin
        // the offset at an end the Grid has not reached, and the next move
        // that way would be lost or read as a jump to the end. Keep up to a
        // viewport of room, never more than the content left that way.
        const travel = content[axis] - v;
        if (max > 2 * v) {
          const before = Math.min(v, Math.ceil(fraction * travel));
          const after = Math.min(v, Math.ceil((1 - fraction) * travel));
          target = Math.min(Math.max(target, before), max - after);
        }
        // Keep the offset clear of an end the Grid has not reached, or the next move would read as a jump there.
        // On a huge table the fraction is only good to a few px.
        const now = offset(axis);
        const slack = 2 * Number.EPSILON * travel;
        const clear =
          now >= Math.min(v, fraction * travel - slack) / 2 &&
          max - now >= Math.min(v, (1 - fraction) * travel - slack) / 2;
        const leave = following[axis] && clear && !(still && capped[axis]);
        const off = Math.abs(target - now) >= 1;
        if (!leave && off) {
          if (axis === "rows") port.scrollTop = target;
          else port.scrollLeft = target;
        }
        owed[axis] = leave && off && capped[axis];
        known[axis] = offset(axis);
      }
    },
    settle() {
      still = true;
      return owed.rows || owed.cols;
    },
  };

  /**
   * Whether the Grid moved by just what was taken in as `Scroll` since the last write-back, as one
   * scrolling item in view both times shows; the Grid moves its items together. Unmoved, it is still
   * following if it `was`.
   */
  function followed(axis: AxisName, a: AxisSnapshot, was: boolean): boolean {
    const before = placed[axis];
    const now = new Map<number, number>();
    for (const item of a.strip.slice(a.pinned, a.strip.length - a.trailing)) now.set(item.position, item.offset);
    placed[axis] = now;
    const moved = fed[axis];
    if (moved === null) return false;
    const common = [...now.keys()].find((position) => before.has(position));
    if (common === undefined) return false;
    const shift = before.get(common)! - now.get(common)!;
    return Math.abs(shift - moved) < 0.5 && (moved !== 0 || was);
  }
}
