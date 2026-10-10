# Prior art: how table designs split model, view state and rendering

Research ticket: survey TanStack Table, Qt model/view, Glide Data Grid, egui_table / egui_extras `TableBuilder`, ratatui `Table` + `TableState`, and Textual `DataTable`, then draw conclusions for an immediate-mode + C ABI + wasm table core.

Sources: primary only (source code and in-repo docs), fetched 2026-10-10. Line anchors point at the default branch on that date (TanStack `main` is v9, `@tanstack/table-core` 9.2.8; Qt `qtbase` `dev`). The sandbox could reach `raw.githubusercontent.com` but not `doc.qt.io`, `tanstack.com` or `docs.rs`, so Qt claims cite the qdoc comments in `qtbase` source (the text the Qt docs are generated from) and TanStack claims cite the `docs/` folder in its repo.

## Summary of conclusions

1. **Every system that scales keeps rendering out of the data path and asks only for what is visible.** Glide (`getCellContent` per visible cell), egui_table (`TableDelegate::prepare` with the visible ranges, then `cell_ui`), egui_extras (`TableBody::rows` closure called only for visible rows) and Qt (`QTableView::paintEvent` iterates `visualIndexAt` first to last) all pull. ratatui is the outlier: `Table` owns a `Vec<Row>` of every row, so windowing is the caller's job.
2. **Row identity splits into two camps.** Index-only (Glide, ratatui, egui_extras, egui_table) versus stable key plus a separate position map (TanStack `getRowId`, Textual `RowKey` + `_row_locations`, Qt `QPersistentModelIndex` maintained by the model). Every keyed design has a mapping layer between "logical row" and "visual position": that is exactly druid_table's `LogIdx`/`VisIdx` + `Remap`.
3. **Sort/filter are either a pluggable stage or pushed out.** Qt (`QSortFilterProxyModel`) and TanStack (row-model pipeline with `manual*` bypass flags) make it an optional stage over the source; Glide and egui explicitly refuse and push it to the data source; Textual sorts in-widget by reading cell values.
4. **Selection follows identity.** Index ranges (Glide `CompactSelection`, ratatui `Option<usize>`) are compact but break on reorder; key sets (TanStack `Record<string, true>`) survive server round trips; Qt stores ranges of persistent indexes and pays for bookkeeping in the model.
5. **Measurement is always host-side.** No surveyed core measures text. Units are host-native (terminal cells as `u16`, egui points as `f32`, CSS pixels as JS numbers), and auto-sizing is either over visible rows only (egui_extras) or a full scan the host opts into (Qt `ResizeToContents`).

For our core that points to: a pure-data, pull-by-window boundary (host submits viewport and gets back a visible window as plain arrays, no render callbacks), an optional sort/filter stage that consumes a permutation or host-computed sort keys rather than reading cells, opaque `u64` row keys for durable state alongside index spaces for layout, and an abstract host unit.

## TanStack Table (v9)

**Split.** A "headless UI library" that provides "logic, state, processing, and APIs" but "do[es] not provide markup, styles, or pre-built implementations"; core is framework-agnostic TypeScript with adapters for React, Vue, Solid, Svelte, Angular, Lit and others, or plain `@tanstack/table-core` [T1]. v9 holds state as per-feature atoms (`table.atoms.rowSelection`, `table.store`) on TanStack Store; the app can own a slice by passing external `atoms` or `state` [T2].

**Row identity.** `row.id` defaults to `row.index`; override with the `getRowId` option, e.g. `(originalRow) => originalRow.uuid`; grouping/expanding append to the id [T3]. The server-side guide says to use a stable backend id via `getRowId` "when selection, expansion, or other row state must survive requests" [T4]. `getRowModel().rowsById` gives keyed lookup [T5].

