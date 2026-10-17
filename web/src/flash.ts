// Host-driven value-change flashes: the host marks a cell (or a whole row)
// whose value changed, and the painter lights it in the Theme's flash style,
// fading over `FLASH_MS`.

/** How long a flash takes to fade, ms. */
export const FLASH_MS = 800;

export interface Flashes {
  /** Lights the cell at `row` and `col` (every cell of the row for `null`) from `now`. */
  mark(row: bigint, col: bigint | null, now: number): void;
  /** How lit the cell is at `now`: 1 when just marked, fading to 0. */
  amount(row: bigint, col: bigint, now: number): number;
  /** Forgets the faded flashes; returns whether any is still lit at `now`. */
  sweep(now: number): boolean;
}

const ROW = -1n;

export function createFlashes(): Flashes {
  const byRow = new Map<bigint, Map<bigint, number>>();
  const lit = (at: number | undefined, now: number) => (at === undefined ? 0 : Math.max(0, 1 - (now - at) / FLASH_MS));
  return {
    mark(row, col, now) {
      let cols = byRow.get(row);
      if (!cols) byRow.set(row, (cols = new Map()));
      cols.set(col ?? ROW, now);
    },
    amount(row, col, now) {
      const cols = byRow.get(row);
      if (!cols) return 0;
      return Math.min(1, Math.max(lit(cols.get(col), now), lit(cols.get(ROW), now)));
    },
    sweep(now) {
      for (const [row, cols] of byRow) {
        for (const [col, at] of cols) if (lit(at, now) === 0) cols.delete(col);
        if (cols.size === 0) byRow.delete(row);
      }
      return byRow.size > 0;
    },
  };
}
