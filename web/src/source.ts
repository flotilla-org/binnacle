// The Table Source interface (#47): one object for order and values, split
// per axis so every ID type follows from the axis being implemented. Every
// request may answer at once or with a Promise; the widget settles both the
// same way. Omitted optional methods behave like the Rust `PumpSource`
// defaults: no View Spec applied, every Locate `Gone`, nothing movable.

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

/** How a Text condition matches its column's values: a Text Query without columns to search. */
export type TextMatch = Omit<TextQuery<never>, "searches">;

/**
 * A value as a OneOf condition lists it: text in the Table Source's own text
 * form, or `null` for a blank (no value at all, which is not empty text).
 */
export type ListedValue = string | null;

/**
 * A number, or a date as milliseconds since the Unix epoch (UTC; a date
 * without a time of day is its midnight, as `Date.UTC` gives it), that a
 * comparison compares values with.
 */
export type Scalar = { type: "number"; value: number } | { type: "date"; value: number };

/** The comparisons with one scalar. */
export type CompareOp = "equal" | "notEqual" | "less" | "atMost" | "greater" | "atLeast";

/**
 * How a `Compare` condition compares a value: with one scalar, or between
 * `low` and `high`, both included (a `low` past `high` keeps nothing).
 */
export type Comparison = { op: CompareOp; value: Scalar } | { op: "between"; low: Scalar; high: Scalar };

/** What a condition asks of the values in its item. */
export type ConditionKind =
  | { type: "Text"; query: TextMatch }
  | { type: "OneOf"; values: ListedValue[] }
  | { type: "Compare"; comparison: Comparison };

/**
 * One condition on the items of an axis, read in one item of the other axis
 * (a column, for the rows): an item is kept if its value in column `id`
 * satisfies `kind`, or with `not` if it does not. A `Text` kind matches as a
 * Text Query does, in that column alone; a blank never matches, so `not`
 * keeps blanks. A `OneOf` kind keeps the values it lists exactly, and blanks
 * only if it lists `null`; listing none keeps nothing. A `Compare` kind keeps
 * the numbers or dates that compare so; a blank, a NaN or a value of the
 * other kind never does, so `not` keeps them.
 */
export interface Condition<CrossId> {
  id: CrossId;
  kind: ConditionKind;
  not: boolean;
}

/** Which items of an axis a View Spec keeps; all of them when every part is empty. */
export interface FilterSpec<CrossId> {
  /** The quick filter: one Text Query across columns, or `null` for none. */
  quick: TextQuery<CrossId> | null;
  /** Column conditions, every one of which must keep an item; several may name one column. */
  conditions: Condition<CrossId>[];
}

/**
 * One axis's part of a View Spec: its filter and sort, naming items of the
 * other axis, and the axis's own items to leave out of view whatever the
 * filter keeps (always empty unless the axis has `hide`).
 */
export interface AxisView<CrossId, Id = unknown> {
  filter: FilterSpec<CrossId>;
  sort: SortSpec<CrossId>;
  hidden: Id[];
}

/** Where a Subtotal or a Grand Total goes: before its group (or the axis), after it, or nowhere. */
export type Placement = "before" | "after" | "off";

/**
 * One level of an axis's grouping, outermost first: the values of a base
 * field, each group with a Subtotal placed as `subtotal` says, or the values
 * pseudo-level (one item per value, where several are shown).
 */
export type Level<FieldId> = { field: FieldId; subtotal: Placement } | { values: true };

/**
 * One value a grouped table shows: an aggregate (an open name such as
 * `sum`, `count`, `average`, `minimum`, `maximum`, `distinct count`, or one
 * the Table Source declares) over a base field, or over nothing (`null`)
 * for a count of items.
 */
export interface AggregateValue<FieldId> {
  field: FieldId | null;
  aggregate: string;
}

/**
 * How one grouped axis is laid out: its Grand Total's place, outline form
 * (each group a Group Item at its own Position) or header form (each level a
 * tier of the cross Header Band), and which groups are open: the first
 * `depth` levels, except the Group Items `except` names, which are the
 * other way.
 */
export interface GroupedAxis<ItemId = unknown> {
  grandTotal: Placement;
  form: "outline" | "header";
  expand: { depth: number | "all"; except: ItemId[] };
}

