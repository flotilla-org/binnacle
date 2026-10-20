# PROTOTYPE: column presentation controls and the pinned edge (web)

Throwaway. For [Prototype: column presentation controls and the pinned edge](https://github.com/flotilla-org/binnacle/issues/140) on [Map: column presentation](https://github.com/flotilla-org/binnacle/issues/131). Delete this folder (and the `prototype:columns` script in `web/package.json`) once the answer is captured.

## The question

What should the column presentation controls look and feel like on the web, and how is the pinned edge drawn? It builds on the hiding model (#134), Key pinning (#135), moves across the frozen edge (#136) and the pinned Strips prototype (#137), and extends the find and filter controls' look (#109).

## How to run

```sh
cd web
pnpm install
pnpm prototype:columns
```

Then open <http://127.0.0.1:6400/demo/PROTOTYPE-column-controls/?variant=A> (or `B`, `C`; `&theme=mocha` or `mono` too). No wasm build is needed: the grid is a DOM fake with sticky Strips, and the Table Source is a fake, since pinning, hiding and moves are not in the core yet.

Flip variants with the floating bar or the left and right arrow keys. The striped strip at the top is prototype chrome: the theme, each axis's sort mode (unsorted, sorted where a move bakes the order, sorted where the source withdraws "movable"), "refuse next move", "pin 6 more left" (to see clipping) and reset. The dark pane on the right prints the whole state on every change: pinned lists in pin order, dormant pins, hidden Keys, Size Overrides, how much of each pinned Strip is drawn, picks, pending moves and the last action.

In every variant: right-click a header (or its button) for the menu; Ctrl, Cmd or Shift with a click picks several headers, and the menu then acts on the picks; drag a column header or a row number to pin, unpin, reorder pins or move. Moves take 1.2 seconds to answer, so the pending shimmer shows.

## The variants

| | A: Spreadsheet | B: Panel | C: Palette |
|---|---|---|---|
| Menu | Flat list: Hide, "Show hidden ▸" (a flyout with search), Pin left, Pin right, Unpin, Freeze up to here, Choose columns… | A pin switch (Left, None, Right), Freeze and Hide buttons, the hidden items as chips, Show all | A command palette: type to narrow; "Show Built" is just another command |
| Chooser | The find and filter checklist with search: a tick shows, an untick hides; pinned and dormant tags | A drawer holding a small grid of the axis's items: #, Name, Shown, Pinned (L, –, R), Width (a Size Override, kept while hidden); its headers sort it | The items grouped by where they draw (Pinned left, Scrolling, Pinned right, Hidden), each with the buttons that move it between groups |
| Count | Status line under the grid: "27 of 30 columns · 40 of 40 rows" | A bar above the grid, where the filter bar sits; each count opens the chooser | The corner over the row numbers: "27/30", which opens the chooser |
| Pinned edge | A line in the border colour, and a shadow once the body has scrolled under it | Same, with a small pin glyph in pinned headers | A 2px line, never a shadow |
| Drag feedback | Drop line in the drop indicator colour with a pill saying what a drop does | The pinned Strips (or empty edge gutters) light up as drop zones while dragging | A ghost of the header follows the pointer, captioned with what a drop does |

All three share the rules from #136: dropping into a pinned Strip pins at that point (the Grid's own, so it works on an unmovable axis); within a Strip it reorders the pins; out of one it unpins, and also moves where the axis can, else the item returns to its own Position and a dashed line shows where that is; within the body it moves where the axis can, else the drop is refused ("Columns can't move while sorted: drag to an edge to pin"). A pending move shimmers the header, one request goes at a time, and a refusal shows a message under the header and moves nothing.

Rows get the same menu (Pin top, Pin bottom), the same chooser and the same drags. Hiding more than 5 rows at once is refused with "use a filter instead" (#134's cap).

## Open questions

Each has a suggestion, clearly marked; none is decided.

1. **Which menu?** A lists exactly the spec's items; B is quicker for pinning but hides "Freeze up to here" among buttons; C scales to many hidden items but needs typing. *Suggestion:* A's flat items, with "Show hidden ▸" as a flyout that gains a search past about eight items. C's palette is a later, optional part.
2. **Which chooser?** The checklist reuses the find and filter parts and works for rows unchanged. The grid chooser is the only one that shows widths and pin state together, and the only one where a kept Size Override of a hidden column is visible. *Suggestion:* ship the checklist as the chooser; leave the grid chooser to hosts, who can build it as a binnacle Grid over the axis's items (it needs nothing new from the core).
3. **Where does "12 of 30 columns" sit?** *Suggestion:* the find and filter rule from #109: in the filter bar when one is mounted (B), else in the status area under the grid (A). The corner (C) runs out of room past three digits and reads as a row number.
4. **Dropping into an empty pinned Strip.** There is nothing to drop on, so this prototype gives each edge a 26px zone. A shows only a line at the edge, which is hard to discover; B lights the zones. *Suggestion:* B's zones, shown only while a drag is in progress.
5. **The drop caption.** A's pill beside the line, or C's captioned ghost? A ghost of the moving header is [#78](https://github.com/flotilla-org/binnacle/issues/78), out of scope here. *Suggestion:* A's pill now; move the caption onto the ghost when #78 lands.
6. **The edge colour.** The Theme's `border` is faint enough to read as one more column rule, so this prototype uses a darker shade. *Suggestion:* a `pinnedEdge` Theme role, defaulting to `border`, so a host can strengthen it.
7. **The heavy line (C).** It reads as a resize handle. *Suggestion:* the 1px line with the scroll shadow (A), as specified.
8. **Marking a clipped pinned Strip.** A fade at the inside edge, and a "‹ 3" badge in the header. *Suggestion:* keep the fade and drop the count; the menu's "Unpin" (marked "clipped") is the way out, as #135 requires.
9. **Unpinning onto a movable axis, refused.** Here the item stays unpinned at its own Position when the move is refused. *Suggestion:* confirm that; the alternative (re-pin it) would undo a Grid change because of a source answer.
10. **Freeze up to here on a trailing pinned item** is left out of the menu. *Suggestion:* keep it out; it pins from the leading edge only.

## Verdict

_To fill in once the variants have been tried._
