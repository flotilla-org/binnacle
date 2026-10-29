# binnacle

A toolkit-agnostic table/grid core, extracted from [druid_table](https://github.com/rjwittams/druid_table), to be driven from a TUI (ratatui), the web (wasm) and native GUIs over a C ABI (first: [wheelhouse](https://github.com/flotilla-org/wheelhouse)).

Status: planning. The design is being worked out as a wayfinder map: the issue labelled `wayfinder:map`, with decision tickets as its sub-issues.

## Layout

- `crates/binnacle-core/`: the Grid and every type on its boundary. No toolkit dependencies.
- `crates/binnacle-memory/`: Table Sources for in-memory data (so far the identity source).
- `crates/binnacle-ratatui/`: the ratatui adapter and the demo (`cargo run -p binnacle-ratatui --bin binnacle-demo`).
- `crates/binnacle-ffi/`: the internal C API for wheelhouse (empty for now).
- `crates/binnacle-layout/`: the portable Layout document (a table's View Spec, order, pins, sizes and band thicknesses) as JSON or CBOR, with its JSON Schema in `layout.schema.json`.
- `crates/binnacle-testkit/`: the scenario harness and Snapshot text renderer that tests share (not published).
- `docs/research/`: cited research notes that feed the map's decisions.
