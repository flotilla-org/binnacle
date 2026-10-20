/*
 * binnacle's internal C API, for wheelhouse. It mirrors the Rust Grid only as
 * far as wheelhouse needs and makes no stability promise: the header and
 * library change together. The Rust docs on binnacle-core describe each
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

/* An axis, also the index into per-axis arrays. */
#define BINNACLE_ROWS 0
#define BINNACLE_COLS 1

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

/* BinnacleSnapshot.mode. Modes the Grid gained after this header read as
 * BINNACLE_MODE_OTHER. */
#define BINNACLE_MODE_OTHER 0
#define BINNACLE_MODE_IDLE 1
#define BINNACLE_MODE_DRAGGING_SELECTION 2
/* The edge of a Header Band is being dragged; the band's axis snapshot shows
 * its thickness as it follows the pointer. */
#define BINNACLE_MODE_RESIZING_BAND 3

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
 * order. */
#define BINNACLE_EFFECT_FIND_STEP_REQUESTED 9

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

/* BinnacleCondition.kind. Kinds the Grid gained after this header read as
 * BINNACLE_CONDITION_OTHER, and are refused as input. TEXT: the value
 * matches the condition's text query. ONE_OF: the value is one of the
 * condition's values exactly (none keeps nothing). */
#define BINNACLE_CONDITION_OTHER 0
#define BINNACLE_CONDITION_TEXT 1
#define BINNACLE_CONDITION_ONE_OF 2

/* BinnacleListedCondition.state: applied and still requested, requested and
 * not yet applied, applied and no longer requested, or requested and left
 * out of the applied View Spec with no answer due (rejected, failed, or no
 * longer supported). */
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
 * item's distinct values. */
#define BINNACLE_SUPPORT_SORT 1u
#define BINNACLE_SUPPORT_TEXT 2u
#define BINNACLE_SUPPORT_TEXT_REGEX 4u
#define BINNACLE_SUPPORT_ONE_OF 8u

/* binnacle_grid_set_quick_filter_support bits and
 * BinnacleAxisSnapshot.quick_filter: the axis has a quick filter, and it
 * takes regexes. 0 is none, the default. REGEX needs QUICK_FILTER. */
#define BINNACLE_QUICK_FILTER 1u
#define BINNACLE_QUICK_FILTER_REGEX 2u

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

/*
 * One visible item on an axis. offset is from the viewport's leading edge,
 * header band included. has_key is 0 while the Table Source has not filled
 * the Position: draw it as loading.
 */
typedef struct {
  uint64_t position;
  uint32_t has_key;
  uint64_t key;
  float offset;
  float size;
} BinnacleStripItem;

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

/* UTF-8 text, length-delimited and not NUL-terminated. */
typedef struct {
  const uint8_t *data;
  size_t len;
} BinnacleText;

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

/* Defined with BinnacleCondition, below. */
typedef struct BinnacleListedCondition BinnacleListedCondition;

typedef struct {
  uint64_t count;
  /* The viewport's length along the axis, header band included. */
  float extent;
  float header;
  float gap;
  BinnacleScrollAnchor anchor;
  BinnacleThumb thumb;
  const BinnacleStripItem *strip;
  size_t strip_len;
  /* The headers of the strip's items, in strip order (headers_len ==
   * strip_len): column headers on BINNACLE_COLS, row headers on ROWS. */
  const BinnacleHeader *headers;
  size_t headers_len;
  /* Set when BinnacleSnapshot.has_focus is. */
  BinnacleFocusEnd focus;
  /* Set when BinnacleSnapshot.has_selection_range is. */
  BinnacleRange selection_range;
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
  /* The axis's picked Keys, in Key order. */
  const uint64_t *picked;
  size_t picked_len;
} BinnacleAxisSnapshot;

/*
 * What to draw this frame. Its arrays stay valid until the next
 * binnacle_grid_snapshot or binnacle_grid_free on the same Grid.
 */
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
  /* The one contiguous range, clamped to the counts. */
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
} BinnacleSnapshot;

/* One column of a Sort Spec: the column's Key and BINNACLE_SORT_*. */
typedef struct {
  uint64_t key;
  uint32_t direction;
} BinnacleSortColumn;

/* A value as a ONE_OF condition lists it: text in the Table Source's own
 * text form, or with blank 1 a blank (no value at all, not empty text), its
 * text ignored. */
typedef struct {
  uint32_t blank;
  BinnacleText text;
} BinnacleValue;

/* One condition on the items of an axis, read in item key of the other axis
 * (a column, for the rows): the items whose value there matches are kept, or
 * with negated 1 those whose value does not. A TEXT condition matches text;
 * its query searches key alone (as input, searched and keys are ignored). A
 * ONE_OF condition lists values_len values; other kinds ignore them. */
