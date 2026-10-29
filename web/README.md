# @flotilla-org/binnacle

The binnacle Grid on a web page. The Grid runs as wasm on the main thread
and paints onto a canvas inside a native scroller, so it scrolls tables of
any size (up to 2^53 - 1 rows). Your data stays in JavaScript behind one
interface, the Table Source, whose answers may arrive at once or later.

- `@flotilla-org/binnacle`: the framework-neutral `GridWidget`, the Table
  Source types, `fromRows` (a ready-made in-memory source), Themes and
  Border Styles.
- `@flotilla-org/binnacle/react`: `BinnacleGrid`, a thin React wrapper,
  plus everything the main entry exports. React is an optional peer
  dependency (18 or later).

## Install from the repo

The package is not published to npm. It ships its wasm build inside it, and
both the wasm and the JavaScript in `dist/` are build outputs, not committed,
so build it once from a checkout (needs Rust with the `wasm32-unknown-unknown`
target, `wasm-pack` and pnpm):

```sh
git clone https://github.com/flotilla-org/binnacle
pnpm -C binnacle/web install
pnpm -C binnacle/web build
```

Then depend on it from your app, either as a member of the same pnpm
workspace (`"@flotilla-org/binnacle": "workspace:*"`, as `consumer/` here
does) or by path (`"@flotilla-org/binnacle": "link:../binnacle/web"`).
Rebuild after pulling.

The wasm loads through `new URL(..., import.meta.url)`, which Vite and
webpack 5 emit as an asset with no configuration (`consumer/` checks Vite).
esbuild leaves that URL alone: copy `wasm/binnacle_wasm_bg.wasm` beside your
bundle and call `loadBinnacle(new URL("binnacle_wasm_bg.wasm", location.href))`
before creating a grid. The module loads once, on the first `loadBinnacle()`
or `GridWidget.create`.

The wasm-bindgen types use `Symbol.dispose`: with `skipLibCheck` off, add
`"esnext.disposable"` to your `lib`.

## Use

```ts
import { GridWidget, fromRows } from "@flotilla-org/binnacle";

const rows = [
  { name: "Endurance", tonnage: 348 },
  { name: "Fram", tonnage: 402 },
];
const source = fromRows(rows, [{ key: "name" }, { key: "tonnage", label: "Tonnage" }]);
const grid = await GridWidget.create(document.querySelector("#grid")!, {
  source,
  onFrame: (frame) => console.log(frame.focusIds), // { row: 1, col: "tonnage" }
});

// Later: stop the frame loop and free the wasm side.
grid.destroy();
source.destroy();
```

The grid fills its container, so give the container a size. Other options:
`rowHeight`, `columnWidth`, `headerHeight`, `theme` (a Theme, or a function
from `"light" | "dark"` to one; it follows the system by default), `border`
(a Border Style), `keymap`, `platform`, `renderer` (a Cell Renderer per
cell, for example `byColumn(...)` over `textRenderer`, `sparklineRenderer`,
`barRenderer` and `valueRenderer`, which draws Typed Values through the
formatter every binnacle host shares), `onCommit` (makes cells editable; without it the grid is
read-only; returning `false` refuses the value, and the cell keeps its own), `editor` (a custom editor per cell), `selectionUnit`
(`"rows"` makes focus select its whole row, for list-style views),
`typingPause` (how long, in ms, the filter bar, column menu and find bar
wait after the last key before sending what was typed; 150 by default),
`emptyMessage` (what the body says when an axis has no items, "No rows" by
default; a source's own `emptyMessage()`, such as "No rows match the
filter", wins) and `onError` (called with what a failed source request threw
and which request it was; the cells of a failed `window` or `cells` request
show an error marker until a Change Notice through the source's sink asks
again). `grid.theme`, `grid.border` and `grid.emptyMessage` change them
later.

To move around from code:

- `grid.focus(rowId, colId?)` focuses a cell by your IDs (by default in the
  focused column) and scrolls it into view. A row outside the rows the grid
  holds is found with the Table Source's `locate`, so the returned Promise
  settles once focus lands, and is rejected if the source no longer has the
  row or focus moves on first.
- `grid.reveal(rowId, colId?)` scrolls a row (and column) into view without
  moving focus; it too returns a Promise.
