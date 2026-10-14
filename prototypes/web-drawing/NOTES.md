# PROTOTYPE: web drawing of a Snapshot (#45)

Throwaway. Delete it, or fold the verdict into the web adapter, once the question is answered.

## Question

Which drawing mode should the web adapter build first, canvas or a DOM with only the visible rows rendered, and what painter interface over a Snapshot keeps the other possible?

Secondary: does a capped native scroller with Glide's hybrid mapping onto `Action::Scroll` / `Action::ScrollTo` feel right compared with a custom-drawn scrollbar driven by the Thumb? Does one persistent textarea at `focus_rect` work as the key and IME target?

## How to run

```sh
cd prototypes/web-drawing
pnpm install
pnpm dev            # builds the wasm (wasm-pack, release), then Vite on http://127.0.0.1:5745/
```

Set `CARGO_TARGET_DIR` to share a target dir; it defaults to `wasm/target`.

- `?variant=A`: canvas painter + capped native scroller (hybrid mapping)
- `?variant=B`: DOM painter (cells reused by Key pair) + the same scroller
- `?variant=C`: canvas painter + scrollbar drawn from the Thumb, wheel intercepted

The bottom bar (or Alt+Left/Right) cycles variants. The overlay at the top right shows frame times, frames drawn, DOM nodes, Scroll Anchor and Thumb, scrollTop against the sizer, focus Position and Key, the Mode and the last Effects.

Keys go to the transparent textarea at the focused cell: arrows (Ctrl/Cmd for the edge, Shift extends), PageUp/PageDown, Home/End, Tab, Enter (Activate, opens the editor), F2 (BeginEdit), Escape, Ctrl/Cmd+A. Typing a printable key or starting an IME composition opens the editor in place. Drag a column header border to resize.

In the console, `await __proto.bench(240, 37)` scrolls 37 px per animation frame through the variant's own input path (scrollTop for A and B, a synthetic wheel event for C) and returns frame-time stats.

## Layout

- `wasm/src/lib.rs`: the bridge (`WebGrid`), its own `[workspace]` so the repo's `cargo test --workspace` never sees it.
- `src/painter.ts`: the `Painter` interface (`resize(w, h, dpr)`, `paint(snapshot, content)`), theme, scrollbar geometry.
- `src/canvasPainter.ts`, `src/domPainter.ts`: the two painters. Swapping them is one line in `src/main.ts`.
- `src/content.ts`: the huge Table Source in TS (10^15 rows, Keys are Positions, values synthesized from the Key, a sparkline column). The `Content` lookup a painter reads.
- `src/widget.ts`: the framework-neutral widget: rAF loop that idles, scroller and hybrid mapping, drawn scrollbar, pointer, keymap, textarea and editor.
- `src/bridge.ts`: decodes the flat Snapshot array.

## What the bridge needed

Everything crosses as numbers; Keys and Positions as `f64` (exact below 2^53, so the huge source fits; a real bridge needs `BigInt` or a split pair for full `u64` Keys). `NaN` stands for `None`.

1. `new(rows, row_size, col_header, cols, col_size, row_header, gap)`: wraps `AxisConfig` and `Grid::new`; marks every column unsortable.
2. `begin_frame(w, h) -> [rowStart, rowEnd, colStart, colEnd]` (`WindowRequests` flattened).
3. `set_window(axis, start, keys: Float64Array)`. The Keys only; values never enter wasm.
4. `snapshot() -> Float64Array`: one flat array per frame. 48 scalars (generation, Damage bits, counts, extents, header bands, gaps, Scroll Anchors, Thumbs, focus Positions and Keys, `focus_rect`, `editor_rect`, Cursor, Mode, `capturing`, resize guide, drop indicator, hovered header, `animating`, selection range), then 4 numbers per row item, per column item and per selection rect. It is copied out (a few KB); typed-array views over wasm memory would avoid the copy but were not needed at these frame times.
5. `animating` computed in Rust (pending sort on a visible header, or a Locate with a visible focus), because `HeaderSort::pending` is per header and the ratatui test is not exported from core.
6. `poll_effects() -> String`: every Effect drained as one JSON array, hand-formatted (no serde).
7. `pointer(kind, x, y, button, mods bitmask, clicks)`. The host counts clicks itself: `pointerdown.detail` is 0 in Chrome.
8. Actions as typed methods: `scroll(dx, dy)`, `scroll_to(axis, fraction)`, `move_focus(axis, step, forward, extend)`, `activate`, `begin_edit`, `cancel`, `select_all`, `type_text(text)`.
9. `edit_started(row, col)`, `edit_ended(commit)`.
10. `set_size(axis, key, size)` (column widths at start) and `set_header(axis, thickness)` (fitting the row-number band to the rows shown, measured with `measureText`).