typedef struct {
  uint64_t key;
  /* BINNACLE_CONDITION_*. */
  uint32_t kind;
  uint32_t negated;
  BinnacleTextQuery text;
  const BinnacleValue *values;
  size_t values_len;
} BinnacleCondition;

struct BinnacleListedCondition {
  /* BINNACLE_CONDITION_APPLIED, ADDING or REMOVING. */
  uint32_t state;
  BinnacleCondition condition;
};

/* One axis's part of a View Spec: its sort, by items of the other axis in
 * order (empty for unsorted), and its filter: the quick filter, which keeps
 * the items with a match for it, when has_quick is 1, and the conditions,
 * which keep the items every one of them keeps. */
typedef struct {
  const BinnacleSortColumn *sort;
  size_t sort_len;
  uint32_t has_quick;
  BinnacleTextQuery quick;
  const BinnacleCondition *conditions;
  size_t conditions_len;
} BinnacleAxisView;

/* The view the Table Source should present: axes[BINNACLE_ROWS] and
 * axes[BINNACLE_COLS]. */
typedef struct {
  BinnacleAxisView axes[2];
} BinnacleViewSpec;

/* The cause of a requested View Spec, a hint: the spec alone is the request. */
typedef struct {
  uint32_t kind;
  uint32_t axis;
  uint64_t key;
  uint32_t multi;
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
  /* FOCUS_CHANGED: the focused cell's row and column Keys. */
  uint64_t row;
  uint64_t col;
  /* VIEW_REQUESTED: the whole requested View Spec, its arrays valid until
   * the next binnacle_grid_poll_effect or binnacle_grid_free on the same
   * Grid, and the edit that caused it. */
  BinnacleViewSpec view;
  BinnacleViewEdit edit;
  /* LOCATE: the axis and the Key to find. BAND_SIZE_CHANGED and
   * BAND_AUTO_SIZE_REQUESTED: the axis the band leads. */
  uint32_t axis;
  uint64_t key;
  /* BAND_SIZE_CHANGED: the band's new thickness. */
  float size;
  /* FIND_REQUESTED and FIND_STEP_REQUESTED: the Find Query, its arrays valid
   * as view's are. */
  uint32_t has_find;
  BinnacleFindQuery find;
  /* FIND_STEP_REQUESTED: the cell to step from, and BINNACLE_FIND_*. */
  uint32_t has_from;
  BinnacleFindMatch from;
  uint32_t direction;
} BinnacleEffect;

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

/* Lays out the frame and fills out. */
int binnacle_grid_snapshot(BinnacleGrid *grid, BinnacleSnapshot *out);

/* Takes the oldest queued Effect into out: 1 if there was one, else 0. */
int binnacle_grid_poll_effect(BinnacleGrid *grid, BinnacleEffect *out);

/* Answers. */

/* The answer to VIEW_REQUESTED, or the Table Source's own news: the View
 * Spec now applied, which may be part of the request, or the previous one if
 * the request was rejected, with each axis's unfiltered count where known.
 * NULL spec is the base view, NULL totals knows neither. 0 for an unknown
 * direction, tag or flag, or text that is not UTF-8. */
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

/* What the Table Source supports when viewing axis by key, an item of the
 * other axis: BINNACLE_SUPPORT_* bits, the whole declaration. Every item
 * supports SORT | TEXT (not ONE_OF) until set otherwise. 0 for unknown bits,
 * or REGEX without TEXT. */
int binnacle_grid_set_support(BinnacleGrid *grid, uint32_t axis, uint64_t key,
                              uint32_t support);

/* The answer to LOCATE for key on axis: BINNACLE_LOCATED_*, with position
 * for FOUND and GONE_SUGGEST (ignored for GONE). */
int binnacle_grid_located(BinnacleGrid *grid, uint32_t axis, uint64_t key, uint32_t answer,
                          uint64_t position);

/* The answer to FIND_STEP_REQUESTED: the match stepped to, or NULL if
 * nothing matches. The match takes focus and is scrolled into view; the
 * selection is left as it is. */
int binnacle_grid_find_stepped(BinnacleGrid *grid, const BinnacleFindMatch *found);

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

/* Scrolling. */

/* By a distance in the host's unit: dx across the columns, dy down the rows. */
int binnacle_grid_scroll(BinnacleGrid *grid, float dx, float dy);

/* Puts axis's thumb at fraction of its travel: 0 the start, 1 the end. */
int binnacle_grid_scroll_to(BinnacleGrid *grid, uint32_t axis, double fraction);

/* By whole items, landing on an item start. */
int binnacle_grid_scroll_by(BinnacleGrid *grid, uint32_t axis, int64_t items);

#ifdef __cplusplus
}
#endif

#endif
