/*
 * binnacle's internal C API, for wheelhouse. It mirrors the Rust Grid only as
 * far as wheelhouse needs and makes no stability promise: the header and
 * library change together, and BINNACLE_ABI_VERSION says which header a
 * library was built with. The Rust docs on binnacle-core describe each
 * behaviour in full.
 *
 * Calls on one Grid are serialised by the host. Input arrays are borrowed for
 * the call only; a NULL array requires zero length. Tags and booleans are
 * uint32_t, not C enums or bool. A call given a NULL Grid or output, or an
 * unknown tag or flag bit, does nothing and returns 0; otherwise calls return
 * 1. Nothing a call fills needs releasing: output arrays point into buffers
 * the Grid owns. Nothing calls back into the host: Effects are polled. A
 * panic aborts the process.
 */

#ifndef BINNACLE_H
#define BINNACLE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* The version of the ABI this header describes. A host checks at startup
 * that binnacle_abi_version() returns it, and refuses a library that
 * doesn't. Any change to this header outside its comments bumps it by one:
 * a struct's fields or layout, a constant's value, a function's signature,
 * or a declaration added or removed. binnacle-ffi's tests/abi.rs fails until
 * the version is bumped and the new header pinned. */
#define BINNACLE_ABI_VERSION 18u

/* The BINNACLE_ABI_VERSION of the header the library was built with. */
uint32_t binnacle_abi_version(void);

/* An axis, also the index into per-axis arrays. */
#define BINNACLE_ROWS 0
#define BINNACLE_COLS 1

/* An edge of an axis items can be pinned to: the leading edge is the left of
 * the columns and the top of the rows, the trailing edge their right and
 * bottom. */
#define BINNACLE_EDGE_LEADING 0
#define BINNACLE_EDGE_TRAILING 1

/* BinnacleSnapshot.damage bits. */
#define BINNACLE_DAMAGE_LAYOUT 1u
#define BINNACLE_DAMAGE_SELECTION 2u
#define BINNACLE_DAMAGE_POSITIONS 4u

/* BinnacleSnapshot.cell_states bits. */
#define BINNACLE_CELL_FOCUSED 1u
/* Inside the range, or in a picked row or column. */
#define BINNACLE_CELL_SELECTED 2u
/* The current find match, drawn apart from the other matches. */
#define BINNACLE_CELL_CURRENT_MATCH 4u
/* The cell the host's editor is open on. */
#define BINNACLE_CELL_EDITING 8u

/* BinnacleHeader.flags bits. SORTED: the item is in the other axis's sort in
 * the applied View Spec, DESCENDING says which way and sort_place where (0
 * first). PENDING: a requested View Spec that would change this item's part
 * of that sort is unanswered. SELECTED: the
 * whole item is selected (the range spans every item across it, or it is
 * picked). HOVERED: the pointer is over it. FILTERED: the applied View
 * Spec has a condition read in this item; FILTER_PENDING: a requested View
 * Spec that would change those conditions is unanswered. */
#define BINNACLE_HEADER_SORTED 1u
#define BINNACLE_HEADER_DESCENDING 2u
#define BINNACLE_HEADER_PENDING 4u
#define BINNACLE_HEADER_SELECTED 8u
#define BINNACLE_HEADER_HOVERED 16u
#define BINNACLE_HEADER_FILTERED 32u
#define BINNACLE_HEADER_FILTER_PENDING 64u
/* FILTER_NOT_APPLIED: no answer is due, and a condition read in this item
 * that the user asked for is left out of the applied View Spec. */
#define BINNACLE_HEADER_FILTER_NOT_APPLIED 128u
/* MOVE_PENDING: a move of this item waits for binnacle_grid_move_answered:
 * draw it pending, like a sort, while the order stays as it is. */
#define BINNACLE_HEADER_MOVE_PENDING 256u

/* BinnacleSnapshot.mode. Modes the Grid gained after this header read as
 * BINNACLE_MODE_OTHER. */
#define BINNACLE_MODE_OTHER 0
#define BINNACLE_MODE_IDLE 1
#define BINNACLE_MODE_DRAGGING_SELECTION 2
/* The edge of a Header Band is being dragged; the band's axis snapshot shows
 * its thickness as it follows the pointer. */
#define BINNACLE_MODE_RESIZING_BAND 3
/* Headers are being dragged: see has_drop and pin_zones for where a release
 * would drop them. With no drop, the release does nothing (unpinned items
 * over the body of an axis that can't move). */
#define BINNACLE_MODE_MOVING_HEADERS 4
/* The host's editor is open on the cell with edit_row and edit_col
 * (binnacle_grid_edit_started): place it over editor_rect. */
#define BINNACLE_MODE_EDITING 5

/* BinnacleSnapshot.drop_target: what releasing the dragged headers would do.
 * MOVE: request a move to before drop_position (the count for the end),
 * unpinning the dragged item first if drop_unpin. UNPIN: unpin the dragged
 * item, which returns to its own Position, drop_position (the axis can't
 * move). PIN: pin the dragged items to drop_edge, drawn before the pinned
 * drop_before if has_drop_before, else after the last pin there; drop_reorder
 * when they are pinned there already. REFUSED: releasing does nothing, as the
 * Grid will not ask to move the dragged items to before drop_position, for
 * the reason drop_refusal (BINNACLE_MOVE_REFUSAL_*): show it as a caption. */
#define BINNACLE_DROP_MOVE 0
#define BINNACLE_DROP_UNPIN 1
#define BINNACLE_DROP_PIN 2
#define BINNACLE_DROP_REFUSED 3

/* BinnacleSnapshot.drop_refusal, for BINNACLE_DROP_REFUSED: BETWEEN_GROUPS,
 * the drop lies outside the Leaves' group ("Can't move between groups");
 * SUMMARY, a Group Item, Subtotal or Grand Total is dragged. OTHER for
 * reasons added after this header. */
#define BINNACLE_MOVE_REFUSAL_OTHER 0
#define BINNACLE_MOVE_REFUSAL_BETWEEN_GROUPS 1
#define BINNACLE_MOVE_REFUSAL_SUMMARY 2

/* BinnacleEffect.kind. Effects the Grid gained after this header arrive as
 * BINNACLE_EFFECT_OTHER: skip them. */
#define BINNACLE_EFFECT_OTHER 0
#define BINNACLE_EFFECT_TEXT_TYPED 1
#define BINNACLE_EFFECT_FOCUS_CHANGED 2
#define BINNACLE_EFFECT_SELECTION_CHANGED 3
/* Answer with binnacle_grid_view_applied, then tell the Grid how the axes
 * changed with binnacle_grid_notify. */
#define BINNACLE_EFFECT_VIEW_REQUESTED 4
/* Answer with binnacle_grid_located. */
#define BINNACLE_EFFECT_LOCATE 5
/* A drag of the edge of the Header Band leading axis ended at size. */
#define BINNACLE_EFFECT_BAND_SIZE_CHANGED 6
/* The edge of the Header Band leading axis was double-clicked: measure its
 * content and answer with binnacle_grid_set_header. */
#define BINNACLE_EFFECT_BAND_AUTO_SIZE_REQUESTED 7
/* The Find Query changed (has_find 1, find) or find closed (has_find 0):
 * fetch the marks for the cells shown, and the count, again. */
#define BINNACLE_EFFECT_FIND_REQUESTED 8
/* Step from the cell from (none when has_from is 0: from the start) to the
 * next or previous match of find, in direction (BINNACLE_FIND_NEXT or
 * PREVIOUS), wrapping. Answer each with binnacle_grid_find_stepped, in
 * order, or binnacle_grid_find_stepped_within for a match inside groups. */
#define BINNACLE_EFFECT_FIND_STEP_REQUESTED 9
/* The user dropped the headers of the items with keys on axis before the
 * item at Position to (the count for the end), Positions as they were before
 * the move. The Grid reorders nothing: move them and tell the Grid with
 * binnacle_grid_notify, or refuse; either way answer with
 * binnacle_grid_move_answered. No other move starts until then. */
#define BINNACLE_EFFECT_MOVE_REQUESTED 10
/* The Keys pinned to edge (BINNACLE_EDGE_*) of axis changed to keys, in pin
 * order, dormant pins included: by a pin, an unpin, a freeze or
 * binnacle_grid_set_pins. A host that saves layouts keeps them and restores
 * them with binnacle_grid_set_pins. At most one per edge is queued. */
#define BINNACLE_EFFECT_PINS_CHANGED 11
/* The user activated the cell with row and col (Enter, or a double-click):
 * open it, edit it or ignore it. On a Summary Item's cell has_drill_down is
 * 1 and drill_down lists the conditions that pick out the base rows under
 * it: the grouping's slicers, then a ONE_OF per group on the row's and the
 * column's paths, outermost first. */
#define BINNACLE_EFFECT_ACTIVATED 12
/* The user asked to edit the cell with row and col (F2): open an editor on
 * it and call binnacle_grid_edit_started, or ignore it. */
#define BINNACLE_EFFECT_EDIT_REQUESTED 13
/* The Grid ended the edit of the cell with row and col itself (its Key left,
 * the user cancelled, or a press elsewhere): drop the editor uncommitted. */
#define BINNACLE_EFFECT_EDIT_CANCELLED 14
/* A resize of the item with key on axis ended at size, its new Size
 * Override. A host that saves layouts restores it with
 * binnacle_grid_set_size. */
#define BINNACLE_EFFECT_SIZE_CHANGED 15
/* The border after the item with key on axis was double-clicked: measure
 * its content and answer with binnacle_grid_set_size. */
#define BINNACLE_EFFECT_AUTO_SIZE_REQUESTED 16
/* The user asked to copy (binnacle_grid_copy): the cells where the items at
 * copy_runs[BINNACLE_ROWS] cross those at copy_runs[BINNACLE_COLS], each
 * runs of Positions in drawn order, as shown (a collapsed group's items are
 * at no Position, so they are left out). copy_labels[axis] 1: the axis is
 * grouped, and its labels (a Group Item's value, without indent) lead each
 * copied row, or head each column. Fill the clipboard, or copy something
 * else instead. */
#define BINNACLE_EFFECT_COPY_REQUESTED 17

/* binnacle_grid_edit_ended: the host kept the new value, or threw it away. */
#define BINNACLE_EDIT_COMMIT 0
#define BINNACLE_EDIT_CANCEL 1

/* BinnacleEffect.direction for FIND_STEP_REQUESTED. */
#define BINNACLE_FIND_NEXT 0
#define BINNACLE_FIND_PREVIOUS 1

/* binnacle_grid_set_find_support bits and BinnacleSnapshot.find_support: the
 * Table Source finds, and takes regexes. 0 is none, the default. REGEX needs
 * FIND. */
