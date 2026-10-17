// Click counting: the widget counts presses itself, since browsers disagree
// on `detail` for pointer events (Chrome reports 0).

export interface Press {
  /** Milliseconds, as `Event.timeStamp`. */
  time: number;
  x: number;
  y: number;
  button: number;
}

export interface ClickCounter {
  /** The click count for a press: one more than the press before it if that was close enough, else 1. */
  press(p: Press): number;
}

export interface ClickOptions {
  /** The longest pause between presses of one multi-click, ms. */
  interval?: number;
  /** How far a press may land from the one before, CSS px along each axis. */
  slop?: number;
}

export function createClickCounter({ interval = 500, slop = 4 }: ClickOptions = {}): ClickCounter {
  let last: (Press & { count: number }) | null = null;
  return {
    press(p) {
      const dt = last ? p.time - last.time : -1;
      const close =
        last !== null &&
        p.button === last.button &&
        dt >= 0 &&
        dt < interval &&
        Math.abs(p.x - last.x) <= slop &&
        Math.abs(p.y - last.y) <= slop;
      const count = close ? last!.count + 1 : 1;
      last = { ...p, count };
      return count;
    },
  };
}