/**
 * The whole-table grouping part of a View Spec, naming the Table Source's
 * base fields (its items before any pivot; for a plain records table, the
 * column IDs). No column levels: no pivot, and the Leaves stay.
 */
export interface Grouping<FieldId, RowId = unknown, ColId = unknown> {
  rowLevels: Level<FieldId>[];
  colLevels: Level<FieldId>[];
  values: AggregateValue<FieldId>[];
  /** Conditions on the base fields, applied before aggregating. */
  slicers: Condition<FieldId>[];
  rowAxis: GroupedAxis<RowId>;
  colAxis: GroupedAxis<ColId>;
}

/** A grouping of the rows by `field`, counting each group, laid out as by default. */
export function rowsBy<FieldId>(field: FieldId): Grouping<FieldId, never, never> {
  return {
    rowLevels: [{ field, subtotal: "off" }],
    colLevels: [],
    values: [{ field: null, aggregate: "count" }],
    slicers: [],
    rowAxis: { grandTotal: "off", form: "outline", expand: { depth: "all", except: [] } },
    colAxis: { grandTotal: "off", form: "header", expand: { depth: "all", except: [] } },
  };
}

/**
 * The view the Table Source should present for the whole table, one part per
 * axis, and its `grouping`: `null` (or absent) for a plain table. A grouped
 * table's base fields are the column IDs.
 */
export interface ViewSpec<RowId, ColId> {
  rows: AxisView<ColId, RowId>;
  cols: AxisView<RowId, ColId>;
  grouping?: Grouping<ColId, RowId, ColId> | null;
}

/**
 * What an item of a grouped axis is: a Leaf from the data, or a Summary
 * Item (a Group Item heading a group, a Subtotal, or the Grand Total).
 */
export type ItemKind = "leaf" | "group" | "subtotal" | "grandTotal";

/**
 * Where an item sits in a grouped axis, one per item of a Window: its kind,
 * its depth (0 for the top groups; a Leaf of a group at depth `d` is at
 * `d + 1`), whether it is open (Leaves, Subtotals and the Grand Total
 * can't), the IDs of its ancestor Group Items from the outermost down to its
 * parent (`depth` of them), and on Group Items and Subtotals their level's
 * field and the group's value.
 */
export interface Outline<Id, FieldId> {
  kind: ItemKind;
  depth: number;
  open: "open" | "closed" | "cantOpen";
  ancestors: Id[];
  group?: { field: FieldId; value: ListedValue } | null;
}

/** A Window of a grouped axis: its IDs, and each one's outline. */
export interface OutlinedWindow<Id, CrossId> {
  ids: Id[];
  outlines: Outline<Id, CrossId>[];
}

/**
 * What find looks for: `query` matched against each cell of the columns it
 * searches, as a quick filter matches, and the order find steps through the
 * matches in: `rows` goes along each row's columns, then the next row;
 * `cols` down each column's rows, then the next column. Both follow the view.
 */
export interface FindQuery<ColId> {
  query: TextQuery<ColId>;
  by: "rows" | "cols";
}

/** A cell that matches the Find Query, by its row and column. */
export interface FindMatch<RowId, ColId> {
  row: RowId;
  col: ColId;
}

/**
 * Where a find step landed: the match, its 0-based index among all the
 * matches in the Find Query's order if the source knows it, and whether the
 * step wrapped past the end (or the start).
 */
export interface FindStep<RowId, ColId> extends FindMatch<RowId, ColId> {
  index?: number;
  wrapped?: boolean;
}

/** How many cells match, and whether the source stopped counting early (`total` is then a floor). */
export interface FindCount {
  total: number;
  partial?: boolean;
}

/**
 * Find, worked out by the Table Source: which cells match, where the next
 * match is, and how many there are. The widget never sees a value to match.
 * A step to a match outside the Window is placed with the axes' `locate`.
 */
