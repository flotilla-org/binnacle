# Wheelhouse provider shape: how a table core would slot in

Research ticket: how flotilla-org/wheelhouse consumes Rust libraries over a C ABI, how its immediate-mode UI does lists and scrolling, what ADR 0010 implies for scroll-offset ownership, and what a table core's C boundary must therefore look like.

Sources: a read-only shallow clone of wheelhouse (paths below are relative to its root, `path:line`), plus shallow clones of the sibling repos at these commits: cleat `ee9058e`, andamento `f8aaf40`, jackstay `5fc30cc`. Link prefixes used below:

- `[cleat.h]` = https://github.com/flotilla-org/cleat/blob/ee9058e/crates/cleat/include/cleat_provider.h
- `[andamento.h]` = https://github.com/flotilla-org/andamento/blob/f8aaf40/crates/andamento-ffi/include/andamento.h
- `[ring.h]` = https://github.com/flotilla-org/jackstay/blob/5fc30cc/crates/jackstay/include/jackstay_ring.h

## Summary of constraints

1. **Pull, not push.** Every Rust library is polled once per UI frame for an immutable, versioned snapshot. Callbacks are only edge-triggered "wake" nudges, callable from any thread, that post a UI wakeup.
2. **Opaque handles, POD structs, explicit acquire/release**, `{ptr,len}` UTF-8 text, and an ABI version checked at compile time that grows additively.
3. **Single owner thread.** Calls are serialized on the UI thread; Rust may run workers internally but reaches the host only via wake.
4. **The content owns scroll position (ADR 0010).** The scroll region only lays out bars and returns *requested* positions; the core holds the authoritative offset, takes absolute or delta requests, and reports `{position, range, visible}` in its snapshot.
5. **Mixed units by design.** Layout in F32 pixels; scroll positions as `S64 index + fraction` in content units (rows for lists and terminals, pixels for free-form content).
6. **Snapshot-scoped action identity** (Andamento): clicks dispatch against the snapshot that was displayed.

The numbered constraints at the end expand these.

## C ABI patterns in use

### Build and linking

`build.sh` builds each sibling Rust crate with `cargo build` and links the resulting shared library with an rpath, adding the crate's `include/` to the C include path: cleat at `build.sh:31-49` (`-lcleat`, header dir `crates/cleat/include`), jackstay at `build.sh:52-60` (`-ljackstay`, `-DWHEELHOUSE_JACKSTAY=1`), andamento at `build.sh:62-74` (`-landamento_ffi -lwheelhouse_ingress`, header dir `crates/andamento-ffi/include`). Headers are hand-written C (no cbindgen marker was seen in the files read). Wheelhouse also ships its own Rust staticlib, `src/ingress/` (`lib.rs` plus a 36-line `ingress.h`).

ABI versioning is enforced at compile time: `src/uishell/uishell_terminal_provider.h:8-10` fails the build unless `CLEAT_PROVIDER_ABI_VERSION == 11u`. Cleat also exposes `cleat_provider_abi_version()` at runtime and takes `abi_version` in `cleat_provider_desc` ([cleat.h] L12, L158-159, L538). Andamento states "ABI 2 ... Additive symbols preserve ABI 2; statically linked hosts pin a library revision, while dynamic hosts can use dlsym to probe optional symbols" and "Tags have uint32_t storage; do not use C enum size assumptions" ([andamento.h] L15-18). Cleat's newer structs carry a leading `size_t size` field for forward-compatible extension (for example `cleat_render_update`, `cleat_render_row`, [cleat.h] L359-394, L460-479).

### Terminal Provider ABI (cleat)

CONTEXT.md defines the **Terminal Provider ABI** as "a C-callable interface used by UIShell to create, drive, and render terminal sessions" and the **Terminal Snapshot** as "a coherent pull-based view of terminal cells, cursor state, and damage", explicitly *avoiding* "Callback event" (`CONTEXT.md:46-52`). Resolved decisions: "Terminal providers may poll PTYs or transports on background threads. Resolved: UIShell consumes pull-based Terminal Snapshots with dirty or damage reporting" (`CONTEXT.md:408`), and input is structured events, with protocol encoding on the provider side (`CONTEXT.md:409-410`).