- `grid.scrollTo(row, col?)` scrolls so the row at that Position (and the
  column) starts the grid.

To filter and view from code:

- `grid.setQuickFilter({ text, caseSensitive, regex, searches: "shown" })`
  keeps the rows with the text in any shown column (or in the columns
  `searches` names); `null` or empty text clears it. Each call asks the
  Table Source for the View Spec in force with this quick filter.
- `grid.addCondition({ id, kind: { type: "Text", query }, not })` adds a
  column condition: keep the rows whose value in column `id` matches `query`
  (`{ text, caseSensitive, regex }`), or with `not`, does not. Conditions
  AND with each other and with the quick filter, and a column may hold
  several. A `{ type: "OneOf", values }` kind keeps the rows whose value is
  one of `values` exactly (`null` for a blank); with `not`, the others.
  `grid.removeCondition(c)`, `grid.replaceCondition(from, to)` and
  `grid.clearConditions(colId)` change them; each asks the Table Source for
  the View Spec in force with the change.
- Each of these takes the axis to filter last, the rows by default: with
  `"cols"` it filters the columns by their values in rows, so
  `grid.addCondition({ id: rowId, kind, not }, "cols")` keeps the columns
  whose value in that row matches, and `grid.setQuickFilter(query, "cols")`
  keeps those with the text in a searched row. The source declares it on its
  `cols` axis (`quickFilter`, `support`, `distinct`, `unmatched`); without
  `cols.support` the columns offer no controls.
- `grid.hide(ids, axis?)` hides rows (or with `"cols"`, columns): the grid
  asks the Table Source to leave them out whatever the filter keeps, so
  clearing a filter leaves them hidden. Focus moves to a neighbour, picks
  and a partial selection go, and their sizes stay for when they come back.
  `grid.show(ids, axis?)` brings them back in their places, and
  `grid.hidden(axis?)` lists them. `grid.canHide(axis)` says whether the
  source hides that axis: it declares `hide: true` on the axis (with
  `view`), and its `view` then leaves out the items each part's `hidden`
  names, answering with those it has. `fromRows` hides both axes. Mod+Alt+0
  hides the focused column and Mod+Alt+9 its row.
- `grid.requestView(spec)` asks the Table Source for a whole View Spec, such
  as a saved view.
- `grid.reapplyView()` asks for the View Spec in force again, so rows the
  user's edits left in view though they no longer match the filter (drawn
  dimmed) are filtered.
- `grid.subscribe(listener)` calls `listener` after every frame, as
  `onFrame` does. `frame.rows.total` is the row count before any filter (if
  the source gave one), `frame.rows.unmatched` how many rows in view are
  unmatched, `frame.quickFilter` says whether the rows take a
  quick filter, `frame.conditions` lists the conditions with their state
  (`applied`, `adding` and `removing` until the source answers, or
  `notApplied` once it answers without one), `frame.quickState` does the
  same for the quick filter, `frame.viewPending` says whether a View Spec is
  still to be answered, `frame.viewError` holds the source's message for the
  part it left out, and `frame.columns` the column headers in view, each
  with whether it is filtered (and pending, or not applied) and what it
  supports. `frame.columnsFilter` gives the same for the columns' filter
  (`quickFilter`, `quick`, `quickState`, `conditions`), and `frame.cols`
  their count, total and unmatched. While a View Spec is pending the rows
  shown stay as they are; a newer one aborts the request in flight, so only
  the latest answer paints.
- `grid.columnSupport(colId)` says whether a column sorts and takes Text
  conditions (and regexes) and OneOf conditions; `grid.columns()` lists the
  table's columns with their labels and support. `grid.rowSupport(rowId)`
  says the same of the columns by a row, and `grid.rowLabel(rowId)` gives a
  row's header text.
- `grid.distinct(colId, search, limit, apply)` asks the source for a
  column's distinct values, narrowed by the filter in force less the
  column's own conditions, and calls `apply` with them; it returns a request
  to `abort()`, or `null` if the column takes no OneOf conditions. With a
  last argument `"cols"`, a row's values across the columns.