#define BINNACLE_FIND 1u
#define BINNACLE_FIND_REGEX 2u

/* BinnacleSortColumn.direction. Nulls go last either way. */
#define BINNACLE_SORT_ASCENDING 0
#define BINNACLE_SORT_DESCENDING 1

/* BinnacleViewEdit.kind. Edits the Grid gained after this header read as
 * BINNACLE_VIEW_EDIT_OTHER. CYCLE_SORT: a header click cycled the sort of
 * axis by key, an item of the other axis; multi is 1 for a Shift-click,
 * which kept the rest of the sort. */
#define BINNACLE_VIEW_EDIT_OTHER 0
#define BINNACLE_VIEW_EDIT_CYCLE_SORT 1
/* The quick filter of axis was set or cleared: the spec has it. */
#define BINNACLE_VIEW_EDIT_SET_QUICK_FILTER 2
/* The host asked for a whole View Spec (binnacle_grid_request_view). */
#define BINNACLE_VIEW_EDIT_REPLACE 3
/* Items were shown or hidden on axis and a quick filter searches the shown
 * ones: the same spec, to apply again. */
#define BINNACLE_VIEW_EDIT_SHOWN_CHANGED 4
/* A condition on axis, read in item key, was added, removed (the first equal
 * one), or put in place of another (key is the new one's). */
#define BINNACLE_VIEW_EDIT_ADD_CONDITION 5
#define BINNACLE_VIEW_EDIT_REMOVE_CONDITION 6
#define BINNACLE_VIEW_EDIT_REPLACE_CONDITION 7
/* The user asked for the same spec to be applied again
 * (binnacle_grid_reapply_view), so rows their edits left unmatched are
 * filtered. */
#define BINNACLE_VIEW_EDIT_REAPPLY 8
/* The items of axis with keys (keys_len of them) were hidden, after those
 * hidden already, or shown again. */
#define BINNACLE_VIEW_EDIT_HIDE 9
#define BINNACLE_VIEW_EDIT_SHOW 10
/* axis was grouped by key, a base field, as its innermost level (a plain
 * table becomes grouped, counting each group), or every level of axis was
 * removed (with none left on either axis and no slicers, the table is plain
 * again). */
#define BINNACLE_VIEW_EDIT_GROUP 11
#define BINNACLE_VIEW_EDIT_UNGROUP 12
/* The group of axis headed by the Group Item key was opened or closed: the
 * Key joins or leaves the axis's expand except list. */
#define BINNACLE_VIEW_EDIT_EXPAND 13
#define BINNACLE_VIEW_EDIT_COLLAPSE 14
/* The group headed by key and every group under it the Grid knows of were
 * opened. */
#define BINNACLE_VIEW_EDIT_EXPAND_ALL 15
/* axis was opened to expand_depth levels (every level when expand_all is
 * 1) and its exceptions cleared. */
#define BINNACLE_VIEW_EDIT_EXPAND_TO 16
/* A slicer, read in base field key, was added, removed (the first equal
 * one; with no slicers and no levels left the table is plain again), or put
 * in place of another (key is the new one's). Adding one to a plain table
 * makes a grouping with no levels, counting. */
#define BINNACLE_VIEW_EDIT_ADD_SLICER 17
#define BINNACLE_VIEW_EDIT_REMOVE_SLICER 18
#define BINNACLE_VIEW_EDIT_REPLACE_SLICER 19
/* axis is drawn in form (BINNACLE_FORM_*). */
#define BINNACLE_VIEW_EDIT_SET_FORM 20
/* The level of axis named by level (its kind and field) was removed (with
 * no level left on either axis and no slicer, the table is plain again), or
 * moved to the other axis as its innermost field level. */
#define BINNACLE_VIEW_EDIT_REMOVE_LEVEL 21
#define BINNACLE_VIEW_EDIT_MOVE_LEVEL 22
/* value joined the values, after the others, or left them. */
#define BINNACLE_VIEW_EDIT_ADD_VALUE 23
#define BINNACLE_VIEW_EDIT_REMOVE_VALUE 24

/* BinnacleCondition.kind. Kinds the Grid gained after this header read as
 * BINNACLE_CONDITION_OTHER, and are refused as input. TEXT: the value
 * matches the condition's text query. ONE_OF: the value is one of the
 * condition's values exactly (none keeps nothing). COMPARE: the value, a
 * number or a date, compares with the condition's scalars as its compare
 * says; a blank, a NaN or a value of the other kind never does. NATIVE: an
 * opaque condition in the Table Source's own syntax (a SQL fragment, say),
 * the text of the condition's text query; binnacle never reads it. */
#define BINNACLE_CONDITION_OTHER 0
#define BINNACLE_CONDITION_TEXT 1
#define BINNACLE_CONDITION_ONE_OF 2
#define BINNACLE_CONDITION_COMPARE 3
#define BINNACLE_CONDITION_NATIVE 4

/* BinnacleCondition.compare: the value is equal to, not equal to, less
 * than, at most, greater than or at least scalar, or BETWEEN scalar and
 * high, both ends included. */
#define BINNACLE_COMPARE_EQUAL 0
#define BINNACLE_COMPARE_NOT_EQUAL 1
#define BINNACLE_COMPARE_LESS 2
#define BINNACLE_COMPARE_AT_MOST 3
#define BINNACLE_COMPARE_GREATER 4
#define BINNACLE_COMPARE_AT_LEAST 5
#define BINNACLE_COMPARE_BETWEEN 6

/* BinnacleScalar.kind: a number, or a date in milliseconds since the Unix
 * epoch (UTC; a date without a time of day is its midnight). */
#define BINNACLE_SCALAR_NUMBER 0
#define BINNACLE_SCALAR_DATE 1

/* BinnacleListedCondition.state, and that of a listed level or value:
 * applied and still requested, requested and not yet applied, applied and
 * no longer requested, or requested and left out of the applied View Spec
 * with no answer due (rejected, failed, no longer or never supported). */
#define BINNACLE_CONDITION_APPLIED 0
#define BINNACLE_CONDITION_ADDING 1
#define BINNACLE_CONDITION_REMOVING 2
#define BINNACLE_CONDITION_NOT_APPLIED 3

/* BinnacleTextQuery.flags bits. Without CASE_SENSITIVE, case is ignored;
 * with REGEX, the text is a pattern in the Table Source's own syntax. */
#define BINNACLE_TEXT_CASE_SENSITIVE 1u
#define BINNACLE_TEXT_REGEX 2u

/* BinnacleTextQuery.searched: every item of the other axis the Table Source
 * shows (the Grid asks again when they change), or the keys given. */
#define BINNACLE_SEARCHED_SHOWN 0
#define BINNACLE_SEARCHED_KEYS 1

/* binnacle_grid_set_support bits and BinnacleHeader.support. SORT: the axis
 * may be sorted by the item. TEXT: it may be filtered by Text conditions on
 * the item, and with TEXT_REGEX they take regexes. REGEX needs TEXT. ONE_OF:
 * it may be filtered by OneOf conditions, the Table Source listing the
 * item's distinct values. COMPARE_NUMBERS or COMPARE_DATES (not both): it
 * may be filtered by COMPARE conditions on numbers, or on dates. NATIVE: by
 * NATIVE conditions. GROUP: it may be grouped by the item, a base field (a
 * level of the axis). PIVOT: the other axis may be grouped by the item's
 * values (a level of the other axis). SUBTOTAL_BEFORE and SUBTOTAL_AFTER:
 * where a level grouping by the item may place its Subtotals, beyond off.
 * Slicers on the item take the condition kinds the axis's filter does. */
#define BINNACLE_SUPPORT_SORT 1u
#define BINNACLE_SUPPORT_TEXT 2u
#define BINNACLE_SUPPORT_TEXT_REGEX 4u
#define BINNACLE_SUPPORT_ONE_OF 8u
#define BINNACLE_SUPPORT_COMPARE_NUMBERS 16u
#define BINNACLE_SUPPORT_COMPARE_DATES 32u
#define BINNACLE_SUPPORT_GROUP 64u
#define BINNACLE_SUPPORT_NATIVE 128u
#define BINNACLE_SUPPORT_PIVOT 256u
#define BINNACLE_SUPPORT_SUBTOTAL_BEFORE 512u
#define BINNACLE_SUPPORT_SUBTOTAL_AFTER 1024u

/* binnacle_grid_set_quick_filter_support bits and
 * BinnacleAxisSnapshot.quick_filter: the axis has a quick filter, and it
 * takes regexes. 0 is none, the default. REGEX needs QUICK_FILTER. */
#define BINNACLE_QUICK_FILTER 1u
#define BINNACLE_QUICK_FILTER_REGEX 2u

/* Where a Subtotal or a Grand Total goes: nowhere, before its group (or the
 * axis), or after it. */
#define BINNACLE_PLACEMENT_OFF 0
#define BINNACLE_PLACEMENT_BEFORE 1
#define BINNACLE_PLACEMENT_AFTER 2

/* BinnacleLevel.kind. FIELD: groups the axis by the values of a base field.
 * VALUES: the values pseudo-level, one item per value where several values
 * are shown. Levels the Grid gained after this header read as
 * BINNACLE_LEVEL_OTHER, and are refused as input. */
#define BINNACLE_LEVEL_FIELD 0
#define BINNACLE_LEVEL_VALUES 1
#define BINNACLE_LEVEL_OTHER 2

/* How a grouped axis is drawn: each group a Group Item at its own Position,
 * or each level a tier of the cross Header Band. */
#define BINNACLE_FORM_OUTLINE 0
#define BINNACLE_FORM_HEADER 1

/* BinnacleOutline.kind: a Leaf from the data, or a Summary Item: a Group
 * Item heading a group, a Subtotal aggregating one group, or the Grand
 * Total aggregating the axis. */
#define BINNACLE_ITEM_LEAF 0
#define BINNACLE_ITEM_GROUP 1
#define BINNACLE_ITEM_SUBTOTAL 2
#define BINNACLE_ITEM_GRAND_TOTAL 3

/* The Key of an axis's Grand Total, whoever groups it: a grouped Table
 * Source names its Grand Total so, and the Grid pins one placed after at
 * the trailing edge by default, before it is in a Window. Where the values
 * pseudo-level is on the axis, its Grand Total is one item per value, each
 * keyed by the source; the Grid pins each as a Window first brings it. */
#define BINNACLE_GRAND_TOTAL_KEY 0x8000000000000000ull

/* The field a group of the values pseudo-level names in its outline's group
 * (BinnacleOutline.field), beside the value's identity as its text,
 * such as "sum(3)": the aggregate, its field's Key and any parameters. A
 * pivot's columns under the values pseudo-level are such groups. */
