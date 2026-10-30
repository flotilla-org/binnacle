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
One axis's list of visible items in a Snapshot (Position, Key, offset, size), which the host crosses with the other axis's Strip to draw cells. Its items come in drawn order: the leading pinned items, the scrolling ones in Position order, then the trailing pinned ones. The web's ARIA table reads them in that order, pins included, and numbers each (`aria-rowindex`, `aria-colindex`) by its place in it, not by its Position.
_Avoid_: Band, row list, track

**Snapshot**:
The Grid's per-frame, read-only description of what to draw (cell and header placements with Keys and flags, selection, scroll extents, damage), stamped with a generation.
_Avoid_: Frame, render list, view model

**Change Notice**:
A message from the Table Source telling the Grid how an axis changed (reset, reordered, refiltered, inserted, removed, content changed, and on a grouped axis collapsed and expanded). Refiltered means the same Keys, now a subset or superset and possibly reordered, so focus, picks, Size Overrides and the scroll position follow their Keys; Reset promises nothing about Keys. A group toggle answered with Refiltered keeps the group in view: if the Scroll Anchor's item was in the group, the anchor goes to where the group now starts (its Group Item, or in header form the first item it spans), and focus that was in the group and left goes there too, as with Collapsed.
_Avoid_: Invalidation, update event, data changed

**Typed Value**:
A cell's value with its kind (null, bool, integer, number, text, bytes, date, category, JSON tree, or invalid: what the source could not read as the kind its column expects), as a described table gives it, and drawn by every host through the one shared formatter with its column's unit, role and options, so the same value shows the same text everywhere. `Value` in `binnacle-format`, `TypedValue` in TypeScript.
_Avoid_: Cell value (as a term), datum, variant

**Zone**:
The local time zone a host hands the formatter, as a lookup from an instant to its offset from UTC, so each date draws in the offset in force at that date, across daylight-saving changes. It supersedes the single `utc_offset` the #296 brief gave `FormatContext`. `Zone` and `FixedOffset` in `binnacle-format`; a function from ms to seconds east in TypeScript.
_Avoid_: UTC offset (for the zone as a whole), timezone offset

**Stable Text**:
A Typed Value's text as search, filters, copy and assistive tech read it: the formatted text with every date absolute, so it never goes stale as time passes. `format_stable` in `binnacle-format`.
_Avoid_: Plain text, display text

### Described tables

**Described Table**:
A table its server describes on the wire: a `table` message carries a Frictionless-style schema (fields with a type, title, roles, unit, categories and constraints, a primary key and foreign keys), then rows come as maps from field name to value, as newline-delimited JSON or a CBOR sequence, with live changes after. Its Table Source (`DescribedSource` in `binnacle-described`) reads each value once, as it comes, as a Typed Value of its field's type, and keys each row by its primary key.
_Avoid_: Remote table, schema table, dynamic table

**Scope**:
What a Described Table's rows are read within: a namespace, which every request needs (one without gets no rows, and no error), and fixed `where` clauses. A Described Table source can't be built without one.
_Avoid_: Namespace (for the whole), query, filter (for the `where` clauses)

**Resync**:
A Described Table source starting its rows again without showing the table empty: after a `resync` message, or when its stream ends and it asks to open the watch again, every row is unconfirmed; the `table`, `rows` and `synced` that follow replace the rows in place, each row that comes confirmed and keeping its Key, and `synced` removes those still unconfirmed. A `table` message once the columns are known is diffed by field name the same way.
_Avoid_: Reload, refresh, reconnect (for the whole)

**Fake Table Server**:
`fake-tables` in `binnacle-testkit` (behind the feature of that name): a stand-in for flotillad's table API, serving recorded fixtures or generated tables, and playing a script of live changes (upserts, deletes, resyncs, schema changes, stream ends, slowness and errors) to its watches, for tests in process and for demos as a binary.
_Avoid_: Mock server, stub, test server

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

**Layout**:
The portable, versioned document of how a table is presented, by the Table Source's IDs: its View Spec, a partial order of its items per axis (those listed first, the rest in the source's order), its pins, its Size Overrides and its Header Band thicknesses, each part optional. A source ships one as its default view and a user's saved layout is another, laid over it part by part: each part the user's sets replaces the default's, but the order layers per axis and Size Overrides merge per item. Its IDs name every item of their axis, Group Items included. It reads and writes as JSON (the web's and the Rust crate's alike) or CBOR.
_Avoid_: Saved layout (as a term), view state, preset, profile