export interface FindMethods<RowId, ColId> {
  /** Whether a Find Query may be a regex; `false` by default. */
  regex?: boolean;
  /** Whether each cell where `rowIds` and `colIds` cross matches, one array per row. */
  marks(rowIds: RowId[], colIds: ColId[], query: FindQuery<ColId>, signal: AbortSignal): MaybePromise<boolean[][]>;
  /**
   * The first match after `from` in the query's order (or before it, for
   * `previous`), wrapping round; from the start (or the end) if `from` is
   * `null` or not in the view. `null` if nothing matches.
   */
  next(
    from: FindMatch<RowId, ColId> | null,
    direction: "next" | "previous",
    query: FindQuery<ColId>,
    signal: AbortSignal,
  ): MaybePromise<FindStep<RowId, ColId> | null>;
  /**
   * How many cells in the view match: a partial count to start with is fine,
   * with running counts pushed through the sink's `findCounted`.
   */
  count?(query: FindQuery<ColId>, signal: AbortSignal): MaybePromise<FindCount>;
}

/** Each axis's count before any filter, where the Table Source knows it. */
export interface Totals {
  rows?: number;
  cols?: number;
}

/**
 * One axis's part of the View Spec a Table Source applies; an omitted
 * filter, or part of one, is none, and so is an omitted `hidden`. A source
 * that hides answers with the hidden items it has, leaving out IDs it does
 * not know.
 */
export type AppliedAxisView<CrossId, Id = unknown> = Omit<AxisView<CrossId, Id>, "filter" | "hidden"> & {
  filter?: Partial<FilterSpec<CrossId>>;
  hidden?: Id[];
};

/**
 * The View Spec a Table Source applies, with its totals for "1,204 of 5,001",
 * and why it left out part of what was asked for (an invalid regex, say),
 * which the control the part came from shows.
 */
export interface AppliedView<RowId, ColId> {
  rows: AppliedAxisView<ColId, RowId>;
  cols: AppliedAxisView<RowId, ColId>;
  /** The grouping applied; none if omitted. */
  grouping?: Grouping<ColId, RowId, ColId> | null;
  totals?: Totals;
  error?: string;
}

/** How an axis takes a quick filter. */
export interface QuickFilterSupport {
  /** Whether a Text Query may be a regex; `false` by default. */
  regex?: boolean;
}

/**
 * How an axis may be viewed by one item of the other axis; an omitted kind
 * takes its default. Without `view` nothing is supported.
 */
export interface Support {
  /** Whether the axis may be sorted by the item; `true` by default while the source has `view`. */
  sort?: boolean;
  /**
   * Whether the axis may be filtered by Text conditions on the item, and with
   * regexes; `false` for none. Contains without regex by default while the
   * source has `view`.
   */
  text?: { regex?: boolean } | false;
  /**
   * Whether the axis may be filtered by OneOf conditions on the item, picked
   * from a checklist of its values; `true` by default while the source has
   * `view` and the axis has `distinct`, and never without them.
   */
  oneOf?: boolean;
  /**
   * Whether the axis may be filtered by Compare conditions on the item, and
   * on which kind of value; none by default.
   */
  compare?: "numbers" | "dates" | null;
  /**
   * Whether the axis may be grouped by the item, a base field; `false` by
   * default. A source that groups answers a View Spec's `grouping` and
   * sends outlines with its Windows.
   */
  group?: boolean;
}

/**
 * A checklist's question: the distinct values of one item of the other axis
 * (a column, for the rows), up to `limit` of them, containing `search`
 * (ignoring case; all when empty, and a search lists no blanks), among the
 * items `narrow` keeps. `narrow` is the applied filter less the item's own
 * conditions; a source that can't narrow ignores it.
 */
export interface DistinctQuery<CrossId> {
  search: string;
  limit: number;
  narrow: FilterSpec<CrossId>;
}

/**
 * The answer: the values in the item's natural order (numbers numerically,
 * dates chronologically), each in the source's text form with how many
 * items hold it if counted; `blanks`, a count or `true`, if some are blank;
 * `partial` if there were more than the limit; `narrowed` if `narrow` was
 * applied.
 */
export interface Distinct {
  values: { text: string; count?: number }[];
  blanks?: number | true;
  partial: boolean;
  narrowed: boolean;
}

