# PROTOTYPE: column presentation controls and the pinned edge (ratatui)

Throwaway. For [Prototype: column presentation controls and the pinned edge](https://github.com/flotilla-org/binnacle/issues/140) on [Map: column presentation](https://github.com/flotilla-org/binnacle/issues/131). Delete this folder once the answer is captured.

## The question

What should the column presentation controls look and feel like in a terminal, how is the pinned edge drawn, and which keys reach hide, pin and the chooser in each keymap preset? It builds on #134 (hiding), #135 (Key pinning), #136 (moves across the edge), #137 (pinned Strips) and the find and filter controls' presets (#109).

## How to run

```sh
cargo run -p binnacle-ratatui --bin prototype-column-controls -- --variant a --preset chords
```

`--variant` takes `a`, `b` or `c`; `--preset` takes `chords`, `modes` or `bare-keys`. `--dump` prints scripted scenes for every variant from a TestBackend instead (`--scene menu`, `--width 120` narrow it), which is how the drawings below were checked.

The grid is painted straight into the buffer and the Table Source is a fake: pinning, hiding and moves are not in the core yet. The yellow line at the top is prototype chrome, on Ctrl keys no preset uses: Ctrl+V (or F12) the variant, Ctrl+P the preset, Ctrl+B the Border Style, Ctrl+T the Theme, Ctrl+E and Ctrl+Y each axis's sort mode (unsorted, sorted where a move bakes, sorted where "movable" is withdrawn), Ctrl+R refuse the next move, Ctrl+X pin past what fits (to see clipping), Ctrl+S the state pane, Ctrl+Q quit. The state pane on the right prints both axes on every change: pinned lists in pin order, dormant pins, hidden Keys, how much of each pinned Strip is drawn, the Scroll Anchor, pending moves and the last action.

Arrows move focus in drawn order across pinned and scrolling items (#137); focus on a clipped pinned item slides its Strip. With the mouse: click focuses, right-click a header opens its menu, dragging a column header or row number pins, unpins, reorders pins or moves, and the wheel scrolls (Shift for columns).

## The variants

| | A | B | C |
|---|---|---|---|
| Menu | A popup under the header with mnemonics (`h` Hide, `s` Show hidden ▸ opening a list beside it, `<` `>` pin, `u` Unpin, `f` Freeze up to here, `c` chooser) | A which-key line in the status line: `Port: h hide  s show hidden (3)  < pin left …`; `s` swaps in the hidden list, numbered | The popup with the hidden items inline under the actions, numbered, and "a Show all" |
| Chooser | The checklist with search: typing searches at once, Space ticks, pinned and dormant tags | A column sheet down the right (VisiData's `C`): Name, Shown, Pin, Width; Space, `<` `>` `u`, `+` `-` width, Tab sorts it, `/` searches | Grouped by where items draw: Pinned left, Scrolling, Pinned right, Hidden |
| Count | Status line: `27 of 30 columns · 40 of 40 rows` | A bar above the grid, where the filter bar sits | The corner over the row numbers: `27/30` |
| Column edge, with vertical rules | `║` in the edge Gap; the header keeps its `│` handle; `╥` where it meets the band rule, `╨` at Grid's closing rule | `║` through the header too, `╫` at the band rule | `║` in the body only, the band rule crossing left as `┼` |
| Column edge, without vertical rules (None, Headers) | `║` anyway | `┊` | No glyph: pinned cells are tinted |
| Row edge | A `═` rule under the top Strip (over the bottom one), costing a line, `╬` where it crosses the column edge and `╪` across rules | The last top-pinned row underlined | Tinted without vertical rules; nothing with them |
| Clipped Strip | `›` (leading) or `‹` (trailing) at its inside edge in the header; `▾` or `▴` in the row-number band for rows | same | same |

Drag feedback is the same in all three: a `┃` drop line in the Gap in the drop indicator's style (dashed `┇` where an unpinned item will return to its own Position on an unmovable axis), and the status line saying what a drop does ("Pin left", "Reorder the pins", "Unpin and move here", "Unpin: returns to its place (off screen)", or in the error style "Columns can't move while sorted: drag to an edge to pin"). Rows have no Gap, so their line is `━━━━▶` in the row-number band beside the row it lands above. A pending move shimmers its header; a refusal shows in the status line and moves nothing.

Under a double Border Style every rule is already `║`, so the edge goes heavy (`┃`, with `┰` and `┸` caps).

## Keys by preset

Every function key has a second binding (macOS sends media keys for the top row). Shift with a column binding gives the row one where the key allows.

| Control | Chords | Modes | Bare keys |
|---|---|---|---|
| Hide column / row | Alt+0 / Alt+9 (Excel's Ctrl+0 and Ctrl+9; terminals drop Ctrl with a digit) | `-` (VisiData), or `z` then `h` / `Z` then `h` | `-` / `_` |
| Show hidden list | Alt+) / Alt+( (Excel's Ctrl+Shift+0 and 9) | `z s` / `Z s` | `+` / `*` |
| Pin left, right | Alt+, / Alt+. | `z <` / `z >`; `!` pins left or unpins (VisiData's key column) | `<` / `>` |
| Pin top, bottom | Alt+< / Alt+> | `Z <` / `Z >` | `[` / `]` |
| Unpin | Alt+U / Alt+Shift+U | `z u` / `Z u` | `u` / `U` |
| Freeze up to here | Alt+W / Alt+Shift+W (Excel's Alt, W, F) | `z f` / `Z f` | `F` / `T` |
| Menu | Shift+F10 or Alt+M / Alt+Shift+M | `z m` / `Z m` | `.` / `,` |
| Chooser | Alt+O / Alt+Shift+O | `C`, or `z c` / `Z c` | `c` / `C` |

In the modes preset `z` (vim's view prefix) enters a COLUMNS mode and `Z` a ROWS mode, shown in reverse on the keys line; the mode stays until Esc, Tab swaps the axis, and the arrows still move focus inside it. None of these collide with the find and filter presets' keys.

## Open questions

Each has a suggestion, clearly marked; none is decided.

1. **Caps in the header.** A keeps the header's `│` resize handle and starts the double line at the band rule (`╥`); B runs `║` through the header (`╫`). *Suggestion:* A. The header Gap is where a resize grabs, and a cap where the rules meet is what the ticket describes.
2. **The mark without vertical rules.** `║` anyway (A), `┊` (B) or a tint (C). The tint fails in the mono Theme and under zebra stripes; `┊` is easy to miss. *Suggestion:* A: the edge is always drawn in its Gap, whatever the Border Style.
3. **The row edge.** A spends a line on `═`; B's underline costs nothing but collides with the mono Theme, which already uses underline for the selection and picks. *Suggestion:* A's `═` rule, drawn only while rows are pinned.
4. **Double Border Styles.** `║` is already every rule there. *Suggestion:* the heavy `┃` edge, as drawn here, rather than a new glyph set.
5. **The scroll shadow** has no terminal twin. *Suggestion:* none; the edge glyph is enough.
6. **Clip marks** overwrite the first visible character at the inside edge. *Suggestion:* keep them in the header and row-number band only, in the loading style, never in the body.
7. **Which menu.** The popup (A) suits chords and the mouse; the which-key line (B) suits the modes and bare-keys presets, where keys already act directly. *Suggestion:* the popup for every preset, with C's inline hidden list in place of A's second popup (one less level in a terminal).
8. **Which chooser.** *Suggestion:* the checklist (A) as the default, since it is the find and filter checklist; the column sheet (B) as an optional part for hosts that want widths, matching the web suggestion.
9. **Where the count sits.** *Suggestion:* the status line (A), moving to the conditions line when one is shown, as the row count does today. The corner (C) is five cells wide.
10. **Alt chords on macOS** need "Option as Meta" in the terminal, as the find and filter chords already do. *Suggestion:* note it with the presets; the modes and bare-keys presets need nothing.

## Verdict

_To fill in once the variants have been tried._