#define BINNACLE_VALUES_FIELD 0xffffffffffffffffull

/* BinnacleOutline.open: whether a Group Item is open. Other items, and Group
 * Items with nothing under them to show, can't open. */
#define BINNACLE_OPEN_STATE_OPEN 0
#define BINNACLE_OPEN_STATE_CLOSED 1
#define BINNACLE_OPEN_STATE_CANT_OPEN 2

/* binnacle_grid_located's answer: the Key is at position, has left the axis,
 * or has left and position is where focus should go instead. */
#define BINNACLE_LOCATED_FOUND 0
#define BINNACLE_LOCATED_GONE 1
#define BINNACLE_LOCATED_GONE_SUGGEST 2

/* binnacle_grid_notify's Change Notice: how an axis changed. */
/* Anything may have changed; n is the new count. Always safe to send. */
#define BINNACLE_NOTICE_RESET 0
/* The items are in a new order; the count is unchanged. */
#define BINNACLE_NOTICE_REORDERED 1
/* n items inserted before the item at Position at (the count for the end). */
#define BINNACLE_NOTICE_INSERTED 2
/* The n items from Position at were removed. */
#define BINNACLE_NOTICE_REMOVED 3
/* The content of the n items from Position at changed. */
#define BINNACLE_NOTICE_CONTENT_CHANGED 4
/* The same items, some left out or brought back and possibly reordered (a
 * filter changed); n is the new count. */
#define BINNACLE_NOTICE_REFILTERED 5
/* The group headed by the Group Item at at - 1 closed, hiding the n items
 * from Position at: they stay alive (Size Overrides kept, pins dormant), and
 * focus, a range corner or the Scroll Anchor on one moves to the Group
 * Item. */
#define BINNACLE_NOTICE_COLLAPSED 6
/* The group headed by the Group Item at at - 1 opened, showing n items
 * before the item at Position at. */
#define BINNACLE_NOTICE_EXPANDED 7

/* How far binnacle_grid_move_focus goes. */
#define BINNACLE_FOCUS_ITEM 0
/* A viewport's length (at least one item). */
#define BINNACLE_FOCUS_PAGE 1
#define BINNACLE_FOCUS_EDGE 2

/* BinnaclePointerEvent.kind. */
#define BINNACLE_POINTER_DOWN 0
#define BINNACLE_POINTER_MOVE 1
#define BINNACLE_POINTER_UP 2

/* BinnaclePointerEvent.button. Gestures use only the primary button. */
#define BINNACLE_BUTTON_PRIMARY 0
#define BINNACLE_BUTTON_SECONDARY 1
#define BINNACLE_BUTTON_MIDDLE 2

/* BinnaclePointerEvent.modifiers bits. Ctrl and Meta (Cmd) both toggle picks. */
#define BINNACLE_MOD_SHIFT 1u
#define BINNACLE_MOD_CTRL 2u
#define BINNACLE_MOD_ALT 4u
#define BINNACLE_MOD_META 8u

/* What a plain click on a cell selects. */
#define BINNACLE_SELECT_CELLS 0
#define BINNACLE_SELECT_ROWS 1
#define BINNACLE_SELECT_COLS 2

/* What clicking a header does. */
#define BINNACLE_HEADER_CLICK_SORT 0
#define BINNACLE_HEADER_CLICK_SELECT 1
#define BINNACLE_HEADER_CLICK_NONE 2

/* The view controller for one table. */
typedef struct BinnacleGrid BinnacleGrid;

/*
 * How one axis starts out. default_size must be positive and finite, gap
 * finite and not negative. header is the thickness of the band leading the
 * axis: on rows the column header band's height, on columns the row header
 * band's width.
 */
typedef struct {
  uint64_t count;
  float default_size;
  float header;
  float gap;
} BinnacleAxisConfig;

/* The Window one axis needs: Positions start..end, if requested. */
typedef struct {
  uint32_t requested;
  uint64_t start;
  uint64_t end;
} BinnacleWindowRequest;

typedef struct {
  BinnacleWindowRequest axes[2];
} BinnacleWindowRequests;

/* Positions start..end. */
typedef struct {
  uint64_t start;
  uint64_t end;
} BinnacleRange;

/* A viewport-relative rectangle in the host's unit. */
typedef struct {
  float x;
  float y;
  float width;
  float height;
} BinnacleRect;

/* Where a tier lies across its Header Band, from the band's outer edge. */
typedef struct {
  float start;
  float end;
} BinnacleExtent;

/* A level the corner names: its place among its axis's levels (0
 * outermost) and BINNACLE_LEVEL_FIELD with the field it groups by, or
 * BINNACLE_LEVEL_VALUES for the values pseudo-level (field 0). */
typedef struct {
  uint32_t index;
  uint32_t kind;
  uint64_t field;
} BinnacleCornerLevel;

/* One tier of a Header Band where it crosses the corner: where it lies
 * across the band, and the levels it names, corner_levels[levels_start]
 * on for levels_len. */
typedef struct {
  BinnacleExtent across;
  size_t levels_start;
  size_t levels_len;
} BinnacleCornerTier;

/* One spanning cell of a tier in header form: the group of axis keyed key
 * (its Group Item Key), at level tier (0 outermost), whose level groups by
 * field (has_field 0 for the values pseudo-level), open or closed
 * (BINNACLE_OPEN_STATE_*), over the drawn items from Position first to
 * last that one Strip draws side by side. rect is unclipped in the Header
 * Band; visible is the part its Strip draws, where its label goes so it
 * stays in view. cut_before and cut_after are 1 where the group goes on
 * past the viewport, the Window or a pinned edge. */
typedef struct {
  uint32_t axis;
  uint32_t tier;
  uint64_t key;
  uint32_t has_field;
  uint64_t field;
  uint32_t open;
  uint64_t first;
  uint64_t last;
  BinnacleRect rect;
  BinnacleRect visible;
  uint32_t cut_before;
  uint32_t cut_after;
} BinnacleHeaderSpan;

/* UTF-8 text, length-delimited and not NUL-terminated. */
typedef struct {
  const uint8_t *data;
  size_t len;
} BinnacleText;

/* A value as a ONE_OF condition lists it, or a group has it: text in the
 * Table Source's own text form, or with blank 1 a blank (no value at all,
 * not empty text), its text ignored. */
typedef struct {
  uint32_t blank;
  BinnacleText text;
} BinnacleValue;

/* A group: its level's field and its value there. */
typedef struct {
  uint64_t field;
  BinnacleValue value;
} BinnacleGroupOf;

/* Where an item sits in a grouped axis, as the Table Source sends it with a
 * Window (binnacle_grid_set_outlines). kind is BINNACLE_ITEM_*, open
 * BINNACLE_OPEN_STATE_*. depth is 0 for the top groups and a Grand Total; a
 * Leaf of a group at depth d is at d + 1. ancestors are the Group Item Keys
 * from the outermost level down to the parent, so the parent is the last
 * (there are depth of them). Group Items and Subtotals have has_group 1, with
 * their level's field and the group's value there. span: on a Group Item,
 * how many items are shown under it (its Subtotal included; 0 while
 * closed), so a click on its header selects the group. path: the group of
 * each ancestor, outermost first, from which an activated cell's drill-down
 * is built; a source that sends none gets drill-downs naming the item's own
 * group alone. has_set 1 places the item among its parent's children:
 * set_index (0 first) of set_size, a group's Subtotal first or last where
 * its level places one, and at the top a placed Grand Total likewise, open
 * or closed and in either form. */
typedef struct {
  uint32_t kind;
  uint32_t depth;
  uint32_t open;
  const uint64_t *ancestors;
  size_t ancestors_len;
  uint32_t has_group;
  uint64_t field;
  BinnacleValue value;
  uint64_t span;
  const BinnacleGroupOf *path;
  size_t path_len;
  uint32_t has_set;
  uint64_t set_index;
  uint64_t set_size;
} BinnacleOutline;

/*
 * One visible item on an axis. offset is from the viewport's leading edge,
 * header band included. has_key is 0 while the Table Source has not filled
 * the Position: draw it as loading. has_outline is 1 when the Table Source
 * sent an outline for the item with the Window (a grouped axis); its arrays
 * are valid as the strip's are.
 */
typedef struct {
  uint64_t position;
  uint32_t has_key;
  uint64_t key;
  float offset;
  float size;
  uint32_t has_outline;
  BinnacleOutline outline;
  /* 1 while a toggle of this Group Item waits for the Table Source: draw
   * its disclosure mark pending once it has waited about 150 ms. */
  uint32_t toggle_pending;
} BinnacleStripItem;

/* An aggregate the Table Source offers over a base field (see
 * binnacle_grid_set_aggregates): its name, and the names of the
 * parameters_len parameters a value with it takes, in order. */
typedef struct {
  BinnacleText name;
  const BinnacleText *params;
  size_t params_len;
} BinnacleAggregate;

/* The header of one visible item, in its band; unclipped like the strip. */
typedef struct {
  uint64_t position;
  uint32_t has_key;
  uint64_t key;
  BinnacleRect rect;
  /* BINNACLE_HEADER_* bits. */
  uint32_t flags;
  uint32_t sort_place;
  /* BINNACLE_SUPPORT_* bits: what the Table Source supports when viewing the
   * other axis by this item. Adapters offer only these controls. */
  uint32_t support;
  /* The aggregates the Table Source offers over this item, a base field,
   * when grouping the other axis. */
  const BinnacleAggregate *aggregates;
  size_t aggregates_len;
} BinnacleHeader;

/* The item at the viewport's leading edge and how far into it the view is. */
typedef struct {
  uint64_t position;
  float offset;
} BinnacleScrollAnchor;

/* An estimated scrollbar thumb: fraction of its travel and share shown. */
typedef struct {
  double fraction;
  double size;
} BinnacleThumb;

/* One axis's half of the focused cell: its last known Position and its Key
 * once the Window holds it. */
typedef struct {
  uint64_t position;
  uint32_t has_key;
  uint64_t key;
} BinnacleFocusEnd;

/* Text to look for in the cells of items of the other axis, as the Table
 * Source writes their values: anywhere in a value, as BINNACLE_TEXT_* flags
 * say, in the items BINNACLE_SEARCHED_* says (keys for KEYS). A blank never
 * matches. */
typedef struct {
  BinnacleText text;
  uint32_t flags;
  uint32_t searched;
  const uint64_t *keys;
  size_t keys_len;
} BinnacleTextQuery;

/* The Find Query: query, matched against each cell of the items it
 * searches, and the axis stepped along first (BINNACLE_ROWS: each row's
 * searched cells in column order, then the next row; COLS the transpose). */
typedef struct {
  BinnacleTextQuery query;
  uint32_t by;
} BinnacleFindQuery;