`createFilterBar(grid)` builds the filter bar: the quick filter field with
match case, regex (when the source takes regexes) and clear buttons, and the
row count ("1,204 of 5,001", or "5,001 rows"), and "Re-apply" while rows in
view are unmatched. Place `bar.element` where you
like, usually above the grid; it draws in the grid's Theme and stays hidden
while the source's rows take no quick filter. Typing sets the quick filter
once the typing pause has passed (Enter, the toggles and clear at once), the
count reads "filtering…" while the source works, and the grid's live region
announces the count once the filter applies. A quick filter the source did
not apply turns the field red with the source's message, the text kept; a
condition it did not apply shows a red chip.

`createColumnMenu(grid)` mounts the column menu: every column header whose
column takes Text or OneOf conditions then draws a funnel (solid once
filtered, faded while pending), and pressing it, or Alt+ArrowDown in the
grid, opens a popover under the header: "Contains" or "Does not contain",
the text with match case and (when the column supports them) regex toggles,
and Clear. Below it, where the column takes OneOf conditions, a checklist of
its values with their counts: a search, "(Select all)" (or "(Select all
matching …)" while searching), "(Blanks)", and a note when the list stops at
1,000 values and whether the other filters narrowed it. Each change edits
the column's first Text condition and first OneOf at once, writing whichever
condition is exact: unticking from all excludes the values unticked, ticking
from none includes those ticked, and "select all matching" keeps the
search as a Text condition. A value the Text condition rejects is shown
disabled. The text waits for the typing pause, and a Text condition the
source did not apply turns the field red with its message. Escape, a press outside or Close shuts it. Pass it to the filter bar,
`createFilterBar(grid, { columnMenu })`, and the bar shows a chip for each
condition ("port: not Bergen ×": the label reopens the menu, × removes the
condition) and "+ Filter", which picks a column and opens its menu. With a
column menu the bar shows even when the rows take no quick filter.

The column menu is the row menu too, where the source filters its columns:
a row header's funnel (drawn while hovered, and as a mark while the row
filters the columns), Alt+ArrowRight in the grid, or `menu.openRow(rowId)`
opens the same popover beside the row header, "Filter columns by" the row,
with the checklist of the row's values across the columns. The filter bar
then lists those conditions after "Columns where" ("3: not Bergen ×"), shows
the column count beside the row count while the columns are filtered ("4
of 5 columns"), and its "Re-apply" counts edited rows and columns. Its
options name each axis's items for the counts (`nouns: { rows: ["field",
"fields"], cols: ["record", "records"] }`, giving "5 of 12 records"), and
`axis: "cols"` makes its field the columns' quick filter.

Where the source hides an axis, the menu leads with "Hide column" (or "Hide
row") and "Show hidden columns ▸", whose flyout lists the hidden ones to
show again, with a search past eight of them, and "Show all". Mod+Alt+Shift+0
(or 9 for rows) opens it at that list, and Shift+F10 or the context menu key
opens the focused column's menu. A column that takes no conditions gets
just those items. The filter bar counts the columns left, "4 of 5 columns",
hidden or filtered alike.

### React

```tsx
import { fromRows } from "@flotilla-org/binnacle";
import { BinnacleGrid } from "@flotilla-org/binnacle/react";
import { useEffect, useMemo, useState } from "react";

