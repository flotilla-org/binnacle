# PROTOTYPE: a grouped and a pivoted table in both adapters

Throwaway. For [Prototype: a grouped and a pivoted table in both adapters](https://github.com/flotilla-org/binnacle/issues/186) on [Map: hierarchical axes](https://github.com/flotilla-org/binnacle/issues/177). It covers both adapters: this folder is the web half, and the ratatui half is `crates/binnacle-ratatui/src/bin/prototype-hierarchical-axes/main.rs`. Delete both once the answer is captured.

## The question

How should a grouped and a pivoted table look and feel on the web and in a terminal? That covers disclosure marks and indents, how Summary Items stand out, stacked header tiers with spans that scroll and pin, the corner grid, and the keys and menu that drive it all. The model is settled by #181 to #185: the `grouping` part of the View Spec, an outline per item, outline and header forms per axis, spans merged by ancestor Key, the corner grid, `expand: { depth, except }` and pending toggles. This prototype only shows it.

Neither half touches the core. Each fakes the Grid and the Table Source: the source recomputes items, outlines and spans from the spec on every change, from about 400 generated shipments (vessel, region, port, quarter, cargo, weight, crew).

## How to run

Web (port 6860):

```sh
cd web && pnpm install && pnpm prototype:hierarchy
```

Then open http://127.0.0.1:6860/demo/PROTOTYPE-hierarchical-axes/. The striped bar at the top is prototype chrome: preset (Grouped, Pivot), rows in outline or header form, corner layout (Tiers, Compact), "Pin 3 columns left" and the Theme. The URL takes the same settings: `?preset=pivot&form=header&corner=compact&pin=3&theme=mono`. The pane on the right prints the `grouping` spec (with `expand` for each axis), the focused row's outline (kind, depth, parent Key, ancestor Keys, open state, field and value) and any pending toggles after every action.

Ratatui:

```sh
cargo run -p binnacle-ratatui --bin prototype-hierarchical-axes            # add -- --pivot to start on the pivot
cargo run -p binnacle-ratatui --bin prototype-hierarchical-axes -- --dump  # scripted scenes from a TestBackend
```

`--dump` takes `--scene <prefix>`, `--width`, `--height`, and `--state 1` to include the state pane. Harness keys: F2 or Ctrl+G switches the preset, F3 or Ctrl+F the row form, F4 or Ctrl+K the corner layout, F5 or Ctrl+X pins three columns left, F6 or Ctrl+T cycles the Theme, F7 or Ctrl+E shows the state pane, and `q` quits.

Keys, the same in both:

| Key | Does |
|---|---|
| Arrows | Move focus (by Key on each axis, so it survives toggles). In a header-form level column a span is one cell. |
| Right | In the outline column (or a level column): opens a closed group; on an open group, steps in (outline form: to its first child; header form: to the next level's column); elsewhere, moves right |
| Left | Closes an open group; otherwise steps out to the parent Group Item (outline form) or the parent level's span (header form) |
| `*` | Opens everything under the focused group |
| `1` to `3`, `0` | Expand to that level (`ExpandTo(n)`: sets `depth` and clears `except`); `0` collapses everything |
| Enter | Toggles the focused group |
| `m`, or right-click a header | The grouping menu |
| Click on ▸ / ▾ | Toggles that group (row or column) |

Every toggle waits about 450 ms for the fake source. Meanwhile the mark pulses on the web and shows `⋯` in the terminal. A second toggle before the answer supersedes the first. Bulk changes (`*`, digits) wait the same and are answered as `Refiltered`.

## What it shows

- **Grouped preset:** rows by region, then port, in outline form. Leaves are shown, a Subtotal follows each open group, and the Grand Total is pinned at the bottom. Fields used as row levels leave the body, since the band already shows them. As in Excel, an open Group Item with a Subtotal after it stays blank and a closed one carries the aggregates. The web adds the group's count beside its label when `count` is a value. North and Hull start closed so both cases show.
- **Pivot preset:** region, then port, down the side; quarters across; the values `sum(weight)` and `count`. The column Header Band therefore has two tiers: the quarter spans its two value columns. The "All" columns (the column Grand Total) are pinned right. Ports are the deepest level and can't open, so they carry no mark.
- **Header form for rows:** one band column per row level. Each group is one cell spanning its rows, and its label stays at the top of the visible part while scrolling. A closed group keeps its Subtotal under the span, with ▸ on it.
- **Spans and pins:** column spans keep their label in view while scrolling sideways. "Pin 3 columns left" pins Q1's two columns and Q2's first, so the Q2 span breaks in two at the pinned edge, with each part labelled.
- **Column collapse:** add a second column level from the menu (pivot, then "Add cargo as a column level") and the outer spans get disclosure marks. A closed span shows its "Q1 total" under it, as in Excel.
- **Theme roles:** the prototype adds `groupItem`, `subtotal` and `grandTotal` (the last with a rule and a stronger fill where the total row meets the total column), plus a disclosure colour, defined for each preset (light, Catppuccin Mocha, mono).
- **The corner grid, two layouts:**
  - **Tiers:** the values caption on top at the left; each column level's name right-aligned in its own tier row, with an arrow; row level names along the bottom row, one per band column in header form, or joined with › in outline form. The values pseudo-level is `Σ values`. Every name opens that level's menu.
  - **Compact (Excel):** row levels share one indented column headed "Row labels ▾", the values caption sits top-left, and an extra row over the body reads "Column labels: quarter". It forces outline form.
- **The grouping menu:** from a base column: group rows by this, add as a column level, add a value (sum, avg, min, max, count). From a pivot column or a corner level name: remove the level, move it to the other axis, collapse to it or expand through it, remove a value, and group by the fields not in use. Changing levels prunes `except`, since old Keys no longer fit (#185).

## Questions it raises

Each comes with a suggestion, clearly marked; none is decided.

1. **The tiers corner gets crowded.** Its bottom row must hold the row level names and the bottom tier's own name. With two values that reads "region › port Σ values →", and with one value and one column level, the caption, the column level and the row levels all compete for one row. *Suggestion:* put the values pseudo-level's name in the top row with the caption, and let the corner take an extra row only when a column level would otherwise share the bottom row.
2. **Right in the outline column.** By the tree convention, Right on an open group steps to its first child, so from the outline column the only way into the body is a leaf, a Subtotal or a group that can't open. ARIA's treegrid moves into the row's cells instead. *Suggestion:* keep the tree rule, which is what the map asked for, and let Tab leave the outline column for the body.
3. **Keys for column groups.** Column spans toggle by mouse only. *Suggestion:* let focus enter the header tiers, then use the same Right and Left there, with `*` and the digits acting on the column axis.
4. **The label of an open group's Subtotal in header form.** At the deepest level (port, with leaves) the Subtotal row sits inside the port span and has no label of its own on the web; ratatui writes "Felixstowe total" in the band. *Suggestion:* label it in the band, as ratatui does: a row of numbers with no name reads like one more leaf.
5. **Open Group Items stay blank** (Excel's rule when a Subtotal follows). That leaves a row of empty cells on every open group. *Suggestion:* keep it blank, as Excel does, but show the count beside the label (the web does): it names the group's size without repeating the Subtotal.
6. **A span part cut off by the viewport or a pinned edge has no mark that it continues.** *Suggestion:* a `›` (terminal) or a faded edge (web) on the side that continues, in the loading style.
7. **Grouped fields leave the body.** Both halves drop region and port from the base columns while they are row levels. *Suggestion:* keep that, and say so in the column chooser (the field shows as "grouped").
8. **Pending marks.** A pulsing mark (web) or `⋯` (terminal) is easy to miss on a fast source and fine on a slow one. *Suggestion:* draw the pending mark only after about 150 ms, as the shimmer on a pending sort should.
9. **Where a Group Item's label sits.** Both halves put it in the row Header Band: the outline column, or a level column. The other choice was the first body column. *Suggestion:* the band, since it stays put when the body scrolls sideways and keeps the body's columns meaning one field each.

## Verdict

_To fill in once both halves have been tried._
