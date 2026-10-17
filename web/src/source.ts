// The Table Source interface as far as this slice uses it: counts, Windows
// and cells, answered synchronously, with position identity. The shape
// follows the spec (#53, #47); the rest of it (sort, locate, move, attach,
// Key Interners for the user's own IDs, Promise answers) comes later and only
// adds to this.

export type MaybePromise<T> = T | Promise<T>;

export interface AxisSource<Id, CrossId> {
  count(): MaybePromise<number>;
  window(start: number, end: number, signal: AbortSignal): MaybePromise<Id[]>;
  /** Header text for an item; defaults to `String(id)` for columns and Position + 1 for rows. */
  label?(id: Id, position: number): string;
  /** The axis's IDs are its Positions, so the widget keeps no Key Interner for it. */
  identity?: "position";
}

export interface TableSource<RowId, ColId, V = unknown> {
  rows: AxisSource<RowId, ColId>;
  cols: AxisSource<ColId, RowId>;
  cells(rows: RowId[], cols: ColId[], signal: AbortSignal): MaybePromise<V[][]>;
}