function Ships({ rows }: { rows: Ship[] }) {
  const source = useMemo(() => fromRows(rows, [{ key: "name" }, { key: "tonnage" }]), [rows]);
  const [focus, setFocus] = useState<{ row: number; col: keyof Ship } | null>(null);
  useEffect(() => () => source.destroy(), [source]);
  return <BinnacleGrid source={source} style={{ height: 400 }} onFrame={(f) => setFocus(f.focusIds)} />;
}
```

`BinnacleGrid` takes every `GridWidget` option as a prop, plus `className`,
`style` and `widgetRef` (receives the `GridWidget`, then `null`; call
`focus`, `reveal` and `scrollTo` through it). The widget
runs its own frame loop: React renders the container once and never on
frames. Props reach it like this:

- Function options (`onFrame`, `onCommit`, `editor`, `renderer`) are always
  called as they are in the latest render, so inline closures are fine.
  Adding or removing one makes a new widget (no `onCommit` means read-only).
  A new `renderer` draws from the next frame the grid paints; it does not
  force one.
- `theme` and `border` restyle the same widget.
- Any other option is read when the widget is created; a new value (a new
  `source`, a new `keymap` object) makes a new widget. Memoize them.

The wrapper does not destroy your Table Source: you made it, you free it.

To find from code (when the Table Source has `find`):

- `grid.setFind({ query: { text, caseSensitive, regex, searches: "shown" }, by: "rows" })`
  marks the cells that match, leaving the view alone; `by: "cols"` steps
  down each column in turn. `null` closes find and clears the marks.
- `grid.findNext()` and `grid.findPrevious()` step to the next or previous
  match, wrapping round, and focus it without changing the selection. A step
  asked while one is due waits for it: before a query's first step lands,
  further presses are dropped, and after it at most one waits.
- `frame.find` holds what find shows: `support`, the `query` in force, the
  `current` match, the source's `count` (and whether it is `counting`), the
  source's `error` if it failed the query, and where the last `step` landed
  (its `index` among the matches and whether it `wrapped`). The current match
  stays until the first step for a new query lands.

`createFindBar(grid)` builds the find bar: the find field with match case
and regex (when the source takes regexes) toggles, the order (by rows or by
columns), the count ("3 of 120", "3 of 120+" while the source is still
counting, "120 matches", "No matches", "finding…" until it answers), a note
when a step wraps (or the source's message, in red, when it fails the
query), Previous and Next, "Show only matching rows" (which copies the
text to the quick filter) and close. Place `bar.element` above the grid,
full width. Ctrl+F (Cmd+F on macOS) in the grid opens it, Enter steps to the
next match and Shift+Enter to the previous, F3 and Shift+F3 step from the
grid while find is open, and Escape closes it and hands focus back to the
grid. It never opens while the source does not find.

## The Table Source

A Table Source answers for order and values, per axis, in your own ID and
value types. Every method may return a value or a Promise; late answers are
fine, stale ones (from before a Change Notice) are dropped.

```ts
type MaybePromise<T> = T | Promise<T>;

interface TableSource<RowId, ColId, V = unknown> {
  rows: AxisSource<RowId, ColId>;
  cols: AxisSource<ColId, RowId>;
  /**
   * Presents the table as `spec` asks; returns the View Spec now in force (all, part or none of
   * it, an omitted filter being none), with `totals: { rows?, cols? }`, the counts before any filter.
   */
  view?(spec: ViewSpec<RowId, ColId>, signal: AbortSignal): MaybePromise<AppliedView<RowId, ColId>>;
  // An answer may carry `error`: why it left part of `spec` out (an invalid regex, say).
  /** The values where `rows` and `cols` cross, one array per row. */
  cells(rows: RowId[], cols: ColId[], signal: AbortSignal): MaybePromise<V[][]>;
  /** Find: which cells match a Find Query, the next match, and how many there are. */
  find?: FindMethods<RowId, ColId>;
  /** Hands the source its sinks for Change Notices and news; returns how to detach. */
  attach?(sink: TableSink<RowId, ColId>): () => void;
}

interface TableSink<RowId, ColId> {
  rows: AxisSink<RowId, ColId>; // notify(notice), supportChanged(ids?)
  cols: AxisSink<ColId, RowId>;
  /** The View Spec the source applies now, unprompted, or new totals; answers due stay due. */
  viewChanged(view: AppliedView<RowId, ColId>): void;
  /** A running count for `query` (the object `find.count` was asked with), `partial` until the last. */
  findCounted(query: FindQuery<ColId>, count: FindCount): void;
}

