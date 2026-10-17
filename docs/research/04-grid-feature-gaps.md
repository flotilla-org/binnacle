# Grid feature gaps

Which features do users of data grids and table viewers ask for, which of them matter for binnacle, and which does binnacle lack? This note merges two feature inventories (ten web data grids, and ten spreadsheets, terminal viewers and native toolkits), audits binnacle against every feature, scores demand and fit with Jev, and ranks the gaps. The merged, audited and scored list is [`data/grid-feature-gaps.json`](./data/grid-feature-gaps.json); the web adapter's gaps demo ([#81](https://github.com/flotilla-org/binnacle/issues/81)) renders it in a binnacle table.

Everything here is as of 2026-10-17. binnacle was read at the top of the web stack: branch `web/flaky-e2e` at `a81e1ff`, which carries the web adapter tickets #54 to #62 on top of `main`. File references below are relative to that tree.

## Summary

- **143 features**, from 92 web-grid features and 90 others, with 39 merged where both inventories described the same user-visible feature. 108 of them are gaps (missing or partial in at least one adapter where they apply): 66 widget, 26 source and 16 app features.
- **binnacle is already strong where it was designed to be.** The features with the highest combined score are row virtualization, click-to-sort, keyboard navigation, horizontal scrolling, a sticky header and auto-size, and binnacle has all of them in the core, ratatui and the web.
- **The top gaps cluster in four places.** Finding and filtering (quick filter, regex search, per-column and multi-column filters, set filters); column management (show and hide, frozen columns, flex widths); long values (wrapping, an indicator when a value is cut, expanding a cell); and small API holes in one adapter (no scrollbar in ratatui, no scroll-to or row cursor option on the web widget).
- **Several high-demand features are out of scope by an earlier decision**, chiefly clipboard copy, CSV export, a totals row and aggregation (#1, #13, #53). Clipboard copy scores as high as anything in the top ten on demand and is worth reopening.
- **Recommended next:** show and hide columns; quick filter and find with match highlighting; frozen columns; the web widget's missing focus and scroll API with #81; a ratatui scrollbar; clipboard copy of the selection; long-value handling (wrap, truncation marker, expand). See the last section.

## Method

1. **Inventories.** Two research agents each surveyed ten products and wrote one JSON object per user-visible feature: a category, per-product support (`core`, `paid`, `plugin`, `partial` or `no`), and cited demand evidence (issue reactions, Stack Overflow question counts, discussions, docs). The web set covered 92 features and the other set 90.
2. **Merge.** Features that were the same thing to a user were unified under one id, keeping the evidence and product cells from both (for example `column-resize` into `column-resizing`, `quick-search` into `find-highlight`, `range-copy-paste` into `clipboard-paste`, `row-grouping-pivot` into `row-grouping`). The other inventory's `row-selection` meant a single-row cursor, not multi-select, so it became `row-cursor`. Every feature carries a cell for all 20 products. Where only one inventory covered a feature, the other ten products are `unknown`, not a guess, so `products_with` must be read against `products_surveyed`.
3. **Corrections.** For the 30 features with the most cited evidence, product cells flagged as unverified were checked against vendor sources. Only Tad cells were flagged. Tad's source at `272ffa0` shows range copy (SlickGrid `CellCopyManager`, now `core`), saved `.tad` workspaces (`core`), cell-range selection without row or column modes (`partial`), nulls drawn blank like empty strings (`no`), and no ARIA roles (`no`). The other Tad cells flagged as unverified (zebra stripes, truncation marker) sit outside the top 30 and were left as the researcher marked them.
4. **Kind.** Each feature is `widget` (the table component itself), `source` (a data operation a Table Source owns: sort, filter, group, pivot, lazy loading) or `app` (the application around a table: tabs, saved views, history, file loading and export). Per CONTEXT.md, sorting and filtering are `source` even though the user starts them from a header.
5. **Audit.** Each feature has a status for `core` (binnacle-core plus the In-Memory Table Source in binnacle-memory), `ratatui` and `web`: `has`, `partial`, `missing`, `n/a` or `out-of-scope`, with a one-line note citing file and line. Demo-only support counts as `partial`. `out-of-scope` is used only where a spec issue's Out of Scope section names the feature outright; deferrals marked "later" stay `missing`, with the statement in `scope_ref`. A gap is a feature that is `missing` or `partial` in any adapter.
6. **Scoring.** Jev (TypeSafe System One, requested as `jev-latest`, served by `jev-1.13.0`) answered two five-level Score questions per feature in one request. The state held the feature's name, description, category, kind, per-product support and demand signals, and never binnacle's status, so the scores are independent of the audit. `demand` asks how much users of data tables and grids want the feature (niche, specialist, common, expected, table stakes). `fit` asks how much it matters for binnacle's uses (a keyboard-driven terminal viewer for result sets, a terminal CSV viewer, a table embedded in terminal, web and native apps, and later a table served over a protocol), from irrelevant to essential. Both run from 0 to 4; the ranking uses `demand x fit` (0 to 16). Each answer's `confidence` and per-level `probabilities` are kept in the data. A third question about paid tiers was dropped: the product cells already record which vendors charge, and the data carries that as `paid_share`. The script is [`scripts/score_gaps_jev.py`](./scripts/score_gaps_jev.py).
7. **Sanity check.** Before the full run, twelve features expected at opposite ends were scored. They ordered sensibly: row virtualization (demand 3.40, fit 3.63), click-to-sort (3.91, 3.10) and keyboard navigation (3.06, 3.62) at the top; spreadsheet formulas (1.03, 0.40), cell comments (0.67, 1.21) and an AI assistant (0.19, 1.17) at the bottom; sparkline cells low (1.01, 1.56); and reading from stdin low on demand but high on fit (1.14, 2.97), as a CSV viewer would expect. One rewording of the fit levels was tried; it moved fit by at most 0.21 and pushed the low end up, which separated features less, so the original wording was kept. The full run used about 162,000 input and 4,900 output tokens, including the sanity runs.

## Products

| Product | What it is | Weekly npm downloads (2026-10-06 to 10-12) | GitHub stars |
|---|---|---|---|
| [AG Grid](https://www.ag-grid.com/) | Web grid; MIT Community, paid Enterprise | 3,641,895 community, 1,787,724 enterprise | 15,640 |
| [MUI X Data Grid](https://mui.com/x/react-data-grid/) | React grid; MIT core, paid Pro and Premium | 3,117,967 MIT, 1,475,188 Pro, 604,003 Premium | 5,863 |
| [Glide Data Grid](https://grid.glideapps.com/) | Canvas React grid, MIT | 370,569 | 5,396 |
| [TanStack Table](https://tanstack.com/table) | Headless table logic, MIT | 24,105,379 react-table, 25,715,060 table-core | 28,496 |
| [Handsontable](https://handsontable.com/) | Spreadsheet-like grid, commercial licence | 323,680 | 22,067 |
| [Tabulator](https://tabulator.info/) | Web grid, MIT | 189,648 | 7,786 |
| [RevoGrid](https://rv-grid.com/) | Web grid; MIT core, paid Pro | 43,879 | 3,450 |
| [FINOS Perspective](https://perspective.finos.org/) | Wasm analytics grid and charts, Apache-2.0 | 17,811 engine, 17,867 viewer | 11,313 |
| [react-data-grid](https://github.com/Comcast/react-data-grid) | React grid, MIT | 756,939 | 7,689 |
| [DataTables](https://datatables.net/) | jQuery table plugin; MIT, paid Editor | 689,381 | 7,392 |
| [Microsoft Excel](https://www.microsoft.com/microsoft-365/excel) | Spreadsheet | commercial | n/a |
| [Google Sheets](https://workspace.google.com/products/sheets/) | Spreadsheet | commercial | n/a |
| [VisiData](https://github.com/saulpw/visidata) | Terminal data explorer | n/a | 9,351 |
| [ratatui Table](https://docs.rs/ratatui/latest/ratatui/widgets/struct.Table.html) | Terminal table widget | n/a | 22,993 (ratatui) |
| [csvlens](https://github.com/YS-L/csvlens) | Terminal CSV viewer (tabiew noted where it differs) | n/a | 4,013 (tabiew 3,142) |
| [k9s](https://github.com/derailed/k9s) | Kubernetes TUI built on tables (lazygit as secondary evidence) | n/a | 34,827 (lazygit 83,253) |
| [Qt QTableView](https://doc.qt.io/qt-6/qtableview.html) | Native model/view table | n/a | 3,089 on the qtbase mirror, not representative |
| [egui table](https://github.com/rerun-io/egui_table) | Immediate-mode Rust tables (egui_extras and egui_table) | n/a | 31,536 (egui), 83 (egui_table) |
| [Datasette](https://github.com/simonw/datasette) | Web explorer for SQLite | n/a | 11,515 |
| [Tad](https://github.com/antonycourtney/tad) | Desktop CSV and Parquet viewer with pivots | n/a | 3,491 |

Stars were read on 2026-10-17.

## Biases and how they were handled

- **Reaction counts do not compare across trackers.** MUI X opens one issue per feature and collects hundreds of thumbs-up on each; ratatui and csvlens issues collect single digits; k9s has a very large user base. Counts were passed to Jev as evidence for one feature within its tracker, and the demand levels are phrased around how many products ship a feature and whether its absence draws complaints, not around raw counts.
- **Tracker shape inflates some features.** A feature that is easy to name gets its own issue (column pinning, quick filter); a feature everyone already has gets none (a sticky header has one cited request). Product coverage partly offsets this: sticky header still scores demand 3.42 because all ten surveyed products have it.
- **Half the product cells are `unknown`.** 104 features were described by only one inventory, so they were surveyed against ten products, not twenty. `products_with` is out of `products_surveyed`, not out of 20.
- **Some product cells came from the researchers' knowledge.** Only the Tad cells were flagged as unverified, and those in the top 30 by evidence were checked against source. The remaining web-grid cells were marked from vendor docs, which describe claims rather than quality (for example IME support).
- **k9s demand is often for the application.** Requests for persisted layouts, tabs, history and keybinding files are about k9s as an app, not its table. These are classed `app` and listed separately below.
- **Jev's fit leans toward the terminal viewers.** The uses named in the fit question are mostly read-mostly viewers, so editing features score low on fit (inline editing 1.36, framework wrappers 0.79) even though embedded web apps want them. Read fit as "fit for the viewer uses first".
- **Demand saturates near "expected".** Most well-known features land between 2.8 and 3.1 on demand with high confidence, so in the top 25 the ranking is driven largely by fit. Treat differences under about 0.5 in `demand x fit` as ties.

## Top 25 gaps (widget and source)

Ranked by `demand x fit`. Status is core / ratatui / web. Products is how many surveyed products have the feature in any form.

| # | Feature | Kind | Demand | Fit | D x F | Core / ratatui / web | Products | Evidence |
|---|---|---|---|---|---|---|---|---|
| 1 | Row cursor and highlight | widget | 3.03 | 3.48 | 10.5 | has / has / partial | 9/10 | [issue](https://github.com/emilk/egui/issues/2000) |
| 2 | Pagination | widget | 3.03 | 3.26 | 9.9 | missing / missing / missing | 8/10 | [so](https://stackoverflow.com/search?q=%5Bag-grid%5D%20title%3A%22pagination%22), [issue](https://github.com/revolist/revogrid/issues/149) |
| 3 | Infinite scroll / lazy row loading | source | 2.99 | 3.27 | 9.8 | partial / partial / has | 9/10 | [issue](https://github.com/mui/mui-x/issues/10857), [issue](https://github.com/mui/mui-x/issues/10858) |
| 4 | Scrollbar | widget | 2.70 | 3.62 | 9.8 | has / missing / has | 7/10 | [issue](https://github.com/ratatui/ratatui/issues/173), [issue](https://github.com/ratatui/ratatui/issues/174) |
| 5 | Server-side sort/filter/group | source | 2.99 | 3.26 | 9.7 | partial / partial / partial | 9/10 | [so](https://stackoverflow.com/search?q=%5Bag-grid%5D%20title%3A%22server%20side%22), [issue](https://github.com/mui/mui-x/issues/3377) |
| 6 | Quick filter / global search box | source | 3.00 | 3.12 | 9.4 | partial / missing / partial | 17/20 | [issue](https://github.com/mui/mui-x/issues/202), [issue](https://github.com/mui/mui-x/issues/2842) |
| 7 | Scroll to row/cell | widget | 3.01 | 3.08 | 9.3 | has / has / missing | 10/10 | [issue](https://github.com/glideapps/glide-data-grid/issues/129), [issue](https://github.com/glideapps/glide-data-grid/issues/705) |
| 8 | Per-column filter menu | source | 3.04 | 3.04 | 9.2 | partial / missing / partial | 15/20 | [so](https://stackoverflow.com/search?q=%5Bag-grid%5D%20title%3A%22filter%22), [issue](https://github.com/glideapps/glide-data-grid/issues/1002) |
| 9 | Show/hide columns (column chooser) | widget | 3.24 | 2.85 | 9.2 | partial / missing / missing | 20/20 | [issue](https://github.com/mui/mui-x/issues/5700), [issue](https://github.com/mui/mui-x/issues/6651) |
| 10 | Regex search and filter | source | 2.79 | 3.17 | 8.8 | missing / missing / missing | 7/10 | [docs](https://k9scli.io/topics/commands/), [docs](https://github.com/YS-L/csvlens) |
| 11 | Filters on several columns at once | source | 2.99 | 2.94 | 8.8 | partial / partial / partial | 8/10 | [issue](https://github.com/mui/mui-x/issues/201), [docs](https://mui.com/x/react-data-grid/filtering/multi-filters/) |
| 12 | Column pinning (freeze left/right) | widget | 3.00 | 2.91 | 8.7 | missing / missing / missing | 15/20 | [issue](https://github.com/mui/mui-x/issues/193), [issue](https://github.com/mui/mui-x/issues/297) |
| 13 | Filter by value list (set filter) | source | 2.94 | 2.88 | 8.5 | missing / missing / missing | 8/10 | [so](https://stackoverflow.com/search?q=%5Bag-grid%5D%20title%3A%22set%20filter%22), [issue](https://github.com/mui/mui-x/issues/9243) |
| 14 | Asynchronous, incremental loading | source | 2.53 | 3.33 | 8.4 | partial / partial / partial | 8/10 | [issue](https://github.com/simonw/datasette/issues/1275), [issue](https://github.com/rerun-io/egui_table/issues/4) |
| 15 | Loading, empty and error overlays | widget | 2.81 | 2.91 | 8.2 | partial / partial / partial | 7/10 | [issue](https://github.com/mui/mui-x/issues/1685), [issue](https://github.com/ag-grid/ag-grid/issues/1830) |
| 16 | Type-aware alignment | widget | 2.75 | 2.80 | 7.7 | n/a / partial / partial | 10/10 | [issue](https://github.com/ratatui/ratatui/issues/257), [issue](https://github.com/YS-L/csvlens/issues/80) |
| 17 | Wrap text in cells | widget | 2.86 | 2.66 | 7.6 | partial / missing / missing | 17/20 | [issue](https://github.com/mui/mui-x/issues/886), [issue](https://github.com/mui/mui-x/issues/1040) |
| 18 | Row selection with checkboxes | widget | 3.02 | 2.46 | 7.4 | partial / partial / partial | 16/20 | [so](https://stackoverflow.com/search?q=%5Bag-grid%5D%20title%3A%22checkbox%22), [so](https://stackoverflow.com/search?q=%5Bag-grid%5D%20title%3A%22row%20selection%22) |
| 19 | Truncation indicator | widget | 2.47 | 2.99 | 7.4 | n/a / missing / has | 8/10 | [issue](https://github.com/shshemi/tabiew/issues/35) |
| 20 | Auto row height from content | widget | 2.91 | 2.46 | 7.2 | has / partial / partial | 16/20 | [issue](https://github.com/mui/mui-x/issues/417), [issue](https://github.com/mui/mui-x/issues/438) |
| 21 | Flex / percentage column widths | widget | 2.96 | 2.40 | 7.1 | missing / missing / missing | 10/10 | [issue](https://github.com/mui/mui-x/issues/347), [issue](https://github.com/TanStack/table/issues/43) |
| 22 | Number formatting | widget | 2.45 | 2.87 | 7.0 | n/a / missing / has | 7/10 | [issue](https://github.com/YS-L/csvlens/issues/197), [issue](https://github.com/saulpw/visidata/issues/454) |
| 23 | Expand a truncated cell | widget | 2.40 | 2.92 | 7.0 | missing / missing / missing | 7/10 | [issue](https://github.com/YS-L/csvlens/issues/83), [docs](https://jsvine.github.io/intro-to-visidata/intermediate/large-cells/) |
| 24 | Native browser text selection | widget | 2.72 | 2.50 | 6.8 | n/a / n/a / missing | 7/10 | [issue](https://github.com/ag-grid/ag-grid/issues/87), [issue](https://github.com/tabulator-tables/tabulator/issues/4178) |
| 25 | Pinned rows (top/bottom) | widget | 2.96 | 2.26 | 6.7 | missing / missing / missing | 9/10 | [issue](https://github.com/mui/mui-x/issues/1251), [issue](https://github.com/glideapps/glide-data-grid/issues/653) |

What binnacle has today for each:

1. **Row cursor and highlight.** SelectionUnit::Rows makes focus select its row (crates/binnacle-core/src/selection.rs:8, crates/binnacle-core/src/grid.rs:185); FocusChanged Effect (crates/binnacle-core/src/effect.rs:26). Web has no unit option, reports focus via onFrame (web/src/widget.ts:70). Issue #81.
2. **Pagination.** No paging; all adapters scroll one virtual surface instead.
3. **Infinite scroll / lazy row loading.** Grid asks only for Window keys and draws unfilled ones as loading (crates/binnacle-core/src/grid.rs:262, ratatui crates/binnacle-ratatui/src/lib.rs:250); web source window/cells may answer late with AbortSignal (web/src/source.ts:41,71); no block loader. Scope: #13 later: async and lazy row loading beyond rendering unfilled positions as loading; #53 later add-on: server-backed sources and a wire protocol.
4. **Scrollbar.** Core gives Snapshot::thumb and Action::ScrollTo (crates/binnacle-core/src/snapshot.rs:181, crates/binnacle-core/src/effect.rs:103); web uses a native scroller (web/src/scroller.ts:1); ratatui draws none.
5. **Server-side sort/filter/group.** Sort delegated to the source via SortRequested (crates/binnacle-core/src/effect.rs:33), web sort async (web/src/source.ts:46); filter is host-side via Reset; no grouping, paging or server source. Issue #48. Scope: #53 later add-on: server-backed sources and a wire protocol; #13 later: tree view and grouping; #1 out: pivots and aggregation.
6. **Quick filter / global search box.** Core: row predicate filter only (crates/binnacle-memory/src/memory_source.rs:118). Web demo only: search box filters name/port live (web/demo/sources/records.ts:61). ratatui demo only has type-ahead jump. Scope: #53 out of scope: whole-table find.
7. **Scroll to row/cell.** Grid::set_focus(row,col) by Key scrolls to show it (crates/binnacle-core/src/grid.rs:492), ratatui hosts call it directly; GridWidget exposes no set_focus or scroll method (web/src/widget.ts:85).
8. **Per-column filter menu.** Only a whole-row predicate filter (crates/binnacle-memory/src/memory_source.rs:118, web/src/rows.ts:34); no per-column conditions or filter menu UI.
9. **Show/hide columns (column chooser).** No hide API; a source could drop columns via column-axis Change Notices, but In-Memory filter is rows only (crates/binnacle-memory/src/table.rs:384); fromRows columns are fixed (web/src/rows.ts:128).
10. **Regex search and filter.** Nothing; no regex anywhere. Closest is the host row predicate filter (crates/binnacle-memory/src/memory_source.rs:118, web/src/rows.ts:35).
11. **Filters on several columns at once.** One host row predicate may test several columns (crates/binnacle-memory/src/memory_source.rs:118, web/src/rows.ts:35); no per-column filter model or AND/OR; ratatui demo Ctrl+F is one fixed condition.
12. **Column pinning (freeze left/right).** Only the row-number Header Band stays fixed; no frozen data columns. Scope: #13 later: pinned or frozen rows and columns.
13. **Filter by value list (set filter).** Nothing; no distinct-value listing or checklist. Closest is the row predicate filter (crates/binnacle-memory/src/memory_source.rs:118).
14. **Asynchronous, incremental loading.** MemorySource::appended keeps sort and filter (crates/binnacle-memory/src/memory_source.rs:138), Inserted notices; web count may be a Promise (web/src/source.ts:39), FrameInfo.loading; no loader or progress. Scope: #13 later: async and lazy row loading beyond rendering unfilled positions as loading; #13 later: background sort and filter for large in-memory tables.
15. **Loading, empty and error overlays.** Per-cell loading marker only: core unfilled Positions draw as loading (crates/binnacle-core/src/grid.rs:261); ratatui crates/binnacle-ratatui/src/lib.rs:33, 250; web web/src/canvas/body.ts:100, FrameInfo.loading web/src/widget.ts:72. No empty or error overlay.
16. **Type-aware alignment.** Manual per-column alignment only: ratatui Align::Right crates/binnacle-ratatui/src/renderer.rs:171; web textRenderer align end web/src/renderers/text.ts:8. Not type-aware, no sign padding.
17. **Wrap text in cells.** Core only gives the content-size hook (crates/binnacle-core/src/grid.rs:267, wrapped text height); ratatui TextRenderer is one line (crates/binnacle-ratatui/src/renderer.rs:178); web text is one ellipsised line (web/src/canvas/body.ts:70).
18. **Row selection with checkboxes.** Ctrl-click toggles row picks (crates/binnacle-core/src/gesture.rs:220), Snapshot::picked (crates/binnacle-core/src/snapshot.rs:284); no keyboard pick Action, checkbox column or pick-all; web reports only a count (web/src/widget.ts:68). Issue #81. Scope: #1/#13 later: multiple selection ranges and richer selection modes.
19. **Truncation indicator.** web ellipsises cells and headers via measure.fit (web/src/measure.ts:7, web/src/canvas/body.ts:70, web/src/canvas/bands.ts:73); ratatui clips with no marker (crates/binnacle-ratatui/src/renderer.rs:178).
20. **Auto row height from content.** Core takes per-item content sizes, set_window_sized crates/binnacle-core/src/grid.rs:272; ratatui draws only first visible line (crates/binnacle-ratatui/src/renderer.rs:213); web has drag row resize but no content sizing (web/src/autosize.ts:28).
21. **Flex / percentage column widths.** Fixed default size plus Size Overrides only (crates/binnacle-core/src/measure.rs:40); no flex or percentage widths.
22. **Number formatting.** web textRenderer format takes Intl.NumberFormat or a function (web/src/renderers/text.ts:10); ratatui leaves strings to the host's Content::cell (crates/binnacle-ratatui/src/lib.rs:52).
23. **Expand a truncated cell.** Nothing; closest is the inline editor (ratatui crates/binnacle-ratatui/src/lib.rs:310 shows the text end).
24. **Native browser text selection.** Web cells are painted on canvas (web/src/widget.ts:135 aria-hidden canvas); no selectable DOM text.
25. **Pinned rows (top/bottom).** Nothing; only the column header band stays at the top. Scope: #13 later: pinned or frozen rows and columns.

Two of these are worth not chasing as stated. **Pagination** (rank 2) is a workaround for grids that cannot virtualize; binnacle scrolls any count by Scroll Anchor, and the demand is from DOM grids. If a host wants pages, a page-sized Window and a position label are enough. **Native browser text selection** (rank 24) is something the canvas painter gives up by design ([03](https://github.com/flotilla-org/binnacle/blob/research/web-data-grids/docs/research/03-web-data-grids.md)); clipboard copy covers most of the need.

## Out of scope but in demand

These are `out-of-scope` in at least one adapter because a spec said so. Most of the statements were about the v1 and web-adapter milestones, not about binnacle forever.

| Feature | Kind | Demand | Fit | D x F | Core / ratatui / web | Scope statement |
|---|---|---|---|---|---|---|
| Copy cells to clipboard | widget | 3.01 | 2.54 | 7.6 | out-of-scope / out-of-scope / out-of-scope | #1/#13 out of scope: clipboard, export, context and column menus; #53 out of scope: clipboard |
| Totals / summary footer row | source | 2.91 | 2.07 | 6.0 | out-of-scope / out-of-scope / out-of-scope | #13 out of scope: pivots and aggregation; #13 later: pinned or frozen rows |
| Aggregation (sum, avg, count) on groups | source | 2.89 | 2.04 | 5.9 | out-of-scope / out-of-scope / out-of-scope | #13 out of scope: pivots and aggregation |
| Column header menu | widget | 2.68 | 2.10 | 5.6 | out-of-scope / out-of-scope / out-of-scope | #1/#13 out of scope: clipboard, export, context and column menus |
| Row grouping by column | source | 2.81 | 1.86 | 5.2 | missing / missing / out-of-scope | #13 later: tree view and grouping; #53 out of scope: nested grids, tree views and grouping |
| Preserve expand/collapse state | source | 2.67 | 1.91 | 5.1 | missing / missing / out-of-scope | #13 later: tree view and grouping; #53 out of scope: nested grids, tree views and grouping |
| Find in grid with match highlighting | widget | 1.81 | 2.73 | 4.9 | missing / missing / out-of-scope | #53 out of scope: whole-table find |
| Tree data (hierarchical rows) | source | 2.78 | 1.74 | 4.8 | missing / missing / out-of-scope | #13 later: tree view and grouping; #53 out of scope: nested grids, tree views and grouping |
| Set column type | source | 1.86 | 2.20 | 4.1 | partial / partial / out-of-scope | #53 out: per-column value typing beyond fromRows<R> |

Clipboard copy stands out: demand 3.01, 17 of 20 products, and the most reacted-to issue in the Perspective repository. The Grid holds no values, so copy would be an Effect carrying the selection range for the host to fill from its Table Source, the same pattern as editing. Aggregation, a totals row and grouping belong to the Table Source by CONTEXT.md; binnacle's part would be a pinned row for totals and depth and expansion state for groups.

## Already strong

High-demand features (demand 2.5 or more) that binnacle has in every adapter where they apply:

| Feature | Demand | Fit | Core / ratatui / web | Where |
|---|---|---|---|---|
| Row virtualization for large data | 3.40 | 3.63 | has / has / has | Only the Window is requested (crates/binnacle-core/src/axis.rs:50); ratatui and canvas draw Snapshot Strips; 10^15-row demos (web/demo/sources/huge.ts:7, scroller cap web/src/scroller.ts:1). |
| Click-to-sort columns | 3.91 | 3.10 | has / has / has | Header click cycles asc/desc/none (crates/binnacle-core/src/sort.rs:90), MemorySource::sort (crates/binnacle-memory/src/memory_source.rs:101); ratatui marker crates/binnacle-ratatui/src/lib.rs:428; web fromRows sort web/src/rows.ts:118, marker web/src/canvas/bands.ts:14. |
| Keyboard cell navigation | 3.06 | 3.62 | has / has / has | Action::MoveFocus item/page/edge on both axes (crates/binnacle-core/src/effect.rs:85); default keymaps crates/binnacle-ratatui/src/keymap.rs:66, web/src/keymap.ts:104. No Tab binding outside editing. |
| Horizontal scrolling for wide tables | 3.05 | 3.61 | has / has / has | Action::Scroll dx (crates/binnacle-core/src/effect.rs:100); ratatui wheel left/right (crates/binnacle-ratatui/src/lib.rs:39); web native overflow scroller (web/src/widget.ts:81). |
| Sticky header row | 3.42 | 3.13 | has / has / has | Column Header Band leads the row axis (crates/binnacle-core/src/snapshot.rs:173); ratatui draws headers on line 0 (crates/binnacle-ratatui/src/lib.rs:132); web web/src/canvas/bands.ts:53. |
| Autosize column to content | 3.22 | 3.26 | has / has / has | Double-click border emits AutoSizeRequested (crates/binnacle-core/src/gesture.rs:197, crates/binnacle-core/src/effect.rs:44); ratatui fit_width helper crates/binnacle-ratatui/src/renderer.rs:366 (host answers); web answers itself web/src/autosize.ts:27. No explicit autosize API on GridWidget. |
| Strongly typed rows and columns (TypeScript) | 3.11 | 2.37 | n/a / n/a / has | GridOptions<RowId,ColId,V> with NoInfer callbacks (web/src/widget.ts:24); fromRows RowsColumn<R> types renderers per property (web/src/rows.ts:19). |
| Mouse click and wheel | 3.19 | 2.30 | has / has / has | Grid::pointer gestures incl. header-click sort (crates/binnacle-core/src/gesture.rs:160); ratatui Mouse and wheel (crates/binnacle-ratatui/src/mouse.rs:30); web pointer events (web/src/input.ts:210). |
| Multi-column sort | 2.93 | 2.42 | has / has / has | Shift-click adds or cycles a column in place (crates/binnacle-core/src/sort.rs:96); places shown in ratatui (crates/binnacle-ratatui/src/lib.rs:129) and web (web/src/canvas/bands.ts:31). |
| Column resizing by drag | 3.04 | 2.28 | has / has / has | Drag a header border: core crates/binnacle-core/src/gesture.rs:201 Resizing, SizeChanged crates/binnacle-core/src/effect.rs:40; ratatui Mouse crates/binnacle-ratatui/src/mouse.rs:30 + guide crates/binnacle-ratatui/src/lib.rs:287; web web/src/input.ts:210 + guide web/src/canvas/selection.ts:71. No keyboard resize. |
| Theming and styling | 2.99 | 2.28 | n/a / has / has | ratatui Theme with classic, catppuccin_mocha, mono presets and ColorSupport (crates/binnacle-ratatui/src/theme.rs:12, 131, 172); web Theme, presets, systemTheme dark mode, themeFromCss (web/src/theme.ts:16, web/src/index.ts:18-30). |
| Custom cell renderers | 2.91 | 2.31 | n/a / has / has | ratatui CellRenderer trait with Text/Sparkline/Bar (crates/binnacle-ratatui/src/renderer.rs:17); web canvas CellRenderer and presets (web/src/renderer.ts:9); no DOM or framework components. |
| Column virtualization | 2.56 | 2.53 | has / has / has | Axes are symmetric: the column Window is computed the same way (crates/binnacle-core/src/axis.rs:50) and both adapters draw only the column Strip. |
| Live / streaming updates | 2.88 | 1.98 | has / has / has | ContentChanged/Reordered notices keep focus by Key (crates/binnacle-core/src/notice.rs:19), MemorySource::edited (crates/binnacle-memory/src/memory_source.rs:162); ratatui flash (crates/binnacle-ratatui/src/renderer.rs:155); web sinks + widget.flash (web/src/widget.ts:273). |
| Cell range selection | 2.72 | 1.96 | has / has / has | One range by drag, Shift-click or Shift+arrow (crates/binnacle-core/src/gesture.rs:224, crates/binnacle-core/src/effect.rs:85 extend); drawn by ratatui crates/binnacle-ratatui/src/lib.rs:197 and web/src/canvas/selection.ts:9. |
| Screen-reader accessibility (ARIA grid) | 2.84 | 1.82 | n/a / n/a / has | Hidden role=grid table with row/col indexes, aria-selected, aria-sort, activedescendant and a live status (web/src/aria.ts:102). Terminal has no ARIA layer. |
| Row number / row header column | 2.51 | 1.96 | has / has / has | Row-number Header Band (crates/binnacle-core/src/snapshot.rs:312 headers); ratatui Content::row_header crates/binnacle-ratatui/src/lib.rs:58; web web/src/canvas/bands.ts:79 with source label() web/src/source.ts:53. |
| Column reordering by drag | 2.96 | 1.66 | has / has / has | Header drag emits MoveRequested (crates/binnacle-core/src/gesture.rs:400, 501); In-Memory move_columns crates/binnacle-memory/src/table.rs:348; ratatui drop indicator crates/binnacle-ratatui/src/lib.rs:270; web web/src/feed.ts:386, web/src/rows.ts:132. No keyboard move; no web ghost (#78). |

These are mostly the things binnacle was built around: the Window and Scroll Anchor for size, Actions and keymaps for keyboard use, the Sort Spec, Size Overrides and Header Bands, Change Notices for live data, and the web adapter's ARIA layer and IME handling. Inline editing scores high on demand (3.03) but is partial in ratatui, where the adapter draws the editor and the host owns keys and commit.

## App-level wishes

These belong to the application around a table (the flotilla viewer, a CSV viewer app, a demo), not to the Grid. Listed with `demand x fit` of 3 or more.

| Feature | Demand | Fit | D x F | Core / ratatui / web | Products | Evidence |
|---|---|---|---|---|---|---|
| Save and restore grid state | 2.97 | 2.58 | 7.7 | partial / partial / missing | 17/20 | [discussion](https://github.com/TanStack/table/discussions/5002) |
| CSV export | 2.85 | 2.67 | 7.6 | out-of-scope / out-of-scope / out-of-scope | 13/20 | [issue](https://github.com/mui/mui-x/issues/197) |
| CSV dialect and delimiter detection | 2.14 | 3.05 | 6.5 | n/a / missing / missing | 6/10 | [issue](https://github.com/antonycourtney/tad/issues/64) |
| Saved views and startup presets | 2.47 | 2.62 | 6.5 | n/a / missing / missing | 7/10 | [issue](https://github.com/derailed/k9s/issues/789) |
| Multiple tables / tabs | 2.62 | 2.32 | 6.1 | n/a / missing / partial | 7/10 | [issue](https://github.com/derailed/k9s/issues/1430) |
| Help overlay and command palette | 1.94 | 2.91 | 5.6 | n/a / missing / missing | 5/10 | [issue](https://github.com/shshemi/tabiew/issues/24) |
| Open many formats | 2.17 | 2.48 | 5.4 | n/a / missing / missing | 6/10 | [issue](https://github.com/saulpw/visidata/issues/112) |
| Headerless files and skipped preamble | 1.72 | 3.03 | 5.2 | n/a / missing / missing | 5/10 | [issue](https://github.com/YS-L/csvlens/issues/60) |
| Export to other formats | 1.87 | 2.58 | 4.8 | out-of-scope / out-of-scope / out-of-scope | 5/10 | [issue](https://github.com/simonw/datasette/issues/1096) |
| Customisable keybindings | 1.34 | 2.67 | 3.6 | n/a / partial / has | 4/10 | [issue](https://github.com/derailed/k9s/issues/1391) |
| Read from stdin | 1.14 | 2.97 | 3.4 | n/a / missing / n/a | 2/10 | [issue](https://github.com/YS-L/csvlens/issues/81) |
| Back / forward between views | 1.17 | 2.77 | 3.2 | n/a / missing / missing | 3/10 | [issue](https://github.com/derailed/k9s/issues/1786) |

For a terminal CSV viewer the loading features (delimiter detection, header and preamble options, stdin) fit well (about 3.0) even though few products have them; they are the first thing a CSV viewer app would need, and none of them touches the Grid. Saving and restoring state is the strongest app wish; binnacle's part is already there in the core (the `SizeChanged` Effect and `set_size`, and the applied Sort Spec), but the web widget exposes none of it.

## Recommendations

1. **Show and hide columns.** Demand 3.24 and all 20 surveyed products have it, which makes it the most universal gap. The core already handles column-axis Change Notices, so the work is a column filter in the In-Memory Table Source (and the TypeScript source), plus an Action or API to hide and restore; it is also a step toward axis parity (#80).
2. **Quick filter and find with match highlighting.** Quick filter, regex search and find score 9.4, 8.8 and 4.9, and the terminal viewers are built around `/`. Since the Grid holds no values, matching is a Table Source operation that returns Keys; the Grid needs only a match flag on cells and a next-match Action. #53 put whole-table find out of scope for the web milestone, so this needs that decision revisited.
3. **Frozen columns.** Column pinning has the most cited evidence of any feature (10 citations, 427 thumbs-up on one MUI X issue) and 15 of 20 products. #13 already planned it as an extra pair of Strips. Frozen rows would follow from the same change.
4. **The web widget's focus and scroll API, with #81.** The core has `Grid::set_focus` and selection units, and ratatui hosts use them directly, but `GridWidget` exposes no scroll-to, no set-focus and no row-cursor option. These are cheap, and an embedding app needs them as soon as it has selection events.
5. **A ratatui scrollbar.** Fit 3.62, the highest of any gap. The Snapshot already exposes the thumb and `Action::ScrollTo` exists, and ratatui ships a `Scrollbar` widget, so this is a small adapter change.
6. **Clipboard copy of the selection.** Out of scope for v1 and the web spec, but demand 3.01 and 17 of 20 products. Shape it as an Effect carrying the range, filled by the host from the Table Source, with TSV as the default format.
7. **Long values.** Wrapping (17 of 20 products), a truncation marker (ratatui clips silently today) and expanding a cut-off cell together serve result sets with long text. The core already accepts per-item content sizes through `set_window_sized`, so the remaining work is in the adapters' text renderers.
