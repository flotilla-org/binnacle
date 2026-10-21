// What Hide and "Show only these" act on, and the hide cap's rule. Both act
// on the chosen items of an axis: its picks if there are any, else the items
// the selection spans, else the focused item. A selection comes as runs of
// Positions, since the Grid knows Keys only in its Window; the widget asks
// the Table Source for them, and only once the cap allows the hide.

import type { AxisName } from "./codes";
import type { Snapshot, Span } from "./snapshot";

export type Chosen = { type: "keys"; keys: bigint[] } | { type: "runs"; runs: Span[] };

/** What a hide or "show only" did: asked for it, refused it past the cap, or found nothing to hide. */
export type HideOutcome = "hidden" | "capped" | "nothing";

/** The chosen items of `axis` in `s`, if any. */
export function chosen(s: Snapshot, axis: AxisName): Chosen | null {
  const picked = s.picked[axis];
  if (picked.length > 0) return { type: "keys", keys: [...picked] };
  const runs = s.selectionRange?.[axis];
  if (runs && runsLength(runs) > 1) return { type: "runs", runs: runs.map((r) => ({ ...r })) };
  const key = s[axis].focus?.key;
  return key == null ? null : { type: "keys", keys: [key] };
}

/** How many items `c` names. */
export function chosenCount(c: Chosen): number {
  return c.type === "keys" ? c.keys.length : runsLength(c.runs);
}

const runsLength = (runs: Span[]) => runs.reduce((n, r) => n + r.end - r.start, 0);

/** The runs of Positions below `count` that `runs` (sorted, apart) leaves out. */
export function gaps(runs: Span[], count: number): Span[] {
  const out: Span[] = [];
  let at = 0;
  for (const r of runs) {
    if (r.start > at) out.push({ start: at, end: r.start });
    at = r.end;
  }
  if (count > at) out.push({ start: at, end: count });
  return out;
}

/** Whether `c` names the item at `position` with `key`. */
export function chosenHas(c: Chosen, key: bigint, position: number | undefined): boolean {
  if (c.type === "keys") return c.keys.includes(key);
  return position !== undefined && c.runs.some((r) => position >= r.start && position < r.end);
}

/** The hidden Keys of `axis` as the latest request has them, else the applied View Spec. */
export function hiddenKeys(s: Snapshot, axis: AxisName): bigint[] {
  return (s.requestedView ?? s.appliedView)[axis].hidden;
}

/** Whether hiding `adding` more items of `axis` would pass its cap (counting none hidden already twice is the caller's part). */
export function overCap(s: Snapshot, axis: AxisName, adding: number): boolean {
  return hiddenKeys(s, axis).length + adding > s[axis].hideCap;
}

/** How many items "show only" `c` hides: every other item shown. */
export function othersCount(s: Snapshot, axis: AxisName, c: Chosen): number {
  return Math.max(0, s[axis].count - chosenCount(c));
}

/** What to tell the user when a hide on `axis` passed its cap of `cap`: to filter instead, since a filter scales. */
export function capHint(axis: AxisName, cap: number): string {
  const noun = axis === "cols" ? (cap === 1 ? "column" : "columns") : cap === 1 ? "row" : "rows";
  return `Can't hide over ${cap.toLocaleString("en")} ${noun}: use a filter instead`;
}
