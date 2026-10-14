# Prior art: web data grids

Research ticket [#44](https://github.com/flotilla-org/binnacle/issues/44): survey Glide Data Grid, FINOS Perspective and regular-table, AG Grid, TanStack Table plus TanStack Virtual, and `~/dev/quantrish`, asking how each draws (canvas or DOM), handles huge row counts, mounts editors and IME, stays accessible, moves data across wasm or worker boundaries, and handles high DPI and font measurement. Then draw conclusions for the drawing prototype ([#45](https://github.com/flotilla-org/binnacle/issues/45)) and the wasm boundary ([#46](https://github.com/flotilla-org/binnacle/issues/46)).

Sources: primary only (source code at a pinned commit, in-repo docs, project issues), fetched 2026-10-14. No code was run, so behaviour claims are read from source; where I infer rather than read, I say so. Pinned commits:

- Glide Data Grid `0875d78` (2026-01-21), `packages/core`.
- regular-table v0.9.2, `52e86bf` (2026-10-09).
- Perspective v5.6.0, `0842ad7` (2026-10-13). `finos/perspective` now redirects to `perspective-dev/perspective`.
- AG Grid `main` at `a2acae4` (2026-10-14), `36.2.0-beta`, community package only (the server-side row model itself is enterprise and was not read).
- TanStack Virtual `3c61b6e` (virtual-core 3.19.4), TanStack Table `95c4768` (2026-10-12).
- quantrish (local, read-only) at `ae5ed79` (2026-04-27).

## Summary of conclusions

1. **Canvas wins on scroll cost, DOM wins on everything else for free.** Glide moved from react-virtualized to canvas because swapping "hundreds of DOM elements per frame" could not keep up [G1]. The DOM grids (AG Grid, regular-table, TanStack) get text selection, IME, editors and screen readers for free but pay in element churn, which AG Grid fights with a per-frame task budget [A3].
2. **Everyone who handles huge counts maps a capped native scroller onto the model, and none trusts the browser's height limit.** Caps differ: Glide hard-codes 33,554,400 px [G3], regular-table 10M (5M on Firefox) [R3], AG Grid probes the limit at runtime and caps Firefox at 6M because it "renders blank after ~6M px" [A5]. Past the cap, regular-table and AG Grid scale linearly (one scroll pixel spans many rows); Glide is a hybrid that keeps small moves 1:1 and maps only big jumps by fraction [G4]. TanStack Virtual does nothing and breaks around 33M px; the fix is an open, unmerged PR [V4][V5].
3. **Editors are always a real DOM element over the cell, and nobody handles IME start.** Glide portals a `<textarea>` over the canvas [G7]; AG Grid puts the editor in the cell or a popup [A11]. Both start "type to edit" only on a single printable `event.key` [G8][A12], so (my inference, not tested) an IME keystroke reported as `"Process"` does not open the editor in either. Neither has any `composition*` handling.
4. **Accessibility over canvas is a hidden DOM table of the visible window.** Glide renders a `<table role="grid">` as the canvas's fallback content with `aria-rowcount`, `aria-rowindex`, `aria-colindex`, `aria-selected`, and moves real DOM focus to the focused `<td>` [G10][G11]. AG Grid uses the same attribute set on its live DOM [A8]. regular-table and TanStack prescribe no ARIA at all [R9][V8].
5. **Data crosses into wasm or a worker as one serialized message per call, never as callbacks.** Perspective sends protobuf over `postMessage` with transferred buffers, runs the engine in a Worker, and returns Arrow bytes or a JSON string [P2][P5]; quantrish exposes one `dispatch_postcard` entry point (plus a JSON twin, which its TS client actually uses) [Q6][Q7]. The data grids themselves ask for a window and accept a late, async answer (regular-table's `Promise<DataResponse>`, AG Grid's `getRows` with `success`, Glide's `onVisibleRegionChanged`) [R5][A14][G16].
6. **High DPI is "backing store times DPR, CSS size unchanged, scale the context"** in both canvas grids [G14][Q3]. **Font measurement** is a `measureText` cache (Glide, cleared on web-font load) [G15] or measuring cloned DOM cells (AG Grid, regular-table) [A16][R11]. All of it stays in the host, as [01](./01-prior-art-headless-tables.md) already found.

For binnacle that points to: canvas first behind a painter interface over a Snapshot; a capped native scroller driven by Glide's hybrid mapping onto the Grid's existing `Action::Scroll` and `Action::ScrollTo`; a hidden ARIA grid of the visible window built from the Strips; one persistent focused DOM element that doubles as the IME target and the editor; and a main-thread Grid whose Snapshot crosses as typed-array views, with cell values never entering wasm. Details in the last section.

## Glide Data Grid (canvas)

**Why canvas.** The README says their earlier grid used react-virtualized and scrolled too slowly: "Once you need to load/unload hundreds of DOM elements per frame nothing can save you" [G1]. The DOM is a main `<canvas tabIndex=0>` plus a header canvas [G2].

**Huge counts.** Native scrolling: a `.dvn-scroller` div (`overflow: auto`) sits over an underlay holding the canvas, and invisible padder divs, each at most 5M px, supply the height up to `BROWSER_MAX_DIV_HEIGHT = 33_554_400` [G3]. Past the cap the scroll handler is a hybrid: a jump over 2000 px, or reaching an edge, maps the scroll fraction onto the full height; smaller moves are applied 1:1 to a tracked position, so wheel and trackpad scrolling keep pixel precision at any depth [G4]. There is no Firefox-specific cap. `rowHeight` is a number or `(index) => number` (default 34) [G5]; with a function, total height and first visible row are both O(rows) walks [G6].

**Editors, IME, clipboard.** The overlay editor is a React portal (into `portalElementRef` or `#portal`, throwing if neither exists), absolutely positioned at the cell rectangle, slightly enlarged, nudged back on screen [G7]. Text cells edit in a real `<textarea>` that takes focus. Typing starts an edit from the canvas `keydown` when `editOnType` is on, no Ctrl or Meta is held, and `event.key` is a single printable character; the editor opens pre-filled with that key [G8]. There is no hidden IME input and no `composition`/`isComposing` handling anywhere in `src/`, so composition only works once the textarea is open (inference: an IME first key, `key === "Process"`, does not open it) [G8]. Copy and paste are window listeners that act only when focus is inside the grid; paste tries `navigator.clipboard.read()` then falls back to `clipboardData`; copy writes `text/plain` and `text/html` [G9]. There is no DOM text selection over the canvas.

**Accessibility.** A hidden `<table>` is rendered as the canvas's fallback content (children of `<canvas>` are exposed to assistive tech but not painted) [G10]. Table: `role="grid"`, `aria-rowcount={rows+1}`, `aria-colcount`, `aria-multiselectable`. Header cells: `role="columnheader"`, `aria-colindex`, `aria-selected`. Rows: `role="row"`, `aria-rowindex={row+2}`, `aria-selected`. Cells: `role="gridcell"`, `aria-colindex`, `aria-selected`, `aria-readonly`, an `id`, `tabIndex=-1` [G10]. Only visible rows are included, rebuilt on a debounce, skipped below 50 px wide, and switchable off [G11]. It does not use `aria-activedescendant`: it moves real focus to the focused `<td>` with `preventScroll`, only while the grid already has focus [G11]. Cell text comes from each renderer's `getAccessibilityString` [G12].

**Drawing pipeline.** `renderStrategy` is `single-buffer`, `double-buffer` (default on Safari) or `direct` [G13]. On scroll it blits the previous frame shifted by the scroll distance and redraws only the newly exposed strips; `damage(cells)` redraws just those cells, clipped; changing `getCellContent` identity redraws everything [G13]. The main draw runs synchronously in `useLayoutEffect` when inputs change; per-cell redraws are de-duplicated and batched to `requestAnimationFrame`; animated cells get `requestAnimationFrame` in their draw arguments [G13].

**High DPI and fonts.** Scale is `min(maxScaleFactor, ceil(devicePixelRatio))`; backing store is CSS size times scale, CSS size stays in px, context scaled to match; an opt-in lowers the scale while scrolling [G14]. `measureTextCached` keys on string, font and baseline, holds up to 10k entries, and clears when web fonts finish loading [G15].

**Data path.** Pull: `getCellContent([col,row])` per visible cell during draw, with a loading cell for rows not yet available; `onVisibleRegionChanged(range, tx, ty, extras)` fires when the visible range changes, the hook for windowed loading [G16]. A renderer is `{kind, draw(args, cell), drawPrep?, measure?, provideEditor?}` [G12]. No Worker or OffscreenCanvas.

[G1]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/README.md#L147-L151
[G2]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/data-grid/data-grid.tsx#L1929-L1945
[G3]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/scrolling-data-grid/infinite-scroller.tsx#L73-L132
[G4]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/scrolling-data-grid/infinite-scroller.tsx#L239-L284
[G5]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/API.md#L645-L651
[G6]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/scrolling-data-grid/scrolling-data-grid.tsx#L105-L190
[G7]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/data-grid-overlay-editor/data-grid-overlay-editor.tsx#L213-L258
[G8]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/data-editor/data-editor.tsx#L3506-L3530
[G9]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/data-editor/data-editor.tsx#L3642-L3690
[G10]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/data-grid/data-grid.tsx#L1763-L1847
[G11]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/data-grid/data-grid.tsx#L1679-L1750
[G12]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/cells/cell-types.ts#L58-L115
[G13]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/data-grid/render/data-grid-render.ts#L367-L556
[G14]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/data-grid/render/data-grid-render.ts#L172-L253
[G15]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/data-grid/render/data-grid-lib.ts#L308-L352
[G16]: https://github.com/glideapps/glide-data-grid/blob/0875d78cc41535a39f8b417ff88ce1ebe129c363/packages/core/src/internal/scrolling-data-grid/scrolling-data-grid.tsx#L203-L238

(G13 also covers the strategy default at `data-grid.tsx#L846`, the blit at `data-grid-render.blit.ts#L21-L233`, and the rAF queue at `use-animation-queue.ts#L8-L39`.)

## regular-table (DOM, async windowed)

**Why DOM.** A real `<table>` holding only visible cells, "queried from a natively `async` virtual data model" [R1]. The shadow DOM is a `.rt-virtual-panel` sizer next to a sticky `.rt-scroll-table-clip` that holds the user's table; the host is `overflow: scroll; overflow-anchor: none` [R2].

**Huge counts.** `BROWSER_MAX_HEIGHT = isFirefox ? 5000000 : 10000000`, the "largest size virtual <div> in (px) that Chrome can support without glitching" [R3]. Panel height is `min(BROWSER_MAX_HEIGHT, nrows * row_height + header_height)`; the start row is pure proportion, `start_row = (nrows - visible_rows) * scrollTop / (panel_height - containerHeight)`, so past the cap one scroll pixel spans several rows [R4]. The fractional part of `start_row` becomes a part-row offset through CSS variables, and the repo ships a two-billion-row example. Wheel and touch handlers compute `scrollTop` ahead of the scroll event [R4].

**Data model.** `setDataListener((x0, y0, x1, y1) => Promise<DataResponse>)` with `{num_rows, num_columns, data, row_headers?, column_headers?, metadata?, ...}`; data is column-major (`data[x][y]`) [R5]. An example answers the window from a Web Worker over `postMessage` [R6].

**Editing and selection.** Left to user code via `addStyleListener` (runs after every render, including scroll) and `getMeta(td)` (maps a cell back to data coordinates) [R7]. The spreadsheet example sets `contenteditable` on cells; Perspective's datagrid makes only the active cell `contenteditable` [R8]. Native DOM, so IME works wherever `contenteditable` is set.

**Accessibility.** No `aria-*` or `role` in `src/` or `features/`; the only hook is `tabindex="0"` on the host [R9]. It relies on native table semantics, so a screen reader sees only the visible rows, with no `aria-rowcount` or `aria-rowindex`.

**Measurement.** Auto column width reads every rendered cell's `getBoundingClientRect()` before writing (avoiding layout thrash), stores widths per `size_key`, and applies them through one stylesheet; row height comes from a hidden one-cell test table; unmeasured columns are assumed 60 px [R11].

[R1]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/README.md#L16-L18
[R2]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/src/ts/scroll_panel.ts#L39-L80
[R3]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/src/ts/constants.ts#L15-L18
[R4]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/src/ts/scroll_panel.ts#L723-L878
[R5]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/src/ts/types.ts#L196-L275
[R6]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/examples/web_worker.html#L60-L87
[R7]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/src/ts/regular-table.ts#L130-L255
[R8]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/packages/viewer-datagrid/src/ts/event_handlers/edit_focus.ts#L68-L88
[R9]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/src/ts/regular-table.ts#L62
[R11]: https://github.com/finos/regular-table/blob/52e86bf8b74fc5948bb0047afbc8bb9207dd933c/src/ts/table.ts#L397-L545

## FINOS Perspective (wasm engine in a worker)

**Engine and protocol.** A C++ query engine and a Rust UI, both compiled to wasm [P1]. Client and engine talk protobuf: `Request`/`Response` carry `msg_id`, `entity_id` and a oneof body; a window is `ViewPort{start_row, start_col, end_row, end_col}` [P2]. Arrow comes back as bytes (`ViewToArrowResp`, optionally lz4 or zstd), JSON as a string (`ViewToColumnsStringResp`); the docs call Arrow the preferred interchange because it "loads without parsing and preserves types" [P3]. `to_columns_string` exists to save "round trip serialize/deserialize cycles" [P4].

**Worker and transfer.** `perspective.worker()` accepts a `SharedWorker`, `Worker` or `MessagePort`, defaulting to a module Worker named `perspective-server`; the worker compiles the wasm and runs one server with a session per connection [P5]. Each protobuf message is copied into its own `ArrayBuffer` and posted with `{transfer: [buf]}` in both directions [P6]. The stated reasons: "WebWorker isolation for parallel rendering", "WebAssembly is single-threaded per worker", and "Rendering is separate from querying"; the client API is identical in-process, in a Worker, or over a WebSocket [P7].

**How its datagrid fetches.** The datagrid is regular-table. Its data listener turns `(x0, y0, x1, y1)` into a `ViewPort`, calls `view.to_columns_string(window)` and `view.num_columns()` in parallel, `JSON.parse`s the result and returns it as a `DataResponse` [P8]. So the visible-window path uses JSON, not Arrow: Arrow is for bulk load and export.

[P1]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/docs/md/glossary.md#L128-L137
[P2]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/rust/perspective-client/perspective.proto#L84-L172
[P3]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/rust/perspective-client/perspective.proto#L497-L537
[P4]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/rust/perspective-client/src/rust/view.rs#L451-L475
[P5]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/rust/perspective-js/src/ts/perspective.browser.ts#L344-L367
[P6]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/rust/perspective-js/src/ts/wasm/browser.ts#L51-L104
[P7]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/docs/md/benchmarks.md#L86-L90
[P8]: https://github.com/perspective-dev/perspective/blob/0842ad79117b0d4a589df1655f2679cad827f8a7/packages/viewer-datagrid/src/ts/data_listener/index.ts#L70-L330

(P5 continues in `perspective-server.worker.ts#L23-L58`; P7's isolation quote is in `docs/md/explanation/architecture/client_only.md#L8-L15`.)

## AG Grid (DOM, row and column virtualisation)

**Virtualisation.** The row range is the viewport plus `rowBuffer` rows (default 10) converted to pixels, then mapped to rows with `getRowIndexAtPixel` [A1]. At most `max(rowBuffer, 500)` rows render unless overridden [A1]. Rows are absolutely positioned with `top` or `translateY` [A2]. Columns render if they overlap the viewport plus or minus 200 px [A2]. An `AnimationFrameService` queues row and cell rendering in priority lists and drains them each frame within a time budget, so a fast scroll paints rows progressively instead of dropping frames [A3]. `debounceVerticalScrollbar` debounces vertical scroll handling by 100 ms [A4].

**Huge counts ("stretching").** `_getMaxDivHeight()` probes once: it appends a div and doubles its height from 1M px until `clientHeight` stops matching, capped at 6M on Firefox ("reports the height back but still renders blank after ~6M px") and 1e9 elsewhere [A5]. When the model height exceeds that, `RowContainerHeightService` sets `uiContainerHeight = maxDivHeight` and `divStretchOffset = floor(scrollTop / maxScrollY * (modelHeight - uiContainerHeight))`; the row range is computed at `scrollTop + divStretchOffset` and each row's DOM top is `modelTop - divStretchOffset` [A6]. The reverse map for scroll-into-view is `maxScrollY * rowTop / (modelHeight - bodyHeight)`; row animation is disabled while stretching [A7].

**Accessibility.** The body viewport is `role="grid"` (or `treegrid` with grouping or tree data) with `aria-rowcount` (header rows + pinned + model rows, or `-1` when the last row is unknown), `aria-colcount` and `aria-multiselectable` [A8]. Rows: `role="row"`, `aria-rowindex`, `aria-expanded`, `aria-selected`. Cells: `role="gridcell"` (overridable), `aria-colindex`. Headers: `aria-sort`, `aria-colindex` [A8]. Focus is real DOM focus on the cell element: every cell has `tabindex="-1"` and the focused cell calls `.focus({preventScroll: true})`; Tab is handled by the grid [A9]. The docs recommend `ensureDomOrder` and turning virtualisation off (or paginating) for screen-reader users [A10].

**Editing.** Editors live inside the cell or in a popup anchored to it [A11]. Type to edit: a cell `keydown` with `key.length === 1` and no Alt, Ctrl or Meta calls `startEditing` and `preventDefault()`, and the text editor seeds itself with that key [A12]. No composition handling in grid core; same IME inference as Glide [A12].

**Async data.** The infinite model's `IDatasource.getRows({startRow, endRow, successCallback(rows, lastRow?), failCallback})` [A14]. Blocks of `cacheBlockSize` rows are created as empty stub nodes, a version stamp drops late responses, requests are capped by `maxConcurrentDatasourceRequests`, eviction is least-recently-used but keeps on-screen and focused blocks, and the row count grows by `cacheOverflowSize` while the last row is unknown [A15]. Stub rows render as loading cells.

**Measurement.** Auto-size clones the column's rendered cells and header into a fixed-position `<form>` inside the viewport so they inherit the grid's CSS, and reads `offsetWidth`; rows outside the rendered range are not measured [A16].

[A1]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/rendering/rowRenderer.ts#L1363-L1450
[A2]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/columns/columnViewportService.ts#L135-L162
[A3]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/misc/animationFrameService.ts#L22-L217
[A4]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/gridBodyComp/gridBodyScrollFeature.ts#L167-L173
[A5]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-stack/src/utils/browser.ts#L96-L125
[A6]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/rendering/rowContainerHeightService.ts#L45-L117
[A7]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/rendering/rowContainerHeightService.ts#L131-L140
[A8]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/gridBodyComp/gridBodyCtrl.ts#L395-L561
[A9]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/rendering/cell/cellCtrl.ts#L869-L910
[A10]: https://www.ag-grid.com/javascript-data-grid/accessibility/
[A11]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/rendering/cell/cellComp.ts#L495-L576
[A12]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/rendering/cell/cellKeyboardListenerFeature.ts#L267-L287
[A14]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/interfaces/iDatasource.ts#L12-L30
[A15]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/infiniteRowModel/infiniteCache.ts#L122-L205
[A16]: https://github.com/ag-grid/ag-grid/blob/a2acae416bda7212bc093c3e900c916781aadb39/packages/ag-grid-community/src/rendering/autoWidthCalculator.ts#L23-L139

(Row ARIA is at `rowCtrl.ts#L308` and `#L1487`, header `aria-sort` at `headerCellComp.ts#L46`, printable-key test at `ag-stack/src/utils/keyboard.ts#L2-L13`, block versioning at `infiniteBlock.ts#L43-L146`.)

## TanStack Table plus TanStack Virtual (headless DOM)

**Model.** `useVirtualizer({count, getScrollElement, estimateSize, overscan, horizontal, measureElement, ...})` (overscan default 1) returns `getVirtualItems()` as `{key, index, start, end, size, lane}` and `getTotalSize()` [V1]. The host renders a sizer of total size and positions each item with `translateY(start)` [V2]. A React `directDomUpdates` option writes transforms straight to the DOM to skip re-renders on scroll [V3].

**Huge counts.** Nothing: total size is the raw sum of item sizes. Issue #616 "Reach browser max height pixels" is open (quoting Firefox 33,554,400 and Chrome 33,554,428 px) [V4]; PR #1172 adding a `maxScrollSize` option with a scale factor is open and unmerged as of 2026-10-14 [V5]. Issue #998 reports scrolling stopping at row 745,531 of 3M [V4].

**Dynamic sizes.** One `ResizeObserver` measures every mounted item's border box (index from `data-index`); when an item above the viewport changes size the scroll offset is corrected so content does not jump, except while scrolling backward or smooth scrolling [V6]. That correction is what a Scroll Anchor makes unnecessary.

**Accessibility, editors, workers.** No ARIA in any virtualised example; Table's stance is that "your application owns markup, styles, interactions, and accessibility" [V8]. Items are host DOM, so selection, IME and inputs work, but a focused item unmounts once it leaves the overscan range unless `rangeExtractor` keeps it [V9]. Table has an experimental worker plugin that runs filtering, grouping and sorting in a Worker "shadow table" for 100k+ client-side rows [V10]. No wasm.

[V1]: https://github.com/TanStack/virtual/blob/3c61b6e30723e6af24d375ea1c5221d12a2e894d/packages/virtual-core/src/index.ts#L68-L117
[V2]: https://github.com/TanStack/table/blob/95c4768496c07b45b1e65d8e33c3291ac66eae9b/examples/react/virtualized-rows/src/main.tsx#L233-L277
[V3]: https://github.com/TanStack/virtual/blob/3c61b6e30723e6af24d375ea1c5221d12a2e894d/packages/react-virtual/src/index.tsx#L36-L62
[V4]: https://github.com/TanStack/virtual/issues/616
[V5]: https://github.com/TanStack/virtual/pull/1172
[V6]: https://github.com/TanStack/virtual/blob/3c61b6e30723e6af24d375ea1c5221d12a2e894d/packages/virtual-core/src/index.ts#L1940-L2040
[V8]: https://github.com/TanStack/table/blob/95c4768496c07b45b1e65d8e33c3291ac66eae9b/docs/agent-skills.md#L113
[V9]: https://github.com/TanStack/virtual/blob/3c61b6e30723e6af24d375ea1c5221d12a2e894d/docs/api/virtualizer.md#L143-L149
[V10]: https://github.com/TanStack/table/blob/95c4768496c07b45b1e65d8e33c3291ac66eae9b/docs/guide/worker-row-models.md#L5-L80

(The 3M-row report is https://github.com/TanStack/virtual/issues/998.)

## quantrish (React canvas grid plus a wasm message bridge)

Local, read-only, at `~/dev/quantrish` commit `ae5ed79`. Paths below are relative to that checkout.

**Drawing.** `grid/src/Grid.tsx` renders a focusable `overflow: auto` scroll div containing a sizer of the full content size, with a `position: sticky` canvas inside it (`pointer-events: none`, so clicks reach the scroller) [Q1]. `geometry.ts` builds `Float32Array` prefix offsets for every row and column (`computeBodyOffsets(count, size)`, O(count)) and binary-searches them for hit tests and the visible range [Q2]. `paint.ts` clears and redraws the visible range every time a prop or state changes, from a `useLayoutEffect` [Q3]. High DPI: backing store is `floor(css * devicePixelRatio)`, then `ctx.setTransform(dpr, 0, 0, dpr, 0, 0)` [Q3]. Fonts are fixed CSS font strings; text is clipped with `fillText`'s `maxWidth` rather than measured [Q4].

**Huge counts.** Not handled: the sizer is the full pixel height, so it would hit the browser limit around a million rows. Not a goal for quantrish's pivot views.

**Editors and keys.** On Enter, F2 or double-click an `<input>` is absolutely positioned at the cell rectangle inside the sizer, `autoFocus`ed, committing on Enter or blur and cancelling on Escape [Q5]. The keymap is a `switch` on `e.key` in the scroller's `onKeyDown`. No type-to-edit, no IME handling, no clipboard. Hover tooltips use the scroller's `title` attribute.

**Accessibility.** None: no `role` or `aria-*` in `grid/src` or `ui/src`.

**Wasm bridge.** `engine-wasm/src/lib.rs` exposes one `#[wasm_bindgen]` handle with `dispatch_postcard(&[u8]) -> Vec<u8>`, decoding an `EngineRequest` enum and encoding an `EngineResponse`, errors included as a response variant [Q6]. A `dispatch_json(&str) -> String` twin exists "for debugging and for TS-side tests", and the TS client (`ui/src/engine-client.ts`) actually calls `dispatch_json` with `JSON.stringify` [Q7]. The whole viewport render (every visible and invisible cell of the pivot) comes back in one `ViewportRender` response; the engine runs on the main thread. Tests: vitest for geometry and paint, wasm-bindgen-test plus native tests for the bridge, Playwright under `ui/e2e`.

[Q1]: ~/dev/quantrish/grid/src/styles.css (lines 1-29) and grid/src/Grid.tsx (lines 370-414)
[Q2]: ~/dev/quantrish/grid/src/geometry.ts (lines 17-21, 65)
[Q3]: ~/dev/quantrish/grid/src/Grid.tsx (lines 124-150)
[Q4]: ~/dev/quantrish/grid/src/paint.ts (lines 49-101)
[Q5]: ~/dev/quantrish/grid/src/Grid.tsx (lines 381-410)
[Q6]: ~/dev/quantrish/engine-wasm/src/lib.rs (lines 37-57)
[Q7]: ~/dev/quantrish/ui/src/engine-client.ts (lines 110-115) and engine-wasm/src/lib.rs (lines 59-80)

## Comparison

| Project | Draws with | Past the max element height | Editor and IME | Accessibility | Data and calls across a boundary | High DPI and fonts |
|---|---|---|---|---|---|---|
| Glide | canvas (plus header canvas) | cap 33.5M; hybrid: small deltas 1:1, jumps by fraction | portaled `<textarea>` over the cell; type-to-edit on one printable key; no IME start | hidden `<table role="grid">` of visible rows as canvas fallback; real focus on `<td>` | pull `getCellContent` per visible cell; `onVisibleRegionChanged`; no worker | `ceil(dpr)` capped; blit on scroll; `measureText` cache cleared on font load |
| regular-table | DOM `<table>`, visible cells only | cap 10M (5M Firefox); linear proportional map | user code, `contenteditable` | native table semantics only; no ARIA counts | `Promise<DataResponse>` per window, column-major; worker example | measures rendered cells via `getBoundingClientRect` |
| Perspective | regular-table (datagrid plugin) | as regular-table | active cell `contenteditable` | as regular-table | protobuf over `postMessage` with transfer; Worker or SharedWorker; Arrow bytes or JSON string; window as JSON | as regular-table |
| AG Grid | DOM, row and column virtualisation, rAF task budget | probe limit (Firefox 6M); linear `divStretchOffset` | in-cell or popup DOM editor; one printable key; no IME start | `grid`/`treegrid`, `aria-rowcount` (or -1), `aria-rowindex`, `aria-colindex`, `aria-sort`; real focus on cells | `getRows({startRow,endRow})` with late `success`; LRU blocks; version stamps drop late answers | clones cells off screen and reads `offsetWidth` |
| TanStack | host DOM, headless | none (open issue, unmerged PR) | native DOM; focused item unmounts | none prescribed | none (Table: experimental worker row models) | `ResizeObserver` per item; scroll correction |
| quantrish | canvas, sticky inside a full-size sizer | none | `<input>` over the cell on Enter/F2 | none | one `dispatch` call with an enum request (postcard, JSON used in practice); main thread | `dpr` backing store and `setTransform`; fixed fonts |

## Recommendations for binnacle

Binnacle's frame loop already settles several of these questions. The Grid holds nothing proportional to rows, owns scrolling as a Scroll Anchor (Position plus offset) with an `f64` Thumb, accepts `Action::Scroll { dx, dy }` and `Action::ScrollTo { axis, fraction }`, and hands out a Snapshot whose Strips list only the visible items with Keys, offsets and sizes. So the web adapter never needs a pixel coordinate for the whole table, which is exactly what every grid above struggles with.

### For the drawing prototype (#45)

1. **Build canvas first, and make the painter take a Snapshot plus a cell-content lookup.** Glide's stated reason (DOM churn while scrolling) is strongest exactly where #45 is aimed: the 10^15-row huge source, fast scrolls, a sparkline column and the shimmer. A painter interface of `paint(snapshot, content)`, where `content` answers by Key like the ratatui adapter's `Content` trait, keeps a DOM painter possible: the DOM painter would keep one element per visible `(row Key, col Key)` and reuse elements by Key across frames, the way AG Grid keys rows by node. Use `Damage` to skip frames; leave Glide's blit-on-scroll and per-cell damage as later optimisations, since #45 judges by feel and frame time.
2. **Scroll with a capped native scroller and Glide's hybrid mapping, feeding the Grid's existing Actions.** Put a sizer of `min(cap, estimated extent)` inside an `overflow: auto` element (cap around 10M px, or AG Grid's probe with a 6M Firefox ceiling), with the canvas `position: sticky` inside it as quantrish does. On each `scroll` event compare `scrollTop` with the last value the adapter set: a small delta goes in as `Action::Scroll { dy: delta }` (pixel-exact at any depth), a jump over a threshold or a hit on either end goes in as `Action::ScrollTo { fraction: scrollTop / (sizer - viewport) }`. After each frame, write `Thumb::fraction * (sizer - viewport)` back to `scrollTop` and remember it so the echo is ignored. This keeps native scrollbars, trackpad momentum and overlay scrollbars on macOS, which a custom-drawn scrollbar would lose. Wheel events can still be intercepted (as regular-table does) if the echo proves jittery; #45 should test both.
3. **Size the canvas as Glide and quantrish do.** Backing store `round(css * devicePixelRatio)`, CSS size in px, `setTransform(dpr, ...)`; watch `matchMedia('(resolution: Xdppx)')` for DPR changes when a window moves between screens. Keep the Grid's unit as CSS px.
4. **Measure text in TS with a cache.** A `measureText` cache keyed by font and string, cleared on `document.fonts` `loadingdone` (Glide), is enough for clipping and ellipsis. Auto-size stays host-side: answer `AutoSizeRequested` by measuring the visible cells' text and calling `set_size`, which matches AG Grid's and egui's visible-rows scope from [01](./01-prior-art-headless-tables.md).
5. **Make one persistent, focusable DOM element the keyboard and IME target, and position it at `focus_rect`.** This is where binnacle can beat the surveyed grids, none of which can start an edit from an IME key. A transparent `<textarea>` placed at the focused cell receives keys (mapped through the web keymap to Actions) and, because it sits at the cell, the IME candidate window appears in the right place. On `compositionstart` (or `keydown` with `isComposing` or `key === "Process"`) the adapter sends `Action::BeginEdit`, and when `EditRequested` arrives it makes the same element visible at `editor_rect` with the composition still running, so no keystroke is lost. Plain printable keys can follow Glide's and AG Grid's single-printable-key rule. This is a design to try in #45, not something any project ships.
6. **Build the accessibility layer from the Strips, Glide-style.** Render a hidden `<table role="grid">` as fallback content of the canvas (or a visually hidden sibling) with `aria-rowcount = count + 1`, `aria-colcount`, `aria-multiselectable`; one `role="row"` per Rows Strip item with `aria-rowindex = Position + 2`; `role="gridcell"` with `aria-colindex`, `aria-selected` from `cell_state`, and `role="columnheader"` with `aria-sort` from `header_sort`. Rebuild it only when `Damage` says positions or selection changed, debounced. Unfilled Window items (`key: None`) can carry `aria-busy="true"`. Positions on the huge source exceed what assistive tech will meaningfully announce but stay exact in JS numbers (10^15 is below 2^53). Use real focus on the focused cell (Glide, AG Grid) or `aria-activedescendant` from the IME element in point 5; the prototype should pick one, since they compete for focus.
7. **Copy and paste** follow Glide: listen for `copy` and `paste` while focus is inside the widget, write `text/plain` and `text/html`. Off-screen cells come from the Table Source, which may answer late.

### For the wasm boundary (#46)

1. **Cell values never cross into wasm.** The Grid holds Keys, not values, so the Arrow and columnar question in the map's "Not yet specified" list is about the TS Table Source to painter path, not the wasm boundary. Perspective's own datagrid uses a JSON window, not Arrow, for display [P8]; Arrow pays off for bulk load, which is the out-of-scope server add-on.
2. **Run the Grid on the main thread.** Perspective moves its engine to a Worker because the engine does the heavy query work [P7]; binnacle's Grid is the cheap part, and every frame needs a synchronous Snapshot to paint and to place the editor and ARIA layer. A worker would add a frame of latency to every scroll. The Table Source is the thing that may live in a worker or on a server, and binnacle's late-answer model (`set_window` whenever the answer lands) already fits Perspective's and regular-table's async window and AG Grid's `getRows` with a late `success`. Copy AG Grid's version stamp idea only if the Grid does not already discard stale window answers.
3. **Snapshot out as typed-array views, inputs in as typed methods, Effects out as a small serialized batch.** The Strips are a few hundred items: export accessors returning a pointer and length so TS builds `Float32Array` views (offsets, sizes) and a `BigUint64Array` (Keys) over wasm memory for each frame, recreated after any call that might grow memory. Positions fit in `Float64Array` exactly. Inputs (`begin_frame`, `set_window`, pointer events, Actions) are few per frame and benefit from types, so per-method `wasm-bindgen` exports beat quantrish's single serialized `dispatch`. Effects are rare and varied (they carry Sort Specs and text), so draining them as one serialized batch per frame (JSON for a start, as quantrish does in practice) is the right place for a message format.
4. **Let the widget own `requestAnimationFrame` and stop when idle.** Schedule a frame on any input, a Table Source answer, a resize or a DPR change; keep scheduling while the ratatui adapter's `animating(snapshot)` test is true (the Shimmer on a pending sort or Locate); otherwise stop, since a late Table Source answer schedules its own frame. Drain Effects after each frame's input and before painting, so answers arrive as inputs on a later frame, as the core map requires.
5. **Testing** can follow quantrish: vitest for the painter and scroll mapping against fixture Snapshots, wasm-bindgen-test for the bridge, Playwright for scroll, IME and screen-reader-tree checks.