**Virtualization.** Not in the core: "The TanStack Table packages do not come with any virtualization APIs or features built-in"; pair with TanStack Virtual or react-window [T6]. Virtualization "renders only the visible portion of the rows already loaded" and does not reduce filter/sort work [T4].

**Sort/filter.** A fixed pipeline of row models: `getCoreRowModel -> getFilteredRowModel -> getGroupedRowModel -> getSortedRowModel -> getExpandedRowModel -> getPaginatedRowModel -> getRowModel` [T5]. Each has a `manual*` flag (`manualSorting`, `manualFiltering`, `manualPagination`, ...) meaning "use the data you provide as already processed"; the feature still owns state and APIs, and with manual pagination the host also supplies `rowCount` or `pageCount` [T4]. Mixing client and server per feature is discouraged [T4].

**Selection.** `type RowSelectionState = Record<string, true>`, keyed by `row.id` [T7].

**Per frame.** `data`, `columns`, and any controlled state; the adapter renders `table.getRowModel().rows`. Column sizes are numbers "usually interpreted as pixel unit values", default `size: 150` [T8].

[T1]: https://github.com/TanStack/table/blob/main/docs/overview.md#L60-L68
[T2]: https://github.com/TanStack/table/blob/main/docs/framework/react/guide/table-state.md#L29-L38
[T3]: https://github.com/TanStack/table/blob/main/docs/guide/rows.md#L43-L58
[T4]: https://github.com/TanStack/table/blob/main/docs/guide/client-side-vs-server-side.md#L79-L112
[T5]: https://github.com/TanStack/table/blob/main/docs/guide/row-models.md#L159-L182
[T6]: https://github.com/TanStack/table/blob/v8/docs/guide/virtualization.md#L16
[T7]: https://github.com/TanStack/table/blob/main/docs/reference/index/type-aliases/RowSelectionState.md
[T8]: https://github.com/TanStack/table/blob/v8/docs/guide/column-sizing.md#L22-L53

