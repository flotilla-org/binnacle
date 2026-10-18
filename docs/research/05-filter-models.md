# Filter models: prior art

How do other tables model filters and find, and how does a data source say what it supports? This note answers the research ticket [#103](https://github.com/flotilla-org/binnacle/issues/103) for the map [Find and filter (#102)](https://github.com/flotilla-org/binnacle/issues/102). It surveys flotilla's ADR 0067 condition grammar, AG Grid, Excel, Glide Data Grid, Perspective, MUI X, VisiData, csvlens, tabiew, Tad (DuckDB-backed), lnav and k9s, then closes with recommendations for the open tickets #104 to #109.

The map has already settled the pattern: the Grid holds a Filter Spec next to the Sort Spec, the Table Source does the work and declares what it supports, filtering applies to rows only, and v1 is a quick filter, find with stepping, and per-column text, regex and list-of-values conditions, each invertible, ANDed across columns. Terms follow [`CONTEXT.md`](https://github.com/flotilla-org/binnacle/blob/main/CONTEXT.md).

Everything here is as of 2026-10-18. Sources are official docs or source code pinned to a commit:

| Short name | Pinned source |
|---|---|
| flotilla | [`flotilla-org/flotilla@2e189f4`](https://github.com/flotilla-org/flotilla/tree/2e189f422ca248bc3fbba97f9f72a07d5c73c43d) |
| AGS | [`ag-grid/ag-grid@1773972f`](https://github.com/ag-grid/ag-grid/tree/1773972f) plus [ag-grid.com docs](https://www.ag-grid.com/javascript-data-grid/) |
| GL | [`glideapps/glide-data-grid@0875d78c`](https://github.com/glideapps/glide-data-grid/tree/0875d78c) |
| PSP, RT | [`finos/perspective@0842ad79`](https://github.com/finos/perspective/tree/0842ad79), [`finos/regular-table@52e86bf8`](https://github.com/finos/regular-table/tree/52e86bf8) |
| MUI | [`mui/mui-x@90a9c061`](https://github.com/mui/mui-x/tree/90a9c061) |
| VD | [`saulpw/visidata@131d749`](https://github.com/saulpw/visidata/tree/131d7492a02a22f0b72a014ec904955187ad8852) |
| CL | [`YS-L/csvlens@6acf060`](https://github.com/YS-L/csvlens/tree/6acf0606df0ff4d58f3bf5d87795dc4cb4a64a5d) |
| TW | [`shshemi/tabiew@bd72510`](https://github.com/shshemi/tabiew/tree/bd72510f9c33ba20c9b32811d0fe04957a84af80) |
| TAD | [`antonycourtney/tad@272ffa0`](https://github.com/antonycourtney/tad/tree/272ffa0092024cd57b82fc813af1f8c6568bc3f0) |
| LN, K9 | [`tstack/lnav@753b61b`](https://github.com/tstack/lnav/tree/753b61bf9c4987342a3f23000ed2a466187d4429), [`derailed/k9s@aeecafa`](https://github.com/derailed/k9s/tree/aeecafaadcc41596dae517616eed71bdc98ff000) |

## Summary

- **flotilla's grammar is narrower than binnacle's v1.** ADR 0067's `where` is ADR 0066's `<field-path> <operator> <literal>` with six comparison operators (`== != < <= > >=`), repeated clauses ANDed. It has no contains, regex, list of values or OR; the ADR says "OR, IN and text search are additions to the grammar". So binnacle's v1 conditions cannot all be expressed on flotilla's wire today, and the inspector would have to hide them through the per-field capabilities of decision 16.2.
- **Inversion is per operator everywhere.** No product surveyed has a general NOT on a column filter: they pair each operator with its negation (`notContains`, `not in`, `!=`). A list-of-values filter inverts by unticking. Only VisiData (`gt`, `zEnter`) and k9s (`/!`) invert a whole match.
- **AND across columns, a small OR inside a column, and nesting as a separate mode.** Excel and AG Grid allow two conditions per column joined by one And/Or; AG Grid's Advanced Filter is a nested tree but cannot coexist with column filters. Perspective and MUI X have a flat list with one AND or OR. Tad's model nests but its editor is flat.
- **The quick filter is a separate field, ANDed with column filters.** AG Grid and MUI X split the text into words, require every word to match some column, and AND the result with the column filters. k9s and tabiew filter by a row's joined text.
- **Find highlights and steps; it never filters.** A match is a cell (AG Grid counts occurrences within cells). Big-data tools scan incrementally and show a partial count: Glide stops at 1000 ("over 1000"), csvlens shows `i/N+` while scanning. Tools that call their box "search" (tabiew, k9s) are filters, not finds.
- **List-of-values filters split on narrowing.** Excel and AG Grid's client model narrow the list by the other columns' filters; AG Grid's server model, Perspective and Tad do not (AG Grid leaves it to the host). Caps: Excel 10,000, Tad 50, Perspective 10 suggestions; all have a search box.
- **Only Perspective lets the source declare its filters:** `filter_ops`, a list of operators per column type, sent from the server and used to build each column's operator menu. ADR 0067 decision 16.2 asks for the same per field. Everyone else configures operators on the client.
- **Keys:** the terminal tools use `/` find, `n`/`N` step, and a prompt line with history; binnacle's typing-on Keymap makes `/` and `n` unavailable unmodified. Of the web grids only Glide takes Ctrl/Cmd-F, opt-in and only while the canvas has focus.

## flotilla: ADR 0067 and ADR 0066

**Condition kinds.** ADR 0067 decision 12: "`?where=<field-path> <operator> <literal>`, the ADR 0066 condition grammar without its address part" ([ADR 0067](https://github.com/flotilla-org/flotilla/blob/2e189f422ca248bc3fbba97f9f72a07d5c73c43d/docs/adr/0067-tables-describe-themselves-on-the-wire.md), decision 12). The operators are `Equal`, `NotEqual`, `LessThan`, `LessThanOrEqual`, `GreaterThan`, `GreaterThanOrEqual`, printed `== != < <= > >=` ([`flotilla-address/src/condition.rs` L17-L48](https://github.com/flotilla-org/flotilla/blob/2e189f422ca248bc3fbba97f9f72a07d5c73c43d/crates/flotilla-address/src/condition.rs#L17-L48)). Field paths and literals are bare words or JSON-quoted strings, and the printer is canonical (same file, L1-L9; [ADR 0066](https://github.com/flotilla-org/flotilla/blob/2e189f422ca248bc3fbba97f9f72a07d5c73c43d/docs/adr/0066-addresses-name-things-by-natural-key.md) §4). The storage layer's own index predicates have `In { column, codes }` and `NotNull` ([`flotilla-schema/src/descriptor.rs` L272-L277](https://github.com/flotilla-org/flotilla/blob/2e189f422ca248bc3fbba97f9f72a07d5c73c43d/crates/flotilla-schema/src/descriptor.rs#L272-L277)), but those are not part of the `where` grammar.

**Not there yet:** text contains, regex, list of values, blanks, case options. "OR, IN and text search are additions to the grammar, not to the routes" (ADR 0067 decision 12). Null ordering is specified (null lowest) but null matching is not (decision 16.3).

**Inversion:** only `!=`.

**Combining:** "Repeated `where` clauses are ANDed" (decision 12). No OR, no nesting.

**Find:** none. The view subscription (decision 15) answers count, windows, positional changes and "what is the position of key K?", which is Locate. It has no "next match after K" query and no per-cell match marks.

**Distinct values:** none.

**Declaring support.** Two rules matter. A clause the store cannot push down "is refused with its own error code ... never answered by listing and filtering" (decision 12), so the server never fakes a filter. And decision 16.2: "Field properties say whether a field can be sorted and which `where` operators it supports, so a viewer can disable a control up front rather than learn from a 400." That came from binnacle's review, [flotilla#3427](https://github.com/flotilla-org/flotilla/issues/3427) point 2. It is per field and per operator.

## AG Grid

**Condition kinds.** Text: `contains` (default), `notContains`, `equals`, `notEqual`, `startsWith`, `endsWith`, `blank`, `notBlank` ([text filter](https://www.ag-grid.com/javascript-data-grid/filter-text/)). Number: the comparisons, `inRange`, `blank`, `notBlank`; blanks fail every comparison unless an `includeBlanksIn...` flag says otherwise ([number filter](https://www.ag-grid.com/javascript-data-grid/filter-number/#blank-cells)). Date: comparisons, `inRange`, blanks and presets such as `last7Days` ([date filter](https://www.ag-grid.com/javascript-data-grid/filter-date/)). Custom options carry `displayKey`, `predicate` and `numberOfInputs` ([filter conditions](https://www.ag-grid.com/javascript-data-grid/filter-conditions/)). The set filter is a list of values, model `{filterType: 'set', values: [...]}`, with missing values grouped as "(Blanks)" ([set filter list](https://www.ag-grid.com/javascript-data-grid/filter-set-filter-list/#missing-values)). No regex operator; `textMatcher` can replace matching.

**Case.** Text and set filters are case-insensitive unless `caseSensitive: true` (text filter page; [set filter case](https://www.ag-grid.com/javascript-data-grid/filter-set-filter-list/#enabling-value-case-sensitivity)).

**Inversion.** Negated operator keys only; the set filter inverts by unticking. The Advanced Filter adds `is any of` and `is none of` ([SSRM filtering](https://www.ag-grid.com/javascript-data-grid/server-side-model-filtering/#advanced-filter)).

**Combining.** Inside a column: `{filterType, operator: 'AND' | 'OR', conditions: [...]}`, `maxNumConditions` default 2, one shared join operator ([filter conditions](https://www.ag-grid.com/javascript-data-grid/filter-conditions/)). Across columns: AND, keyed by column id. The Advanced Filter is a nested tree of `{filterType: 'join', type: 'AND' | 'OR', conditions}`, typed as text or built in a builder, and cannot be active alongside column filters ([advanced filter](https://www.ag-grid.com/javascript-data-grid/filter-advanced/)).

**Quick filter.** `quickFilterText` is split into words; each word must match some column, case-insensitively; the result works "in addition to column-specific filtering". Hooks: `quickFilterParser`, `quickFilterMatcher` (for example a regex), `getQuickFilterText` per column, `cacheQuickFilter` above 10,000 rows. "The Quick Filter is only supported with the Client-Side Row Model" ([quick filter](https://www.ag-grid.com/javascript-data-grid/filter-quick/)).

**Find** (Enterprise, client-side model only). "All matches highlighted and navigable, similar to find (Ctrl+F) within the browser", set through `findSearchValue` ([find](https://www.ag-grid.com/javascript-data-grid/find/)). A match is a substring occurrence inside a cell's formatted value, so one cell may hold several; text is lowercased unless `findOptions.caseSensitive` ([`findService.ts` L35-L55, L354-L360](https://github.com/ag-grid/ag-grid/blob/1773972f/packages/ag-grid-enterprise/src/find/findService.ts#L35-L55)). API: `findNext`, `findPrevious`, `findGoTo(n)`, `findGetTotalMatches`, `findGetActiveMatch`; the `findChanged` event carries `activeMatch.numOverall` and `totalMatches`. Find runs "after filtering and sorting", skips hidden columns and does not filter ([find, content to search](https://www.ag-grid.com/javascript-data-grid/find/#content-to-search)). The docs example binds Enter and Shift+Enter in its own input; no Ctrl-F capture is documented.

**List of values on big data.** Client side, the list is taken from the data and narrowed to rows that pass the *other* filters ([`setValueModel.ts` L631](https://github.com/ag-grid/ag-grid/blob/1773972f/packages/ag-grid-enterprise/src/setFilter/setValueModel.ts#L631)), refreshed on another filter's change ([`setFilter.ts` L190-L197](https://github.com/ag-grid/ag-grid/blob/1773972f/packages/ag-grid-enterprise/src/setFilter/setFilter.ts#L190-L197)). Server side, the host supplies `values` as an array or an async callback; they "will not change unless you ask", so cascading is the host calling `refreshFilterValues()` when another filter changes ([SSRM set filter values](https://www.ag-grid.com/javascript-data-grid/server-side-model-filtering/#set-filter-values); [refreshing values](https://www.ag-grid.com/javascript-data-grid/filter-set-filter-list/#refreshing-values)). The mini filter searches the list (substring, case-insensitive) and Enter selects what passes ([mini filter](https://www.ag-grid.com/javascript-data-grid/filter-set-mini-filter/)). No list cap found.

**Declaring support.** None from the server. The client configures `filter` and `filterOptions` per column; the server-side row model sends `filterModel` in each `getRows` request and the server must interpret it ([SSRM filtering](https://www.ag-grid.com/javascript-data-grid/server-side-model-filtering/)).

## Excel AutoFilter and Find

**Condition kinds.** "By a list value or by criteria." Text: Equals, Does Not Equal, Begins With, Does Not Begin With, Ends With, Does Not End With, Contains, Does Not Contain; plus number and date filters, colour and icon filters, top 10 and average ([filter data](https://support.microsoft.com/en-us/office/filter-data-in-a-range-or-table-01832226-31b5-4568-8806-38c37dcc180e); [`XlAutoFilterOperator`](https://learn.microsoft.com/en-us/office/vba/api/excel.xlautofilteroperator)). In the API, `"="` means blanks and `"<>"` non-blanks ([`Range.AutoFilter`](https://learn.microsoft.com/en-us/office/vba/api/excel.range.autofilter)). Wildcards `?`, `*`, `~` escape; no regex.

**Case.** "Excel doesn't distinguish between uppercase and lowercase characters" ([advanced criteria](https://support.microsoft.com/en-us/office/filter-by-using-advanced-criteria-4c9222fe-8529-4cd7-a898-3f16abdff32b)).

**Inversion.** Negated operators, or unticking values.

**Combining.** Two conditions per column, And or Or ([screen reader guide](https://support.microsoft.com/en-us/office/use-a-screen-reader-to-sort-or-filter-a-table-in-excel-d6ae119c-5fab-4b7f-8869-6ef02ece71f5); `Criteria1`, `Criteria2` with `xlAnd` or `xlOr`). Columns combine by AND. No quick filter.

**Find.** A dialog: Find Next, Find All (a list of matching cells, choosing one selects it), Match case, Match entire cell contents, by rows or columns, in values, formulas or notes ([find or replace](https://support.microsoft.com/en-us/office/find-or-replace-text-and-numbers-on-a-worksheet-0e304ca5-ecef-4808-b90f-fdb42f892e90)). A match is a cell. Find does not filter, and on filtered data "only the data that is displayed is searched" (filter data page).

**List of values.** It narrows: "the only filters available for other columns are the values visible in the currently filtered range". It caps: "Only the first 10,000 unique entries in a list appear". (Blanks) appears only when the column has blanks (filter data page). The list has a search box with wildcards ([use AutoFilter](https://support.microsoft.com/en-us/office/use-autofilter-to-filter-your-data-7d87d63e-ebd0-424b-8106-e2ab61133d92)).

**Keys.** Ctrl+Shift+L toggles AutoFilter; Alt+Down opens a header's filter menu; Ctrl+F opens Find; Shift+F4 repeats Find ([keyboard shortcuts](https://support.microsoft.com/en-us/office/keyboard-shortcuts-in-excel-1798d9d5-842a-42b8-9c99-9b7213f0040f); screen reader guide).

## Glide Data Grid (canvas)

**Filters:** none; the host filters its own data (no filter props on `DataEditor`).

**Find.** Props `showSearch`, `searchValue`, `searchResults` (a host override) and `onSearchResultsChanged(results, navIndex)` ([`data-grid-search.tsx` L53-L88](https://github.com/glideapps/glide-data-grid/blob/0875d78c/packages/core/src/internal/data-grid-search/data-grid-search.tsx#L53-L88)). A match is a cell: the escaped query tested with flag `i` against a per-kind text (display text for text and numbers) (L167, L203-L227). The built-in search reads rows through `getCellsForSelection` from the current scroll row, about 10 ms per animation frame, wraps, shows a progress bar and **stops at 1000 results**, labelled "over 1000" (L165-L268, L393-L394). Matches are painted as `prelightCells` (L491). Enter and Shift+Enter step, with "N of M results" (L352-L361). It does not filter.

**Ctrl-F.** The `search` keybinding is off by default and `primary+f` when enabled ([`data-editor-keybindings.ts` L88, L163](https://github.com/glideapps/glide-data-grid/blob/0875d78c/packages/core/src/data-editor/data-editor-keybindings.ts#L88)); it calls `preventDefault`, so it replaces browser find only while the grid has focus. Inside the box, Ctrl/Cmd-F or Escape closes it. Browser find cannot see canvas text, and Glide's accessibility table covers only visible rows, which is why it ships its own search.

## Perspective

**Condition kinds.** A filter is `[column, op, value]`. Strings: `==`, `!=`, comparisons, `begins with`, `contains`, `ends with`, `matches` (regex) and their `not` forms, `in`, `not in`, `is null`, `is not null`; numbers and dates: comparisons and null tests ([view config docs](https://github.com/finos/perspective/blob/0842ad79/docs/md/explanation/view/config/selection_and_ordering.md)).

**Case.** `begins with`, `contains`, `ends with` lowercase both sides ([`scalar.cpp` L1446-L1481](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-server/cpp/perspective/src/cpp/scalar.cpp#L1446-L1481)); `matches` is a case-sensitive RE2 partial match. Null cells match no string operator, negated ones included.

**Inversion.** Negated operators only.

**Combining.** A flat list with one reducer: `repeated Filter filter`, `FilterReducer filter_op` of `AND` (default) or `OR` ([`perspective.proto` L643-L675](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-client/perspective.proto#L643-L675)). No nesting. No quick filter and no find; the datagrid draws an HTML table of visible cells only ([regular-table README](https://github.com/finos/regular-table/blob/52e86bf8/README.md)), so browser find sees only the viewport.

**List of values.** Suggestions for `==`, `!=`, `in`, `not in` come from a temporary view grouped by the column over the whole table with no filter, so they do not narrow ([`column_values.rs` L26-L60](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-viewer/src/rust/queries/column_values.rs#L26-L60)). They are searched by substring, case-insensitively, and capped at 10 ([`filter_dropdown.rs` L273-L287](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-viewer/src/rust/components/filter_dropdown.rs#L273-L287)).

**Declaring support (the one real example).** `Features.filter_ops`, "Available filter operators per column type" ([`features.rs` L52-L55](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-client/src/rust/virtual_server/features.rs#L52-L55)), on the wire as `GetFeaturesResp.filter_ops` ([`perspective.proto` L240-L249](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-client/perspective.proto#L240-L249)). The viewer builds each column's operator menu from it ([`filter_column.rs` L384-L390](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-viewer/src/rust/components/column_selector/filter_column.rs#L384-L390)). A DuckDB virtual server declares its own lists ([`duckdb.ts` L314-L321](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-js/src/ts/virtual_servers/duckdb.ts#L314-L321)); the default handler declares nothing. Granularity is per column type, not per column. The engine's own declaration disagrees with its docs for booleans ([`server.cpp` L2329-L2376](https://github.com/finos/perspective/blob/0842ad79/rust/perspective-server/cpp/perspective/src/cpp/server.cpp#L2329-L2376)), a reminder that a declaration needs a test against the implementation.

## MUI X Data Grid

**Condition kinds.** Strings: `contains`, `doesNotContain`, `equals`, `doesNotEqual`, `startsWith`, `endsWith`, `isEmpty`, `isNotEmpty`, `isAnyOf` ([`gridStringOperators.ts`](https://github.com/mui/mui-x/blob/90a9c061/packages/x-data-grid/src/colDef/gridStringOperators.ts#L67-L127)); numbers add comparisons. Per column `filterOperators`, or `filterable: false` ([filtering customization](https://mui.com/x/react-data-grid/filtering/customization/)). Case-insensitive by regex flag `i`, with `ignoreDiacritics` available.

**Combining.** `filterModel = {items: [{field, operator, value}], logicOperator}`, flat, And by default; more than one item needs Pro ([filtering](https://mui.com/x/react-data-grid/filtering/)).

**Quick filter.** `quickFilterValues` (the text split on spaces), `quickFilterLogicOperator` (And by default), hidden columns excluded; numbers must equal a term, strings contain it; ANDed with the items ([quick filter](https://mui.com/x/react-data-grid/filtering/quick-filter/); [`gridFilterUtils.ts` L544-L600](https://github.com/mui/mui-x/blob/90a9c061/packages/x-data-grid/src/hooks/features/filter/gridFilterUtils.ts#L544-L600)). `filterMode="server"` hands the model to the host; no capability declaration ([server-side filtering](https://mui.com/x/react-data-grid/filtering/server-side/)). No find.

## VisiData

**Model.** Find moves the cursor; filtering is select, then derive a new sheet from the selection.

**Find.** `/` and `?` search the current column forward and back, `g/` and `g?` all visible columns, `n` and `N` step, `z/` searches by Python expression ([`search.py` L144-L153](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/search.py#L144-L153)). It runs in a thread from the cursor with wraparound and stops at the first row where a searched cell matches `regex.search`, placing the cursor on that cell (L15-L20, L63-L70). Plain `/` reports no count. With `highlight_search` on (default), matched substrings are drawn highlighted ([`hlsearch.py` L68-L114](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/features/hlsearch.py#L68-L114)).

**Case.** `regex_flags` defaults to `I` (case-insensitive) ([`text_source.py` L6](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/text_source.py#L6)), and every search prompt has a second "regex flags" field prefilled from it ([`search.py` L80-L90](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/search.py#L80-L90)).

**Filter primitives.** `|` and `\` select and unselect rows whose current column matches a regex, `g|` any column; `,` selects rows equal to the current cell; `z|` selects by expression ([`selection.py` L229-L240](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/selection.py#L229-L240)). Selection is a set, so repeated selects are OR, `\` is AND NOT, `gt` inverts the selection; AND is `"` (open the selected rows as a new sheet) then select again ([`sheets.py` L1319-L1321](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/sheets.py#L1319-L1321)). Bulk selects report "selected N rows" with progress.

**List of values.** `F` builds a frequency table of the current column with counts, percentages and a histogram, sorted by count, in a background thread over every row, uncapped ([`freqtbl.py` L123-L147](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/freqtbl.py#L123-L147); [`pivot.py` L189-L230](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/pivot.py#L189-L230)). It is a sheet, so `/` searches it. Selecting bins selects their source rows; `Enter` opens one bin's rows, `gEnter` the union of selected bins, `zEnter` everything else (inversion) ([`freqtbl.py` L43-L73, L211-L215](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/freqtbl.py#L43-L73)). It narrows only when run on a derived sheet.

**Prompt.** Esc or Ctrl-C cancels; Up and Down walk per-prompt history; empty input reuses the last regex; readline editing keys ([`_input.py` L250-L299, L559](https://github.com/saulpw/visidata/blob/131d7492a02a22f0b72a014ec904955187ad8852/visidata/_input.py#L250-L299)).

## csvlens

**Keys and conditions.** `/` finds (regex), `n` and `N` step, `&` filters rows, `*` filters columns, `#` finds and `@` filters rows equal to the selected cell (built as `^escaped$`), `r` resets ([`help.rs` L29-L54](https://github.com/YS-L/csvlens/blob/6acf0606df0ff4d58f3bf5d87795dc4cb4a64a5d/src/help.rs#L29-L54); [`app.rs` L827-L838](https://github.com/YS-L/csvlens/blob/6acf0606df0ff4d58f3bf5d87795dc4cb4a64a5d/src/app.rs#L827-L838)). Regex only: no comparisons, blanks or inversion, and one row find or filter at a time (L861-L885). The column filter composes with the row filter.

**Case.** `-i` is smart case: case-insensitive unless the pattern has an uppercase letter ([README L71-L72](https://github.com/YS-L/csvlens/blob/6acf0606df0ff4d58f3bf5d87795dc4cb4a64a5d/README.md#L71-L72)).

**Scope and match.** With a column selected, find and filter search that column only; otherwise all. A hit is a row plus its matching column indexes ([`find.rs` L485-L578](https://github.com/YS-L/csvlens/blob/6acf0606df0ff4d58f3bf5d87795dc4cb4a64a5d/src/find.rs#L485-L578)).

**Big files.** A background thread re-reads the file, appending hits under a mutex; the UI waits 5 ms for the first hit (L809-L824 of `app.rs`). The status shows `[Find "pat" in Col: i/N+]`, the `+` meaning the scan is still going, then `Not found` if none ([`ui.rs` L1095-L1135](https://github.com/YS-L/csvlens/blob/6acf0606df0ff4d58f3bf5d87795dc4cb4a64a5d/src/ui.rs#L1095-L1135)). Matched substrings are highlighted (L508-L545).

**Prompt.** A one-line buffer; Enter applies; Esc or Enter on an empty buffer *clears the current find or filter*, not just the prompt; Up and Down give one shared history ([`input.rs` L215-L298](https://github.com/YS-L/csvlens/blob/6acf0606df0ff4d58f3bf5d87795dc4cb4a64a5d/src/input.rs#L215-L298)). CLI flags `--filter`, `--find`, `--columns` set initial state.

## tabiew

**Filter.** From the command palette (`:`), `f <expr>` filters with a SQL `WHERE`, run by polars-sql as `SELECT * FROM _ where {value}` ([`inline_query_picker.rs` L37-L45](https://github.com/shshemi/tabiew/blob/bd72510f9c33ba20c9b32811d0fe04957a84af80/src/tui/popups/pickers/inline_query_picker.rs#L37-L45)). So conditions are full SQL: comparisons, LIKE, IN, IS NULL, AND, OR, NOT, nesting. Each filter pushes a frame on a stack, so filtering again ANDs; `q` pops ([`pane.rs` L283-L287, L466-L471](https://github.com/shshemi/tabiew/blob/bd72510f9c33ba20c9b32811d0fe04957a84af80/src/tui/pane.rs#L283-L287)). Completion offers keywords and column names, not values.

**Search is a filter.** `/` is fuzzy and `?` exact (case-sensitive contains) ([`pane.rs` L455-L466](https://github.com/shshemi/tabiew/blob/bd72510f9c33ba20c9b32811d0fe04957a84af80/src/tui/pane.rs#L455-L466)). Each keystroke restarts a scorer over every cell on a rayon thread, and the shown frame is rebuilt every 100 ms, ordered by score ([`search.rs` L203-L262](https://github.com/shshemi/tabiew/blob/bd72510f9c33ba20c9b32811d0fe04957a84af80/src/misc/search.rs#L203-L262)). Enter keeps the result, Esc pops it. No next and previous, highlighting, match count or list of values found.

## Tad (DuckDB-backed)

**Condition kinds.** EQ, NEQ, GT, GE, LT, LE; for strings BEGINS, ENDS, CONTAINS and NOT forms; IN and NOTIN; ISNULL and NOTNULL ([`FilterExp.ts` L25-L63](https://github.com/antonycourtney/tad/blob/272ffa0092024cd57b82fc813af1f8c6568bc3f0/packages/reltab/src/FilterExp.ts#L25-L63)). Operators are offered by column type (L92-L100). Text operators compile to `[NOT] ILIKE '%x%'`, so case-insensitive (L105-L157).

**Combining.** `FilterExp {op: AND | OR, opArgs}` nests (L217-L305), but the editor is a flat list with "All Of" or "Any Of" ([`FilterEditor.tsx` L41-L109](https://github.com/antonycourtney/tad/blob/272ffa0092024cd57b82fc813af1f8c6568bc3f0/packages/tadviewer/src/components/FilterEditor.tsx#L41-L109)).

**List of values.** An async multi-select runs `distinct(col)` on the base query with limit 50, searched as server-side CONTAINS, cached, and not narrowed by other filters ([`FilterEditorRow.tsx` L28-L50, L286-L299](https://github.com/antonycourtney/tad/blob/272ffa0092024cd57b82fc813af1f8c6568bc3f0/packages/tadviewer/src/components/FilterEditorRow.tsx#L28-L50)). No find.

## lnav and k9s (briefly)

**lnav.** `:filter-in` and `:filter-out` take regexes, always case-insensitive ([`cmds.filtering.cc` L510-L530](https://github.com/tstack/lnav/blob/753b61bf9c4987342a3f23000ed2a466187d4429/src/cmds.filtering.cc#L510-L530)). A line shows if it matches any filter-in (or there are none) and no filter-out ([`filter_observer.hh` L71-L80](https://github.com/tstack/lnav/blob/753b61bf9c4987342a3f23000ed2a466187d4429/src/filter_observer.hh#L71-L80)), capped at 32 filters. `:filter-expr` takes an SQL expression. `/` finds, `n` and `N` step, the status shows current hit and total, hits are reverse video with scrollbar ticks, and find does not filter ([`hotkeys.rst` L108-L112](https://github.com/tstack/lnav/blob/753b61bf9c4987342a3f23000ed2a466187d4429/docs/source/hotkeys.rst#L108-L112); [`ui.rst` L191, L282](https://github.com/tstack/lnav/blob/753b61bf9c4987342a3f23000ed2a466187d4429/docs/source/ui.rst#L191)).

**k9s.** `/` filters live (debounced 100 ms) with a case-insensitive regex over a row's joined visible fields, `/!` inverts, `/-f` is fuzzy, `/-l` a label selector ([README L392-L396](https://github.com/derailed/k9s/blob/aeecafaadcc41596dae517616eed71bdc98ff000/README.md#L392-L396); [`table_data.go` L144-L215](https://github.com/derailed/k9s/blob/aeecafaadcc41596dae517616eed71bdc98ff000/internal/model1/table_data.go#L144-L215)). Esc clears and leaves the prompt; Enter commits ([`prompt.go` L144-L190](https://github.com/derailed/k9s/blob/aeecafaadcc41596dae517616eed71bdc98ff000/internal/ui/prompt.go#L144-L190)).

## Side by side

| | Text / regex | List of values | Comparisons, blanks | Case default | Inversion | Combining | Find |
|---|---|---|---|---|---|---|---|
| flotilla ADR 0067 | no / no | no (IN is a planned addition) | yes / no | n/a | `!=` | AND of clauses | none |
| AG Grid | yes / via matcher | set filter, narrows client side | yes / yes | insensitive | per operator | 2 per column, AND across, nested in Advanced | highlight, step, count occurrences |
| Excel | wildcards / no | yes, narrows, cap 10,000 | yes / yes | insensitive | per operator, untick | 2 per column, AND across | dialog, Find All, cell matches |
| Glide | none | none | none | insensitive (find) | none | none | highlight, step, stop at 1000 |
| Perspective | yes / `matches` | `in`, suggestions cap 10, no narrowing | yes / yes | insensitive (regex sensitive) | per operator | flat AND or OR | none |
| MUI X | yes / no | `isAnyOf` | yes / yes | insensitive | per operator | flat AND or OR | none |
| VisiData | regex | `F` table, uncapped, threaded | via expression | insensitive | unselect, `gt`, `zEnter` | select OR, derive for AND | moves cursor, highlight |
| csvlens | regex | none | none | smart case (`-i`) | none | one row filter | highlight, step, `i/N+` |
| tabiew | SQL LIKE | SQL IN | SQL | sensitive | SQL NOT | SQL, stacked AND | search filters |
| Tad | yes / no | IN, distinct limit 50, no narrowing | yes / yes | insensitive | per operator | AND or OR, nests in model | none |

## Recommendations for the open tickets

These are recommendations, not decisions; each ticket settles its own question.

**#104, the Filter Spec and condition model.**
- Hold three things side by side, as the map says: a quick filter, a list of per-column conditions, and a find query. Keep find out of the filter: no surveyed product's find filters, and the products whose "search" filters (tabiew, k9s) are really a quick filter.
- Make the quick filter its own field, ANDed with the column conditions, as AG Grid and MUI X do. Decide whether it splits into words (both do) or matches the whole string; whole string is simpler to explain with a regex option.
- A condition is `{column Key, kind, not: bool}`, with kinds `Contains { text, case }`, `Regex { pattern, case }`, `OneOf { values }` in v1, and room for `Compare { op, literal }` and `Blank`. A `not` flag is cleaner than doubling the operators and maps onto every surveyed negation (`!=`, `not in`, `NOT LIKE`). Default case-insensitive, which is the near-universal default; consider csvlens's smart case for regex.
- Shape the combination as a tree from day one (`All([...])`, later `Any`), but have v1 build only a flat `All` of conditions. That is the shape of AG Grid's Advanced Filter and Tad's `FilterExp`, and it matches ADR 0067's ANDed clauses.
- Copy the Sort Spec's pattern (`SortState` holds applied, requested and a pending count, [`sort.rs`](https://github.com/flotilla-org/binnacle/blob/main/crates/binnacle-core/src/sort.rs)): a `FilterRequested` Effect carrying the whole spec, answered by the source with the spec it applied. A source that cannot apply part of it says so, as ADR 0067 refuses a clause it cannot push down.
- On ADR 0067: its six comparisons are a subset of the model above, so the model "lines up" in the sense that a flotilla source can declare `Compare` and refuse the rest. Contains, regex and lists of values need the grammar additions ADR 0067 already names (IN, text search) before the inspector can offer them.

**#105, declaring what a source supports.**
- Declare per column and per condition kind, the granularity ADR 0067 decision 16.2 already asks of flotilla's field properties. Perspective's per-type `filter_ops` is the one shipped precedent, but per column subsumes per type.
- Declare the quick filter and find separately (a source may support find but not filtering, like Glide's host-supplied results). Make declarations changeable at runtime, like `set_sortable`, since a server learns its capabilities after connecting.
- Hide unsupported controls rather than disable them, except where a column supports some kinds: then show only those kinds in its menu, as Perspective does.

**#106, find and matches.**
- A match is a cell (row Key, column Key). AG Grid counts several occurrences per cell; counting cells is enough for stepping and is what Glide, Excel and csvlens do.
- Step with a source query "the next match after (row, column) in this direction", answered with Keys, then Locate. Wrap around at the end, as VisiData and Glide do.
- Show a count that may be partial or absent: csvlens's `i/N+` and Glide's "over 1000" show that a running or capped count is acceptable. ADR 0067's "partial results are always marked" points the same way.
- Mark matches in the visible window as a per-cell flag the source supplies with the window, so highlights arrive with the cells they belong to.
- For flotilla, note that the view subscription has Locate but no next-match query; a server-backed find needs an addition to ADR 0067.

**#107, the list-of-values filter.**
- A source query for distinct values of a column: capped (Tad 50, Excel 10,000), searchable by substring on the source side (Tad, Perspective, AG Grid's mini filter), and answered late. Counts per value are a nice-to-have (VisiData's `F`) and should be optional.
- Narrow by the *other* columns' conditions by default, as Excel and AG Grid's client model do, and let a source that cannot narrow say so; Perspective and Tad do not narrow and AG Grid's server mode leaves it to the host, so both behaviours are normal.
- Show a capped list as capped ("first 1,000 values; type to search") and always offer a blanks entry when the column has blanks.

**#108, filter and existing features.** The survey does not bear on most of this. One point: every surveyed product applies a filter as a whole new result, never row by row as edits land, so "a row that stops matching stays until the filter is re-applied" has no direct precedent but is also not contradicted.

**#109, controls.**
- ratatui: VisiData, csvlens and lnav all use `/` and `n`/`N`, which collide with the default Keymap's typing. Use Ctrl-F to open a find prompt (Excel, browsers and Glide all use it) and have the prompt line itself handle Enter and Shift-Enter (or Down and Up) for next and previous, as Glide and AG Grid's example do. Note that the ratatui demo binds Ctrl+F to its fixed filter today ([`binnacle-demo.rs`](https://github.com/flotilla-org/binnacle/blob/main/crates/binnacle-ratatui/src/bin/binnacle-demo.rs)), so that moves. Give the prompt Esc to close (decide whether Esc also clears the find: csvlens and k9s clear, VisiData only cancels the prompt) and Up and Down history.
- Web: take Ctrl/Cmd-F only while the grid has focus, opt-in, as Glide does. Browser find cannot see canvas text, and a canvas grid without its own find leaves the user with nothing. Put the filter menu behind a header button with a searchable checklist, the Excel and AG Grid shape, and a quick filter field outside the canvas.
