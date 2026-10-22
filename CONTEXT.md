# Binnacle

A toolkit-agnostic table/grid core: the view controller that hosts (TUI, web, native GUIs over a C ABI) drive, with all data and data operations left to the host's source.

## Language

### The two sides

**Grid**:
The core's view controller: geometry, interaction and view state for a table, holding no cell values and nothing proportional to the number of rows or columns.
_Avoid_: Table, table view, widget, core (in prose about behaviour)

**Table Source**:
The host-side owner of a table's data and every operation on it (sort, filter, roll-up, pivot, reordering), which supplies the Grid with counts and keys.
_Avoid_: Storage, data source, model, backend

**Effect**:
A request the Grid emits for its host or Table Source to act on, such as a requested View Spec, a column move or the start of an edit.
_Avoid_: Callback, event, command

**Action**:
A semantic input the host sends the Grid on the user's behalf (move focus, select all, activate, cancel, typed text), mapped from native keys by the host's keymap.
_Avoid_: Command, key event, shortcut

**Mode**:
The single interaction the Grid is in the middle of: idle, dragging a selection, resizing, moving headers or editing.
_Avoid_: State, gesture, tool

**Strip**:
One axis's list of visible items in a Snapshot (Position, Key, offset, size), which the host crosses with the other axis's Strip to draw cells.
_Avoid_: Band, row list, track

**Snapshot**:
The Grid's per-frame, read-only description of what to draw (cell and header placements with Keys and flags, selection, scroll extents, damage), stamped with a generation.
_Avoid_: Frame, render list, view model

**Change Notice**:
A message from the Table Source telling the Grid how an axis changed (reset, reordered, refiltered, inserted, removed, content changed). Refiltered means the same Keys, now a subset or superset and possibly reordered, so focus, picks, Size Overrides and the scroll position follow their Keys; Reset promises nothing about Keys.
_Avoid_: Invalidation, update event, data changed

### Axes and identity

**Axis**:
One of the Grid's two symmetric dimensions (rows or columns), each described by a count, a window of keys and a size measure.
_Avoid_: Dimension, direction

**Position**:
An index along an axis in the Table Source's current order, from zero to the axis count, gapless by definition.
_Avoid_: Visual index, row number, offset

**Key**:
An opaque 64-bit identity the Table Source assigns to a row or column, compared only for equality and used for Grid state that must survive reordering.
_Avoid_: ID, logical index, handle

**Key Interner**:
A Table Source helper that maps the host's own identities (composite or string keys) to Keys without collisions.
_Avoid_: Key hasher, ID map

**Window**:
The span of positions on an axis that the Grid currently needs keys for (the visible range plus overscan), filled in by the Table Source.
_Avoid_: Page, viewport, chunk

**Scroll Anchor**:
The Grid's scroll state on one axis: a Position plus an offset into that item, used instead of an absolute coordinate so huge tables stay precise.
_Avoid_: Scroll offset, scroll position

**Size Override**:
A size set for one item on an axis (by a user resize, auto-size or restored layout) that replaces the axis default size.
_Avoid_: Custom width, stored width

**Gap**:
The gridline thickness between items on an axis, part of the Grid's layout and hit testing.
_Avoid_: Border, padding, gutter

**Header Band**:
The band of headers leading an axis, whose thickness the host sets and the user can drag or fit: the column header band leads the row axis (it runs along the top), and the row-number band leads the column axis (it runs down the left).
_Avoid_: Header row, header column, gutter

