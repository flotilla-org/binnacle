// The Table Source interface (#47): one object for order and values, split
// per axis so every ID type follows from the axis being implemented. Every
// request may answer at once or with a Promise; the widget settles both the
// same way. Omitted optional methods behave like the Rust `PumpSource`
// defaults: no View Spec applied, every Locate `Gone`, every move refused.

import type { Primitive } from "./interner";
import type { RendererChoice } from "./renderer";
import type { SortDirection } from "./snapshot";

export type MaybePromise<T> = T | Promise<T>;

export type { Primitive, SortDirection };

/** The items of the other axis to sort an axis by (columns, for the rows), most significant first. */
export type SortSpec<Id> = { id: Id; direction: SortDirection }[];

/**
 * Text to look for in the items of the other axis (columns, for the rows):
 * an item is kept if some searched column's value contains it. Case is
 * ignored unless `caseSensitive`; with `regex` the text is a pattern in the
 * Table Source's own syntax. `searches` is `"shown"` for the columns shown
 * now, or names the columns.
 */
export interface TextQuery<CrossId> {
  text: string;
  caseSensitive: boolean;
  regex: boolean;
  searches: "shown" | CrossId[];
}

/** Which items of an axis a View Spec keeps; all of them when every part is empty. */
export interface FilterSpec<CrossId> {
  /** The quick filter: one Text Query across columns, or `null` for none. */
  quick: TextQuery<CrossId> | null;
}

/** One axis's part of a View Spec, naming items of the other axis. */
export interface AxisView<CrossId> {
  filter: FilterSpec<CrossId>;
  sort: SortSpec<CrossId>;
}

/** The view the Table Source should present for the whole table, one part per axis. */
export interface ViewSpec<RowId, ColId> {
  rows: AxisView<ColId>;
  cols: AxisView<RowId>;
}

/** Each axis's count before any filter, where the Table Source knows it. */
export interface Totals {
  rows?: number;
  cols?: number;
}

/** One axis's part of the View Spec a Table Source applies; an omitted filter is none. */
export type AppliedAxisView<CrossId> = Omit<AxisView<CrossId>, "filter"> & { filter?: FilterSpec<CrossId> };

/** The View Spec a Table Source applies, with its totals for "1,204 of 5,001". */
export interface AppliedView<RowId, ColId> {
  rows: AppliedAxisView<ColId>;
  cols: AppliedAxisView<RowId>;
  totals?: Totals;
}

/** How an axis takes a quick filter. */
export interface QuickFilterSupport {
  /** Whether a Text Query may be a regex; `false` by default. */
  regex?: boolean;
}

/** How an axis may be viewed by one item of the other axis; an omitted kind takes its default. */
export interface Support {
  /** Whether the axis may be sorted by the item; `true` by default while the source has `view`. */
  sort?: boolean;
}

/** Where an item sits now, or that it left the axis (with a Position for focus to fall back to). */
export type Located = { type: "Found"; position: number } | { type: "Gone"; suggest: number | null };

/** How an axis changed, pushed by the Table Source through its sink. */
export type ChangeNotice =
  /** The same items in a new order. */
  | { type: "Reordered" }
  /** Anything may have changed; the widget asks for the count again. */
  | { type: "Reset" }
  /**
   * The same items, some left out or brought back and possibly reordered (a
   * filter changed); the widget asks for the count again. Unlike a Reset,
   * values, picks, row heights and the scroll position follow their IDs.
   */
  | { type: "Refiltered" }
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
  | { type: "view" }
  | { type: "locate"; axis: "rows" | "cols" }
  | { type: "move"; axis: "rows" | "cols" };

export interface AxisSink<_Id> {
  notify(notice: ChangeNotice): void;
}

export interface AxisMethods<Id, CrossId> {
  count(): MaybePromise<number>;
  /** The IDs at Positions `start..end` (fewer past the end). */
  window(start: number, end: number, signal: AbortSignal): MaybePromise<Id[]>;
  /** How this axis may be viewed by `by`; asked as `by` comes into view. */
  support?(by: CrossId): Support;
  /** Set if this axis takes a quick filter (the source must also have `view`). */
  quickFilter?: QuickFilterSupport;
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
  /**
   * Presents the table as `spec` asks and returns the View Spec now in force:
   * the request, the part of it applied, or the old one to reject it, with
   * the totals if known. The reorder or refilter itself arrives through the
   * sinks as Change Notices.
   */
  view?(spec: ViewSpec<RowId, ColId>, signal: AbortSignal): MaybePromise<AppliedView<RowId, ColId>>;
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