(T4's `getRowId` advice is at line 172 and the virtualization note at lines 302 to 305 of the same file. T6 and T8 are from the v8 branch; I did not find the v9 equivalents in `docs/config.json`, so treat them as v8 facts.)

## Qt model/view (QAbstractItemModel, QSortFilterProxyModel, QHeaderView)

**Split.** Model (`QAbstractItemModel`: `rowCount`, `columnCount`, `data(index, role)`, `headerData`), view (`QTableView`), and separate view-state objects: `QHeaderView` owns section sizes and order, `QItemSelectionModel` owns selection. Data is pulled per role (`Qt::DisplayRole`, `Qt::ToolTipRole`, ...) [Q1].

**Row identity.** `QModelIndex` is (row, column, internal pointer or id); models call `createIndex()` with an identifier and "the combination of these values must be unique for each item" [Q1]. Indexes are ephemeral: "Model indexes should be used immediately and then discarded ... If you need to keep a model index over time use a QPersistentModelIndex" [Q2]. Persistence is the model's job: it must bracket structural edits with `beginInsertRows()`/`endInsertRows()` etc. ("If you want selections to be handled properly, you must ensure that you call these functions") and call `changePersistentIndex()` around `layoutAboutToBeChanged`/`layoutChanged` [Q1][Q3].

**Virtualization.** Implicit in the view. `QTableView::paintEvent` computes `firstVisualRow = verticalHeader->visualIndexAt(0)` and `lastVisualRow = verticalHeader->visualIndexAt(height())` and paints only that window [Q4]. Since Qt 6.9 `QHeaderView` stores per-section memory only once sections are resized or reordered, so "millions of sections" are cheap if you keep `ResizeMode::Fixed` [Q5]. Incremental loading: `canFetchMore()`/`fetchMore()` [Q1].

**Sort/filter.** Push-down is the default contract: `QAbstractItemModel::sort(column, order)` is virtual and "The base class implementation does nothing" [Q6]. Or wrap the source in `QSortFilterProxyModel`, which "transforms the structure of a source model by mapping the model indexes ... without duplicating the data", exposes `mapToSource()`/`mapFromSource()`, and is customised by overriding `lessThan()` and `filterAcceptsRow()` (which read source data through `sortRole()`/`filterRole()`, `Qt::DisplayRole` by default) [Q7]. By default it re-sorts dynamically on source change (`dynamicSortFilter`) [Q7].

**Column view state.** `QHeaderView` separates logical index (model column) from visual index (on-screen order): "The visual index will change if a section is moved, but the logical index will not change" [Q5]. `ResizeToContents` sizes "based on the contents of the entire column or row" [Q5]. Header labels come from `headerData()` [Q5].

**Selection.** `QItemSelectionModel` stores selections "using ranges" (`QItemSelectionRange`, `QItemSelection`) with a two-layer committed/current model [Q8]; each range holds `QPersistentModelIndex tl, br` [Q9], so it is keyed by persistent index and relies on model notifications to stay correct.

**Per frame.** Nothing explicit: retained-mode. The view calls `data()` for visible cells during paint.

[Q1]: https://github.com/qt/qtbase/blob/dev/src/corelib/itemmodels/qabstractitemmodel.cpp#L1505-L1562
[Q2]: https://github.com/qt/qtbase/blob/dev/src/corelib/itemmodels/qabstractitemmodel.cpp#L1251-L1254
[Q3]: https://github.com/qt/qtbase/blob/dev/src/corelib/itemmodels/qabstractitemmodel.cpp#L1710-L1760
[Q4]: https://github.com/qt/qtbase/blob/dev/src/widgets/itemviews/qtableview.cpp#L1552-L1560
[Q5]: https://github.com/qt/qtbase/blob/dev/src/widgets/itemviews/qheaderview.cpp#L81-L198
[Q6]: https://github.com/qt/qtbase/blob/dev/src/corelib/itemmodels/qabstractitemmodel.cpp#L2530-L2540
[Q7]: https://github.com/qt/qtbase/blob/dev/src/corelib/itemmodels/qsortfilterproxymodel.cpp#L1963-L2082
[Q8]: https://github.com/qt/qtbase/blob/dev/src/corelib/itemmodels/qitemselectionmodel.cpp#L1139-L1167
[Q9]: https://github.com/qt/qtbase/blob/dev/src/corelib/itemmodels/qitemselectionmodel.h#L82

## Glide Data Grid

**Split.** A canvas-rendered React component (`DataEditor`). The host supplies "which columns you have, how many rows, and ... a function it can call to get the data for a cell" [G1]. Props: `columns: readonly GridColumn[]`, `rows: number`, `getCellContent: (cell: Item) => GridCell` [G2], with `type Item = readonly [col: number, row: number]` [G3].

**Row identity.** Plain integer row index; no row key concept in `Item` or `GridSelection` [G3]. Columns may carry an optional `id` (`SizedGridColumn { width: number; id?: string }`) [G3].

**Virtualization.** Intrinsic: "Cells are rendered lazily on demand", "scales to millions of rows" [G1]. The README says they abandoned DOM virtualization (react-virtualized) for canvas because of scroll performance [G1]. The grid reports what it shows via `onVisibleRegionChanged(range: Rectangle, tx, ty, extras)` so hosts can prefetch [G2]. `rowHeight: number | ((index: number) => number)`, default 34 [G4].

**Sort/filter.** Pushed out: "Filtering and sorting are something you would have to implement with your data source ... can often also be implemented more efficiently in the data source, via a database query" [G1]. A separate `@glideapps/glide-data-grid-source` package adds sorting on top [G5]. Off-screen data for copy goes through `getCellsForSelection(selection: Rectangle)`, which may return a thunk for async loading [G4].

**Selection.** `GridSelection { current?: { cell: Item; range: Rectangle; rangeStack }, columns: CompactSelection, rows: CompactSelection }`, all index-based, controlled via `gridSelection` / `onGridSelectionChange` [G3][G2].

**Per frame.** Retained React props; the grid pulls `getCellContent` for visible cells during canvas draw.

[G1]: https://github.com/glideapps/glide-data-grid/blob/main/README.md#L125-L150
[G2]: https://github.com/glideapps/glide-data-grid/blob/main/packages/core/src/data-editor/data-editor.tsx#L286-L546
[G3]: https://github.com/glideapps/glide-data-grid/blob/main/packages/core/src/internal/data-grid/data-grid-types.ts#L17-L177
[G4]: https://github.com/glideapps/glide-data-grid/blob/main/packages/core/API.md#L560-L650
[G5]: https://github.com/glideapps/glide-data-grid/blob/main/packages/source/README.md

## egui_table (rerun-io)

**Split.** "Batteries not included" [E1]. `Table` (builder: `num_rows(u64)`, `columns`, `num_sticky_cols`, `headers`) plus a `TableDelegate` trait the app implements [E2]. Persisted view state is `TableState { col_widths: IdMap<f32>, parent_width }`, stored in egui memory via `load`/`store` [E2].

**Row identity.** `CellInfo { col_nr: usize, row_nr: u64, table_id }`: index only [E2].

**Virtualization.** The core computes the window and tells the delegate first: `fn prepare(&mut self, info: &PrefetchInfo)` where `PrefetchInfo { num_sticky_columns, visible_columns: Range<usize>, visible_rows: Range<u64> }`, documented as "You can use this to only load the data required to be viewed"; then `cell_ui`/`header_cell_ui` per visible cell [E2]. Variable heights come from `fn row_top_offset(&self, ctx, table_id, row_nr) -> f32` (cumulative offset, must be 0 for row 0) with `default_row_height()` 20.0 [E2].

**Sort/filter, selection.** None in the crate (no sort or select API in `table.rs`) [E2]; both live in the app.

**Per frame.** Rebuild `Table` and call `show(ui, &mut delegate)`; the delegate draws each visible cell directly into an egui `Ui`.

[E1]: https://github.com/rerun-io/egui_table/blob/main/README.md
[E2]: https://github.com/rerun-io/egui_table/blob/main/egui_table/src/table.rs#L29-L230

## egui_extras TableBuilder

**Split.** Immediate-mode builder: `TableBuilder::new(ui).column(Column::...).header(h, |row| ..).body(|body| ..)` [X1]. Persisted state is a private `TableState { column_widths: Vec<f32>, max_used_widths }` (index-keyed) [X1].

**Row identity.** `TableRow::index()` returns a `usize` [X1].

**Virtualization.** `TableBody::rows(row_height, total_rows, |row| ..)` "Is a lot more performant than adding each individual row as non visible rows must not be rendered"; `heterogeneous_rows(heights: impl Iterator<Item = f32>, ..)` must iterate all heights to size the space above and below the window [X1].

**Sort/filter.** None; caller orders data. `Column::auto()` sizes from content, but with `rows`/`heterogeneous_rows` "the automatic size will only be based on the currently visible rows" [X1].

**Selection.** Purely presentational: `TableRow::set_selected(bool)` sets highlight; the app holds the selected set [X1].

**Per frame.** Everything: columns, row count, heights and a closure that draws each visible row.

[X1]: https://github.com/emilk/egui/blob/main/crates/egui_extras/src/table.rs#L45-L1360

(Relevant spots: `Column::auto` L45 to 56, `TableState` L577 to 584, `rows` L1003 to 1030, `heterogeneous_rows` L1084 to 1116, `set_selected` L1327 to 1331, `index` L1357.)

## ratatui Table + TableState

**Split.** `Table<'a>` holds `rows: Vec<Row<'a>>` and `widths: Vec<Constraint>` and implements `StatefulWidget`; `TableState { offset: usize, selected: Option<usize>, selected_column: Option<usize> }` must be kept in app state "so that the selected row is preserved across renders" [R1][R2].

**Row identity.** Index into `rows` [R2].

**Virtualization.** The widget windows rendering (`visible_rows(state, area)` from `offset` and per-row `height: u16`, default 1) and writes back `state.offset` [R1][R3]. But the host must construct a `Row` for every row each frame since `Table::new` takes them all [R1]; skipping off-screen rows is an app-level trick, not an API.

**Sort/filter.** None; the app builds rows in display order.

**Selection.** Single row/column/cell by index; render clamps `selected` to `rows.len() - 1` [R1]. No multi-select.

**Per frame.** All rows (as styled `Text`), width `Constraint`s in terminal cells, and `&mut TableState`.

[R1]: https://github.com/ratatui/ratatui/blob/main/ratatui-widgets/src/table.rs#L235-L246
[R2]: https://github.com/ratatui/ratatui/blob/main/ratatui-widgets/src/table/state.rs#L36-L58
[R3]: https://github.com/ratatui/ratatui/blob/main/ratatui-widgets/src/table/row.rs#L11

(In `table.rs`: clamp at L784 to 785, `render_rows` writes `state.offset` at L883, `visible_rows` at L1026.)

## Textual DataTable

**Split.** A retained widget that owns the data: `_data: dict[RowKey, dict[ColumnKey, CellType]]`, plus `rows`/`columns` metadata dicts [T9].

**Row identity.** `RowKey`/`ColumnKey` (wrapping an optional string; auto-generated if absent) "Uniquely identifies a row ... Even if the visual location of the row changes due to sorting"; `CellKey = (row_key, column_key)` [T10]. Order is a separate `_row_locations: TwoWayDict[RowKey, int]`, so "rows and columns are location independent" [T9]. `add_row(..., key=)` raises `DuplicateKey` on reuse [T11].

**Virtualization.** Line-based: `DataTable` is a `ScrollView` and `render_line(y)` renders one terminal line on demand, backed by several `LRUCache`s (row, cell, line) [T12][T9].

**Sort/filter.** `sort(*columns, key=None, reverse=False)` reads the stored cell values, sorts, and only rewrites `_row_locations` [T13]. No filter API (no `filter` in the module).

**Selection.** A single cursor: `cursor_type: Literal["cell", "row", "column", "none"]` and `cursor_coordinate: Coordinate` (positional), convertible via `coordinate_to_cell_key` [T14].

**Per frame.** Nothing; data is pushed with `add_row`/`update_cell` and the widget repaints.

[T9]: https://github.com/Textualize/textual/blob/main/src/textual/widgets/_data_table.py#L728-L771
[T10]: https://github.com/Textualize/textual/blob/main/src/textual/widgets/_data_table.py#L128-L157
[T11]: https://github.com/Textualize/textual/blob/main/src/textual/widgets/_data_table.py#L1669-L1693
[T12]: https://github.com/Textualize/textual/blob/main/src/textual/widgets/_data_table.py#L2478-L2496
[T13]: https://github.com/Textualize/textual/blob/main/src/textual/widgets/_data_table.py#L2592-L2638
[T14]: https://github.com/Textualize/textual/blob/main/src/textual/widgets/_data_table.py#L1278

(`CursorType` at L53, `cursor_coordinate` at L422.)

## Comparison

| System | Row identity | Virtualization | Sort / filter | Selection keyed by | Host supplies per frame |
|---|---|---|---|---|---|
| TanStack | `row.id` = index, or `getRowId` | none in core (TanStack Virtual) | in-core row-model pipeline, or `manual*` push-down | `row.id` string set | data + controlled state; adapter renders |
| Qt | `QModelIndex` (ephemeral) + `QPersistentModelIndex` | view paints visible range from header positions | model `sort()` push-down or `QSortFilterProxyModel` mapping | ranges of persistent indexes | nothing (retained; pull `data()` on paint) |
| Glide | integer index | canvas, lazy per visible cell | pushed to data source | index ranges (`CompactSelection`) | retained props; pull `getCellContent` |
| egui_table | `u64` index | core computes window, `prepare(PrefetchInfo)` | none | none | `num_rows`, columns, delegate draws cells |
| egui_extras | `usize` index | `rows`/`heterogeneous_rows` | none | none (highlight flag only) | everything, closure per visible row |
| ratatui | `usize` index | renders window, but takes all rows | none | `Option<usize>` | all rows + `TableState` |
| Textual | `RowKey` + location map | `render_line` + LRU caches | in-widget sort over owned values; no filter | positional cursor | nothing (data pushed in) |

## Implications for our core

**Does the core read cell values?** Mostly no. Glide, egui_table, egui_extras and ratatui never look at values; Qt and Textual do only to sort/filter, and TanStack does so only in client-side row models it lets you bypass. The core should treat cells as opaque, emitting (row, column, rect) and letting the adapter fetch and draw. For in-core sort/filter, avoid Qt's per-comparison `lessThan()` shape: over a C ABI or the wasm/JS boundary that is O(n log n) cross-boundary calls. Instead accept either a host-computed permutation (TanStack `manualSorting`, druid_table's `Remap::Internal`) or a one-shot batch of host-extracted sort keys per row (numbers or byte strings) that the core sorts natively. Filtering likewise takes a host-supplied bitmask or row list. That keeps the core value-free and the C ABI simple.