/* A cell matching the Find Query, by its row and column Keys. */
typedef struct {
  uint64_t row;
  uint64_t col;
} BinnacleFindMatch;

/* Defined with BinnacleCondition, BinnacleLevel and BinnacleAggregateValue,
 * below. */
typedef struct BinnacleListedCondition BinnacleListedCondition;
typedef struct BinnacleListedSort BinnacleListedSort;
typedef struct BinnacleListedLevel BinnacleListedLevel;
typedef struct BinnacleListedValue BinnacleListedValue;

typedef struct {
  uint64_t count;
  /* The viewport's length along the axis, header band included. */
  float extent;
  float header;
  float gap;
  BinnacleScrollAnchor anchor;
  BinnacleThumb thumb;
  /* The drawn items in drawn order: the leading pinned Strip's pinned_len
   * items, then the scrolling Strip, then the trailing pinned Strip's
   * trailing_len items. */
  const BinnacleStripItem *strip;
  size_t strip_len;
  /* The headers of the strip's items, in strip order (headers_len ==
   * strip_len): column headers on BINNACLE_COLS, row headers on ROWS. */
  const BinnacleHeader *headers;
  size_t headers_len;
  /* Set when BinnacleSnapshot.has_focus is. */
  BinnacleFocusEnd focus;
  /* Set when BinnacleSnapshot.has_selection_range is: the Positions the range
   * covers on the axis, as sorted runs apart from each other. It is
   * contiguous in drawn order, so with items pinned it may be several. */
  const BinnacleRange *selection_runs;
  size_t selection_runs_len;
  /* The axis's unfiltered count, if the Table Source gave it with the
   * applied View Spec: shown beside count. */
  uint32_t has_total;
  uint64_t total;
  /* BINNACLE_QUICK_FILTER* bits: whether the axis has a quick filter. */
  uint32_t quick_filter;
  /* The axis's conditions, as a filter's chips list them: the latest
   * request's, then the applied ones it drops. */
  const BinnacleListedCondition *conditions;
  size_t conditions_len;
  /* The axis's levels, listed as conditions are: a level the Table Source
   * could not apply (or not with its Subtotal) is NOT_APPLIED. */
  const BinnacleListedLevel *levels;
  size_t levels_len;
  /* The axis's picked Keys, in Key order. */
  const uint64_t *picked;
  size_t picked_len;
  /* How many of strip's items lead it as the leading pinned Strip: drawn
   * from the header band on, whatever the scroll (slid back while focus is
   * on one it clips). */
  size_t pinned_len;
  /* How many of strip's items end it as the trailing pinned Strip: drawn
   * ending flush with the viewport's far end, whatever the scroll. */
  size_t trailing_len;
  /* Where the scrolling Strip draws along the axis: clip its items (those
   * between the pinned ones) to [scrolling_start, scrolling_end). */
  float scrolling_start;
  float scrolling_end;
  /* Where each pinned Strip draws along the axis: clip the leading pinned
   * items to [leading_start, leading_end) and the trailing ones to
   * [trailing_start, trailing_end). Too long to leave the minimum scrolling
   * region (binnacle_grid_set_min_scrolling), a Strip is clipped at its
   * inside end, the trailing one first; one clipped whole still lists its
   * items. */
  float leading_start;
  float leading_end;
  float trailing_start;
  float trailing_end;
  /* Header drags move the axis's items (binnacle_grid_set_movable). */
  uint32_t movable;
  /* Whether the Table Source hides items of the axis: offer Hide only then. */
  uint32_t hide_support;
  /* The axis's hidden Keys in the order they were hidden, as the latest
   * request has them: what a "Show hidden" list offers. */
  const uint64_t *hidden;
  size_t hidden_len;
  /* The most hidden Keys the axis takes: past it a hide is refused and the
   * host suggests a filter (binnacle_grid_set_hide_cap). */
  size_t hide_cap;
  /* The Keys pinned to each edge, in pin order, dormant pins (their items
   * gone for now) included: what a saved layout keeps. */
  const uint64_t *leading_pins;
  size_t leading_pins_len;
  const uint64_t *trailing_pins;
  size_t trailing_pins_len;
  /* While the axis is grouped in header form: where each level's tier lies
   * across the Header Band heading its items, outermost first, and the
   * spans, each tier's in drawn order, Strip by Strip. Empty otherwise. An
   * item's own header (in headers) starts past the tiers its spans take. */
  const BinnacleExtent *tiers;
  size_t tiers_len;
  const BinnacleHeaderSpan *spans;
  size_t spans_len;
  /* The corner grid: the tiers of the band heading this axis's items where
   * it crosses the corner, outermost first, the items' own headers last
   * (ROWS: the corner's columns, COLS: its rows). In header form a level's
   * tier names that level; in outline form the one tier names every level.
   * A cell is where a COLS tier crosses a ROWS tier. corner_levels holds the
   * levels the tiers name, in tier order. */
  const BinnacleCornerTier *corner_tiers;
  size_t corner_tiers_len;
  const BinnacleCornerLevel *corner_levels;
  size_t corner_levels_len;
  /* Whether focus is in the Header Band heading this axis's items, where
   * the tree keys act on its groups: ROWS the outline column (or a level
   * column in header form) on the focused row, COLS a tier of the column
   * band on the focused column. ROWS is the same as outline_focused. */
  uint32_t band_focused;
  /* The axis's sort terms, listed as conditions are: on a pivot, a term
   * naming an item the data lacks for now (a pivot column gone) stays
   * requested, NOT_APPLIED, and applies again when the item returns. */
  const BinnacleListedSort *sorts;
  size_t sorts_len;
} BinnacleAxisSnapshot;

/*
 * What to draw this frame. Its arrays stay valid until the next
 * binnacle_grid_snapshot or binnacle_grid_free on the same Grid.
 */
/* A pin zone lit while headers are dragged on axis: the edge (BINNACLE_EDGE_*)
 * whose pinned Strip is empty, size long from offset; hot while a drop would
 * pin there. */
typedef struct {
  uint32_t axis;
  uint32_t edge;
  float offset;
  float size;
  uint32_t hot;
} BinnacleZone;

typedef struct {
  uint64_t generation;
  /* BINNACLE_DAMAGE_* bits: what changed since the previous Snapshot. */
  uint32_t damage;
  BinnacleAxisSnapshot axes[2];
  /* Focus is placed on both axes or neither. */
  uint32_t has_focus;
  /* The focused cell, unclipped, if it is visible with its Keys known. */
  uint32_t has_focus_rect;
  BinnacleRect focus_rect;
  /* The one contiguous range: see each axis's selection_runs. */
  uint32_t has_selection_range;
  /* Rects to fill: the visible part of the range, then each visible picked
   * row, then each visible picked column. They may overlap. */
  const BinnacleRect *selection_rects;
  size_t selection_rects_len;
  /* BINNACLE_CELL_* bits for each visible cell, row-major: the cell crossing
   * rows strip item r and columns strip item c is at r * cols strip_len + c. */
  const uint8_t *cell_states;
  size_t cell_states_len;
  /* BINNACLE_MODE_*. */
  uint32_t mode;
  /* A gesture follows a held button: capture the pointer. */
  uint32_t capturing;
  /* The header under the pointer: the axis it heads (a row header is on
   * BINNACLE_ROWS) and its index in that axis's strip. */
  uint32_t has_hovered_header;
  uint32_t hovered_axis;
  size_t hovered_index;
  /* The Find Query in force, if find is open. */
  uint32_t has_find;
  BinnacleFindQuery find;
  /* The match the last find step landed on, until its row or column leaves,
   * a step for a new Find Query lands, or find closes. */
  uint32_t has_find_match;
  BinnacleFindMatch find_match;
  /* BINNACLE_FIND* bits: whether to offer find. */
  uint32_t find_support;
  /* Nonzero while a binnacle_grid_view_applied is due: requested parts are
   * pending; once zero, requested parts not applied are not applied. */
  uint32_t view_pending;
  /* 1 while focus is in the outline column: on the focused row's band
   * cell (focus_rect), where the tree keys act, not on a body cell. */
  uint32_t outline_focused;
  /* The move requested and not yet answered: its axis and the Keys it
   * moves. */
  uint32_t has_pending_move;
  uint32_t pending_move_axis;
  const uint64_t *pending_move_keys;
  size_t pending_move_keys_len;
  /* The last move the Table Source refused and its reason, until the next
   * press: a short message to show near the headers or in a status line. */
  uint32_t has_move_refusal;
  uint32_t move_refusal_axis;
  BinnacleText move_refusal;
  /* While headers are dragged and a release would do something: a line
   * across the viewport at drop_offset along drop_axis, and what the drop
   * does (BINNACLE_DROP_*, with the fields it names). */
  uint32_t has_drop;
  uint32_t drop_axis;
  uint32_t drop_target;
  float drop_offset;
  uint64_t drop_position;
  uint32_t drop_unpin;
  uint32_t drop_edge;
  uint32_t has_drop_before;
  uint64_t drop_before;
  uint32_t drop_reorder;
  uint32_t drop_refusal;
  /* The lit pin zones while headers are dragged: the first pin_zones_len. */
  BinnacleZone pin_zones[2];
  size_t pin_zones_len;
  /* The slicers and the values of the grouping, listed as conditions are. */
  const BinnacleListedCondition *slicers;
  size_t slicers_len;
  const BinnacleListedValue *values;
  size_t values_len;
  /* The tier of the Header Band focus is in while it is in one (see each
   * axis's band_focused) and that axis is in header form; 0 otherwise. */
  uint32_t focused_tier;
  /* While mode is EDITING: the edited cell's row and column Keys, and its
   * rect, unclipped like focus_rect, while it is visible (scrolled away,
   * has_editor_rect is 0 and the editor hides until it returns). */
  uint64_t edit_row;
  uint64_t edit_col;
  uint32_t has_editor_rect;
  BinnacleRect editor_rect;
} BinnacleSnapshot;

/* One column of a Sort Spec: the column's Key and BINNACLE_SORT_*. */
typedef struct {
  uint64_t key;
  uint32_t direction;
} BinnacleSortColumn;

/* A number or a date a COMPARE condition compares with: number when kind is
 * BINNACLE_SCALAR_NUMBER, date when BINNACLE_SCALAR_DATE; the other is
 * ignored. */
typedef struct {
  uint32_t kind;
  double number;
  int64_t date;
} BinnacleScalar;

/* One condition on the items of an axis, read in item key of the other axis
 * (a column, for the rows): the items whose value there matches are kept, or
 * with negated 1 those whose value does not. A TEXT condition matches text;
 * its query searches key alone (as input, searched and keys are ignored). A
 * ONE_OF condition lists values_len values; other kinds ignore them. A
 * COMPARE condition compares as compare says with scalar, and for BETWEEN
 * high; other kinds ignore them. */