Shape of the ABI ([cleat.h]):

- **Handles:** opaque `cleat_provider` and `cleat_session` (L141-142); `cleat_provider_open/close` (L540, L549); `cleat_session_create/attach/destroy` (L562-570). Descriptors are caller-owned structs passed by pointer; strings in them are borrowed `ptr,len` (L158-219).
- **Wake:** `cleat_provider_set_wake_callback` is "an edge-triggered provider wake callback ... a scheduling nudge, not a rendering callback ... may be invoked ... from provider-owned IO threads. Keep it non-blocking and bounce to the session-owner/UI thread before calling session APIs" (L541-548). Wheelhouse's callback only calls `wm_send_wakeup_event()` (`src/uishell/uishell_views.c:3686-3691`).
- **Dirty and versioning:** `cleat_session_poll` pumps IO and returns `cleat_dirty_state` (CLEAN, PARTIAL, FULL; L146-150, L648-653), "Call this from the session-owner thread". Each snapshot or update carries a `uint64_t render_generation` (L320, L470); `cleat_session_mark_observed(session, generation)` acknowledges it, and "Snapshots do not clear dirty state by themselves" (L658-662).
- **Snapshot vs render update:** `cleat_session_snapshot` returns a full grid with `dirty_rows` populated only for PARTIAL damage (L674-685, struct L312-327). "Only one snapshot may be live per session"; release with `cleat_session_release_snapshot` (L674-677, L745). Daemon sessions return false for snapshots and must use `cleat_session_render_update`, an operation list (`FULL_VISIBLE_REPLACE`, `ROW_REPLACE`, `SCROLL_COPY`, L126-128, L382-394) that is "versioned, operation-shaped" (L686-693); "Scrolling currently falls back to full visible replacement" (L690-691). All pointers in it are borrowed until `cleat_session_release_render_update` (L746; see the lifetime note on hyperlink bytes at L350-352).
- **Damage:** row granularity only (`dirty_rows`, `cleat_render_row.dirty`, L323-324, L379); feature bits advertise `CELL_SNAPSHOTS`, `DAMAGE_ROWS`, `RENDER_UPDATES` (L119-124).
- **Input:** one flat tagged `cleat_input_event` struct covering key, text, mouse, focus, paste, resize (L31-36, L486-511). Mouse events carry both `cell_col/cell_row` ("authoritative terminal-cell coordinates") and `x_px/y_px` relative to the content area (L481-485). Batch and sequence-numbered variants exist (L643-646, L513-516).
- **Scrolling:** provider-owned. `cleat_terminal_scrollbar_state {viewport_kind, total_rows, viewport_rows, viewport_top_row, at_bottom}` (L293-299) rides in every snapshot and update; the host moves it with `cleat_session_scroll_viewport` and a `cleat_viewport_command {TOP | BOTTOM | DELTA_ROWS, delta_rows}` returning `MOVED | NO_OP | UNSUPPORTED` (L113-118, L529-536, L670-672).

How wheelhouse drives it, once per frame on the UI thread (`src/uishell/uishell_views.c:4511-4530`): `cleat_session_poll`; if not clean (or no cache yet) and admitted, `cleat_session_render_update`, then `uishell_terminal_apply_update` copies the borrowed data into a wheelhouse-owned, arena-backed `UIShell_TerminalCellCache` (`src/uishell/uishell_terminal_glyph.c:5445-5468`), then `mark_observed(render_generation)` and `release_render_update`. Drawing reads only the cache. So the borrowed snapshot never outlives the frame; wheelhouse keeps its own copy.

### andamento-ffi (sidebar core)

[andamento.h] is the closest analogue to a table core: a Rust model producing a tree of rows for an immediate-mode renderer.