**View Spec**:
The declared view a Table Source should present for a whole table, one part per axis, each with a filter and a sort, and a whole-table grouping part (row levels, column levels, values and Slicers, all naming the source's base fields; absent for a plain table); the Grid holds the one the source last applied and the latest one requested.
_Avoid_: View state, query, view model

**View Edit**:
The named cause of a requested View Spec, such as cycling a column's sort, applied by the Grid to the latest requested spec and carried with it as a hint; a Table Source must be correct from the spec alone.
_Avoid_: View change, view event, view command

**Sort Spec**:
The sort part of one axis of a View Spec: the ordered list of items on the other axis (columns, for the rows) and directions to sort by.
_Avoid_: Sort order, remap spec

**Filter Spec**:
The filter part of one axis of a View Spec: which of its items the Table Source keeps, as the parts it holds (the quick filter, a Text Query across the other axis's items, and the Conditions) all keeping an item. An empty Filter Spec keeps every item.
_Avoid_: Filter state, predicate

**Condition**:
One test in a Filter Spec on the value an item has in one item of the other axis (a column, for the rows), such as a Text Query searching that column alone, OneOf (the value is exactly one of those listed, blank among them or not) or a comparison, kept or with `not` inverted. Conditions AND with each other and with the quick filter, and one column may hold several. A Table Source declares per item which kinds it takes, and leaves out of the applied View Spec any it does not.
_Avoid_: Column filter, criterion, rule

**Comparison**:
A Condition comparing the value with a number or a date: equal, not equal, less, at most, greater, at least, or between two (both ends included). A date is an instant in milliseconds since the Unix epoch, UTC; a date without a time is its midnight. An item compares numbers or dates, as its Table Source declares; a blank, a NaN or a value of the other kind never passes, so `not` keeps it, and a comparison of the other kind is left out.
_Avoid_: Range filter, numeric filter, predicate

**Distinct Values**:
The values one item of the other axis holds across an axis (a column's values, for the rows), as a Table Source lists them for a checklist: in the item's natural order, each with how many items hold it, blanks apart, narrowed by the filter in force less the item's own Conditions, and capped, saying when there were more.
_Avoid_: Unique values, facets

**Text Query**:
Text to look for in the items of the other axis, with whether case matters, whether it is a regex in the Table Source's own syntax, and which items it searches (the shown ones, or named ones). The quick filter is one; a Find Query holds another.
_Avoid_: Search string, pattern, quick filter text

**Find Query**:
What find looks for: a Text Query, and the order find steps through the matching cells (by rows, along each row in turn, or by columns, down each column). The Table Source works out which cells match, the next match from a cell and how many there are; the Grid holds the Find Query and the current match, and draws the matching cells as marks without changing the view.
_Avoid_: Search, find string, highlight query

**Totals**:
Each axis's count before any filter, as the Table Source gives it with the View Spec it applied, so "1,204 of 5,001" can be shown; unknown when the source does not say.
_Avoid_: Unfiltered count, base count

**Unmatched Row**:
A row the Table Source keeps in view after the user's edit stopped the filter keeping it, until the next View Spec it applies (a re-apply asks for the same one again); the source flags it for the rows shown, like find marks, and the adapters draw it dimmed. Changes from the data itself and inserted rows are filtered as they arrive instead.
_Avoid_: Stale row, held row, edited row (as a term)

### Grouping

**Leaf**:
An item of a grouped axis that is the data itself (a row of the table), keeping the Key it has when the axis is not grouped.
_Avoid_: Detail row, data row, child

**Group Item**:
The Summary Item heading one group of a level: drawn in outline form at its own Position, with an indent and a disclosure mark, opening onto its children.
_Avoid_: Group row, group header, parent row

**Subtotal**:
The Summary Item holding a group's figures, placed before or after the group's items or left out; it does not open.
_Avoid_: Group total, footer row

**Grand Total**:
The Summary Item holding the figures for a whole grouped axis, placed before or after it or left out. Not to be confused with Totals, the unfiltered count.
_Avoid_: Total row, overall total

**Summary Item**:
Any item a grouped axis shows that the source computes rather than holds: a Group Item, a Subtotal or the Grand Total. Its Key is interned from its path (each level's field and value, outermost first), so state on it survives sorting, filtering and data changes.
_Avoid_: Aggregate row, synthetic row, virtual row

**Outline**:
Where an item sits in a grouped axis, given with each Window item: its kind (Leaf or which Summary Item), its depth, its ancestors' Keys from the outermost Group Item down to its parent, whether it is open, and on a Group Item or Subtotal its level's field and value.
_Avoid_: Tree info, hierarchy, node

**Grouping Layer**:
The optional sans-IO module (`binnacle-group`) that answers a grouped View Spec as a Table Source over a backend that only answers Aggregate Queries: it owns the structure, interns Summary Item Keys and holds only what open groups need.
_Avoid_: Group engine, tree builder, pivot engine

**Aggregate Query**:
One request the Grouping Layer sends its backend, in a batch carrying an id: the groups (values, counts and figures) under one path of a level, or the Leaves of one group, over a range. A backend may answer by id in any order, in parts, jointly or with more than asked.
_Avoid_: Group query, rollup request

**Slicer**:
A Condition in the grouping part of a View Spec on a base field, applied before aggregating (`WHERE`), even on a field not shown.
_Avoid_: Pre-filter, where clause

**Corner Grid**:
Where the two Header Bands' tiers cross on a table grouped in header form: each cell by column tier and row tier, showing level names and the values caption.
_Avoid_: Top-left corner, pivot corner

**Drill-Down Spec**:
What activating a Summary Item's cell hands the app with the `Activated` Effect: the Slicers in force plus a OneOf for each field and value on the row's and column's paths, picking the Leaves behind the cell. binnacle builds no drill-down view.
_Avoid_: Detail query, drill-through

### Inside an In-Memory Table Source

**In-Memory Table Source**:
The shared Table Source implementation for simple sources: it holds no cell data, only the Remap and the machinery to sort, filter and move from keys and results the host feeds it. In Rust it is `InMemoryTableSource`, and `MemorySource<R>` is its type name when wrapped with closures over the host's rows; both type names are fine in code and prose about code.
_Avoid_: Memory source (as a prose term), default source, engine

**Logical Index**:
The index of a base row or column in the host's data, as an In-Memory Table Source tracks it; never seen by the Grid.
_Avoid_: Base index, storage index

**Remap**:
An In-Memory Table Source's mapping from Position to Logical Index, produced by its sort, filter and moves.
_Avoid_: Permutation, view mapping