typedef struct {
  uint64_t key;
  /* BINNACLE_CONDITION_*. */
  uint32_t kind;
  uint32_t negated;
  BinnacleTextQuery text;
  const BinnacleValue *values;
  size_t values_len;
  /* BINNACLE_COMPARE_*. */
  uint32_t compare;
  BinnacleScalar scalar;
  BinnacleScalar high;
} BinnacleCondition;

struct BinnacleListedSort {
  /* BINNACLE_CONDITION_*, as for a listed condition. */
  uint32_t state;
  BinnacleSortColumn column;
};

struct BinnacleListedCondition {
  /* BINNACLE_CONDITION_APPLIED, ADDING or REMOVING. */
  uint32_t state;
  BinnacleCondition condition;
};

/* One axis's part of a View Spec: its sort, by items of the other axis in
 * order (empty for unsorted), its filter: the quick filter, which keeps the
 * items with a match for it, when has_quick is 1, and the conditions, which
 * keep the items every one of them keeps; and its hidden Keys, items of this
 * axis the Table Source leaves out apart from the filter. A source answers
 * with the hidden Keys its data still has. */
typedef struct {
  const BinnacleSortColumn *sort;
  size_t sort_len;
  uint32_t has_quick;
  BinnacleTextQuery quick;
  const BinnacleCondition *conditions;
  size_t conditions_len;
  const uint64_t *hidden;
  size_t hidden_len;
} BinnacleAxisView;

/* One level of an axis's grouping: BINNACLE_LEVEL_*. A FIELD level groups by
 * field, a base field of the Table Source, each group with a Subtotal placed
 * as subtotal (BINNACLE_PLACEMENT_*) says; VALUES ignores both. */
typedef struct {
  uint32_t kind;
  uint64_t field;
  uint32_t subtotal;
} BinnacleLevel;

/* One value a grouped table shows: an aggregate, an open name such as "sum",
 * "count", "average", "minimum", "maximum", "distinct count" or one the
 * Table Source declares, over the base field field, or with has_field 0 a
 * count of items, with the parameters a declared aggregate takes, passed to
 * the Table Source as they are. */
typedef struct {
  uint32_t has_field;
  uint64_t field;
  BinnacleText aggregate;
  const BinnacleText *params;
  size_t params_len;
} BinnacleAggregateValue;

struct BinnacleListedLevel {
  /* BINNACLE_CONDITION_APPLIED, ADDING, REMOVING or NOT_APPLIED. */
  uint32_t state;
  BinnacleLevel level;
};

struct BinnacleListedValue {
  /* BINNACLE_CONDITION_APPLIED, ADDING, REMOVING or NOT_APPLIED. */
  uint32_t state;
  BinnacleAggregateValue value;
};

/* How one axis is grouped: its levels, outermost first (none: the axis is
 * not grouped); where its Grand Total goes (BINNACLE_PLACEMENT_*); its form
 * (BINNACLE_FORM_*); and which groups are open: with expand_all 1 every
 * level (expand_depth ignored), else the first expand_depth levels (0 shows
 * only the top groups), except the Group Items keyed in except, which are
 * the other way. */
typedef struct {
  const BinnacleLevel *levels;
  size_t levels_len;
  uint32_t grand_total;
  uint32_t form;
  uint32_t expand_all;
  uint32_t expand_depth;
  const uint64_t *except;
  size_t except_len;
} BinnacleGroupedAxis;

/* The whole-table grouping part of a View Spec: each axis's levels and
 * layout, axes[BINNACLE_ROWS] and axes[BINNACLE_COLS] (no column levels: no
 * pivot, and the Leaves stay), the values shown, and the slicers, conditions
 * on base fields applied before aggregating. */
typedef struct {
  BinnacleGroupedAxis axes[2];
  const BinnacleAggregateValue *values;
  size_t values_len;
  const BinnacleCondition *slicers;
  size_t slicers_len;
} BinnacleGrouping;

/* The view the Table Source should present: axes[BINNACLE_ROWS] and
 * axes[BINNACLE_COLS], and with has_grouping 1 how the table is grouped
 * (has_grouping 0: a plain table, grouping ignored). */
typedef struct {
  BinnacleAxisView axes[2];
  uint32_t has_grouping;
  BinnacleGrouping grouping;
} BinnacleViewSpec;

/* The cause of a requested View Spec, a hint: the spec alone is the request.
 * keys is set for HIDE and SHOW, key for GROUP (the field) and for EXPAND,
 * COLLAPSE and EXPAND_ALL (the Group Item), expand_all and expand_depth for
 * EXPAND_TO, form (BINNACLE_FORM_*) for SET_FORM, level for REMOVE_LEVEL and
 * MOVE_LEVEL (a FIELD level's field, or VALUES; its subtotal unused), and
 * value for ADD_VALUE and REMOVE_VALUE (its texts valid as the View Spec's
 * are). */
typedef struct {
  uint32_t kind;
  uint32_t axis;
  uint64_t key;
  uint32_t multi;
  const uint64_t *keys;
  size_t keys_len;
  uint32_t expand_all;
  uint32_t expand_depth;
  uint32_t form;
  BinnacleLevel level;
  BinnacleAggregateValue value;
} BinnacleViewEdit;

/* Each axis's unfiltered count, where the Table Source knows it. */
typedef struct {
  uint32_t known;
  uint64_t count;
} BinnacleTotal;

typedef struct {
  BinnacleTotal axes[2];
} BinnacleTotals;

/*
 * A request for the host to act on, stamped with the generation of the
 * Snapshot the user was looking at. Fields a kind does not use are zero.
 */
typedef struct {
  uint64_t generation;
  uint32_t kind;
  /* TEXT_TYPED: the text, valid until the next binnacle_grid_poll_effect or
   * binnacle_grid_free on the same Grid. */
  BinnacleText text;
  /* FOCUS_CHANGED, ACTIVATED, EDIT_REQUESTED and EDIT_CANCELLED: the
   * cell's row and column Keys. */
  uint64_t row;
  uint64_t col;
  /* VIEW_REQUESTED: the whole requested View Spec, its arrays valid until
   * the next binnacle_grid_poll_effect or binnacle_grid_free on the same
   * Grid, and the edit that caused it. */
  BinnacleViewSpec view;
  BinnacleViewEdit edit;
  /* LOCATE: the axis and the Key to find. SIZE_CHANGED and
   * AUTO_SIZE_REQUESTED: the item's axis and Key. BAND_SIZE_CHANGED and
   * BAND_AUTO_SIZE_REQUESTED: the axis the band leads. */
  uint32_t axis;
  uint64_t key;
  /* SIZE_CHANGED: the item's new size. BAND_SIZE_CHANGED: the band's new
   * thickness. */
  float size;
  /* FIND_REQUESTED and FIND_STEP_REQUESTED: the Find Query, its arrays valid
   * as view's are. */
  uint32_t has_find;
  BinnacleFindQuery find;
  /* FIND_STEP_REQUESTED: the cell to step from, and BINNACLE_FIND_*. */
  uint32_t has_from;
  BinnacleFindMatch from;
  uint32_t direction;
  /* MOVE_REQUESTED: the Keys of the items moved (axis is theirs), valid as
   * text is, and the Position they drop before. PINS_CHANGED: the Keys now
   * pinned to edge, in pin order. */
  const uint64_t *keys;
  size_t keys_len;
  uint64_t to;
  /* PINS_CHANGED: the edge (BINNACLE_EDGE_*). */
  uint32_t edge;
  /* ACTIVATED: with has_drill_down 1, the drill-down's conditions, valid as
   * view's arrays are. */
  uint32_t has_drill_down;
  const BinnacleCondition *drill_down;
  size_t drill_down_len;
  /* COPY_REQUESTED: per axis, the runs of Positions to copy, valid as text
   * is, and whether the axis's labels lead. */
  const BinnacleRange *copy_runs[2];
  size_t copy_runs_len[2];
  uint32_t copy_labels[2];
} BinnacleEffect;

/* Every Size Override on an axis, in Key order: keys[i] has sizes[i]. Valid
 * until the next binnacle_grid_size_overrides or binnacle_grid_free on the
 * same Grid. */
typedef struct {
  const uint64_t *keys;
  const float *sizes;
  size_t len;
} BinnacleSizeOverrides;

/* A raw pointer event in viewport coordinates (the host's unit, origin at the
 * top-left including the header bands). clicks is the host's click count for
 * a press. */
typedef struct {
  uint32_t kind;
  float x;
  float y;
  uint32_t button;
  uint32_t modifiers;
  uint32_t clicks;
} BinnaclePointerEvent;

/* A new Grid, or NULL if either config is invalid. */
BinnacleGrid *binnacle_grid_new(BinnacleAxisConfig rows, BinnacleAxisConfig cols);

void binnacle_grid_free(BinnacleGrid *grid);

/* Settings. */

/* BINNACLE_SELECT_*; default CELLS. */
int binnacle_grid_set_selection_unit(BinnacleGrid *grid, uint32_t unit);

/* BINNACLE_HEADER_CLICK_* for the headers of items on axis (ROWS for row
 * headers). Row headers default to SELECT, column headers to SORT. Only
 * columns sort: SORT on row headers makes a click do nothing. Under SORT or
 * SELECT, Ctrl or Meta toggles the item's pick instead. */
int binnacle_grid_set_header_click(BinnacleGrid *grid, uint32_t axis, uint32_t click);

/* How far a pressed pointer must move to drag; default 1. Values that are not
 * finite and non-negative are ignored. */
int binnacle_grid_set_drag_threshold(BinnacleGrid *grid, float threshold);

/* Whether the Table Source can move the items of axis now (1) or not (0),
 * which it may change at any time. Header drags move items only on a movable
 * axis; withdrawing it ends a move being dragged there. No axis is movable
 * until set. 0 for other values. */
int binnacle_grid_set_movable(BinnacleGrid *grid, uint32_t axis, uint32_t movable);

/* The frame loop. */

/*
 * Starts a frame with the viewport's size (header bands included), advancing
 * the generation, and fills out with the Windows to answer with
 * binnacle_grid_set_window before taking the Snapshot.
 */
int binnacle_grid_begin_frame(BinnacleGrid *grid, float width, float height,
                              BinnacleWindowRequests *out);

/* Fills the Window on axis with the Keys for Positions start... Keys past the
 * count are ignored; Positions left unfilled draw as loading. */
int binnacle_grid_set_window(BinnacleGrid *grid, uint32_t axis, uint64_t start,
                             const uint64_t *keys, size_t len);