- **Threading and errors:** "Serialize calls on a sidebar. All input buffers/arrays are borrowed for the call only ... Mutations return 1 on success, 0 on error. Optional error_out ... receives an owned NUL-terminated error; free with string_free ... A panic poisons the sidebar; destroy/recreate it" (L20-31, `andamento_string_free` L264).
- **Snapshot ownership:** `andamento_snapshot_acquire` returns an owned `AndamentoSnapshot*` that "owns all returned text. It survives sidebar mutation/destruction" until `andamento_snapshot_release` (L89-96, L103, L236). Rows are read by index through getters filling caller structs (`andamento_snapshot_node_count`, `andamento_snapshot_node`, L127-128); nodes are preorder with parent indices and opaque stable keys (L92-96, L105-114).
- **Versioning:** no exposed counter; `andamento_snapshot_is_current(core, snapshot)` answers staleness and "does not build a snapshot. Check after draining a batch; acquire only when stale" (L97-102). Wheelhouse does exactly this in `uishell_sidebar_refresh` (`src/uishell/uishell_sidebar.c:478-491`) and releases the old snapshot only after invalidating derived caches (`:467-476`).
- **Actions:** "Action references belong to one snapshot ... Dispatch captured clicks against their displayed snapshot BEFORE draining queued facts ... never replay old coordinates or reinterpret an old action index against a new snapshot" (L228-235). Host side effects come back through a drained queue, `andamento_effects_take` (L238-263), not callbacks.
- **Scroll intent is host-owned:** controls `ANDAMENTO_CONTROL_SCROLL_DOWN/UP` have "action NONE; their tagged intent belongs to the host (e.g. scrolling). The renderer decides geometry" (L118-121).

ADR 0012 (accepted, not implemented) generalizes this: "Andamento owns all logical state ... Wheelhouse renders from a typed read model built from Andamento's snapshot and sends it edit operations. It never writes the logical tree", while RAD's config keeps only Presentation State (`docs/adr/0012-andamento-owns-logical-state.md:22-43`), and "their ABI is versioned" (`:54`).

### wheelhouse_ingress (wheelhouse's own Rust lib)

`src/ingress/ingress.h:9-12,30`: "wake runs on the transport thread; it must only signal the UI event loop. poll and stop run exclusively on the UI thread. poll borrows patch bytes for each callback"; "No host pointers cross to the worker". A data callback appears only synchronously inside a UI-thread `poll`.

### jackstay (pixel streams)

Wheelhouse does not call jackstay's raw ABI from UI code. It wraps it in a C module, `src/jackstay/wheelhouse_jackstay.{h,c}`, whose header says "Thread-safe session owner ... Returned pixels are caller-owned. CPU leases stay on the worker; a D3D11 frame for import moves to the caller, who hands it back with wh_js_frame_release" (`wheelhouse_jackstay.h:5-10`). The wrapper owns a mutex and two worker threads (`wheelhouse_jackstay.c:36-38`) and calls `wake` after state changes (`:60-63`). `wh_js_snapshot(session, state, frame, after)` copies state under the mutex and returns a freshly `malloc`ed RGBA copy only if `frame_version != after` (`wheelhouse_jackstay.c:812-838`); the UI passes its last seen version (`src/uishell/uishell_jackstay.c:367`). Input is queued with bounded capacity (`wh_js_send`, `wheelhouse_jackstay.c:784-811`). `wh_js_destroy` "only succeeds after both workers finish; never blocks a GUI thread" and is retried each frame (`wheelhouse_jackstay.h:91-92`, `uishell_jackstay.c:243,306`). Underneath, jackstay's own contract is a shared-memory seqlock ring, single producer, many readers ([ring.h] L1-40).

## Immediate-mode list and scroll machinery

`UI_ScrollPt` (`src/ui/ui_core.h:267-274`) is the position type: `S64 idx`, `F32 off` (animation displacement), `F64 target_off` (persistent fraction), `F64 remainder` (unapplied input under snapping). `ui_scroll_pt_scroll` treats `delta` as "in the content's units (rows for lists, coordinates for pixel views)" and keeps "the large index out of floating-point arithmetic" (`src/ui/ui_core.c:378-416`); `ui_scroll_pt_clamp_idx` bounds it to an inclusive range (`:354-369`).

