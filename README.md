# binnacle

A toolkit-agnostic table/grid core, extracted from [druid_table](https://github.com/rjwittams/druid_table), to be driven from a TUI (ratatui), the web (wasm) and native GUIs over a C ABI (first: [wheelhouse](https://github.com/flotilla-org/wheelhouse)).

Status: planning. The design is being worked out as a wayfinder map: the issue labelled `wayfinder:map`, with decision tickets as its sub-issues.

## Layout

- `docs/research/`: cited research notes that feed the map's decisions.
- `legacy/druid_table/`: the original druid widget, kept as reference for harvesting (remap, selection, axis measures). It depends on a 2021 druid git master and is not expected to build as-is. Its charting/animation code (`vis.rs`, `interp.rs`) was left out as out of scope.