/* Like binnacle_grid_set_window, with each item's content size (sizes has len
 * entries) and a hint that no content size on axis is below min_size. */
int binnacle_grid_set_window_sized(BinnacleGrid *grid, uint32_t axis, uint64_t start,
                                   const uint64_t *keys, const float *sizes, size_t len,
                                   float min_size);

/* The Size Override for the item with key on axis. Sizes that are not positive
 * and finite are ignored. */
int binnacle_grid_set_size(BinnacleGrid *grid, uint32_t axis, uint64_t key, float size);

/* The thickness of the Header Band leading axis: ROWS for the column header
 * band, COLS for the row-number band. 0 hides the band. Values that are not
 * finite and non-negative are ignored. */
int binnacle_grid_set_header(BinnacleGrid *grid, uint32_t axis, float thickness);

/* The Edge Gap on axis: the least space between a pinned Strip and the
 * scrolling Strip, where the pinned edge is drawn; the Gap is used where it is
 * larger. Values that are not finite and non-negative are ignored. */
int binnacle_grid_set_edge_gap(BinnacleGrid *grid, uint32_t axis, float gap);

/* How far the pin zone at each end of axis reaches while headers are
 * dragged: a drop there, or past it over the Header Band or beyond the
 * viewport, pins to that edge while its pinned Strip is empty. Default a
 * quarter of the axis's default size. Values that are not finite and
 * non-negative are ignored. */
int binnacle_grid_set_pin_zone(BinnacleGrid *grid, uint32_t axis, float size);

/* The sizes of the tiers of the Header Band heading axis's items while axis
 * is grouped in header form (ROWS: the row band's level columns), one per
 * level from the band's outer edge. Levels past sizes share what the band
 * has left; what the tiers leave heads the items themselves. Sizes that are
 * not finite and non-negative are taken as 0. */
int binnacle_grid_set_tiers(BinnacleGrid *grid, uint32_t axis, const float *sizes, size_t len);

/* Draws the grouped axis in form (BINNACLE_FORM_*) and requests the result
 * (BINNACLE_VIEW_EDIT_SET_FORM). Nothing happens if the axis is not grouped
 * or the latest request draws it so already. */
int binnacle_grid_set_form(BinnacleGrid *grid, uint32_t axis, uint32_t form);

/* The least length along axis the pinned Strips leave to the scrolling Strip
 * (0 at first): pinned Strips longer than the rest are clipped at their inside
 * ends, the trailing one first, and nothing unpins. Focus on a clipped pinned
 * item slides its Strip to show it. Values that are not finite and
 * non-negative are ignored. */
int binnacle_grid_set_min_scrolling(BinnacleGrid *grid, uint32_t axis, float length);

/* The End Gap on axis (0 at first): the room kept past the last item when
 * scrolled to the end, where a closing line is drawn. The end clamp stops the
 * End Gap short of the viewport's end, and a trailing pinned Strip ends that
 * far short of it. Values that are not finite and non-negative are ignored. */
int binnacle_grid_set_end_gap(BinnacleGrid *grid, uint32_t axis, float gap);

/* Pins the item with key on axis to edge (BINNACLE_EDGE_*), after the keys
 * pinned there before: it is drawn in that edge's pinned Strip whatever the
 * sort or scroll. A key outside the Window is found with a LOCATE Effect.
 * Unknown edges are ignored. */
int binnacle_grid_pin(BinnacleGrid *grid, uint32_t axis, uint32_t edge, uint64_t key);

/* "Freeze up to here": pins every item from the start of axis to the one with
 * key, in their current order, to the leading edge, as far as they fit with
 * room left to scroll. On a leading pin, unpins the pins after it. If the
 * Window lacks some of their keys, the axis scrolls to its start and the
 * freeze finishes once the next Window holds them. */
int binnacle_grid_freeze(BinnacleGrid *grid, uint32_t axis, uint64_t key);

/* Unpins the item with key on axis; the scroll stays where it is. */
int binnacle_grid_unpin(BinnacleGrid *grid, uint32_t axis, uint64_t key);

/* Lists into keys and len every Key of axis the Grid keeps state on apart
 * from its Windows: its Size Overrides and pins (dormant too), and in the
 * latest View Spec its hidden Keys and the other axis's sort terms and
 * conditions naming its items. Valid until the next call on grid. A host
 * whose Table Source retires Keys (binnacle_group_layer_fits) passes those
 * it retired to binnacle_grid_forget after each View Spec applied. */
int binnacle_grid_held_keys(BinnacleGrid *grid, uint32_t axis, const uint64_t **keys,
                            size_t *len);

/* The Table Source will never show the len keys of axis again (path Keys
 * whose paths no longer fit the levels): drops their Size Overrides and
 * pins, and from the latest View Spec the axis's hidden Keys and the other
 * axis's sort terms and conditions naming them. Requests nothing. */
int binnacle_grid_forget(BinnacleGrid *grid, uint32_t axis, const uint64_t *keys, size_t len);

/* Makes the len keys the pins of edge on axis, in that order, say to restore
 * a saved layout: keys pinned there before and not listed are unpinned, a
 * key pinned to the other edge moves here, a key listed twice counts once.
 * A key outside the Window is found with a LOCATE Effect; one the Table
 * Source lacks sleeps as a dormant pin until it comes back. Unknown edges
 * are ignored. */
int binnacle_grid_set_pins(BinnacleGrid *grid, uint32_t axis, uint32_t edge,
                           const uint64_t *keys, size_t len);

/* Lists every Size Override on axis into out, for Keys shown or not: what a
 * saved layout keeps, restored with binnacle_grid_set_size. */
int binnacle_grid_size_overrides(BinnacleGrid *grid, uint32_t axis,
                                 BinnacleSizeOverrides *out);

/* Lays out the frame and fills out. */
int binnacle_grid_snapshot(BinnacleGrid *grid, BinnacleSnapshot *out);

/* Takes the oldest queued Effect into out: 1 if there was one, else 0. */
int binnacle_grid_poll_effect(BinnacleGrid *grid, BinnacleEffect *out);

/* Answers. */

/* The answer to VIEW_REQUESTED, or the Table Source's own news: the View
 * Spec now applied, which may be part of the request, or the previous one if
 * the request was rejected, with each axis's unfiltered count where known.
 * NULL spec is the base view, NULL totals knows neither. 0 for an unknown
 * direction, tag or flag, or text that is not UTF-8, in the grouping too. */
int binnacle_grid_view_applied(BinnacleGrid *grid, const BinnacleViewSpec *spec,
                               const BinnacleTotals *totals);

/* The Table Source's news, not an answer: the View Spec it applies now, or
 * new totals. Answers still due stay due; a condition the user asked for
 * that the spec leaves out stays listed, NOT_APPLIED. Arguments and result
 * as binnacle_grid_view_applied. */
int binnacle_grid_view_changed(BinnacleGrid *grid, const BinnacleViewSpec *spec,
                               const BinnacleTotals *totals);

/* Whether axis has a quick filter and takes regexes: BINNACLE_QUICK_FILTER*
 * bits. Adapters hide the quick filter of an axis without one. */
int binnacle_grid_set_quick_filter_support(BinnacleGrid *grid, uint32_t axis,
                                           uint32_t support);

/* Whether the Table Source hides items of axis: 1 or 0 (the default).
 * Adapters offer Hide only then. 0 for anything else. */
int binnacle_grid_set_hide_support(BinnacleGrid *grid, uint32_t axis, uint32_t hides);

/* What the Table Source supports when viewing axis by key, an item of the
 * other axis: BINNACLE_SUPPORT_* bits, the whole declaration. Every item
 * supports SORT | TEXT (not ONE_OF or GROUP) until set otherwise. 0 for unknown bits,
 * or REGEX without TEXT. */
int binnacle_grid_set_support(BinnacleGrid *grid, uint32_t axis, uint64_t key,
                              uint32_t support);

/* The aggregates the Table Source offers when grouping axis with a value
 * over key, a base field (an item of the other axis): len of them, the
 * whole declaration (none until set). A value with no field, a count, needs
 * no declaring. 0 for text that is not UTF-8. */
int binnacle_grid_set_aggregates(BinnacleGrid *grid, uint32_t axis, uint64_t key,
                                 const BinnacleAggregate *aggregates, size_t len);

/* The outlines of the len items at Positions start... of a grouped axis
 * (outlines has len entries), sent after binnacle_grid_set_window, which
 * drops them; Positions outside the Window are ignored. 0 for an unknown
 * tag or flag, or text that is not UTF-8. */
int binnacle_grid_set_outlines(BinnacleGrid *grid, uint32_t axis, uint64_t start,
                               const BinnacleOutline *outlines, size_t len);

/* The answer to LOCATE for key on axis: BINNACLE_LOCATED_*, with position
 * for FOUND and GONE_SUGGEST (ignored for GONE). A GONE_SUGGEST position is
 * where the key would sit among the items left: the next one's position in
 * the order it had, or the count when none follows. */
int binnacle_grid_located(BinnacleGrid *grid, uint32_t axis, uint64_t key, uint32_t answer,
                          uint64_t position);

/* The answer to FIND_STEP_REQUESTED: the match stepped to, or NULL if
 * nothing matches. The match takes focus and is scrolled into view; the
 * selection is left as it is. */
int binnacle_grid_find_stepped(BinnacleGrid *grid, const BinnacleFindMatch *found);

/* The answer to FIND_STEP_REQUESTED on a grouped axis: the match found
 * (not NULL) and the Keys of the Group Items it lies under on each axis,
 * outermost first (a group label's own group is not among them). The Grid
 * expands each closed one with VIEW_REQUESTED and focuses the match once
 * the View Spec is applied; a label match focuses the outline column. 0
 * for a NULL found. */
int binnacle_grid_find_stepped_within(BinnacleGrid *grid, const BinnacleFindMatch *found,
                                      const uint64_t *row_ancestors, size_t row_ancestors_len,
                                      const uint64_t *col_ancestors, size_t col_ancestors_len);

/* Whether the Table Source finds, and takes regexes: BINNACLE_FIND* bits. */
int binnacle_grid_set_find_support(BinnacleGrid *grid, uint32_t support);

/* A Change Notice for axis: BINNACLE_NOTICE_*, with at and n as each
 * describes (unused ones are ignored). */
int binnacle_grid_notify(BinnacleGrid *grid, uint32_t axis, uint32_t notice, uint64_t at,
                         uint64_t n);

/* Input. */

int binnacle_grid_pointer(BinnacleGrid *grid, BinnaclePointerEvent event);

/* Sets the quick filter of axis, or clears it with NULL or empty text, and
 * requests the View Spec with BINNACLE_VIEW_EDIT_SET_QUICK_FILTER (nothing if
 * it is unchanged). 0 for an unknown tag or flag, or text that is not UTF-8. */
