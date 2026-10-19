# PROTOTYPE: a column-major demo and the axis parity gaps (#138)

Throwaway branch `prototype/column-major`, cut from `main` at `ccbad8d`. It
answers one question from [#138](https://github.com/flotilla-org/binnacle/issues/138):
which differences between rows and columns does binnacle have today, after the
View Spec landed? It decides nothing; whether each difference is intended is
the next ticket. Delete the branch once the list has been used.

## What was built

The records demo turned on its side: each record is a column and each field
(id, name, weight, crew, port) is a row. Neither demo fakes anything. Both
wrap the records' real Table Source in a transposer that passes every request
on about the other axis and swaps every answer back, so whatever works along
one axis only shows up as a gap.

- **ratatui:** `Table::Transposed`, `Flip` and `transposed()` in
  `crates/binnacle-ratatui/src/bin/binnacle-demo.rs`, over the same
  `MemorySource<Record>` as the records. Row headers are set to
  `HeaderClick::Sort` and column headers to `Select` (the records' settings,
  swapped), and support is declared from the flipped source on both axes.
  Ctrl+R runs the crew filter over the records, which are the columns here.
- **web:** `web/demo/sources/transposed.ts`: `transpose(fromRows(...))` with
  the filter bar, column menu, find bar, the toolbar filter box (over the
  records) and editing, as in the records source.

## How to run them

```sh
# ratatui
cargo run -p binnacle-ratatui --bin binnacle-demo -- --transposed

# web (from web/): builds the wasm, then serves the demo
pnpm wasm && npx vite --port 6380 --strictPort --host 127.0.0.1
# open http://127.0.0.1:6380/?source=transposed
```

The ratatui demo was driven in tmux (SGR mouse sequences sent with
`send-keys -l`) and read with `capture-pane`; the web demo with a headless
Playwright script taking screenshots. Neither script is kept.

## The differences

Each item says whether it was in [#80](https://github.com/flotilla-org/binnacle/issues/80)
and is still true, is new, or is fixed since. "Guess" marks my reading of
whether it is intended; the user decides. Line numbers are on `main` at
`ccbad8d` (the demo binary's are too, before this branch's additions).

### Sort

1. **A header click sorts only along the columns.** Still true from #80. The
   core drops a Sort click on a row header: `crates/binnacle-core/src/view.rs:716`
   (`if axis != Axis::Cols { return; }`), documented as deliberate at
   `grid.rs:195-197`, `selection.rs:21-23` and `binnacle-ffi/include/binnacle.h:558-560`.
   Demo: ratatui `--transposed` sets row headers to Sort; clicking `crew` does
   nothing, no marker, no request. Guess: a gap under parity (the pivot case).
2. **The web cannot choose what a header click does.** New. `wasm` has no
   `set_header_click` (`crates/binnacle-wasm/src/grid.rs`, compare
   `binnacle_grid_set_header_click` in the C API) and `GridOptions` has no
   such option (`web/src/widget.ts:27-95`), so row headers always Select and
   column headers always Sort. Demo: on the web, clicking the `crew` row header
   selects the row. Guess: gap (the C API has it).
3. **Sort markers, pending shimmer and the multi-sort index are drawn only on
   column headers.** New. Web: `web/src/canvas/bands.ts:94-95,114,136-160` read
   `rows.sort` and paint only `s.cols.headers`; `paintRowNumbers`
   (`bands.ts:163-183`) draws a plain label. `canvas/pending.ts:32` animates
   only column headers. ratatui: `crates/binnacle-ratatui/src/lib.rs:103`
   (`animating`), `lib.rs:163-165`. The core's `HeaderPlacement` carries
   `sort` and `filter` for both axes (`snapshot.rs:395-404`), but the
   convenience `Snapshot::header_sort` and `header_filter` take only columns
   (`snapshot.rs:368-375`). Guess: gap, though unreachable until item 1 closes.
4. **The In-Memory Table Source sorts only rows.** Still true (#80's "fromRows
   gives rows sort"). `InMemoryTableSource` holds one `SortSpec`
   (`crates/binnacle-memory/src/table.rs:314`) and `apply_sort` works on the
   rows (`table.rs:412-413`); `MemorySource::view` applies only `spec.rows`
   (`memory_source.rs:268-298`, "The columns part is not applied") and
   `support` gives the columns nothing (`memory_source.rs:595-602`). wasm
   `WebMemory::sort` is rows only (`binnacle-wasm/src/memory.rs:96-106`) and
   `fromRows.view` drops the columns part (`web/src/rows.ts:373-408`). Demo:
   on the web, clicking a record header asks for the sort (the feed defaults
   unknown support to sortable), and it is rejected at once with no marker.
   Guess: gap in the source, needed once item 1 closes.
5. **Sorting along either axis is symmetric everywhere else.** The View Spec,
   View Edits, `ViewState::header` and the pending and applied states are per
   axis (`view.rs:437-445`); the web feed forwards both parts of a View Spec
   (`web/src/feed.ts:638-646`); the testkit renders a columns sort as `cols`
   (`binnacle-testkit/src/render.rs:232-246`). Fixed since #80 (the View Spec).

### Move

6. **Only column headers start a move.** Still true from #80. A dragged header
   becomes a move only for `Press::Header { axis: Axis::Cols, .. }`
   (`crates/binnacle-core/src/gesture.rs:276-281`); a dragged row header does
   nothing, though `MoveRequested` and `start_move` carry an axis. Demo:
   dragging the `id` row header in either demo draws no drop line and moves
   nothing. Guess: gap.
7. **The In-Memory Table Source moves only columns.** Still true from #80.
   `move_columns` (`table.rs:398-401`, "columns, the only axis moved, are never
   filtered" at `table.rs:240-242`), `Over::move_items` refuses rows
   (`memory_source.rs:876-877`), `WebMemory::move_columns`
   (`binnacle-wasm/src/memory.rs:150`), and `fromRows` has `move` on `cols`
   only (`web/src/rows.ts:367`). The internal `AxisOrder::move_items` is
   already per axis. Demo: dragging record `2` in either demo shows the drop
   line (web: the moving header is highlighted) and then nothing moves,
   because the records are the source's rows. Guess: gap.
8. **Moving headers are highlighted only on columns.** Still true from #80,
   moot until item 6 closes: `web/src/canvas/selection.ts:28`. ratatui draws
   no moving highlight on either axis, only the drop line (`lib.rs:340-370`).

### Picks and selection

9. **Picks are rows only.** Still true from #80. Ctrl/Cmd-click toggles a pick
   only on a row header or cell (`gesture.rs:530`); picks are "row Keys"
   (`selection.rs:84`), followed through Refiltered, Removed and Locate only on
   rows (`refilter.rs:63-145`, `notice.rs:231-235,493,522`), flagged on row
   headers only (`snapshot.rs:394`), "picked row Keys" in the C API
   (`binnacle.h:396`). Painters: web `canvas/body.ts:35` and
   `canvas/selection.ts:38`; ratatui `lib.rs:215,247`. Demo: Cmd-click on a
   record header selects the record and picks nothing; Ctrl-click on a cell
   picks its field row (web shows "2 picked" with the bar on `name` and
   `port`). Guess: gap.
10. **Selected headers are now highlighted on both axes on the web.** Fixed
    since #80: `canvas/selection.ts:27-37`. ratatui highlights selected headers
    on neither axis (`lib.rs:182,229` use only `hovered`). Guess: ratatui is a
    separate, symmetric gap.
11. **`SelectionUnit` has `Rows` but no `Cols`.** New. `selection.rs:7-14`,
    `grid.rs:625-635`; web `selectionUnit?: "cells" | "rows"`
    (`widget.ts:94`). Guess: gap for a column-major list view.
12. **There is a SelectColumn action and no SelectRow.** New. `effect.rs:107-108`,
    `grid.rs:699-711`; bound to Ctrl+Space in `web/src/keymap.ts:136` and
    `crates/binnacle-ratatui/src/keymap.rs:105`; C `binnacle_grid_select_column`
    only. Guess: gap (spreadsheets bind Shift+Space).

### Auto-size and Size Overrides

13. **Auto-size fits columns and resets rows.** Still true from #80. Web
    `web/src/autosize.ts:28-31` ("Rows hold one line of text") and band
    auto-size `autosize.ts:39-41` (the column header band goes back to its
    starting height, the row band is measured); ratatui demo `auto_size` gives
    rows 1 (`binnacle-demo.rs:738`) and the bands likewise (`:918`);
    `fit_width` fits only a column (`binnacle-ratatui/src/renderer.rs:382`).
    Demo: double-clicking a record border fits it in both demos; double-clicking
    a dragged-taller field row puts it back to one line; double-clicking the
    corner fits the row band to the field names (web). Guess: intended while
    cells hold one line; a gap for wrapped or tall cells.
14. **The web row band starts sized for numbers.** New. Demo: the field names
    show as `na…`, `we…` until the band is fitted; there is no
    `initialRowHeaderWidth` or label-based start. Guess: minor gap.
15. **Size Overrides drop on Reset and Removed only for rows.** New.
    `notice.rs:339` (`reordered_or_counted(axis, count, axis == Axis::Rows)`)
    and `notice.rs:231-235`. Column widths survive a Reset; row heights do not.
    Guess: intended for "columns are a schema", a gap under parity.
16. **The ratatui Cell Renderer is chosen per column.** New.
    `Content::renderer(&self, col: Key)` (`binnacle-ratatui/src/lib.rs:92`)
    and `fit_width` use it, so a transposed table cannot give each field row
    its own renderer (the live demo's sparkline and bar could not be turned on
    their side). The web chooses per cell (`RendererChoice(rowId, colId)`,
    `web/src/renderer.ts:64`), so `transpose` just swaps the arguments.
    Guess: gap in ratatui.

### Filter

17. **The quick filter field, the conditions and the column menu drive only the
    rows.** New. Web: `widget.setQuickFilter`, `addCondition`,
    `removeCondition`, `replaceCondition`, `clearConditions` all pass
    `AXIS_CODE.rows` (`web/src/widget.ts:507-547`); the filter bar shows only
    the rows' quick filter ("Filter rows…", `filterbar.ts:77,300-302`); the
    column menu opens from column headers only (funnel in
    `canvas/bands.ts:136-160`, `widget.openColumnMenu` at `widget.ts:581`).
    ratatui: `QuickFilter` reads and sets `Axis::Rows` (`filter.rs:411,487,497`),
    the column popup takes the focused column and `support(Axis::Rows, ..)`
    (`filter.rs:817,851,889`), the checklist edits `Axis::Rows`
    (`checklist.rs:79-87`). The core's Filter Spec, View Edits and
    `set_quick_filter_support` are per axis. Demo: ratatui `--transposed`
    shows no quick filter line and Ctrl+L and Alt+Down do nothing; on the web
    the filter bar has no field (the records take a quick filter, as columns,
    but nothing offers it), and a record column's funnel opens a menu whose
    condition shows as a red, not-applied chip. Guess: gap.
18. **The In-Memory Table Source filters only rows.** New (#80 did not cover
    filters). `MemorySource::quick_filter` and `support`
    (`memory_source.rs:595-607`), `Over::distinct` (`memory_source.rs:868-869`),
    `filter`/`clear_filter`/`edited`/`changed`/`appended` (`:424-515`),
    `WebMemory::filter`, `refilter_row`, `defer_row`, `unmatched`
    (`binnacle-wasm/src/memory.rs:108-145`), `fromRows` (`rows.ts:349-372`,
    no `quickFilter`, `support`, `distinct` or `unmatched` on `cols`). The web
    feed asks `distinct` and declares support only for the rows
    (`web/src/feed.ts:355,387-393,630`), so a source's support for viewing the
    columns by a row is never told to the Grid. Demo: the host's own filter
    (ratatui Ctrl+R, the web toolbar box) does filter the records as columns,
    so the core's Refiltered path works on columns. Guess: gap.

### Find

19. **Find works along both axes.** Fixed since #80 (find is new). `FindQuery.by`
    and the source's order are symmetric (`binnacle-core/src/find.rs:13-37`,
    `memory_source.rs:679-726,790-850`). Demo: in both demos "Bergen" finds 485 matches
    and steps by rows or by columns correctly.
20. **"Show only matching rows" sets only the rows' quick filter.** New.
    `web/src/findbar.ts:78,166`, `binnacle-ratatui/src/find.rs:198-211`.
    Guess: gap, follows item 17.
21. **The web find count is not refreshed when the columns are filtered.** New.
    `remark(axis === "rows" && ...)` (`web/src/feed.ts:303`) recounts only for
    a rows notice. Demo: with "Bergen" found (485 matches), typing `bosun` in
    the toolbar box leaves 485 in the transposed demo; the records demo drops to
    75. ratatui recounts every frame and is unaffected. Guess: a bug.
22. **A Text Query naming items to search is read as the source's columns.**
    `TextQuery.searches` names items of the other axis; only "shown" survives
    transposing. Prototype artefact (`flipFind` in `transposed.ts`), noted
    because a column-major source must read named searches as rows.

### Totals and the filtered count

23. **The count shown is the rows'.** New. Web filter bar `filterbar.ts:332-340`;
    ratatui `row_count` (`filter.rs:1671-1685`); ARIA announces rows
    (`web/src/aria.ts:308`). The core keeps Totals per axis (`view.rs:319-345`).
    Demo: with the records filtered to 485 columns, the bar says "5 rows" in
    both demos. Guess: gap.

### Keyboard and keymap presets

24. **Page and Home/End keys move only along the rows.** New. `web/src/keymap.ts:122-125`,
    `binnacle-ratatui/src/keymap.rs:75-78`; only Mod/Ctrl+arrow reaches the
    column edges. Demo: in a 5 by 5,000 table PageDown and End stay in the
    record; Cmd+ArrowRight jumps to record 4,999. Guess: gap (Excel's
    Alt+PageDown).
25. **Every filter key in the presets acts on the focused column.** New.
    `Control::ColumnFilter`, `ColumnText`, `KeepValue`, `Remove`
    (`binnacle-ratatui/src/filter.rs:24-60`, `keep_value` at `:1319`);
    web Alt+ArrowDown opens the focused column's menu (`input.ts:36`). Guess:
    gap, follows item 17.

### Unmatched Rows

26. **Drawn on both axes, counted on the rows.** New. Web asks `unmatched` on
    both axes and dims either (`feed.ts:848-849`, `widget.ts:821-822`);
    ratatui dims either (`lib.rs:304`), but the demo's `Marked::unmatched`
    answers rows only (`binnacle-demo.rs:840`), and the web filter bar's
    Re-apply counts rows only (`filterbar.ts:327-331`). Demo: on the web,
    editing record 1's name to `zzz` under the `bosun` filter dims the whole
    record column and no Re-apply button appears. Guess: the source and the
    count are gaps; the drawing already has parity.

### The C API and wasm

27. **The C API mirrors the core and has no In-Memory Table Source.** Every
    `binnacle_grid_*` takes an axis where the core does; the axis-shaped gaps
    are the core's (items 1, 9, 12). Nothing in `binnacle-ffi` wraps
    `binnacle-memory`. Guess: as designed.
28. **wasm lacks `set_header_click` and `set_drag_threshold`.** New (item 2).
    `crates/binnacle-wasm/src/grid.rs` against `binnacle.h`.

### The testkit

29. **`RecordTable` is row-major and `KeyedTable` symmetric.**
    `binnacle-testkit/src/scenario.rs:126-145` sorts and moves as the
    In-Memory Table Source does; `render` already prints a columns sort and
    filter (`render.rs:232-250`) and calls picks "row Keys" (`render.rs:20-21`).
    Guess: follows the source.

### The demo sources

30. **Every demo source on `main` is row-major.** Still true from #80. ratatui:
    records, slow, huge, live; web: records, slow, huge, live, gaps. The
    records sort and filter rows and move columns; huge and live never sort or
    move columns. Fixed only on this throwaway branch.

## Rechecking #80

| #80 says | Now |
|---|---|
| Sort is columns-only, documented as deliberate | Still true (items 1 to 4) |
| Header drag-to-move is columns-only; the In-Memory Table Source moves only columns | Still true (items 6, 7) |
| Picks are rows-only on the web painter | Still true (item 9) |
| Moving and selected headers highlighted only on columns | Half fixed: selected now both (item 10), moving still columns (item 8) |
| Web auto-size measures columns, resets rows; bands likewise | Still true (item 13) |
| Demo sources are all row-major; `fromRows` sorts rows and moves columns | Still true (items 4, 7, 30) |

## Verdict

To be filled in by the next ticket: which of these are intended.
