# PROTOTYPE: ratatui frame loop (throwaway)

Wayfinder ticket [#5](https://github.com/flotilla-org/binnacle/issues/5). Not production code: no tests, minimal error handling.

**Question:** does the boundary decided in the map (#6 to #11) feel right from an immediate-mode host? A ratatui host drives a stub Grid and a stub In-Memory Table Source through the frame loop: push input, `begin_frame`, fill Windows, pull the Snapshot, drain Effects through a pump, and answer with Change Notices and `located`.

```sh
cargo run                # interactive: mouse + keyboard, 100k rows (q quits)
cargo run -- --huge      # interactive: 10^15 virtual rows, source can't sort
cargo run -- --script    # scripted walkthroughs: prints every frame and the Grid state
```

Interactive keys: arrows, PgUp/PgDn, Home/End (Shift extends), Ctrl-A, Enter, Esc. Mouse: click and drag cells, Shift-click, click a header to sort (Shift-click adds), drag a header border to resize, double-click a border to auto-size, click a row number to select the row, wheel to scroll.

- `src/grid.rs`: stub Grid (no ratatui types; the liftable part).
- `src/memory.rs`: stub In-Memory Table Source plus an identity source.
- `src/main.rs`: the ratatui host: frame, pump, render, keymap, scripts.

Findings are recorded on the ticket.