int binnacle_grid_set_quick_filter(BinnacleGrid *grid, uint32_t axis,
                                   const BinnacleTextQuery *query);

/* Adds condition to the conditions of axis and requests the result; a TEXT
 * condition with no text asks for nothing. 0 for NULL, an unknown kind, tag
 * or flag, or text that is not UTF-8, as for each condition below. */
int binnacle_grid_add_condition(BinnacleGrid *grid, uint32_t axis,
                                const BinnacleCondition *condition);

/* Removes the first condition of axis equal to condition, if the latest
 * request has one, and requests the result. */
int binnacle_grid_remove_condition(BinnacleGrid *grid, uint32_t axis,
                                   const BinnacleCondition *condition);

/* Puts to in place of the first condition of axis equal to from and requests
 * the result: adds to if there is none, removes from if to asks for nothing. */
int binnacle_grid_replace_condition(BinnacleGrid *grid, uint32_t axis,
                                    const BinnacleCondition *from,
                                    const BinnacleCondition *to);

/* Adds condition to the slicers, which filter the base fields before
 * aggregating (WHERE), shown or not, and requests the result with
 * BINNACLE_VIEW_EDIT_ADD_SLICER; a TEXT or NATIVE condition with no text
 * asks for nothing. 0 as for binnacle_grid_add_condition. */
int binnacle_grid_add_slicer(BinnacleGrid *grid, const BinnacleCondition *condition);

/* Removes the first slicer equal to condition, if the latest request has
 * one, and requests the result with BINNACLE_VIEW_EDIT_REMOVE_SLICER. */
int binnacle_grid_remove_slicer(BinnacleGrid *grid, const BinnacleCondition *condition);

/* Puts to in place of the first slicer equal to from and requests the
 * result: adds to if there is none, removes from if to asks for nothing. */
int binnacle_grid_replace_slicer(BinnacleGrid *grid, const BinnacleCondition *from,
                                 const BinnacleCondition *to);

/* The most hidden Keys the Table Source takes on axis, at most (and by
 * default) BINNACLE_HIDE_CAP. */
#define BINNACLE_HIDE_CAP 1000
int binnacle_grid_set_hide_cap(BinnacleGrid *grid, uint32_t axis, size_t cap);

/* What binnacle_grid_hide returns when the axis's hidden Keys would then
 * number more than its cap: nothing is hidden or requested, and the host
 * suggests a filter instead. */
#define BINNACLE_HIDE_CAPPED 2

/* Hides the items of axis with keys (len of them; NULL with 0 for none),
 * adding those not hidden yet to its hidden Keys, and requests the result
 * with BINNACLE_VIEW_EDIT_HIDE. Nothing while the source can't hide axis.
 * BINNACLE_HIDE_CAPPED past the axis's cap. */
int binnacle_grid_hide(BinnacleGrid *grid, uint32_t axis, const uint64_t *keys, size_t len);

/* Shows the hidden items of axis with keys again and requests the result
 * with BINNACLE_VIEW_EDIT_SHOW; Keys not hidden are skipped. */
int binnacle_grid_show(BinnacleGrid *grid, uint32_t axis, const uint64_t *keys, size_t len);

/* Groups axis by field, a base field, as its innermost level, and requests
 * the result with BINNACLE_VIEW_EDIT_GROUP. Nothing if axis is grouped by it
 * already or the Table Source can't group axis by it (BINNACLE_SUPPORT_GROUP
 * for field). */
int binnacle_grid_group_by(BinnacleGrid *grid, uint32_t axis, uint64_t field);

/* Cycles axis's sort by key, an item of the other axis, as a Sort header
 * click on its header does (BINNACLE_VIEW_EDIT_CYCLE_SORT), keeping the rest
 * of the sort if multi is 1: how a level name in the corner sorts by its
 * field. Nothing unless the Table Source sorts axis by key. */
int binnacle_grid_cycle_sort(BinnacleGrid *grid, uint32_t axis, uint64_t key, uint32_t multi);

/* Removes every level of axis and requests the result with
 * BINNACLE_VIEW_EDIT_UNGROUP. Nothing if axis is not grouped. */
int binnacle_grid_ungroup(BinnacleGrid *grid, uint32_t axis);

/* Removes the level of axis grouping by level's field and requests the
 * result with BINNACLE_VIEW_EDIT_REMOVE_LEVEL; with no level left on either
 * axis and no slicer the table is plain again. Nothing if axis has no such level, or for a
 * VALUES level, which leaves when one value is left. level's subtotal is
 * ignored. 0 for an unknown level kind. */
int binnacle_grid_remove_level(BinnacleGrid *grid, uint32_t axis, const BinnacleLevel *level);

/* Moves the level of axis named by level (a FIELD level's field, or VALUES)
 * to the other axis as its innermost level (inside a VALUES level there),
 * keeping its Subtotal, and requests the result with
 * BINNACLE_VIEW_EDIT_MOVE_LEVEL. Nothing if axis has no such level or the
 * Table Source can't group the other axis by its field
 * (BINNACLE_SUPPORT_GROUP for the rows, BINNACLE_SUPPORT_PIVOT for the
 * columns, declared on the rows by the field). Several values beside a
 * column field bring a VALUES level, innermost on the columns; with one
 * value, or no column field left, a VALUES level on the columns leaves. */
int binnacle_grid_move_level(BinnacleGrid *grid, uint32_t axis, const BinnacleLevel *level);

/* Adds value to the values of a grouped table and requests the result with
 * BINNACLE_VIEW_EDIT_ADD_VALUE. Nothing on a plain table, with the value
 * there already, or if the Table Source does not offer its aggregate over
 * its field (binnacle_grid_set_aggregates; a count with no field is always
 * offered). 0 for an invalid value. */
int binnacle_grid_add_value(BinnacleGrid *grid, const BinnacleAggregateValue *value);

/* Removes value from the values and requests the result with
 * BINNACLE_VIEW_EDIT_REMOVE_VALUE. Nothing unless it is one of them and not
 * the only one. 0 for an invalid value. */
int binnacle_grid_remove_value(BinnacleGrid *grid, const BinnacleAggregateValue *value);

/* The most Keys an axis's expand except list takes. */
#define BINNACLE_EXCEPT_CAP 1000

/* What a toggle returns when the axis's exceptions would then number more
 * than BINNACLE_EXCEPT_CAP: nothing is requested, and the host suggests
 * setting a depth instead. */
#define BINNACLE_EXCEPT_CAPPED 2

/* Opens or closes the group of axis headed by the Group Item key, and
 * requests the result with BINNACLE_VIEW_EDIT_EXPAND or COLLAPSE. Toggle
 * does whichever the latest request does not, so a second toggle before the
 * answer takes back the first. Nothing unless the Window holds the Group
 * Item and it can open. BINNACLE_EXCEPT_CAPPED past the cap. */
int binnacle_grid_expand(BinnacleGrid *grid, uint32_t axis, uint64_t key);
int binnacle_grid_collapse(BinnacleGrid *grid, uint32_t axis, uint64_t key);
int binnacle_grid_toggle(BinnacleGrid *grid, uint32_t axis, uint64_t key);

/* Opens the group headed by key and every group under it whose Group Item
 * the Window holds, and those later Windows bring, until another toggle
 * (BINNACLE_VIEW_EDIT_EXPAND_ALL). */
int binnacle_grid_expand_all(BinnacleGrid *grid, uint32_t axis, uint64_t key);

/* Opens axis to depth levels (every level when all is 1) and clears its
 * exceptions (BINNACLE_VIEW_EDIT_EXPAND_TO). */
int binnacle_grid_expand_to(BinnacleGrid *grid, uint32_t axis, uint32_t all, uint32_t depth);

/* Requests the whole of spec (NULL for the base view), say to restore a saved
 * view, with BINNACLE_VIEW_EDIT_REPLACE. 0 as binnacle_grid_view_applied. */
int binnacle_grid_request_view(BinnacleGrid *grid, const BinnacleViewSpec *spec);

/* Requests the latest View Spec again with BINNACLE_VIEW_EDIT_REAPPLY: the
 * Table Source filters rows the user's edits left in view though they no
 * longer match. 0 for NULL. */
int binnacle_grid_reapply_view(BinnacleGrid *grid);

/* Sets the Find Query, or closes find with NULL or empty text, emitting
 * FIND_REQUESTED (nothing if it is unchanged). 0 for an unknown tag or flag,
 * or text that is not UTF-8. */
int binnacle_grid_set_find(BinnacleGrid *grid, const BinnacleFindQuery *query);

/* The Find Next and Find Previous Actions: FIND_STEP_REQUESTED from the
 * current match, else the focused cell. Nothing while find is closed. */
int binnacle_grid_find_next(BinnacleGrid *grid);
int binnacle_grid_find_previous(BinnacleGrid *grid);

/* The answer to MOVE_REQUESTED: NULL refusal once the items moved, else
 * the reason the Table Source refused, len bytes of UTF-8, which the
 * Snapshot carries until the next press. 0 if it is not UTF-8. */
int binnacle_grid_move_answered(BinnacleGrid *grid, const uint8_t *refusal, size_t len);

/* The Activate Action (Enter): ACTIVATED for the focused cell. Nothing
 * without one. */
int binnacle_grid_activate(BinnacleGrid *grid);

/* The Copy Action (Ctrl/Cmd+C): COPY_REQUESTED for the range, or the
 * focused cell without one. Nothing without either. */
int binnacle_grid_copy(BinnacleGrid *grid);

/* The Begin Edit Action (F2): EDIT_REQUESTED for the focused cell. Nothing
 * without one. */
int binnacle_grid_begin_edit(BinnacleGrid *grid);

/* The host opened its editor on the cell with row and col (after
 * EDIT_REQUESTED, ACTIVATED or TEXT_TYPED, say): the Grid follows those Keys
 * and sends EDIT_CANCELLED if it ends the edit itself. A press ends it, so a
 * host that commits on a click elsewhere calls binnacle_grid_edit_ended
 * first. */
int binnacle_grid_edit_started(BinnacleGrid *grid, uint64_t row, uint64_t col);

/* The host closed its editor, with BINNACLE_EDIT_COMMIT or CANCEL. */
int binnacle_grid_edit_ended(BinnacleGrid *grid, uint32_t end);

/* Text the user typed; 0 if it is not UTF-8. */
int binnacle_grid_type_text(BinnacleGrid *grid, const uint8_t *text, size_t len);

/* Moves focus along axis by BINNACLE_FOCUS_*, towards the end if forward, and
 * scrolls to show it; extend stretches the range instead of replacing it. */
int binnacle_grid_move_focus(BinnacleGrid *grid, uint32_t axis, uint32_t by,
                             uint32_t forward, uint32_t extend);

