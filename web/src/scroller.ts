// The capped native scroller and the hybrid mapping between its offsets and
// the Grid: small moves go in as `Scroll` (pixel exact at any depth), jumps
// and both ends as `ScrollTo` by fraction, and after each frame the Thumb is
// written back. The sizer is capped, so on a huge table the native thumb
// barely moves while the content scrolls.

import { AXES, AXIS_CODE } from "./codes";
import type { Snapshot } from "./snapshot";

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
  /** Sizes the sizer from the Thumbs and moves the offsets to match the Grid. */
  writeBack(s: Snapshot): void;
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
        if (max <= 0 || now <= 0 || now >= max - 1 || Math.abs(moved) > view(axis) * jump) {
          grid.scroll_to(AXIS_CODE[axis], max > 0 ? Math.min(1, Math.max(0, now / max)) : 0);
        } else {
          delta[axis] = moved;
        }
        known[axis] = now;
      }
      if (delta.rows || delta.cols) grid.scroll(delta.cols, delta.rows);
      return changed;
    },
    writeBack(s) {
      const content = { rows: 0, cols: 0 };
      for (const axis of AXES) {
        const { header, thumb } = s[axis];
        const v = view(axis);
        content[axis] = thumb.size >= 1 ? v : header + (v - header) / thumb.size;
        size[axis] = Math.round(Math.min(cap, content[axis]));
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
        if (Math.abs(target - offset(axis)) >= 1) {
          if (axis === "rows") port.scrollTop = target;
          else port.scrollLeft = target;
        }
        known[axis] = offset(axis);
      }
    },
  };
}