**Arrangement**:
The items placed first on an axis, in order, as a restored Layout's order and the header moves the Table Source made since leave them, whether or not they are in view: what a Layout saves as the axis's order.
_Avoid_: Column order (the source's whole order), user order

**Sort Spec**:
The sort part of one axis of a View Spec: the ordered list of items on the other axis (columns, for the rows) and directions to sort by.
_Avoid_: Sort order, remap spec

**Sort Order**:
The one total order every In-Memory Table Source sorts each column by. Nulls are the lowest value: first ascending, last descending. NaN is the greatest number, and all NaNs are equal. Integers compare exactly, signed or unsigned. Byte strings and text compare lexicographically by bytes (for UTF-8, code-point order), with no locale or natural ordering. In a column of Typed Values, invalid values sit between the nulls and the typed values, comparing as text among themselves. Descending reverses the whole order, nulls included. Items that tie on every Sort Spec column break the tie on their Key, ascending in both directions, whether sorted from scratch or merged in after a base change.
_Avoid_: Collation

**Filter Spec**:
The filter part of one axis of a View Spec: which of its items the Table Source keeps, as the parts it holds (the quick filter, a Text Query across the other axis's items, and the Conditions) all keeping an item. An empty Filter Spec keeps every item.
_Avoid_: Filter state, predicate

**Condition**:
One test in a Filter Spec on the value an item has in one item of the other axis (a column, for the rows), such as a Text Query searching that column alone, OneOf (the value is exactly one of those listed, blank among them or not) or a comparison, kept or with `not` inverted. Conditions AND with each other and with the quick filter, and one column may hold several. A Table Source declares per item which kinds it takes, and leaves out of the applied View Spec any it does not.
_Avoid_: Column filter, criterion, rule

**Comparison**:
A Condition comparing the value with a number or a date: equal, not equal, less, at most, greater, at least, or between two (both ends included). A date is an instant in milliseconds since the Unix epoch, UTC; a date without a time is its midnight. An item compares numbers or dates, as its Table Source declares; a blank, a NaN or a value of the other kind never passes, so `not` keeps it, and a comparison of the other kind is left out.
_Avoid_: Range filter, numeric filter, predicate

**Native Condition**:
A Condition holding text in the Table Source's own query language, passed to it unchanged (an SQL source might take `port LIKE 'A%'`). Only a field whose Table Source declares `native` takes one; an adapter shows its text as typed, in `‹ ›`. A Native Condition with no text asks for nothing.
_Avoid_: Raw filter, SQL condition, custom condition

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
A row the Table Source keeps in view after the user's edit stopped the filter keeping it, until the next View Spec it applies (a re-apply asks for the same one again); the source flags it for the rows shown, like find marks, and the adapters draw it dimmed. Changes from the data itself and inserted rows are filtered as they arrive instead. In grouped rows the same rule holds for an edit to a grouping field: the Leaf stays in its old group, dimmed, while every figure the edited row is counted in follows the edit at once (`ContentChanged`), a pivot's cells among them; over an async backend those figures draw pending until it answers. The next View Spec groups the Leaf anew.
_Avoid_: Stale row, held row, edited row (as a term)

### Grouping

**Leaf**:
An item of a grouped axis that is the data itself (a row of the table), keeping the Key it has when the axis is not grouped.
_Avoid_: Detail row, data row, child

**Group Item**:
The Summary Item heading one group of a level: drawn in outline form at its own Position, with an indent and a disclosure mark, opening onto its children. In header form it has no Position (a group with no children to open keeps one) and is drawn as a Header Span instead.
_Avoid_: Group row, group header, parent row

**Subtotal**:
The Summary Item holding a group's figures, placed before or after the group's items or left out, per level; it does not open. Its Outline sits one level below its Group Item, which is its parent. While a group is open and its Subtotal placed, the Group Item draws only its label and count, the figures being the Subtotal's; a closed Group Item carries them itself. A group above the values pseudo-level has one Subtotal per value instead, labelled by its group then its value ("Q1 sum(weight)"), each a Summary Item at its own Key interned from the group's path and the value's identity; closed, such a group shows them (after its Group Item in outline form), and its Group Item draws no figures, as the column Grand Total splits per value and as in Excel.
_Avoid_: Group total, footer row

**Grand Total**:
The Summary Item holding the figures for a whole grouped axis, placed before or after it or left out; at depth 0 with no ancestors. Its Key is a fixed one (`GRAND_TOTAL`), so the Grid pins it to the trailing edge whenever the axis's placement turns to after, before any Window lists it; the user may unpin it, and it stays unpinned while the placement stays after. Not to be confused with Totals, the unfiltered count.
_Avoid_: Total row, overall total

**All Column**:
A column Grand Total, labelled "All". Where the column axis splits the values (the values pseudo-level among its levels) each value has its own All column, a Summary Item at its own Key interned from the value's identity, at the top level with no ancestors, and the Grid pins each to the trailing edge on sight.
_Avoid_: Total column, grand total column

**Pivot**:
A grouping with column levels, or with the values pseudo-level among its row levels: both axes are then the Grouping Layer's, each item a Summary Item and each cell the figure where a row's path and a column's path cross. The innermost groups don't open (there are no Leaves), the columns follow the column Sort Spec and can't be moved, and any change to them is answered with Refiltered (or Reset), never Removed. A group above the values pseudo-level shows no figure of its own: its Subtotals, one per value, carry them. Either axis sorts and filters by the other's Summary Items: the rows by a Pivot Column's figures, the columns by a row's (a comparison on the Grand Total row's figure drops the quarters it fails, HAVING across). A sort term or comparison naming an item the data lacks for now is left out of the applied View Spec; the Grid keeps such a sort term asked for, not applied, while the grouping applied pivots, and it applies again when the item returns.
_Avoid_: Cross-tab, matrix

**Pivot Column**:
A column a Pivot builds from the data: its Key is interned from its path (each column level's field and value, then the value's identity when the columns split the values), so state on it (a Size Override, a pin) survives re-sorting and data changes. In TypeScript, `fromRows` with `pivots: true` gives them negative IDs.

**Path Key**:
A Key the Grouping Layer interns from a path of group values: a Summary Item's or a Pivot Column's. State the Grid holds on one (a Size Override, a pin, a hidden Key, a sort term or condition naming it) survives any change of data and goes when the levels change so its path no longer fits them: the Table Source says which Keys still fit (`fits`), and after each View Spec lands the host asks it of the Grid's held Keys and has the Grid forget the rest, never to come back. A Leaf Key always fits.
_Avoid_: Group key, summary id
_Avoid_: Dynamic column, generated column

**Values Pseudo-Level**:
The level of a grouping that spreads the values along an axis, one item per value, placeable at any depth of either axis. Its path step's field is `VALUES_FIELD` (`null` in TypeScript Outlines) and its value the value's identity: the aggregate over the field ID, "sum(3)", or the aggregate alone, "count()".
_Avoid_: Measures level, data level

**Summary Item**:
Any item a grouped axis shows that the source computes rather than holds: a Group Item, a Subtotal or the Grand Total. Its Key is interned from its path (each level's field and value, outermost first), so state on it survives sorting, filtering and data changes.
_Avoid_: Aggregate row, synthetic row, virtual row

**Outline**:
Where an item sits in a grouped axis, given with each Window item: its kind (Leaf or which Summary Item), its depth, its ancestors' Keys from the outermost Group Item down to its parent, whether it is open, on a Group Item or Subtotal its level's field and value, its path (each ancestor's field and value, outermost first), on a Group Item its span (how many items are shown under it, its Subtotal included; 0 while closed), and its Set Position. The span is what a click on a Group Item's header selects; the path is what its Drill-Down Spec names.
_Avoid_: Tree info, hierarchy, node

**Set Position**:
An item's place among its parent's children, as a tree reads it: its index and the set's size. A group's children are its child groups or Leaves, with its Subtotal first or last where its level places one; the top's are the top groups, with a placed Grand Total first or last. It holds whether or not the parent is open, and in either form. The web's ARIA treegrid reads it as `aria-posinset` and `aria-setsize`. `SetPosition` in code.
_Avoid_: Sibling index

**Collapsed / Expanded**:
The Change Notices for a group closing or opening: `Collapsed { at, n }` hides the `n` items from Position `at`, under the Group Item at `at - 1`, and `Expanded { at, n }` shows `n` there. Position-precise like Removed and Inserted, but the hidden items stay alive: their Size Overrides and pins are kept (the pins dormant), focus and range corners inside pull to the Group Item, picks inside are dropped, and the Scroll Anchor moves to the Group Item. In header form, where no Group Item stands before them, they pull instead to the Subtotal of the deepest group beside the hidden run, and the Scroll Anchor goes there too. A change touching several groups comes as Refiltered.
_Avoid_: Fold, hide (for a group), Removed (for a closed group's items)

**Expand State**:
Which groups of an axis are open, part of the grouping in the View Spec and so saved in layouts: a default depth (how many levels open; 0 shows only the top groups) and `except`, the Group Item Keys the other way, capped like the hidden Keys. Toggling a group adds or removes its Key; setting the depth clears `except`. A Key stays while its path fits the levels and is pruned when the levels change so that it cannot.
_Avoid_: Collapse state, open set, tree state

**Outline Form / Header Form**:
The two ways a grouped axis draws its levels, chosen per axis in the grouping (`form`). Outline form gives each Group Item its own Position, indented in one column; header form gives each level a Tier of the Header Band and each group a Header Span over its items. In header form a closed group shows only its Subtotal, under its span, and an open group's own Subtotal sits after or before its span, labelled in band with the group's value ("Felixstowe total").
_Avoid_: Tabular form, compact form, pivot layout

**Tier**:
One level's slice of a Header Band in header form, laid from the band's outer edge: its thickness the host sets per level (`set_tiers`), levels it gives none sharing what is left. Past the tiers an item's spans take, the band heads the item itself.
_Avoid_: Header row, header level, band column

**Header Span**:
One group drawn in header form: a cell of its level's Tier over the run of neighbouring items under it in one Strip, merged by ancestor Key. A group whose items cross a pinned edge has a span in each Strip; a span clipped by the viewport, the Window or a pinned edge is cut on that side, and its label stays in the part drawn. `HeaderSpan` in code.
_Avoid_: Merged cell, group cell, spanning header

**Pending Toggle**:
A Group Item whose open state a request still due changes: its disclosure mark draws pending (adapters wait about 150 ms first, so a fast source never flickers), and a second toggle before the answer takes the first back.
_Avoid_: Loading group, spinner

**Outline Column**:
The row Header Band of rows grouped in outline form, as a column focus can enter (Left from the first column, or a click on a row header): there the arrows walk the tree (Right opens a closed group or steps to its first child, Left closes an open one or steps to the parent), Enter toggles, `*` opens everything under the focused group, a digit opens the rows to that depth, and Tab leaves for the body. In header form focus sits on a row and a Tier: Right opens the closed span there or steps one Tier in (from the last, to the body), Left closes the open span or steps one Tier out, and Enter, `*` and digits act on the span's group; a click on a Tier focuses it.

Grouped columns, always in header form, have the same keys turned: Up from the first row enters the innermost Tier of the column Header Band over the focused column, where Right opens the closed span or steps one Tier in (from the last, to the body), Left closes the open span or steps one Tier out, Up steps one Tier out, Down leaves for the body, and Enter, `*` and digits act on the columns. A closed column span shows its total column under it. Either band holding focus is the axis's band focus (`band_focused(axis)` in a Snapshot).
_Avoid_: Tree column, row header focus, column header focus

**Pending Item**:
An item of a grouped axis whose Key its Window has not had answered yet, the backend still due: the band draws it pending (a "…" in the loading colour) and its cells loading. The Window answers the items it holds and comes again for the rest.
_Avoid_: Loading row, placeholder row

**Grouping Layer**:
The optional sans-IO module (`binnacle-group`) that answers a grouped View Spec as a Table Source over a backend that only answers Aggregate Queries: it owns the structure, interns Summary Item Keys and holds the open groups and a bounded cache of pages (closed groups and runs of Leaves, least recently read let go first, never those the latest two Windows read), so its memory grows with what the user opens, never with the rows. A closed group takes one Position whether fetched or not, so the axis's length is known before its pages are. Hosts reach it from Rust, from TypeScript through wasm (`groupedSource`) and from C (`binnacle_group_layer_*`).
_Avoid_: Group engine, tree builder, pivot engine

**Aggregate Query**:
One request the Grouping Layer sends its backend, in a batch carrying an id: the groups (values, counts and figures) under one path of a level, or the Leaves of one group, over a range; the figures of a whole group; or where one Leaf is. A Window asks for everything it lacks in one batch. A backend may answer by id in any order, in parts, jointly or with more than asked; a query is done once an answer names it or what was answered covers it, and answers for a batch a newer View Spec superseded are dropped.
_Avoid_: Group query, rollup request

**Value Caption**:
The name of the one value a grouped axis shows, drawn in the corner so the figures need no column label: its aggregate over its field's name, with any parameters after it ("sum(weight)", "quantile(weight, 0.9)"), or the aggregate alone for a count of items. With several values there is none.
_Avoid_: Measure label, value header

**Slicer**:
A Condition in the grouping part of a View Spec on a base field, applied before aggregating (`WHERE`), even on a field not shown, hidden or grouped by. Slicers outlive the levels: a grouping with Slicers and no levels is a plain table, sliced. Adding, replacing and removing one are View Edits. The rows' own Conditions differ: a comparison there on a field a value aggregates acts on the groups by that value's figure (`HAVING`), leaving out a group whose figure fails it and everything under it while the figures above stay; any other Condition acts on the Leaves. A group whose child groups all fail drops out too, Group Item and Subtotal alike: an empty group has nothing to total.
_Avoid_: Pre-filter, where clause

**Grouping Support**:
What a Table Source declares per field for grouping: whether the rows group by it, whether the columns pivot by it, where its Subtotals may go (before, after, both or neither; off is always allowed), whether it takes Native Conditions, and separately the aggregates it offers over it, each named with its parameters. A count of items needs no field and is always offered. A source declaring no field that groups shows no grouping or Slicer controls. A level, value or Slicer beyond what it supports is left out of the applied View Spec and listed as not applied, so restoring a layout asking for one never fails.
_Avoid_: Pivot capabilities, aggregation support

**Corner Grid**:
Where the two Header Bands meet, as a grid: one row per Tier of the column Header Band, one column per Tier of the row Header Band, each band's own headers taking the last (an axis in outline form, or not grouped, has one, naming every level of the axis). Each Tier names its level's field, the values pseudo-level marked as having none. Adapters draw it in the Tiers layout: the values caption and the values pseudo-level's name on the top row, each column level's name right-aligned in its Tier with an arrow, and the row levels' names along the bottom row, one per level column in header form or joined with `›` in outline form. A click on a level name sorts the axis by its field (`cycle_sort`), as a click on that field's header would; a right-click opens its Grouping Menu. `CornerGrid` in code.
_Avoid_: Top-left corner, pivot corner

**Grouping Menu**:
The menu that changes a grouping where it shows, built on each adapter's column menu. From a column of the table it groups the rows by the column, adds it as a column level, or adds a value over it; from a Pivot Column (for the column level it is a group of) or a level's name in the Corner Grid it removes the level, moves it to the other axis, collapses to it or expands through it, removes a value, adds one, or groups by a base field no level uses. A level joins its axis innermost but for the values pseudo-level, which the grouping keeps innermost on the columns while there are column levels and two or more values, and drops with fewer than two; a field groups one level at most. Removing the last field level of both axes makes the table plain, its values and any values pseudo-level going with it, unless a Slicer remains (the values pseudo-level alone is no level). A line is offered only where the Table Source supports it: `group` for a row level, `pivot` for a column level, and the aggregates it declares over a field for a value (a count of items always). The column chooser lists the fields the rows group by as "grouped": they have left the body.
_Avoid_: Pivot pane, field list

**Drill-Down Spec**:
What activating a Summary Item's cell hands the app with the `Activated` Effect (`DrillDown`): the Slicers in force plus a OneOf for each field and value on the row's and column's paths, picking the Leaves behind the cell. Only a cell whose row or column is a Summary Item carries one; a Leaf's cell carries none and may begin an edit. binnacle builds no drill-down view.
_Avoid_: Detail query, drill-through

**Hidden Match**:
A find match inside a closed group: counted with the others and named apart ("3 of 12, 5 in collapsed groups"), and stepped to as if every group were open. The source answers the step with the match's closed ancestors, and the Grid opens each with an Expand (the Keys staying as ordinary `except` entries) before focusing it. Find matches a Group Item's label, at its level's field, and never the figures, unless the source says it searches them.
In header form, where a group has no Group Item, a match on its label lands on the row that carries the label in the band: the first row of its span when it is open (past its own Subtotal placed before, which sits outside the span), or its Subtotal standing in for it when it is closed. Each label is counted once, and find never answers with an item that cannot be shown: the groups the source names above a match are those that must open to show it, so a Subtotal standing in names only the groups above its own. Focus takes the label in its level's Tier.
_Avoid_: Collapsed match, buried match

**Refused Drop**:
A drop target the source turns down as the user drags, with why: between groups (a Leaf moves only within its own group) or a Summary Item (Group Items and totals never move). The adapters draw the drop line in the error colour with a caption, and nothing moves on release. Editing a Summary Item's figures is refused the same way, the edit cancelled.
_Avoid_: Invalid drop, blocked move

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