int binnacle_grid_select_all(BinnacleGrid *grid);

/* Every row of the range's columns, or of the focused column. */
int binnacle_grid_select_column(BinnacleGrid *grid);

/* Every column of the range's rows, or of the focused row. */
int binnacle_grid_select_row(BinnacleGrid *grid);

/* Ends the gesture in progress, restoring focus and selection from before it. */
int binnacle_grid_cancel(BinnacleGrid *grid);

/* The Leave Outline Action (Tab): focus leaves the outline column for the
 * body. In the outline column, binnacle_grid_move_focus one item along the
 * columns walks the tree (forward opens or steps in, back closes or steps
 * out), binnacle_grid_activate toggles the focused group, and typed "*" or
 * a digit opens everything under it or the rows to that many levels. One
 * item back from the first column enters it. */
int binnacle_grid_leave_outline(BinnacleGrid *grid);

/* Scrolling. */

/* By a distance in the host's unit: dx across the columns, dy down the rows. */
int binnacle_grid_scroll(BinnacleGrid *grid, float dx, float dy);

/* Puts axis's thumb at fraction of its travel: 0 the start, 1 the end. */
int binnacle_grid_scroll_to(BinnacleGrid *grid, uint32_t axis, double fraction);

/* By whole items, landing on an item start. */
int binnacle_grid_scroll_by(BinnacleGrid *grid, uint32_t axis, int64_t items);

/*
 * The grouping layer (binnacle-group) for a host whose aggregate backend
 * answers when it likes: a remote service, a worker, a database. The layer
 * turns a View Spec's grouping into Aggregate Queries, one batch a frame,
 * and the backend's answers into the grouped axis's items, Summary Item
 * Keys, Subtotals and Grand Total, as a Table Source shows them to the
 * Grid. It holds the open groups and a bounded cache of pages: memory grows
 * with what the user opens, never with the rows.
 *
 * The host gives it each View Spec (binnacle_group_layer_request_view),
 * sends each polled batch to the backend, and hands back the backend's
 * answers as they come: in any order, in parts, jointly (one answer for
 * several queries), with more than asked, or unasked. Answers for a batch a
 * newer View Spec superseded are dropped. A query is done once an answer
 * names it, or once what was answered covers it. Each call's output arrays
 * stay valid until the next call of the same function on the same layer,
 * or binnacle_group_layer_free.
 */
typedef struct BinnacleGroupLayer BinnacleGroupLayer;

/* BinnacleGroupRequest.want: the groups of the next level under the path,
 * start to end of them, each with its value, figures and child count; the
 * group's Leaf Keys, start to end of them; where the Leaf with key is (its
 * group's path and its place among that group's Leaves; the path and range
 * are empty); or the figures of the whole group at the path (at the empty
 * path, the Grand Total's; the range is empty). */
#define BINNACLE_WANT_GROUPS 0
#define BINNACLE_WANT_LEAVES 1
#define BINNACLE_WANT_LOCATE 2
#define BINNACLE_WANT_TOTAL 3

/* BinnacleGroupAnswer.body: which of its parts the answer fills. */
#define BINNACLE_BODY_GROUPS 0
#define BINNACLE_BODY_LEAVES 1
#define BINNACLE_BODY_LOCATED 2
#define BINNACLE_BODY_TOTAL 3

/* BinnacleFigure.kind. */
#define BINNACLE_FIGURE_BLANK 0
#define BINNACLE_FIGURE_NUMBER 1
#define BINNACLE_FIGURE_TEXT 2

/* A figure a backend computes, one per value of the grouping: a number,
 * text, or a blank. */
typedef struct {
  uint32_t kind;
  double number;
  BinnacleText text;
} BinnacleFigure;

/* One level's field and the group's value there. A path is the steps from
 * the outermost level; the empty path is the whole axis. */
typedef struct {
  uint64_t field;
  BinnacleValue value;
} BinnacleGroupStep;

typedef struct {
  uint64_t id;
  const BinnacleGroupStep *path;
  size_t path_len;
  uint32_t want;
  /* LOCATE: the Leaf's Key. */
  uint64_t key;
  uint64_t start;
  uint64_t end;
} BinnacleGroupRequest;

/* What every query of a batch shares: the level fields (outermost first),
 * the values to aggregate and the slicers, as in BinnacleGrouping, and the
 * grouped axis's sort, filter and hidden Leaf Keys. The backend filters the
 * Leaves by the slicers, the filter and the hidden Keys before it groups
 * and aggregates them, and orders siblings by the sort. */
typedef struct {
  const uint64_t *levels;
  size_t levels_len;
  const BinnacleAggregateValue *values;
  size_t values_len;
  const BinnacleCondition *slicers;
  size_t slicers_len;
  BinnacleAxisView rows;
} BinnacleGroupScope;

/* One frame's queries, for the View Spec stamp names. */
typedef struct {
  uint64_t stamp;
  BinnacleGroupScope scope;
  const BinnacleGroupRequest *requests;
  size_t requests_len;
} BinnacleGroupBatch;

/* One group: its value at its level, how many children it has (groups of
 * the next level, or Leaves at the innermost), and its figures. */
typedef struct {
  BinnacleValue value;
  uint64_t children;
  const BinnacleFigure *figures;
  size_t figures_len;
} BinnacleGroupRow;

/*
 * A backend's answer, self-describing so it can come in any order, in
 * parts or unasked: the batch's stamp, the request it answers (has_id 0
 * for data not asked for), and where its body belongs. Fields its body does
 * not use are ignored.
 */
typedef struct {
  uint64_t stamp;
  uint32_t has_id;
  uint64_t id;
  const BinnacleGroupStep *path;
  size_t path_len;
  uint64_t start;
  uint32_t body;
  /* GROUPS: how many groups there are under the path in all, and those
   * from start. */
  uint64_t total;
  const BinnacleGroupRow *groups;
  size_t groups_len;
  /* LEAVES: the group's Leaf Keys from start. */
  const uint64_t *keys;
  size_t keys_len;
  /* LOCATED: the Leaf's Key, and whether it is under path at start (1) or
   * left out (0). */
  uint64_t key;
  uint32_t found;
  /* TOTAL: the figures of the whole group at the path. */
  const BinnacleFigure *figures;
  size_t figures_len;
} BinnacleGroupAnswer;

/* A Change Notice for the Grid (binnacle_grid_notify's arguments). */
typedef struct {
  uint32_t axis;
  uint32_t notice;
  uint64_t at;
  uint64_t n;
} BinnacleGroupNotice;

/* A View Spec the layer applied, for binnacle_grid_view_applied, and the
 * Change Notices it brings. */
typedef struct {
  BinnacleViewSpec applied;
  BinnacleTotals totals;
  const BinnacleGroupNotice *notices;
  size_t notices_len;
} BinnacleGroupViewed;

/* An item of a grouped axis, for binnacle_grid_set_window and
 * binnacle_grid_set_outlines. */
typedef struct {
  uint64_t key;
  BinnacleOutline outline;
} BinnacleGroupItem;

/* A Summary Item: its kind (BINNACLE_ITEM_*), its group's path, how many
 * children it has, and its figures. */
typedef struct {
  uint32_t kind;
  const BinnacleGroupStep *path;
  size_t path_len;
  uint64_t children;
  const BinnacleFigure *figures;
  size_t figures_len;
} BinnacleGroupSummary;

/* A new layer caching up to cache closed groups and Leaves besides those
 * the latest Windows read, or the default if has_cache is 0. NULL for a
 * flag that is not 0 or 1. Free it with binnacle_group_layer_free. */
BinnacleGroupLayer *binnacle_group_layer_new(uint32_t has_cache, uint64_t cache);

void binnacle_group_layer_free(BinnacleGroupLayer *layer);

/* Takes the View Spec asked for; NULL is the base view. 0 as for
 * binnacle_grid_view_applied. It does not pivot yet: it drops the column
 * levels and the values pseudo-level, and the applied View Spec says so. */
int binnacle_group_layer_request_view(BinnacleGroupLayer *layer, const BinnacleViewSpec *spec);

/* Starts the grouped axes afresh under the same View Spec, after the data
 * changed. */
int binnacle_group_layer_refresh(BinnacleGroupLayer *layer);

/* 1 if axis is grouped, and so the layer's; else 0. */
int binnacle_group_layer_grouped(BinnacleGroupLayer *layer, uint32_t axis);

/* Takes the next batch for the backend into out: 1 if there was one. */
int binnacle_group_layer_poll_batch(BinnacleGroupLayer *layer, BinnacleGroupBatch *out);

/* Takes one of the backend's answers. 0 for an unknown tag or flag, or text
 * that is not UTF-8. */
int binnacle_group_layer_answer(BinnacleGroupLayer *layer, const BinnacleGroupAnswer *answer);

/* Takes the next applied View Spec into out: 1 if there was one. */
int binnacle_group_layer_poll_viewed(BinnacleGroupLayer *layer, BinnacleGroupViewed *out);

/* How many items axis shows, into out: 1 if known, else 0. */
int binnacle_group_layer_count(BinnacleGroupLayer *layer, uint32_t axis, uint64_t *out);

/* The items of axis at positions start to end, up to the first one not
 * yet answered, into *items and *len; the next batch asks for the rest,
 * and the host asks again once answers land. */
int binnacle_group_layer_window(BinnacleGroupLayer *layer, uint32_t axis, uint64_t start,
                                uint64_t end, const BinnacleGroupItem **items, size_t *len);

/* Where the item with key is on axis, as binnacle_grid_located takes it:
 * 1 with *answer (BINNACLE_LOCATED_*) and *position, or 0 while the backend
 * is asked. */
int binnacle_group_layer_locate(BinnacleGroupLayer *layer, uint32_t axis, uint64_t key,
                                uint32_t *answer, uint64_t *position);

/* The Summary Item with key into out: 1 if the layer holds it and its
 * figures, else 0. */
int binnacle_group_layer_summary(BinnacleGroupLayer *layer, uint64_t key,
                                 BinnacleGroupSummary *out);

/* 1 if key may still name an item of axis: a Leaf Key, a Key the layer
 * never issued, the Grand Total, a group's (a Group Item's, a Subtotal's)
 * while its path fits the axis's levels as last asked; 0 for one retired,
 * which the host has the Grid forget (binnacle_grid_forget). 1 for NULL. */
int binnacle_group_layer_fits(const BinnacleGroupLayer *layer, uint32_t axis, uint64_t key);

/* How many groups and Leaves the layer holds; 0 for NULL. */
uint64_t binnacle_group_layer_held(const BinnacleGroupLayer *layer);

#ifdef __cplusplus
}
#endif

#endif
