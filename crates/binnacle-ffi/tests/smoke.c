/* Drives one Grid through the C API the way wheelhouse does. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "binnacle.h"

#define CHECK(cond)                                                        \
  do {                                                                     \
    if (!(cond)) {                                                         \
      fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__,     \
              #cond);                                                      \
      exit(1);                                                             \
    }                                                                      \
  } while (0)

/* The Table Source: row Keys are 1000 + Position, column Keys 2000 +
 * Position; once sorted descending the 100 rows are in reverse. */
static int reversed = 0;

static uint64_t row_key(uint64_t position) {
  return 1000 + (reversed ? 99 - position : position);
}

static void fill(BinnacleGrid *grid, const BinnacleWindowRequests *requests) {
  for (uint32_t axis = 0; axis < 2; axis++) {
    const BinnacleWindowRequest *r = &requests->axes[axis];
    if (!r->requested) continue;
    uint64_t keys[64];
    size_t len = (size_t)(r->end - r->start);
    CHECK(len <= 64);
    for (size_t i = 0; i < len; i++)
      keys[i] = axis == BINNACLE_ROWS ? row_key(r->start + i) : 2000 + r->start + i;
    CHECK(binnacle_grid_set_window(grid, axis, r->start, keys, len));
  }
}

static BinnaclePointerEvent at(uint32_t kind, float x, float y, uint32_t modifiers) {
  BinnaclePointerEvent e = {.kind = kind, .x = x, .y = y,
                            .button = BINNACLE_BUTTON_PRIMARY,
                            .modifiers = modifiers, .clicks = 1};
  return e;
}

static void click(BinnacleGrid *grid, float x, float y, uint32_t modifiers) {
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_DOWN, x, y, modifiers)));
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_UP, x, y, modifiers)));
}

/* Drains the queue, returning a bit per Effect kind seen. */
static uint32_t drain(BinnacleGrid *grid, BinnacleEffect *last_focus) {
  uint32_t seen = 0;
  BinnacleEffect effect;
  while (binnacle_grid_poll_effect(grid, &effect)) {
    seen |= 1u << effect.kind;
    if (effect.kind == BINNACLE_EFFECT_FOCUS_CHANGED) *last_focus = effect;
  }
  return seen;
}

static void frame(BinnacleGrid *grid, BinnacleSnapshot *snap) {
  BinnacleWindowRequests requests;
  CHECK(binnacle_grid_begin_frame(grid, 40, 6, &requests));
  fill(grid, &requests);
  CHECK(binnacle_grid_snapshot(grid, snap));
}

