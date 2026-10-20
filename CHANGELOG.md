# Changelog

## Unreleased

### Breaking

- **ratatui:** `Content::renderer` now chooses the Cell Renderer per cell, from the row Key and the column Key, as the web's `RendererChoice` does, so a column-major table can draw each row its own way. A host that chose by column renames its `renderer(&self, col)` to `column_renderer(&self, col)`, which the default `renderer` calls. `fit_width` asks each cell's own renderer.

### Added

- **memory:** `Transposed`, a Table Source on its side: the Grid's rows are the inner source's columns and its columns the inner source's rows. With it a row-major source, such as the In-Memory Table Source over records, serves a column-major table. `transpose_spec`, `transpose_totals`, `transpose_find` and `transpose_match` turn the values that cross it.
- **web:** `transpose(source)`, the same for a web Table Source, so `transpose(fromRows(...))` shows each record as a column.
- **testkit:** `ColumnMajorTable`, the records of a `RecordTable` on their side, for scenarios and properties.
- **demos:** a column-major source in both demos: `binnacle-demo --transposed` and the web demo's `transposed` source.