**Snapshot-pull vs callbacks.** Immediate-mode hosts already think "per frame, what is visible?" (egui_table `prepare`, egui_extras `rows`). Callbacks into the host (`getCellContent`, `cell_ui`) are natural in one language but awkward across C (function pointer + `void*` context, reentrancy rules) and wasm (calls back into JS per cell). Recommend a two-phase, data-only frame: (1) host calls `layout(viewport, scroll, row_count, height_source)`; (2) core returns a plain visible window (visible row range in visual order with logical index and key, column x positions, header layout), which is what `PrefetchInfo` and `onVisibleRegionChanged` already express; (3) host draws. Input events go in as data and come back as state deltas. A retained web adapter wraps the same calls, as TanStack's adapters wrap its core. Variable row heights should be a host-supplied array or prefix-sum buffer (egui_table's `row_top_offset` is effectively a prefix sum), with a uniform-height fast path, rather than a per-row callback.

**Units and measurement.** Every system uses the host's native unit (ratatui `u16` cells, egui `f32` points, Glide/TanStack CSS-pixel numbers, Qt `int` pixels) and none measures text in the core. The core should use one abstract unit chosen by the host (TUI adapter: 1 unit = 1 cell; web: CSS px), and never measure: auto-size takes host-reported content widths. Copy egui_extras' scoping explicitly: auto-size over visible rows by default, full-column (Qt `ResizeToContents`) only on request. Column widths must be keyed by column id, not position (egui_table `IdMap<f32>`, Qt logical vs visual index), so reorder does not lose sizes.

**Stable row keys vs indices.** Index-only designs are simplest but break selection on sort, filter, insert or server refetch (TanStack's guide says this outright [T4]); Qt fixes it with model-side persistent-index bookkeeping, which is a heavy contract for a C host. Textual and TanStack show the lighter pattern: an opaque, host-provided key plus a position map. Recommend keeping druid_table's two index spaces (`LogIdx` for data order, `VisIdx` after sort/filter) for layout and hit-testing, and adding an optional host-supplied `u64` row key used only for durable state (selection, expansion, focus anchor). Store selection as a key set with anchor/focus keys, plus an "all except" inverted form for select-all on huge or remote data. When the host supplies no keys, fall back to logical index (TanStack's default), with the documented caveat that selection then does not survive data replacement.