interface AxisSource<Id, CrossId> {
  count(): MaybePromise<number>;
  /** The IDs at Positions `start..end`. */
  window(start: number, end: number, signal: AbortSignal): MaybePromise<Id[]>;
  /**
   * How this axis may be viewed by `by`, asked as `by` comes into view: `{ sort, text, oneOf }`. With
   * `view`, `sort` is `true` and `text` (Text conditions) is `{ regex: false }` (contains, no regexes)
   * by default; `text: false` takes none. `oneOf` is `true` with `distinct`. The grid draws no control
   * for what is unsupported.
   */
  support?(by: CrossId): Support;
  /**
   * The distinct values of item `by` across this axis, for its checklist: `{ search, limit, narrow }`
   * asks for those containing `search` (ignoring case; a search lists no blanks), up to `limit`, among
   * the items `narrow` keeps. Answer `{ values: { text, count? }[], blanks?, partial, narrowed }` in
   * the item's natural order; `partial` if there were more.
   */
  distinct?(by: CrossId, query: DistinctQuery<CrossId>, signal: AbortSignal): MaybePromise<Distinct>;
  /** Set if the axis takes a quick filter (with `view`); `{ regex: true }` if its Text Queries may be regexes. */
  quickFilter?: { regex?: boolean };
  /** Set if `view` leaves out the items a View Spec's `hidden` names on this axis; the grid offers hiding only then. */
  hide?: boolean;
  /** Where `id` sits after a reorder: `{ type: "Found", position }`, or `{ type: "Gone", suggest }` with where it would sit among the items left (the next one's position, or the count when none follows). */
  locate?(id: Id, signal: AbortSignal): MaybePromise<Located>;
  /**
   * Whether each of `ids` is unmatched: kept in view after the user's edit though the filter drops
   * it, until the next View Spec. The grid draws such items dimmed.
   */
  unmatched?(ids: Id[], signal: AbortSignal): MaybePromise<boolean[]>;
  /** Moves `ids` before Position `to`; `false` refuses. */
  move?(ids: Id[], to: number): MaybePromise<boolean>;
  /** Header text; `String(id)` for columns and Position + 1 for rows by default. */
  label?(id: Id, position: number): string;
  /** Required when `Id` is not a string, number or bigint: its primitive form. */
  idKey?: (id: Id) => string | number | bigint;
  /** The axis's IDs are its Positions (huge or position-only tables). */
  identity?: "position";
}

/**
 * One part per axis, its filter and sort naming items of the other axis (the rows sort and filter by
 * columns), and its own items to leave out whatever the filter keeps (empty unless the axis hides).
 */
interface ViewSpec<RowId, ColId> {
  rows: { filter: FilterSpec<ColId>; sort: SortSpec<ColId>; hidden: RowId[] };
  cols: { filter: FilterSpec<RowId>; sort: SortSpec<RowId>; hidden: ColId[] };
}

/** Every part must keep an item: the quick filter, and each condition. */
interface FilterSpec<CrossId> {
  quick: TextQuery<CrossId> | null;
  conditions: Condition<CrossId>[];
}

/**
 * Keep the items whose value in item `id` of the other axis matches `query`, or is one of `values`
 * (`null` for a blank); with `not`, those that don't.
 */
interface Condition<CrossId> {
  id: CrossId;
  kind:
    | { type: "Text"; query: { text: string; caseSensitive: boolean; regex: boolean } }
    | { type: "OneOf"; values: (string | null)[] };
  not: boolean;
}

/** Keep the items with `text` in a searched item of the other axis: the shown ones, or those named. */
interface TextQuery<CrossId> {
  text: string;
  caseSensitive: boolean;
  /** A regex in the source's own syntax. */
  regex: boolean;
  searches: "shown" | CrossId[];
}

/** A Text Query and the order find steps in: along each row, or down each column. */
interface FindQuery<ColId> {
  query: TextQuery<ColId>;
  by: "rows" | "cols";
}

