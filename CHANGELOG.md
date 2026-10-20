# Changelog

## Unreleased

### Breaking

- **ratatui:** `Content::renderer` now chooses the Cell Renderer per cell, from the row Key and the column Key, as the web's `RendererChoice` does, so a column-major table can draw each row its own way. A host that chose by column renames its `renderer(&self, col)` to `column_renderer(&self, col)`, which the default `renderer` calls. `fit_width` asks each cell's own renderer.
