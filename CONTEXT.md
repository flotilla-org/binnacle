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
A request the Grid emits for its host or Table Source to act on, such as a sort request, a column move or the start of an edit.
_Avoid_: Callback, event, command

**Snapshot**:
The Grid's per-frame, read-only description of what to draw (cell and header placements with Keys and flags, selection, scroll extents, damage), stamped with a generation.
_Avoid_: Frame, render list, view model

**Change Notice**:
A message from the Table Source telling the Grid how an axis changed (reset, reordered, inserted, removed, content changed).
_Avoid_: Invalidation, update event, data changed

### Axes and identity

**Axis**:
One of the Grid's two symmetric dimensions (rows or columns), each described by a count, a window of keys and a size measure.
_Avoid_: Dimension, direction

**Position**:
An index along an axis in the Table Source's current order, from zero to the axis count, gapless by definition.
_Avoid_: Visual index, row number, offset

**Key**:
An opaque identity the Table Source assigns to a row or column, compared only for equality and used for Grid state that must survive reordering.
_Avoid_: ID, logical index, handle

**Window**:
The span of positions on an axis that the Grid currently needs keys for (the visible range plus overscan), filled in by the Table Source.
_Avoid_: Page, viewport, chunk

**Sort Spec**:
The ordered list of columns and directions the user has asked to sort by, held by the Grid as header state and requested from the Table Source.
_Avoid_: Sort order, remap spec

### Inside an in-memory Table Source

**Logical Index**:
An in-memory Table Source's index into its own stored rows or columns; never seen by the Grid.
_Avoid_: Base index, storage index

**Remap**:
An in-memory Table Source's mapping from Position to Logical Index, produced by its sort, filter and moves.
_Avoid_: Permutation, view mapping