`UI_ScrollList` (`src/ui/ui_basic_widgets.h:192`, implementation `ui_basic_widgets.c:1421-1585`) is a virtualizing list:

- **Params** (`ui_basic_widgets.h:58-69`): `dim_px`, a single `row_height_px`, `row_blocks`, `Rng2S64 cursor_range`, `Rng1S64 item_range`, `cursor_min_is_empty_selection[2]`, `snap_scroll`, and flags `Nav` and `Snap` (`:10-16`).
- **Visible window out:** it computes `num_possible_visible_rows = dim_px.y/row_height_px` (`ui_basic_widgets.c:1426`) and returns `visible_row_range_out` (plus two rows of slack) for the caller to build only those rows (`:1552-1557`). The watch view, for instance, fetches just that window from its row source (`src/uishell/uishell_views.c:2172-2179`).
- **Row blocks:** `UI_ScrollListRowBlock {row_count, item_count}` (`ui_basic_widgets.h:18-23`) is a run-length map from items to rows. `ui_scroll_list_row_from_item` assumes rows are spread evenly over a block's items (`row_count/item_count`, `ui_basic_widgets.c:1182-1204`). This is the only variable-height mechanism; there is no per-row pixel height.
- **Cursor ranges:** the caller owns `Vec2S64 cursor` and `mark`; keyboard events (char, word/line/page, whole) move the cursor inside `cursor_range`, wrapping at the ends for char steps, and the mark follows unless `KeepMark` (`:1430-1486`). `cursor_min_is_empty_selection` reserves the minimum index as "nothing selected", and the Snap code treats `cursor.y-1` as the item index (`:1497`), so the cursor's y is effectively 1-based.
- **Snapping:** with `Snap` and a moved cursor, it nudges `scroll_pt` so the cursor's row range is visible (`:1495-1521`). `snap_scroll` is separate: it snaps *precise wheel* movement to whole rows, keeping `remainder` (`ui_core.c:384-414`).
- **Scroll application:** `UI_ScrollList` builds a `ui_scroll_region` with X `Off` and Y `Always` (`ui_basic_widgets.c:1541-1548`), applies the region's requested position, converts the fractional part to a pixel `view_off` (`:1551`), and in `ui_scroll_list_end` turns precise pixel wheel deltas into rows (`scroll_px.y/row_height_px`) and line wheel deltas into whole rows (`:1570-1583`). There is no horizontal axis in `UI_ScrollList`.

For two-axis content the pattern is to use `ui_scroll_region` directly: the scroll region fixture lays out a pixel grid with both axes `Auto`, owns `state->position` as a `UI_ScrollPt2` in pixels, applies wheel deltas itself and writes `view_off` (`src/uishell/uishell_views.c:6115-6143`).

## ADR 0010 and scroll ownership

`docs/adr/0010-scroll-regions-own-scrollbar-layout-and-interaction.md`:

- Layout and build are separate: `ui_scroll_region_layout` "is a pure calculation" so content can measure against the viewport (a terminal "needs it before choosing its cell grid") and then call `ui_scroll_region_build` with scroll metrics (`:10-14`; API at `src/ui/ui_scroll_region.h:56-65`).
- "Layout coordinates are pixels relative to the parent. Scroll metrics use the caller's units: an inclusive range of legal positions and a visible extent, expressed in rows or pixels as appropriate" (`:14-16`). `UI_ScrollRegionAxis {UI_ScrollPt position; Rng1S64 range /* inclusive legal positions, NOT the content's index range */; S64 visible}` (`ui_scroll_region.h:41-47`).
- **Ownership:** "The content owns its authoritative position, wheel events, keyboard navigation, and offset animation ... A virtualized list applies a row position to its list state. A terminal sends a viewport command to Cleat and reads back the backend's next snapshot. The region does not consume wheel events or translate the terminal canvas" (`:40-44`). `UI_ScrollRegionSignal.position` is documented as "requested positions; content owns their application" (`ui_scroll_region.h:49-54`).
- Thumb length is `visible / (legal travel + visible)` (`:46-47`), so the content must report both numbers honestly.
- Axis policy `Always` is for "content whose measurement depends on the viewport"; `Auto` takes a pixel `content_dim_px` (`:18-24`, `ui_scroll_region.h:7-15,23`).

