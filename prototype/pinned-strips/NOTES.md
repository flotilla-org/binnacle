# PROTOTYPE: pinned Strips in the core

Throwaway model for [Prototype: frozen regions in the core](https://github.com/flotilla-org/binnacle/issues/137), on the map [Map: column presentation](https://github.com/flotilla-org/binnacle/issues/131). Delete it once the open questions below are settled. It is its own Cargo workspace, so it never joins the binnacle build.

## The question

How do Key-pinned Strips work in the core? The decisions from [Grilling: the freezing model](https://github.com/flotilla-org/binnacle/issues/135) and [Grilling: the hiding model](https://github.com/flotilla-org/binnacle/issues/134) are taken as given: two pinned Key lists per axis (leading and trailing) in pin order, held by the Grid; pinned Strips draw from Keys; the scrolling Strip skips pinned Keys using the Window; pins of filtered or hidden items go dormant through `Locate` after `Refiltered`; too wide clips from the inside edge and never unpins; Header Bands stay outermost. The prototype pushes that model through the cases that are hard to reason about on paper and offers both answers, behind a toggle, where the decisions leave a choice open.

## Running it

From the repository root:

```sh
cargo run --manifest-path prototype/pinned-strips/Cargo.toml             # interactive
cargo run --manifest-path prototype/pinned-strips/Cargo.toml -- --walk   # the walkthrough below, printed
```

Interactive mode redraws the whole frame after each typed command. The help line at the bottom lists the commands; an ITEM is a column name (`id name city age email phone score team joined notes total flag`) or a row (`r1` to `r30`), and the item's name picks the axis.

Each frame shows:

- the viewport picture, with an x ruler and a region ruler (`h` Header Band, `L` leading, `B` body, `T` trailing, `.` nothing) along both axes. In `--walk` output, selected cells are upper case and the focused cell starts with `>`; interactively, pinned cells are cyan, selected cells reversed, focus underlined. A header ending in `?` is a pin whose `Locate` is pending;
- per axis: the anchor, the Window, the scroll extent (with the true figure beside it when a pending pin makes it wrong), each Strip with its Keys, Positions, wanted size, region and slide, and what the range covers in drawn order with how many runs it draws as;
- focus (with `NOT VISIBLE` when it is on a clipped or scrolled-away item), the range corners, the `Locate`s out and the toggles.

`model.rs` is the pure part (no terminal code); `main.rs` is the throwaway shell. Shortcuts the core would not take are marked `SHORTCUT` in `model.rs`: whole-cell sizes and no Gap; the source's order read directly where the core would request a Window and wait a frame; focus and range corners relocated at once after `Refiltered` (only pins go through a visible pending `Locate`).

## Walkthrough

`--walk` runs these steps. Columns 72 wide with a 5-wide row-number band and a minimum scrolling region of 10; rows 12 tall with a 1-tall column header band and a minimum of 3.

1. **Pin and reorder.** `pin name l`, `pin city l`, `pin total t`, `mv city 0`. The leading Strip is `[city, name]` in pin order, the trailing `[total]` sits flush with the far edge, and the body reads `id age email ...`: it walks Positions from the anchor and skips pinned Keys found in the Window. The scroll extent drops by each pin's width (96, 86, 78, 71).
2. **Scroll.** `s 20` moves only the body's anchor (`@4 +10`); pinned Strips stay. The Window starts two before the anchor and is long enough for the visible items plus one Position per drawn pin, since each skipped Key costs a slot.
3. **Hit-testing.** `hit 8 3`, `hit 40 3`, `hit 68 3` land in the leading Strip, the body and the trailing Strip, each answering a Key, its Position and the offset within it.
4. **Arrow keys.** Focus `name`, the last leading pin. In Drawn order `l` goes to `id`, the first scrolling item, and the body scrolls back to show it; `h` returns to `name`. In Source order (`t focus`) `l` from `name` (Position 1) goes to `city` (Position 2), which is drawn to its left, then to `age`.
5. **A range across regions.** From `city` (a leading pin) extend right three times. In Source range order the corners `city` and `age` cover Positions 2 to 3, so `name` and `id`, drawn between them, are not selected: two runs on screen. `t range` switches to Drawn order: `city name id age`, one run. Interactively (after `reset`), `pin email`, `pin age t`, `f r2 name`, `L`, `L` makes a range from `name` to `phone`, which in Source order selects `email | name city phone | age`: three runs, one in each region.
6. **Too wide.** `w city 50` asks for 60 + 7 with 57 allowed. Clip-first Leading clips `name` (the inside end of the leading Strip) and keeps `total`; `t clip` keeps leading and clips `total` whole and `name` in part. Nothing unpins.
7. **Focus on a clipped pin.** `f r2 name` with clipped-reveal Nothing: focus is on an item nobody can see. `t reveal` (Slide) slides the leading Strip inside its region to show `name`, hiding most of `city`. The slide is worked out each frame, so resizing `city` back drops it.
8. **Filter removes a pinned Key.** `filter cols` drops columns with an `o`, `total` among them. Every pin goes pending and a `Locate` goes out for each; `total?` is still drawn and still subtracted from the extent (29 where the truth is 36). `aa` answers: `city` and `name` Found, `total` Gone, so it goes dormant, leaves the trailing Strip (the body takes the space) and keeps its Key. `filter cols` again asks about the dormant pin too, and `aa` brings `total` back in its slot.
9. **Hide a pinned item.** `hide name`: focus was on `name` and moves to the suggested neighbour `city`; the pin goes pending, then dormant. `unhide name` and `aa` put it back after `city`, in pin order.
10. **Freeze up to here.** After unpinning, `freeze age` pins `id name city age` in their current order. `freeze name` on a leading pin unpins those after it; `city` and `age` return to the body before the anchor, off screen, and focus on `city` is out of view.
11. **Rows.** `pin r5 l`, `pin r30 t`, then `sort rows reverse` (a `Refiltered`): both pins pending at stale Positions and not moved on screen; `aa` gives `r5@25` and `r30@0`. `r30` is now Position 0 and still drawn at the bottom: pin order ignores the sort.

## What the Snapshot would carry

Per axis, three Strips (leading, body, trailing), each with its region along the axis, and per item its Key, its Position when known (a pending pin may not have one), its unclipped offset and size, and the part left visible after clipping or sliding. The host crosses the two axes' Strips into up to nine regions, plus the Header Bands outside them. The scroll extent and thumb describe the body only. Hit-testing answers a region with the Key, so a press on a pinned item never needs its Position.

## Open questions

Each answer below is a **suggestion**, not a decision.

1. **Arrow-key order** (`t focus`). Drawn order steps through what is on screen: leading pins, the scrolling items by Position, trailing pins. Source order steps by Position, so focus jumps into and out of the pinned Strips.
   **Suggestion:** Drawn order. Source order sends focus backwards on screen, which reads as a bug. Leaving the last leading pin goes to the first scrolling item and scrolls the body back to it (AG Grid does the same); going to the first *visible* scrolling item instead is the alternative, and worth trying in the TUI before choosing.
2. **What the one range is contiguous in** (`t range`). Today's `CellRange` is Positions between the corners. With pins it draws as up to three runs per axis, can select pinned items far from either corner, and skips items drawn between the corners. Drawn order always draws one run, but the range becomes drawn indices: worked out from counts and the pins' Positions (cheap, pins are few, as `drawn_index` shows), then translated back for copy, fill and any Effect that names cells.
   **Suggestion:** Drawn order. The range is a visual gesture, and three disjoint runs from one drag will surprise users. Copy then emits pinned items first, in drawn order.
3. **"Leading before trailing"** (`t clip`). The decision reads as "leading is clipped first", which the prototype defaults to. It can also be read as "leading keeps its space first".
   **Suggestion:** confirm which was meant; I would keep leading and clip trailing first, since leading pins are usually the identifying columns and are what "freeze up to here" builds.
4. **Focus on a clipped pin** (`t reveal`). With nothing done, keyboard focus sits on an invisible item.
   **Suggestion:** Slide. The pinned Strip slides inside its region only while focus is on one of its clipped items, recomputed each frame, never stored; leaving the item drops it.
5. **Trailing order and placement.** The trailing list is drawn left to right in pin order, so the first trailing pin is the innermost and a new one lands at the outer edge. The trailing Strip sits flush with the viewport's far edge, leaving a blank between it and a short body (step 8).
   **Suggestion:** keep both: one rule (list order is drawn order) for both Strips, and fixed regions keep hit-testing simple.
6. **Where an unpinned item lands.** It returns to its Position, which may be before the anchor and off screen (step 10).
   **Suggestion:** keep the anchor, and let the existing reveal-focus rule bring a focused unpinned item into view; unpinning many items should not scroll the view.
7. **"Freeze up to here" on a huge axis or on a pin.** On a scrolling item it needs the Keys of every Position before it, which may be outside the Window; on a leading pin the prototype unpins the pins after it.
   **Suggestion:** cap it at what fits the viewport less the minimum scrolling region (anything past that would be clipped anyway), fetch those Keys with a Window request, and keep "on a pin, unpin the ones after it" so the action toggles cleanly.
8. **Stale pin Positions while pending.** After a reorder the pins' last Positions are wrong until `Locate` answers. Inside the Window the body skips by Key and is right; drawn indices, the end clamp and the extent use the stale Positions, and a pending pin that has gone leaves the extent off by its size.
   **Suggestion:** skip by Key wherever the Window holds the Position, accept the stale figures elsewhere for the frame or two, and treat a Window fill that shows a pending or dormant pin's Key as its answer (not built here), so pins near the view settle without waiting for `Locate`.
9. **The Window's length.** It must grow by one Position per drawn pin, or the body comes up short when pinned Keys fall inside it.
   **Suggestion:** add the drawn pin count, as here; pins are few, so over-fetching is harmless.

## Verdict

To be filled in after the user has driven it.