Not needed: `notify`, `located`, `sort_applied` (the huge identity source never moves Keys), `set_window_sized`, `set_focus`.

## Observations

Frame time is JS time from frame start through `paint`, plus a forced `offsetWidth` read so the DOM painter's style and layout are counted; GPU raster and composite are not. Headless Chromium through Playwright on an M-series Mac, 1400x850 CSS px, about 34 rows x 7 columns visible.

| Variant | DPR | slow scroll (37 px/frame) median / p95 | fast scroll (600 px/frame) median / p95 | DOM nodes |
| --- | --- | --- | --- | --- |
| A canvas + native | 1 | 0.8 / 1.2 ms | 0.9 / 1.7 ms | 19 |
| B DOM + native | 1 | 1.2 / 1.9 ms | 4.1 / 5.8 ms | 403 to 713 |
| C canvas + drawn | 1 | 0.7 / 1.3 ms | 1.2 / 2.1 ms | 17 |
| A canvas + native | 2 | 0.7 / 1.2 ms | 1.1 / 1.9 ms | 19 |
| B DOM + native | 2 | 1.3 / 2.3 ms | 4.8 / 8.5 ms | 403 to 713 |
| C canvas + drawn | 2 | 0.7 / 1.2 ms | 1.0 / 1.6 ms | 17 |

Real wheel input (60 Playwright wheel ticks): A 0.7 / 1.2 ms, B 1.9 / 3.2 ms, C 0.8 / 1.3 ms. All held 60 fps; the widget drew nothing while idle.

- **Canvas versus DOM.** Canvas cost barely moves with scroll speed; DOM is fine for slow scrolls but costs four to five times more on fast ones, when every visible Key pair is new each frame and reuse by Key only helps by pooling. The DOM painter's pool also grows to about twice the visible cells (713 nodes) after a fast scroll. The DOM's wins are free text selection, find-in-page and accessibility; the canvas needs the hidden ARIA layer for those. Both painters take exactly the same `paint(snapshot, content)` call, so the interface held up.
- **Hybrid mapping works, with one real trap.** Small deltas go in as `Scroll` (pixel exact at any depth); jumps beyond 1.5 viewports and both ends go in as `ScrollTo`. On a 10^15-row table the Thumb fraction does not move by a pixel for any plausible wheel distance, so the writeback pins `scrollTop` and the native thumb stands still while content scrolls. That is Glide's behaviour and felt fine, but it means **the writeback must not run over an unprocessed scroll**: a scroll offset that changed after the last `scroll` event but before the frame was silently overwritten. The frame now consumes any pending offset before writing back.
- **The intercepted wheel (C) does not scroll like the native scroller.** For the same wheel tick Chromium scrolled the native scroller 120 px but reported `deltaY` 240 to the wheel listener, so C scrolled twice as far. A custom scrollbar has to own wheel scaling (lines, pages, devices); the native scroller gets it, plus momentum and macOS overlay bars, for free. C's dragged thumb and track paging worked first time and give exact control at the ends.
- **Draining Effects only at the next frame drops keys.** With F2 followed by fast typing, the typed keys arrived before the frame that drained `EditRequested`, became `TypeText`, and the edit then opened over them. Draining Effects synchronously after each key Action (as well as at frame start) fixed it. Related: `TextTyped` names no cell, so the host must track focus from `FocusChanged` in drain order; the last Snapshot's focus can be stale.
- **One textarea at `focus_rect` works as the key and IME target.** A CDP-driven composition (`k`, then `か`, committed as `海`) sent `compositionstart`, `BeginEdit`, `EditRequested`, `edit_started`, and the same textarea grew into the editor at `editor_rect` with the composition still running; nothing was lost. Not yet tried with a real OS IME, where the candidate window position is the thing to check.
- **Text baseline gotcha (canvas).** `textBaseline` set inside a `save()`/`restore()` pair silently reverts; headers drew a few pixels high until it was set once per paint.
- **Row-number band fitting** with `measureText` on the visible row labels each frame (as the ratatui demo does) was cheap and needed a second `begin_frame` and Window fill in the same frame.
- Things not judged here: real trackpad momentum on macOS (headless has no overlay scrollbars to drag), Firefox's lower sizer ceiling, and resizing the window quickly by hand.

## Verdict

_For the user to fill in after trying it: which drawing mode first, which scroller, and whether the textarea design holds._