The terminal is the worked example of a *remote* owner (`src/uishell/uishell_views.c:4330-4342`): it reads `viewport_top_row`, `total_rows`, `viewport_rows` from the cached Cleat scrollbar state, builds `UI_ScrollRegionAxis{ui_scroll_pt(top_row,0), [0, total-visible], visible}`, and if the region's requested `idx` differs, sends the delta as `CLEAT_VIEWPORT_COMMAND_DELTA_ROWS` (`uishell_terminal_scroll_rows`, `:264-274`). Wheel input bypasses the region: precise deltas are divided by cell size and accumulated in `local_scroll_rows`, sending whole rows only, and the accumulator is dropped on `NO_OP` so no "overscroll debt" builds at a bound (`:272-300`). The displayed offset only changes when the next render update arrives.

Implication for the table core: it should own the authoritative scroll offset (like Cleat), expose it in each snapshot in the `{position, inclusive range, visible}` form, and accept requests as absolute or delta commands with a moved/no-op result. Wheelhouse keeps a thin, per-frame, not-authoritative copy only for drawing.

## Units

- **Layout:** F32 pixels, relative to the parent (`ADR 0010:14`, `ui_scroll_region.h:20`). Row heights are computed in the host from font metrics, for example `floor_f32(ui_bottom_font_size()*2.4f)` or `main_font_size*1.45f` (`src/uishell/uishell_views.c:2745,3412`). DPI scaling happens in the host's font and window layer (`wm_dpi_from_window`, `src/window_manager/window_manager.h:274`); I did not trace exactly where it enters UI pixel sizes, so treat "pixels" as host-scaled and unconfirmed as physical or logical.
- **Scroll positions:** `S64` index plus fractional F32/F64 offset, in content units: rows for `UI_ScrollList` and terminals, pixels for the fixture grid (`ui_core.c:378`, `uishell_views.c:6123-6142`).
- **Cleat:** grid sizes `uint16_t` cols and rows; scrollback `uint64_t` rows (clamped to `S64` by wheelhouse, `uishell_views.c:4331-4335`); cell pixel size and content rect as `float` px (`[cleat.h]` L181-184, L284-299); mouse in both cells and px (L481-485).
- **Andamento:** indices are `size_t` with `ANDAMENTO_NONE = SIZE_MAX`; tags `uint32_t`; times are host monotonic milliseconds (`[andamento.h]` L13, L18, L63-65).
- **Text:** UTF-8 `{ptr,len}`, may contain NUL (`[andamento.h]` L12, L21; `cleat_str` `[cleat.h]` L152-156). Terminal graphemes are `uint32_t` codepoint arrays per cell (L266-273).

## Threading

The UI is a single-threaded immediate-mode loop. All three Rust boundaries follow the same rule: library work may happen on library-owned threads, but every API call that touches session or model state is made from the UI/owner thread, serialized ("Serialize calls on a sidebar", `[andamento.h]` L20; "Call this from the session-owner thread", `[cleat.h]` L651; "poll and stop run exclusively on the UI thread", `ingress.h:10`). Background threads communicate only by wake (`wm_send_wakeup_event`, `uishell_views.c:3686-3691`, `uishell_jackstay.c:38`). Where a library is inherently multi-threaded (jackstay), wheelhouse adds a mutex-guarded C wrapper that hands the UI a copied, versioned snapshot (`wheelhouse_jackstay.c:812-838`). Destruction must not block the UI thread (`wheelhouse_jackstay.h:91`). Andamento's details API is explicitly "Not thread-safe with concurrent reads/requests/release of this snapshot" (`[andamento.h]` L219), so snapshots are not assumed shareable across threads.

## Constraints for the table core's C boundary