/**
 * Where an item sits now, or that it left the axis. `suggest` is where it would sit among the items
 * left: the next one's Position in the order it had, or the count when none follows. Focus falls
 * back there, and a selection range's corner pulls in from it; `null` clears a range across it.
 */
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
  | { type: "distinct"; axis: "rows" | "cols" }
  | { type: "locate"; axis: "rows" | "cols" }
  | { type: "unmatched"; axis: "rows" | "cols" }
  | { type: "move"; axis: "rows" | "cols" }
  | { type: "find"; request: "marks" | "next" | "count" };

export interface AxisSink<_Id, CrossId = unknown> {
  notify(notice: ChangeNotice): void;
  /**
   * How this axis may be viewed by the items `ids` of the other axis (every
   * item, without `ids`) changed: the widget asks `support` again for those
   * in view. A condition in force that is no longer supported stays until
   * `viewChanged` leaves it out, then shows as not applied.
   */
  supportChanged(ids?: CrossId[]): void;
  /** Whether this axis's items can move changed: the widget asks `movable` again. */
  movableChanged(): void;
}

/** The sinks a Table Source pushes its news through. */
export interface TableSink<RowId, ColId> {
  rows: AxisSink<RowId, ColId>;
  cols: AxisSink<ColId, RowId>;
  /**
   * The View Spec the source applies now, unprompted (not an answer to
   * `view`), or new totals for the one in force. A part of the request it
   * leaves out shows as not applied; answers still due stay due.
   */
  viewChanged(view: AppliedView<RowId, ColId>): void;
  /**
   * A running count for `query` (the object `find.count` was asked with):
   * push one as counting goes, with `partial` until the last.
   */
  findCounted(query: FindQuery<ColId>, count: FindCount): void;
}

export interface AxisMethods<Id, CrossId> {
  count(): MaybePromise<number>;
  /**
   * The IDs at Positions `start..end` (fewer past the end), with their
   * outlines while the axis is grouped.
   */
  window(start: number, end: number, signal: AbortSignal): MaybePromise<Id[] | OutlinedWindow<Id, CrossId>>;
  /** How this axis may be viewed by `by`; asked as `by` comes into view. */
  support?(by: CrossId): Support;
  /** Set if this axis takes a quick filter (the source must also have `view`). */
  quickFilter?: QuickFilterSupport;
  /**
   * Set if the source leaves the items a View Spec's `hidden` names out of
   * this axis (the source must also have `view`); the widget offers hiding
   * only then.
   */
  hide?: boolean;
  /**
   * The most items of this axis the source hides at once, if fewer than the
   * widget's own cap (1,000): past it a hide is refused and the widget
   * suggests a filter, which scales.
   */
  hideCap?: number;
  /** The distinct values of item `by` across this axis, for a column's checklist; aborted when the search changes. */
  distinct?(by: CrossId, query: DistinctQuery<CrossId>, signal: AbortSignal): MaybePromise<Distinct>;
  /** Where `id` sits now, after a reorder. */
  locate?(id: Id, signal: AbortSignal): MaybePromise<Located>;
  /**
   * Whether each of `ids` is unmatched: kept in view though the filter in
   * force drops it, because the user's edit stopped it matching. Such items
   * stay until the next View Spec (a re-apply asks for the same one again),
   * and the widget draws them dimmed. Asked for the items shown after each
   * Change Notice and View Spec. A source may instead filter edited items at
   * once and leave this out.
   */
  unmatched?(ids: Id[], signal: AbortSignal): MaybePromise<boolean[]>;
  /**
   * Moves `ids` to before Position `to`, read in the order shown. `false`,
   * or `refused` with a reason the widget shows, leaves them where they
   * are. The widget sends the `Reordered`; a source that changes its View
   * Spec to move them (dropping a sort, say) says so through `viewChanged`
   * before it answers.
   */
  move?(ids: Id[], to: number): MaybePromise<boolean | { refused: string }>;
  /**
   * Whether `move` can move this axis's items now, so dragging a header
   * offers a move; `true` by default while the axis has `move`. Asked again
   * after each View Spec and move, and when the sink's `movableChanged` says.
   */
  movable?(): boolean;
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
  /** Set if the source finds: the widget offers find only then. */
  find?: FindMethods<RowId, ColId>;
  /** Hands the source its sinks for Change Notices and news; returns how to detach. */
  attach?(sink: TableSink<RowId, ColId>): () => void;
}