interface FindMethods<RowId, ColId> {
  /** Whether a Find Query may be a regex; `false` by default. */
  regex?: boolean;
  /** Whether each cell where `rowIds` and `colIds` cross matches, one array per row. */
  marks(rowIds: RowId[], colIds: ColId[], query: FindQuery<ColId>, signal: AbortSignal): MaybePromise<boolean[][]>;
  /**
   * The first match after `from` in the query's order (before it, for `previous`), wrapping round;
   * from the start (or the end) if `from` is `null` or not in the view. `null` if nothing matches.
   * The answer, `{ row, col, index?, wrapped? }`, may give the match's 0-based index and whether it wrapped.
   */
  next(
    from: { row: RowId; col: ColId } | null,
    direction: "next" | "previous",
    query: FindQuery<ColId>,
    signal: AbortSignal,
  ): MaybePromise<FindStep<RowId, ColId> | null>;
  /**
   * How many cells in the view match: `{ total, partial? }`, `partial` if counting stopped early.
   * Running counts may follow through the sink's `findCounted`. A failure (an invalid regex) shows
   * as "No matches" with its message.
   */
  count?(query: FindQuery<ColId>, signal: AbortSignal): MaybePromise<FindCount>;
}
```

Find never sees a value: the source decides which cells match, in the view
in force, and the grid asks for the marks of the cells it draws. A step to a
match outside the rows the grid holds is placed with the axis's `locate`.
Marks are asked for again after a Change Notice or a new View Spec, and the
count with them, once a frame however many arrive.

A row the user's edit stops matching may stay where it is until the next
View Spec, as in a spreadsheet's filter: the source keeps it in the rows it
answers and says it is unmatched through `rows.unmatched`, which the grid
asks for the rows it draws, again after each Change Notice and View Spec,
and draws them dimmed (the Theme's `unmatched` wash). Any View Spec, even
the one in force (`grid.reapplyView()`, or "Re-apply" in the filter bar),
filters them. A source may instead filter edited rows at once and leave
`unmatched` out. Changes from the data itself, such as a live feed, and
inserted rows are best filtered as they arrive.

A source tells the grid what changed through the sink:
`sink.rows.notify({ type: "Reset" })`, or `Reordered`, `Refiltered`,
`Inserted { at, n }`, `Removed { at, n }`, `ContentChanged { start, end }`.
A filter of the host's own needs no View Spec: filter however you like, then
notify `Refiltered`, which keeps focus, picks, row heights and the scroll
position with their rows (`fromRows` has `filter(keep)` for this). A sort or
quick filter the source applies arrives as a reorder or a `Refiltered`
through the sink too, and `sink.viewChanged(view)` says the View Spec in
force changed without being asked (a condition dropped, new totals): what
the source left out of what was asked for shows as not applied, and stays
until the user removes it. `sink.rows.supportChanged(colIds?)` says how
the rows may be viewed by those columns (or all of them) changed, and the
grid asks `support` again. Only the rows sort and filter for now: a source may
drop the columns' part from its answer. When the columns shown change (a
`Refiltered`, `Reset`, `Inserted` or `Removed` on the columns), a quick
filter searching the shown columns is asked for again.

`fromRows(rows, columns)` covers arrays of objects: row IDs are array
indices, column IDs property names, every column sorts unless
`sortable: false`, and the rows stay in JavaScript. Its rows take a quick
filter, regexes included (JavaScript `RegExp` syntax, with the `u` flag),
matched against each cell's text as its column's renderer gives it; a
missing value never matches, and a pattern `RegExp` rejects leaves the quick
filter in force, the answer's `error` saying why. It applies Text conditions the same way, in their own
column: a blank never matches, so a `not` condition keeps blanks, and a
condition whose pattern `RegExp` rejects is left out of the View Spec it
answers. OneOf conditions match the same text exactly. Its `distinct` lists
a column's values as its renderer gives them, with counts, in sort order
(numbers numerically), narrowed by the filter asked for. A column with
`filterable: false` takes no conditions. The host's `filter(keep)` applies
as well. Its `find` matches the same way across the
shown columns, steps in the rows' order (and the columns'), gives each
step's index and counts every match; a pattern `RegExp` rejects fails the
step and the count, and marks nothing. Rows are read as they are until you
say one changed in place: `edited(id)` for the user's edit (from `onCommit`),
which leaves a row that stops matching in view, unmatched, until the next
View Spec, and `changed(id)` for a change from the data itself, which the
filter takes at once.

`transpose(source)` turns any Table Source on its side: its rows are the
source's columns and its columns the source's rows, so
`transpose(fromRows(records, fields))` shows each record as a column, as a
pivoted table would. Every request passes on about the other axis and every
answer comes back swapped, so it adds nothing the source lacks. Choose the
renderers by row with `byRow`. A Find Query naming the columns it searches
finds nothing on the turned table (the source can search only its own
columns); the find bar always searches the shown ones.

## Develop

`pnpm dev` serves the demo on http://127.0.0.1:5754 (the React page is
`/react.html`). `pnpm typecheck`, `pnpm test` (vitest and the wasm tests) and
`pnpm e2e` (Playwright, Chromium and Firefox; builds the package and the
`consumer/` fixture first) are the gates.