int main(void) {
  BinnacleAxisConfig bad = {.count = 1, .default_size = 0, .header = 0, .gap = 0};
  BinnacleAxisConfig rows = {.count = 100, .default_size = 1, .header = 1, .gap = 0};
  BinnacleAxisConfig cols = {.count = 10, .default_size = 8, .header = 4, .gap = 1};
  CHECK(binnacle_grid_new(bad, bad) == NULL);
  BinnacleGrid *grid = binnacle_grid_new(rows, cols);
  CHECK(grid != NULL);

  CHECK(!binnacle_grid_snapshot(NULL, NULL));
  BinnacleWindowRequests requests;
  binnacle_grid_begin_frame(grid, 40, 6, &requests);
  CHECK(requests.axes[BINNACLE_ROWS].requested);
  CHECK(requests.axes[BINNACLE_ROWS].start == 0);
  CHECK(requests.axes[BINNACLE_COLS].requested);
  fill(grid, &requests);

  BinnacleSnapshot snap;
  binnacle_grid_snapshot(grid, &snap);
  CHECK(snap.generation == 1);
  CHECK(snap.damage == (BINNACLE_DAMAGE_LAYOUT | BINNACLE_DAMAGE_SELECTION |
                        BINNACLE_DAMAGE_POSITIONS));
  const BinnacleAxisSnapshot *r = &snap.axes[BINNACLE_ROWS];
  const BinnacleAxisSnapshot *c = &snap.axes[BINNACLE_COLS];
  CHECK(r->count == 100 && r->extent == 6 && r->header == 1);
  CHECK(c->count == 10 && c->extent == 40 && c->header == 4 && c->gap == 1);
  /* Five rows of 1 fit below the 1-high header band. */
  CHECK(r->strip_len == 5);
  CHECK(r->strip[0].position == 0 && r->strip[0].has_key);
  CHECK(r->strip[0].key == 1000 && r->strip[0].offset == 1 && r->strip[0].size == 1);
  CHECK(r->strip[4].key == 1004 && r->strip[4].offset == 5);
  /* Columns of 8 with a Gap of 1 after the 4-wide header band: 4, 13, 22, 31. */
  CHECK(c->strip_len == 4);
  CHECK(c->strip[3].key == 2003 && c->strip[3].offset == 31);
  CHECK(r->anchor.position == 0 && r->anchor.offset == 0);
  CHECK(r->thumb.fraction == 0 && r->thumb.size > 0 && r->thumb.size < 1);

  /* Typed text comes back as an Effect stamped with the frame it was typed on. */
  const char typed[] = "h\xc3\xa9llo";
  CHECK(binnacle_grid_type_text(grid, (const uint8_t *)typed, strlen(typed)));
  const uint8_t not_utf8[] = {0xff};
  CHECK(!binnacle_grid_type_text(grid, not_utf8, 1));
  BinnacleEffect effect;
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.generation == 1 && effect.kind == BINNACLE_EFFECT_TEXT_TYPED);
  CHECK(effect.text.len == strlen(typed));
  CHECK(memcmp(effect.text.data, typed, effect.text.len) == 0);
  CHECK(!binnacle_grid_poll_effect(grid, &effect));

  /* The first focus move lands on the first visible cell. */
  CHECK(binnacle_grid_move_focus(grid, BINNACLE_ROWS, BINNACLE_FOCUS_ITEM, 1, 0));
  CHECK(!binnacle_grid_move_focus(grid, 7, BINNACLE_FOCUS_ITEM, 1, 0));
  CHECK(!binnacle_grid_move_focus(grid, BINNACLE_ROWS, 7, 1, 0));
  BinnacleEffect focus;
  CHECK(drain(grid, &focus) == 1u << BINNACLE_EFFECT_FOCUS_CHANGED);
  CHECK(focus.generation == 1 && focus.row == 1000 && focus.col == 2000);

  /* Click row 2, column 1, then Shift-click row 3, column 2. */
  click(grid, 15, 3.5f, 0);
  click(grid, 24, 4.5f, BINNACLE_MOD_SHIFT);
  CHECK(!binnacle_grid_pointer(grid, at(BINNACLE_POINTER_UP, 0, 0, 16)));
  CHECK(drain(grid, &focus) == (1u << BINNACLE_EFFECT_FOCUS_CHANGED |
                                1u << BINNACLE_EFFECT_SELECTION_CHANGED));
  CHECK(focus.row == 1003 && focus.col == 2002);

  frame(grid, &snap);
  CHECK(snap.generation == 2 && (snap.damage & BINNACLE_DAMAGE_SELECTION));
  CHECK(snap.has_focus);
  CHECK(r->focus.position == 3 && r->focus.has_key && r->focus.key == 1003);
  CHECK(c->focus.position == 2 && c->focus.key == 2002);
  CHECK(snap.has_focus_rect && snap.focus_rect.x == 22 && snap.focus_rect.y == 4);
  CHECK(snap.focus_rect.width == 8 && snap.focus_rect.height == 1);
  CHECK(snap.has_selection_range);
  CHECK(r->selection_range.start == 2 && r->selection_range.end == 4);
  CHECK(c->selection_range.start == 1 && c->selection_range.end == 3);
  CHECK(snap.selection_rects_len == 1);
  CHECK(snap.selection_rects[0].x == 13 && snap.selection_rects[0].y == 3);
  CHECK(snap.selection_rects[0].width == 17 && snap.selection_rects[0].height == 2);
  CHECK(snap.picked_len == 0);
  CHECK(snap.cell_states_len == 5 * 4);
  CHECK(snap.cell_states[3 * 4 + 2] == (BINNACLE_CELL_FOCUSED | BINNACLE_CELL_SELECTED));
  CHECK(snap.cell_states[2 * 4 + 1] == BINNACLE_CELL_SELECTED);
  CHECK(snap.cell_states[0] == 0);
  CHECK(snap.mode == BINNACLE_MODE_IDLE && !snap.capturing);
  CHECK(!snap.has_hovered_header);

  /* Ctrl-click picks row 4; hovering column 1's header shows it. */
  click(grid, 6, 5.5f, BINNACLE_MOD_CTRL);
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_MOVE, 15, 0.5f, 0)));
  frame(grid, &snap);
  CHECK(snap.picked_len == 1 && snap.picked[0] == 1004);
  CHECK(snap.has_hovered_header && snap.hovered_axis == BINNACLE_COLS);
  CHECK(snap.hovered_index == 1);

  /* A drag, cancelled. */
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_DOWN, 6, 1.5f, 0)));
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_MOVE, 15, 3.5f, 0)));
  frame(grid, &snap);
  CHECK(snap.mode == BINNACLE_MODE_DRAGGING_SELECTION && snap.capturing);
  CHECK(binnacle_grid_cancel(grid));
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_UP, 15, 3.5f, 0)));
  frame(grid, &snap);
  CHECK(snap.mode == BINNACLE_MODE_IDLE && r->focus.position == 4);

  CHECK(binnacle_grid_select_all(grid));
  frame(grid, &snap);
  CHECK(r->selection_range.end == 100 && c->selection_range.end == 10);
  CHECK(binnacle_grid_select_column(grid));
  drain(grid, &focus);

  /* Scrolling. */
  CHECK(binnacle_grid_scroll(grid, 0, 3));
  frame(grid, &snap);
  CHECK(r->anchor.position == 3 && r->strip[0].key == 1003);
  CHECK(binnacle_grid_scroll_by(grid, BINNACLE_ROWS, 2));
  frame(grid, &snap);
  CHECK(r->anchor.position == 5);
  CHECK(binnacle_grid_scroll_to(grid, BINNACLE_ROWS, 1.0));
  frame(grid, &snap);
  CHECK(r->anchor.position == 95 && r->thumb.fraction == 1);
  CHECK(!binnacle_grid_scroll_to(grid, 2, 0.5));

  /* A sort round trip. Two clicks on column 0's header ask for ascending,
   * then descending. The source rejects the first (answering the previous,
   * unsorted, spec) and applies the second by reversing the rows, says so,
   * then answers where the focused row went. */
  CHECK(binnacle_grid_scroll_to(grid, BINNACLE_ROWS, 0));
  frame(grid, &snap);
  click(grid, 6, 0.5f, 0);
  click(grid, 6, 0.5f, 0);
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  BinnacleAxisView *sorted = &effect.view.axes[BINNACLE_ROWS];
  CHECK(effect.kind == BINNACLE_EFFECT_VIEW_REQUESTED && sorted->sort_len == 1);
  CHECK(sorted->sort[0].key == 2000 && sorted->sort[0].direction == BINNACLE_SORT_ASCENDING);
  CHECK(effect.view.axes[BINNACLE_COLS].sort_len == 0);
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_CYCLE_SORT && effect.edit.axis == BINNACLE_ROWS);
  CHECK(effect.edit.key == 2000 && !effect.edit.multi);
  BinnacleSortColumn unknown[] = {{.key = 2000, .direction = 7}};
  BinnacleViewSpec misdirected = {.axes = {{.sort = unknown, .sort_len = 1}, {0}}};
  CHECK(!binnacle_grid_view_applied(grid, &misdirected));
  CHECK(binnacle_grid_view_applied(grid, NULL));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_VIEW_REQUESTED && sorted->sort_len == 1);
  CHECK(sorted->sort[0].key == 2000 && sorted->sort[0].direction == BINNACLE_SORT_DESCENDING);
  frame(grid, &snap);
  CHECK(c->headers_len == c->strip_len && c->headers[0].key == 2000);
  CHECK(c->headers[0].flags & BINNACLE_HEADER_PENDING);
  CHECK(!(c->headers[0].flags & BINNACLE_HEADER_SORTED));
  BinnacleSortColumn column = sorted->sort[0];
  BinnacleViewSpec spec = {.axes = {{.sort = &column, .sort_len = 1}, {0}}};
  reversed = 1;
  CHECK(binnacle_grid_view_applied(grid, &spec));
  CHECK(!binnacle_grid_notify(grid, BINNACLE_ROWS, 9, 0, 0));
  CHECK(binnacle_grid_notify(grid, BINNACLE_ROWS, BINNACLE_NOTICE_REORDERED, 0, 0));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_LOCATE && effect.axis == BINNACLE_ROWS);
  uint64_t focused = effect.key;
  CHECK(focused == 1004);
  CHECK(!binnacle_grid_located(grid, BINNACLE_ROWS, focused, 9, 0));
  CHECK(binnacle_grid_located(grid, BINNACLE_ROWS, focused, BINNACLE_LOCATED_FOUND,
                              99 - (focused - 1000)));
  drain(grid, &focus);
  frame(grid, &snap);
  CHECK(r->strip[0].key == 1099);
  /* Answered: the arrow shows and nothing is pending. */
  uint32_t sort_bits = BINNACLE_HEADER_SORTED | BINNACLE_HEADER_DESCENDING | BINNACLE_HEADER_PENDING;
  CHECK((c->headers[0].flags & sort_bits) == (BINNACLE_HEADER_SORTED | BINNACLE_HEADER_DESCENDING));
  CHECK(c->headers[0].sort_place == 0 && c->headers[0].rect.x == 4);
  CHECK(c->headers[0].rect.height == 1 && r->headers[0].rect.width == 4);
  CHECK(r->focus.position == 95 && r->focus.key == 1004);
  /* Once the source says it cannot sort by column 0, its header asks for
   * nothing. */
  CHECK(!binnacle_grid_set_support(grid, 2, 2000, 0));
  CHECK(binnacle_grid_set_support(grid, BINNACLE_ROWS, 2000, 0));
  click(grid, 6, 0.5f, 0);
  CHECK(!binnacle_grid_poll_effect(grid, &effect));
  CHECK(binnacle_grid_set_support(grid, BINNACLE_ROWS, 2000, BINNACLE_SUPPORT_SORT));
  /* Rows removed and a departed Key: focus takes the suggestion. */
  CHECK(binnacle_grid_notify(grid, BINNACLE_ROWS, BINNACLE_NOTICE_RESET, 0, 100));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_LOCATE && effect.key == 1004);
  CHECK(binnacle_grid_located(grid, BINNACLE_ROWS, 1004, BINNACLE_LOCATED_GONE_SUGGEST, 7));
  drain(grid, &focus);
  frame(grid, &snap);
  CHECK(r->focus.position == 7);

  /* Settings, sizes and content sizes. */
  CHECK(binnacle_grid_set_selection_unit(grid, BINNACLE_SELECT_ROWS));
  CHECK(!binnacle_grid_set_selection_unit(grid, 2));
  CHECK(binnacle_grid_set_header_click(grid, BINNACLE_COLS, BINNACLE_HEADER_CLICK_NONE));
  CHECK(!binnacle_grid_set_header_click(grid, BINNACLE_COLS, 3));
  CHECK(binnacle_grid_set_drag_threshold(grid, 2));
  CHECK(binnacle_grid_scroll_to(grid, BINNACLE_ROWS, 0));
  CHECK(binnacle_grid_set_size(grid, BINNACLE_COLS, 2000, 12));
  frame(grid, &snap);
  CHECK(c->strip[0].size == 12 && c->strip[1].offset == 17);
  click(grid, 6, 1.5f, 0);
  frame(grid, &snap);
  CHECK(snap.has_selection_range && c->selection_range.start == 0);
  CHECK(c->selection_range.end == 10 && r->selection_range.end == 1);
  uint64_t keys[] = {1000, 1001};
  float sizes[] = {2, 1};
  CHECK(binnacle_grid_set_window_sized(grid, BINNACLE_ROWS, 0, keys, sizes, 2, 1));
  CHECK(binnacle_grid_snapshot(grid, &snap));
  CHECK(r->strip[0].size == 2 && r->strip[1].offset == 3);

  /* The row-number band's edge is its last Gap's width, x 3..4, in the column
   * header band: dragging it resizes the band, reported on release. */
  drain(grid, &focus);
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_DOWN, 3.5f, 0.5f, 0)));
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_MOVE, 6.5f, 0.5f, 0)));
  frame(grid, &snap);
  CHECK(snap.mode == BINNACLE_MODE_RESIZING_BAND && snap.capturing);
  CHECK(c->header == 7 && c->strip[0].offset == 7);
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_UP, 6.5f, 0.5f, 0)));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_BAND_SIZE_CHANGED);
  CHECK(effect.axis == BINNACLE_COLS && effect.size == 7);
  CHECK(!binnacle_grid_poll_effect(grid, &effect));
  /* A double-click on the edge asks the host to fit the band. */
  BinnaclePointerEvent twice = at(BINNACLE_POINTER_DOWN, 6.5f, 0.5f, 0);
  twice.clicks = 2;
  CHECK(binnacle_grid_pointer(grid, twice));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_BAND_AUTO_SIZE_REQUESTED);
  CHECK(effect.axis == BINNACLE_COLS);
  CHECK(binnacle_grid_set_header(grid, BINNACLE_COLS, 0));
  CHECK(!binnacle_grid_set_header(grid, 2, 3));
  frame(grid, &snap);
  CHECK(snap.mode == BINNACLE_MODE_IDLE);
  CHECK(c->header == 0 && c->strip[0].offset == 0);
  CHECK(snap.damage & BINNACLE_DAMAGE_LAYOUT);

  binnacle_grid_free(grid);
  binnacle_grid_free(NULL);
  printf("ok\n");
  return 0;
}
