// The Table Source interface (#47): one object for order and values, split
// per axis so every ID type follows from the axis being implemented. Every
// request may answer at once or with a Promise; the widget settles both the
// same way. Omitted optional methods behave like the Rust `PumpSource`
// defaults: no sort, every Locate `Gone`, every move refused.

import type { Primitive } from "./interner";
import type { RendererChoice } from "./renderer";
import type { SortDirection } from "./snapshot";

export type MaybePromise<T> = T | Promise<T>;

export type { Primitive, SortDirection };

/** The columns (on the rows axis) to sort by, most significant first. */
export type SortSpec<Id> = { id: Id; direction: SortDirection }[];

/** Where an item sits now, or that it left the axis (with a Position for focus to fall back to). */
export type Located = { type: "Found"; position: number } | { type: "Gone"; suggest: number | null };

/** How an axis changed, pushed by the Table Source through its sink. */
export type ChangeNotice =
  /** The same items in a new order. */
  | { type: "Reordered" }
  /** Anything may have changed; the widget asks for the count again. */
  | { type: "Reset" }
  /** `n` items inserted before Position `at`. */
  | { type: "Inserted"; at: number; n: number }
  /** The `n` items from Position `at` removed. */
  | { type: "Removed"; at: number; n: number }
  /** The values of the items at Positions `start..end` changed; their IDs and order did not. */
  | { type: "ContentChanged"; start: number; end: number };

/**
 * A Table Source request that failed (rejected, or threw), as `onError`
 * reports it. The cells of a failed `window` or `cells` request show an
 * error marker until a Change Notice covering them, or a scroll to a new
 * Window, asks again and the answer lands.
 */
export type FailedRequest<RowId = unknown, ColId = unknown> =
  | { type: "count"; axis: "rows" | "cols" }
  | { type: "window"; axis: "rows" | "cols"; start: number; end: number }
  | { type: "cells"; rowIds: RowId[]; colIds: ColId[] }
  | { type: "sort" }
  | { type: "locate"; axis: "rows" | "cols" }
  | { type: "move"; axis: "rows" | "cols" };

export interface AxisSink<_Id> {
  notify(notice: ChangeNotice): void;
}

export interface AxisMethods<Id, CrossId> {
  count(): MaybePromise<number>;
  /** The IDs at Positions `start..end` (fewer past the end). */
  window(start: number, end: number, signal: AbortSignal): MaybePromise<Id[]>;
  /**
   * Sorts by `spec` and returns the Sort Spec now in force (the old one to
   * reject). The reorder itself arrives through the sink as a Change Notice.
   */
  sort?(spec: SortSpec<CrossId>, signal: AbortSignal): MaybePromise<SortSpec<CrossId>>;
  /** Whether clicking `by`'s header may sort by it; all are while `sort` exists. */
  sortable?(by: CrossId): boolean;
  /** Where `id` sits now, after a reorder. */
  locate?(id: Id, signal: AbortSignal): MaybePromise<Located>;
  /** Moves `ids` to before Position `to`; `false` refuses. The widget sends the `Reordered`. */
  move?(ids: Id[], to: number): MaybePromise<boolean>;
  /** Header text for an item; defaults to `String(id)` for columns and Position + 1 for rows. */
  label?(id: Id, position: number): string;
  /** The axis's IDs are its Positions, so the widget keeps no Key Interner for it. */
  identity?: "position";
}

/**
 * One axis of a Table Source. IDs that are not strings, numbers or bigints
 * (tuples, paths, objects) must come with `idKey`, their canonical primitive
 * form, so that equal IDs match by value rather than by reference.
 */
export type AxisSource<Id, CrossId> = AxisMethods<Id, CrossId> &
  ([Id] extends [Primitive] ? { idKey?: (id: Id) => Primitive } : { idKey: (id: Id) => Primitive });

export interface TableSource<RowId, ColId, V = unknown> {
  rows: AxisSource<RowId, ColId>;
  cols: AxisSource<ColId, RowId>;
  /** The values where `rows` and `cols` cross, one array per row. */
  cells(rows: RowId[], cols: ColId[], signal: AbortSignal): MaybePromise<V[][]>;
  /** How each cell draws unless the widget is given its own `renderer`; plain text by default. */
  renderer?: RendererChoice<RowId, ColId, V>;
  /**
   * What the body says when an axis has no items, such as "No rows match the
   * filter"; `undefined` leaves the widget's `emptyMessage`. Asked on each
   * paint of an empty table.
   */
  emptyMessage?(): string | undefined;
  /** Hands the source its sinks for Change Notices; returns how to detach. */
  attach?(sink: { rows: AxisSink<RowId>; cols: AxisSink<ColId> }): () => void;
}
