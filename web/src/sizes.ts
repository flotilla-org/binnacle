// Initial sizes per item, chosen by the app by the Table Source's IDs and
// set as Size Overrides, once per Key: a fixed size when the Key first
// arrives in a Window, a fit once the item's visible values are in, a fill
// once the axis's items are all laid out. A Size Override already there (the
// user resized the item first) wins, and since overrides follow Keys, so do
// these through a sort.

import { AXES, AXIS_CODE, type AxisName } from "./codes";
import type { Content } from "./content";
import type { Snapshot } from "./snapshot";
import type { WebGrid } from "./wasm";

/**
 * An item's starting size: CSS px, `"fit"` to its header and visible values
 * (as a double-click on its edge does), `"fill"` for an equal share of the
 * room the axis has left, or `null` or `undefined` for the default. Rows hold
 * one line of text, so a row's fit is the default height.
 */
export type InitialSize = number | "fit" | "fill" | null | undefined;

export interface SizeChoices<RowId, ColId> {
  rows?(id: RowId): InitialSize;
  cols?(id: ColId): InitialSize;
}

export interface SizingParts {
  grid: WebGrid;
  /** The Table Source's ID for a Key, if known. */
  id(axis: AxisName, key: bigint): unknown;
  /** The width that fits a column's header and visible values; `null` while any of them is still to come. */
  fit(s: Snapshot, content: Content, key: bigint): number | null;
}

export interface InitialSizes {
  /** Keys that arrived in a Window on `axis`: fixed sizes are set now, fits and fills wait. */
  arrived(axis: AxisName, keys: Iterable<bigint>): void;
  /** Sets the fits and fills `s` makes possible; returns whether any size changed. */
  settle(s: Snapshot, content: Content): boolean;
  /** Whether `size` is the fit or fill this set for `key` on `axis`: the widget's own, not the app's or the user's. */
  computed(axis: AxisName, key: bigint, size: number): boolean;
}

export function createInitialSizes(choices: SizeChoices<unknown, unknown>, parts: SizingParts): InitialSizes {
  const { grid } = parts;
  const axes = Object.fromEntries(
    AXES.map((axis) => [axis, { seen: new Set<bigint>(), fits: new Set<bigint>(), fills: new Set<bigint>() }]),
  ) as Record<AxisName, { seen: Set<bigint>; fits: Set<bigint>; fills: Set<bigint> }>;
  const sized = (axis: AxisName, key: bigint) => !Number.isNaN(grid.size_override(AXIS_CODE[axis], key));
  /** The fits and fills set, by Key. */
  const computed: Record<AxisName, Map<bigint, number>> = { rows: new Map(), cols: new Map() };
  const compute = (axis: AxisName, key: bigint, size: number) => {
    grid.set_size(AXIS_CODE[axis], key, size);
    computed[axis].set(key, size);
  };

  /** Shares the room left on `axis` between `fills`, once every item is in the Strip; whether any size changed. */
  const fill = (s: Snapshot, axis: AxisName, fills: Set<bigint>): boolean => {
    const { strip, count, header, extent } = s[axis];
    if (extent <= header || strip.some((i) => i.key === null)) return false;
    const sharing = strip.filter((i) => fills.has(i.key!) && !sized(axis, i.key!));
    const first = strip[0];
    const last = strip.at(-1);
    // Items past the Strip mean the axis already overflows: the fills keep the default.
    const room =
      first && last && first.position === 0 && strip.length === count
        ? extent - header - (last.offset + last.size - first.offset) + sharing.reduce((sum, i) => sum + i.size, 0)
        : 0;
    fills.clear();
    const share = Math.floor(room / sharing.length);
    let changed = false;
    for (const i of sharing) {
      if (share <= i.size) continue;
      compute(axis, i.key!, share);
      changed = true;
    }
    return changed;
  };

  return {
    arrived(axis, keys) {
      const choose = choices[axis];
      if (!choose) return;
      const { seen, fits, fills } = axes[axis];
      for (const key of keys) {
        if (seen.has(key)) continue;
        seen.add(key);
        const id = parts.id(axis, key);
        const size = id === undefined ? undefined : choose(id);
        if (typeof size === "number") {
          if (!sized(axis, key)) grid.set_size(AXIS_CODE[axis], key, size);
        } else if (size === "fit" && axis === "cols") fits.add(key);
        else if (size === "fill") fills.add(key);
      }
    },
    settle(s, content) {
      let changed = false;
      for (const axis of AXES) {
        const { fits, fills } = axes[axis];
        let fitted = false;
        for (const key of fits) {
          if (sized(axis, key)) {
            fits.delete(key);
            continue;
          }
          const size = parts.fit(s, content, key);
          if (size === null) continue;
          fits.delete(key);
          compute(axis, key, Math.ceil(size));
          fitted = true;
        }
        // Fills share what is left once the other sizes are known and laid out.
        if (fills.size && !fits.size && !fitted) changed = fill(s, axis, fills) || changed;
        changed ||= fitted;
      }
      return changed;
    },
    computed: (axis, key, size) => computed[axis].get(key) === size,
  };
}