1. **Hand-written C header, versioned.** Ship `include/<name>.h` with a `#define <NAME>_ABI_VERSION`, a runtime `<name>_abi_version()`, `uint32_t` tag storage (no C enums in structs), and additive evolution; size-prefix extensible structs as Cleat does. Build as a cdylib wheelhouse links via `build.sh` with `-I<crate>/include -l<name> -rpath`.
2. **Opaque handles, explicit lifetimes.** `create/destroy` for the model or view; every snapshot is acquired and released explicitly. Pick one documented lifetime: borrowed until release with at most one live snapshot (Cleat), or owned and surviving mutation (Andamento). Andamento's survives-mutation model fits a table better, because the host may keep the displayed snapshot to validate clicks.
3. **Pull-based, generation-stamped snapshots.** Expose a cheap `is_current` or `dirty` query and a monotonically increasing `uint64_t generation` per snapshot so the UI acquires only when stale; optionally `mark_observed`. Never push row data through callbacks.
4. **Windowed row access, not whole-table copies.** The UI asks only for `visible_row_range` (plus slack). The snapshot or a query must serve a row window (`first_row`, `count`) and per-cell display data by index, with `{ptr,len}` UTF-8 text owned by the snapshot.
5. **Damage at row (and optionally column) granularity.** Report CLEAN, PARTIAL, FULL plus dirty row ranges, mirroring `cleat_dirty_state` and `dirty_rows`, so wheelhouse can keep its own arena cache and patch it.
6. **The core owns the scroll offset.** Each snapshot carries per-axis `{position (S64 idx + fractional), inclusive legal range, visible extent}` ready to drop into `UI_ScrollRegionAxis`. Requests come in as commands (absolute, delta, to-top, to-bottom, reveal-cell) returning MOVED, NO_OP, or UNSUPPORTED, so the host can drop accumulated wheel input at bounds.
7. **Units declared per axis.** Vertical: rows (matching `UI_ScrollList` and terminals); horizontal: pixels (or columns), since `UI_ScrollList` has no X axis and the 2D fixture scrolls in pixels. Variable row heights cannot use `UI_ScrollList` row blocks except as uniform runs, so either expose rows in uniform-height blocks or provide a pixel-unit vertical axis plus a row-from-pixel lookup. Pixel inputs (viewport size, row height, column widths) come from the host, which measures fonts.
8. **Layout-first handshake.** The host calls a pure `ui_scroll_region_layout`, then tells the core the viewport (in px and in visible rows/cols, like `cleat_session_resize` plus `update_geometry`) before acquiring the snapshot for that frame.
9. **Structured input, core-side semantics.** Accept a flat tagged input event (key, text, mouse with both cell and px coordinates, wheel, focus) or narrower commands; selection, cursor and mark semantics (`cursor_range`, `KeepMark`, empty-selection row) should live in the core, with cursor, mark and selection reported in the snapshot.
10. **Snapshot-scoped action identity.** Hit-test results (cell, header, sort, resize handle) resolve to indices or keys valid for one snapshot; dispatch is validated against the displayed snapshot and stale actions are rejected (Andamento L228-235). Side effects the host must perform (open, copy, edit commit) come back through a drained effect queue, not callbacks.
11. **Threading contract.** All calls serialized on one owner thread. Any background work (sorting, filtering, data fetching) stays inside Rust and signals only through an optional `wake(void*)` that may fire from any thread and must only post a UI wakeup. Destroy must not block the UI thread (or offer a try-destroy).
12. **Errors and panics.** Return 0 or 1 status with an optional owned error string and a matching `*_string_free`; catch panics at the boundary and poison the handle rather than unwinding into C.
13. **Stable row and column keys.** Opaque byte keys stable across snapshots (Andamento L94-96) so the host can keep presentation state, such as column widths or collapse, keyed outside the core, consistent with ADR 0012's logical-versus-presentation split.

Unconfirmed: whether any of these headers are generated (none of the files read say so); exact DPI handling of UI pixel sizes; whether wheelhouse plans a generic "content provider" C ABI beyond the per-library headers (CONTEXT.md names Content Provider as a concept but I found no shared C interface for it).
