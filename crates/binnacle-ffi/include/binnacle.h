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
/* Inside the range, or in a picked row. */
#define BINNACLE_CELL_SELECTED 2u

/* BinnacleHeader.flags bits. SORTED: the item is in the other axis's sort in
 * the applied View Spec, DESCENDING says which way and sort_place where (0
 * first). PENDING: a requested View Spec that would change this item's part
 * of that sort is unanswered. SELECTED: the
 * whole item is selected (the range spans every item across it, or its row
 * is picked). HOVERED: the pointer is over it. */
#define BINNACLE_HEADER_SORTED 1u
#define BINNACLE_HEADER_DESCENDING 2u
#define BINNACLE_HEADER_PENDING 4u
#define BINNACLE_HEADER_SELECTED 8u
#define BINNACLE_HEADER_HOVERED 16u

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

/* BinnacleSortColumn.direction. Nulls go last either way. */
#define BINNACLE_SORT_ASCENDING 0
#define BINNACLE_SORT_DESCENDING 1

/* BinnacleViewEdit.kind. Edits the Grid gained after this header read as
 * BINNACLE_VIEW_EDIT_OTHER. CYCLE_SORT: a header click cycled the sort of
 * axis by key, an item of the other axis; multi is 1 for a Shift-click,
 * which kept the rest of the sort. */
#define BINNACLE_VIEW_EDIT_OTHER 0
#define BINNACLE_VIEW_EDIT_CYCLE_SORT 1

/* binnacle_grid_set_support bits. SORT: the axis may be sorted by the item. */
#define BINNACLE_SUPPORT_SORT 1u

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
  /* Picked row Keys, in Key order. */
  const uint64_t *picked;
  size_t picked_len;
  /* Rects to fill: the visible part of the range, then each visible picked
   * row. They may overlap. */
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
} BinnacleSnapshot;

/* One column of a Sort Spec: the column's Key and BINNACLE_SORT_*. */
typedef struct {
  uint64_t key;
  uint32_t direction;
} BinnacleSortColumn;

/* One axis's part of a View Spec: its sort, by items of the other axis in
 * order (empty for unsorted). */
typedef struct {
  const BinnacleSortColumn *sort;
  size_t sort_len;
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

/* UTF-8 text, length-delimited and not NUL-terminated. */
typedef struct {
  const uint8_t *data;
  size_t len;
} BinnacleText;

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
 * columns sort: SORT on row headers makes a click do nothing. */
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
 * the request was rejected. NULL is the base view. 0 for an unknown
 * direction. */
int binnacle_grid_view_applied(BinnacleGrid *grid, const BinnacleViewSpec *spec);

/* What the Table Source supports when viewing axis by key, an item of the
 * other axis: BINNACLE_SUPPORT_* bits. Every item supports everything until
 * set otherwise. */
int binnacle_grid_set_support(BinnacleGrid *grid, uint32_t axis, uint64_t key,
                              uint32_t support);

/* The answer to LOCATE for key on axis: BINNACLE_LOCATED_*, with position
 * for FOUND and GONE_SUGGEST (ignored for GONE). */
int binnacle_grid_located(BinnacleGrid *grid, uint32_t axis, uint64_t key, uint32_t answer,
                          uint64_t position);

/* A Change Notice for axis: BINNACLE_NOTICE_*, with at and n as each
 * describes (unused ones are ignored). */
int binnacle_grid_notify(BinnacleGrid *grid, uint32_t axis, uint32_t notice, uint64_t at,
                         uint64_t n);

/* Input. */

int binnacle_grid_pointer(BinnacleGrid *grid, BinnaclePointerEvent event);

/* Text the user typed; 0 if it is not UTF-8. */
int binnacle_grid_type_text(BinnacleGrid *grid, const uint8_t *text, size_t len);

/* Moves focus along axis by BINNACLE_FOCUS_*, towards the end if forward, and
 * scrolls to show it; extend stretches the range instead of replacing it. */
int binnacle_grid_move_focus(BinnacleGrid *grid, uint32_t axis, uint32_t by,
                             uint32_t forward, uint32_t extend);

int binnacle_grid_select_all(BinnacleGrid *grid);

/* Every row of the range's columns, or of the focused column. */
int binnacle_grid_select_column(BinnacleGrid *grid);

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
