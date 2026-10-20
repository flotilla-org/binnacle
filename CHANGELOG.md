# Changelog

## Unreleased

### Breaking

- **ratatui:** `Content::renderer` now chooses the Cell Renderer per cell, from the row Key and the column Key, as the web's `RendererChoice` does, so a column-major table can draw each row its own way. A host that chose by column renames its `renderer(&self, col)` to `column_renderer(&self, col)`, which the default `renderer` calls. `fit_width` asks each cell's own renderer.
- **core:** a Sort header click on a row header now sorts the columns by that row, as a column header click sorts the rows (it did nothing before). Row headers keep `HeaderClick::Select` by default.
- **memory:** `InMemoryTableSource::apply_sort` and `sort_spec` take the axis, so either axis sorts. `MemorySource::view`, `changed`, `edited` and `replaced` return each axis's Change Notices as `Vec<(Axis, ChangeNotice)>`, since sorting the columns by a row makes a row's change move the columns. The testkit's `RecordTable` and `ColumnMajorTable` `edit` and `replace` follow. A sort now starts from the order column moves left, so ties keep it, and a move on sorted columns makes that order the columns' own and clears their sort.
- **wasm:** `WebMemory::sort` takes the axis first.
- **core:** picks are kept per axis: `Selection::picks` is `[Vec<Key>; 2]`, and `picked`, `toggle_pick` and `Snapshot::picked` take the axis. Ctrl/Cmd-click on a column header picks the column under Sort or Select, as on a row header; under `SelectionUnit::Cols` a Ctrl/Cmd-click on a cell picks its column.
- **core:** a `Reset` keeps the Size Overrides of both axes with their Keys (rows lost theirs before), and a `Removed` drops those of the removed Keys on both axes (columns kept theirs before).
- **C API:** the picked Keys move from `BinnacleSnapshot` to `BinnacleAxisSnapshot` (`picked`, `picked_len`), one list per axis.
- **web:** `SelectionChange.picked` is `{ rows, cols }` of IDs and `FrameInfo.picked` is `{ rows, cols }` of counts.

### Added

- **memory:** `Transposed`, a Table Source on its side: the Grid's rows are the inner source's columns and its columns the inner source's rows. With it a row-major source, such as the In-Memory Table Source over records, serves a column-major table. `transpose_spec`, `transpose_totals`, `transpose_find` and `transpose_match` turn the values that cross it.
- **web:** `transpose(source)`, the same for a web Table Source, so `transpose(fromRows(...))` shows each record as a column.
- **testkit:** `ColumnMajorTable`, the records of a `RecordTable` on their side, for scenarios and properties.
- **demos:** a column-major source in both demos: `binnacle-demo --transposed` and the web demo's `transposed` source.
- **memory:** `MemorySource` sorts its columns by rows' values (integers, numbers or text, whichever every column with a sort closure gives), re-sorts them when such a row changes, and drops rows that left on a replace from the columns' sort. `InMemoryTableSource::base_len` and `base_key`.
- **web:** `fromRows` sorts the columns by rows' values the same way; the `headerClick` option sets what a header click does per axis (`"sort"`, `"select"` or `"none"`), as the C API's `binnacle_grid_set_header_click` does; row headers show the columns' sort marker and pending shimmer, and carry `aria-sort`; the live region says "Columns sorted by …". Fitting the row header band leaves room for a sort marker.
- **ratatui:** row headers show the columns' sort marker (`▲`, `▼`, the place, `~`) and the pending shimmer; `animating` counts them.
- **demos:** in the transposed demos a click on a field's row header sorts the records by it.
- **core:** `SelectionUnit::Cols` (a click or focus move selects the cell's whole column) and `Action::SelectRow` (every column of the range's rows, or of the focused row), beside `SelectColumn`. The C API mirrors them as `BINNACLE_SELECT_COLS` and `binnacle_grid_select_row`, wasm as unit code 2 and `select_row`.
- **ratatui:** the headers of selected rows and columns are highlighted (`Theme::header_selected`); Shift+Space selects the row and Alt+PageUp and Alt+PageDown page the columns; `Keymap::preset(preset)` binds what each preset adds (`V` selects the row in modes and bare keys, `h j k l` move in modes) and `Keymap::keys(action)` lists an Action's keys.
- **web:** the `"cols"` selection unit, a `SelectRow` Action on Shift+Space, Alt+PageUp and Alt+PageDown page the columns, and picked columns are marked under their headers and in their cells. The row header band widens once, on the first rows, to fit their labels (and a sort marker where row headers sort), so a column-major table shows its field names whole.
- **demos:** both demos count picked rows and columns; the ratatui demo's Alt+U switches the selection unit.
