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

/* The grouping layer over a backend the host answers: rows grouped by
 * field 2002 into "a" (3 Leaves) and "b" (2), closed. */
static void group_layer(void) {
  CHECK(binnacle_group_layer_new(2, 0) == NULL);
  BinnacleGroupLayer *layer = binnacle_group_layer_new(0, 0);
  CHECK(layer != NULL);
  BinnacleLevel level = {.kind = BINNACLE_LEVEL_FIELD, .field = 2002,
                         .subtotal = BINNACLE_PLACEMENT_OFF};
  BinnacleAggregateValue count = {.aggregate = {(const uint8_t *)"count", 5}};
  BinnacleViewSpec spec = {.has_grouping = 1};
  spec.grouping.axes[BINNACLE_ROWS].levels = &level;
  spec.grouping.axes[BINNACLE_ROWS].levels_len = 1;
  spec.grouping.values = &count;
  spec.grouping.values_len = 1;
  CHECK(binnacle_group_layer_request_view(layer, &spec));
  CHECK(binnacle_group_layer_grouped(layer, BINNACLE_ROWS));
  CHECK(!binnacle_group_layer_grouped(layer, BINNACLE_COLS));

  /* The first batch asks for the top groups. */
  BinnacleGroupBatch batch;
  CHECK(binnacle_group_layer_poll_batch(layer, &batch));
  CHECK(batch.scope.levels_len == 1 && batch.scope.levels[0] == 2002);
  CHECK(batch.scope.values_len == 1 && !batch.scope.values[0].has_field);
  CHECK(batch.requests_len >= 1);
  BinnacleFigure three = {.kind = BINNACLE_FIGURE_NUMBER, .number = 3};
  BinnacleFigure two = {.kind = BINNACLE_FIGURE_NUMBER, .number = 2};
  BinnacleGroupRow rows[2] = {
      {.value = {.text = {(const uint8_t *)"a", 1}}, .children = 3, .figures = &three,
       .figures_len = 1},
      {.value = {.text = {(const uint8_t *)"b", 1}}, .children = 2, .figures = &two,
       .figures_len = 1},
  };
  for (size_t i = 0; i < batch.requests_len; i++) {
    const BinnacleGroupRequest *q = &batch.requests[i];
    CHECK(q->want == BINNACLE_WANT_GROUPS && q->path_len == 0);
    BinnacleGroupAnswer answer = {.stamp = batch.stamp, .has_id = 1, .id = q->id,
                                  .body = BINNACLE_BODY_GROUPS, .total = 2, .groups = rows,
                                  .groups_len = 2};
    CHECK(binnacle_group_layer_answer(layer, &answer));
    answer.body = 9;
    CHECK(!binnacle_group_layer_answer(layer, &answer));
  }

  /* The spec is applied, and the two closed groups are the rows. */
  BinnacleGroupViewed viewed;
  CHECK(binnacle_group_layer_poll_viewed(layer, &viewed));
  CHECK(viewed.applied.has_grouping && viewed.applied.grouping.axes[BINNACLE_ROWS].levels_len == 1);
  uint64_t n = 0;
  CHECK(binnacle_group_layer_count(layer, BINNACLE_ROWS, &n) && n == 2);
  const BinnacleGroupItem *items;
  size_t len;
  CHECK(binnacle_group_layer_window(layer, BINNACLE_ROWS, 0, 2, &items, &len) && len == 2);
  CHECK(items[0].outline.kind == BINNACLE_ITEM_GROUP);
  CHECK(items[0].outline.open == BINNACLE_OPEN_STATE_CLOSED);
  CHECK(items[1].outline.value.text.len == 1 && memcmp(items[1].outline.value.text.data, "b", 1) == 0);
  BinnacleGroupSummary summary;
  CHECK(binnacle_group_layer_summary(layer, items[0].key, &summary));
  CHECK(summary.kind == BINNACLE_ITEM_GROUP && summary.children == 3);
  CHECK(summary.path_len == 1 && summary.path[0].field == 2002);
  CHECK(summary.figures_len == 1 && summary.figures[0].kind == BINNACLE_FIGURE_NUMBER);
  CHECK(summary.figures[0].number == 3);
  CHECK(binnacle_group_layer_held(layer) == 2);

  binnacle_group_layer_free(layer);
  binnacle_group_layer_free(NULL);
  CHECK(binnacle_group_layer_held(NULL) == 0);
}

/* A fresh Grid over the same table, unsorted: focus, reveal and picks by
 * Key, for Keys in the Window and outside it. */
static BinnacleGrid *fresh(BinnacleSnapshot *snap) {
  BinnacleAxisConfig rows = {.count = 100, .default_size = 1, .header = 1, .gap = 0};
  BinnacleAxisConfig cols = {.count = 10, .default_size = 8, .header = 4, .gap = 1};
  BinnacleGrid *grid = binnacle_grid_new(rows, cols);
  CHECK(grid != NULL);
  reversed = 0;
  frame(grid, snap);
  return grid;
}

static void by_key(void) {
  BinnacleSnapshot snap;
  BinnacleGrid *grid = fresh(&snap);
  const BinnacleAxisSnapshot *r = &snap.axes[BINNACLE_ROWS];
  BinnacleEffect effect;

  /* Row 40 is outside the Window: the Grid asks where it is, then scrolls
   * it to the top. Row 42, in view, needs no scroll. */
  CHECK(!binnacle_grid_reveal(grid, BINNACLE_ROWS, 1040, 2));
  CHECK(binnacle_grid_reveal(grid, BINNACLE_ROWS, 1040, BINNACLE_ALIGN_START));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_LOCATE && effect.key == 1040);
  CHECK(binnacle_grid_located(grid, BINNACLE_ROWS, 1040, BINNACLE_LOCATED_FOUND, 40));
  frame(grid, &snap);
  CHECK(r->anchor.position == 40 && !snap.has_focus);
  CHECK(binnacle_grid_reveal(grid, BINNACLE_ROWS, 1042, BINNACLE_ALIGN_NEAREST));
  frame(grid, &snap);
  CHECK(r->anchor.position == 40);
  /* Nearest scrolls row 3 just into view at the top. */
  CHECK(binnacle_grid_reveal(grid, BINNACLE_ROWS, 1003, BINNACLE_ALIGN_NEAREST));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(binnacle_grid_located(grid, BINNACLE_ROWS, 1003, BINNACLE_LOCATED_FOUND, 3));
  frame(grid, &snap);
  CHECK(r->anchor.position == 3);

  /* Focus on row 70, column 1: found, then focused and shown. */
  CHECK(binnacle_grid_focus(grid, 1070, 2001));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_LOCATE && effect.key == 1070);
  CHECK(binnacle_grid_located(grid, BINNACLE_ROWS, 1070, BINNACLE_LOCATED_FOUND, 70));
  while (binnacle_grid_poll_effect(grid, &effect)) {}
  frame(grid, &snap);
  CHECK(snap.has_focus && r->focus.position == 70 && r->focus.key == 1070);
  CHECK(snap.axes[BINNACLE_COLS].focus.key == 2001);
  CHECK(snap.has_focus_rect && snap.focus_rect.y >= 1 && snap.focus_rect.y + 1 <= 6);

  binnacle_grid_free(grid);
}

int main(void) {
  CHECK(binnacle_abi_version() == BINNACLE_ABI_VERSION);

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
  CHECK(r->hide_cap == BINNACLE_HIDE_CAP && c->hide_cap == BINNACLE_HIDE_CAP);

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

  /* Enter activates the focused cell: the host opens what it shows. */
  CHECK(binnacle_grid_activate(grid));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_ACTIVATED);
  CHECK(effect.row == 1000 && effect.col == 2000);
  CHECK(!binnacle_grid_poll_effect(grid, &effect));

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
  CHECK(r->selection_runs_len == 1 && r->selection_runs[0].start == 2 && r->selection_runs[0].end == 4);
  CHECK(c->selection_runs_len == 1 && c->selection_runs[0].start == 1 && c->selection_runs[0].end == 3);
  CHECK(snap.selection_rects_len == 1);
  CHECK(snap.selection_rects[0].x == 13 && snap.selection_rects[0].y == 3);
  CHECK(snap.selection_rects[0].width == 17 && snap.selection_rects[0].height == 2);
  CHECK(r->picked_len == 0 && c->picked_len == 0);
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
  CHECK(r->picked_len == 1 && r->picked[0] == 1004);
  CHECK(snap.has_hovered_header && snap.hovered_axis == BINNACLE_COLS);
  CHECK(snap.hovered_index == 1);

  /* Ctrl-click on column 1's header picks the column. */
  click(grid, 15, 0.5f, BINNACLE_MOD_CTRL);
  frame(grid, &snap);
  CHECK(c->picked_len == 1 && c->picked[0] == 2001 && r->picked_len == 1);

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
  CHECK(r->selection_runs_len == 1 && r->selection_runs[0].end == 100);
  CHECK(c->selection_runs_len == 1 && c->selection_runs[0].end == 10);
  CHECK(binnacle_grid_select_column(grid));
  CHECK(binnacle_grid_select_row(grid));
  drain(grid, &focus);

  /* The host opens its editor on row 2, column 1: the Snapshot says where
   * to place it until the host closes it. */
  CHECK(binnacle_grid_edit_started(grid, 1002, 2001));
  frame(grid, &snap);
  CHECK(snap.mode == BINNACLE_MODE_EDITING);
  CHECK(snap.edit_row == 1002 && snap.edit_col == 2001);
  CHECK(snap.has_editor_rect && snap.editor_rect.x == 13 && snap.editor_rect.y == 3);
  CHECK(snap.editor_rect.width == 8 && snap.editor_rect.height == 1);
  CHECK(snap.cell_states[2 * 4 + 1] & BINNACLE_CELL_EDITING);
  CHECK(!(snap.cell_states[0] & BINNACLE_CELL_EDITING));
  CHECK(binnacle_grid_edit_ended(grid, BINNACLE_EDIT_COMMIT));
  frame(grid, &snap);
  CHECK(snap.mode == BINNACLE_MODE_IDLE && !snap.has_editor_rect);

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
  CHECK(!binnacle_grid_view_applied(grid, &misdirected, NULL));
  CHECK(binnacle_grid_view_applied(grid, NULL, NULL));
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
  CHECK(binnacle_grid_view_applied(grid, &spec, NULL));
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

  /* A quick filter round trip: the rows declare one, the host sets it, and
   * the source answers with what it kept and the unfiltered total. */
  CHECK(!binnacle_grid_set_quick_filter_support(grid, BINNACLE_ROWS, BINNACLE_QUICK_FILTER_REGEX));
  CHECK(binnacle_grid_set_quick_filter_support(grid, BINNACLE_ROWS,
                                               BINNACLE_QUICK_FILTER | BINNACLE_QUICK_FILTER_REGEX));
  frame(grid, &snap);
  CHECK(r->quick_filter == (BINNACLE_QUICK_FILTER | BINNACLE_QUICK_FILTER_REGEX));
  CHECK(c->quick_filter == 0 && !r->has_total);
  BinnacleTextQuery query = {.text = {(const uint8_t *)"ber", 3}, .flags = BINNACLE_TEXT_REGEX};
  CHECK(binnacle_grid_set_quick_filter(grid, BINNACLE_ROWS, &query));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_VIEW_REQUESTED);
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_SET_QUICK_FILTER && effect.edit.axis == BINNACLE_ROWS);
  BinnacleAxisView *filtered = &effect.view.axes[BINNACLE_ROWS];
  CHECK(filtered->has_quick && filtered->quick.text.len == 3);
  CHECK(memcmp(filtered->quick.text.data, "ber", 3) == 0);
  CHECK(filtered->quick.flags == BINNACLE_TEXT_REGEX);
  CHECK(filtered->quick.searched == BINNACLE_SEARCHED_SHOWN);
  CHECK(!effect.view.axes[BINNACLE_COLS].has_quick);
  BinnacleTotals totals = {.axes = {{.known = 1, .count = 100}, {0}}};
  CHECK(binnacle_grid_view_applied(grid, &effect.view, &totals));
  frame(grid, &snap);
  CHECK(r->has_total && r->total == 100 && !c->has_total);
  /* A column condition: does not contain "x" in column 2001, which supports
   * Text with regexes; column 2000 was declared sortable only. */
  CHECK(binnacle_grid_set_support(grid, BINNACLE_ROWS, 2001,
                                  BINNACLE_SUPPORT_SORT | BINNACLE_SUPPORT_TEXT |
                                      BINNACLE_SUPPORT_TEXT_REGEX));
  CHECK(!binnacle_grid_set_support(grid, BINNACLE_ROWS, 2001, BINNACLE_SUPPORT_TEXT_REGEX));
  BinnacleCondition not_x = {.key = 2001, .kind = BINNACLE_CONDITION_TEXT, .negated = 1,
                             .text = {.text = {(const uint8_t *)"x", 1}}};
  CHECK(binnacle_grid_add_condition(grid, BINNACLE_ROWS, &not_x));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_ADD_CONDITION && effect.edit.key == 2001);
  filtered = &effect.view.axes[BINNACLE_ROWS];
  CHECK(filtered->has_quick && filtered->conditions_len == 1);
  CHECK(filtered->conditions[0].key == 2001 && filtered->conditions[0].negated);
  CHECK(filtered->conditions[0].text.searched == BINNACLE_SEARCHED_KEYS);
  CHECK(filtered->conditions[0].text.keys_len == 1 && filtered->conditions[0].text.keys[0] == 2001);
  frame(grid, &snap);
  CHECK(c->headers[1].flags & BINNACLE_HEADER_FILTER_PENDING);
  CHECK(!(c->headers[1].flags & BINNACLE_HEADER_FILTERED));
  CHECK(r->conditions_len == 1 && r->conditions[0].state == BINNACLE_CONDITION_ADDING);
  CHECK(c->headers[0].support == BINNACLE_SUPPORT_SORT);
  CHECK(c->headers[1].support ==
        (BINNACLE_SUPPORT_SORT | BINNACLE_SUPPORT_TEXT | BINNACLE_SUPPORT_TEXT_REGEX));
  CHECK(binnacle_grid_view_applied(grid, &effect.view, &totals));
  frame(grid, &snap);
  CHECK(c->headers[1].flags & BINNACLE_HEADER_FILTERED);
  CHECK(r->conditions_len == 1 && r->conditions[0].state == BINNACLE_CONDITION_APPLIED);
  /* A checklist's condition: column 2000 is "b" or blank. */
  CHECK(binnacle_grid_set_support(grid, BINNACLE_ROWS, 2000,
                                  BINNACLE_SUPPORT_SORT | BINNACLE_SUPPORT_ONE_OF));
  BinnacleValue listed[] = {{.blank = 0, .text = {(const uint8_t *)"b", 1}}, {.blank = 1}};
  BinnacleCondition b_or_blank = {
      .key = 2000, .kind = BINNACLE_CONDITION_ONE_OF, .values = listed, .values_len = 2};
  CHECK(binnacle_grid_add_condition(grid, BINNACLE_ROWS, &b_or_blank));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  filtered = &effect.view.axes[BINNACLE_ROWS];
  CHECK(filtered->conditions_len == 2 && filtered->conditions[1].kind == BINNACLE_CONDITION_ONE_OF);
  CHECK(filtered->conditions[1].values_len == 2 && !filtered->conditions[1].values[0].blank);
  CHECK(filtered->conditions[1].values[0].text.len == 1 && filtered->conditions[1].values[1].blank);
  CHECK(binnacle_grid_view_applied(grid, &effect.view, &totals));
  frame(grid, &snap);
  CHECK(c->headers[0].support == (BINNACLE_SUPPORT_SORT | BINNACLE_SUPPORT_ONE_OF));
  CHECK(r->conditions_len == 2 && r->conditions[1].condition.values[1].blank);
  /* The source stops supporting text on column 2001 and says so,
   * unprompted: its condition stays listed, not applied. */
  CHECK(!snap.view_pending);
  CHECK(binnacle_grid_view_changed(grid, NULL, &totals));
  frame(grid, &snap);
  CHECK(r->conditions_len == 2 && r->conditions[0].state == BINNACLE_CONDITION_NOT_APPLIED);
  CHECK(c->headers[1].flags & BINNACLE_HEADER_FILTER_NOT_APPLIED);
  /* Restoring a saved view: the base one. */
  CHECK(binnacle_grid_request_view(grid, NULL));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_REPLACE);
  CHECK(!effect.view.axes[BINNACLE_ROWS].has_quick);
  CHECK(binnacle_grid_view_applied(grid, NULL, NULL));
  /* Re-applying asks for the same spec again. */
  CHECK(binnacle_grid_reapply_view(grid));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_REAPPLY);
  CHECK(binnacle_grid_view_applied(grid, NULL, NULL));

  /* Grouping the rows by column 2002, once the source says it can: the
   * request has that one level, counting each group. */
  CHECK(binnacle_grid_group_by(grid, BINNACLE_ROWS, 2002));
  CHECK(!binnacle_grid_poll_effect(grid, &effect));
  CHECK(!binnacle_grid_set_support(grid, BINNACLE_ROWS, 2002, 2048));
  CHECK(binnacle_grid_set_support(grid, BINNACLE_ROWS, 2002,
                                  BINNACLE_SUPPORT_SORT | BINNACLE_SUPPORT_GROUP));
  CHECK(!binnacle_grid_group_by(grid, 2, 2002));
  CHECK(binnacle_grid_group_by(grid, BINNACLE_ROWS, 2002));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_VIEW_REQUESTED && effect.view.has_grouping == 1);
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_GROUP && effect.edit.axis == BINNACLE_ROWS);
  CHECK(effect.edit.key == 2002);
  const BinnacleGrouping *grouping = &effect.view.grouping;
  const BinnacleGroupedAxis *grouped = &grouping->axes[BINNACLE_ROWS];
  CHECK(grouped->levels_len == 1 && grouped->levels[0].kind == BINNACLE_LEVEL_FIELD);
  CHECK(grouped->levels[0].field == 2002 && grouped->expand_all == 1);
  CHECK(grouping->axes[BINNACLE_COLS].levels_len == 0);
  CHECK(grouping->values_len == 1 && !grouping->values[0].has_field);
  CHECK(memcmp(grouping->values[0].aggregate.data, "count", 5) == 0);
  CHECK(binnacle_grid_view_applied(grid, &effect.view, NULL));
  /* The source sends the top row's outline: a closed Group Item for "b". */
  frame(grid, &snap);
  BinnacleOutline group = {.kind = BINNACLE_ITEM_GROUP, .open = BINNACLE_OPEN_STATE_CLOSED,
                           .has_group = 1, .field = 2002,
                           .value = {.text = {(const uint8_t *)"b", 1}}};
  CHECK(!binnacle_grid_set_outlines(grid, BINNACLE_ROWS, r->strip[0].position,
                                    &(BinnacleOutline){.kind = 4}, 1));
  CHECK(binnacle_grid_set_outlines(grid, BINNACLE_ROWS, r->strip[0].position, &group, 1));
  CHECK(binnacle_grid_snapshot(grid, &snap));
  CHECK(r->strip[0].has_outline && r->strip[0].outline.kind == BINNACLE_ITEM_GROUP);
  CHECK(r->strip[0].outline.open == BINNACLE_OPEN_STATE_CLOSED);
  CHECK(r->strip[0].outline.has_group && r->strip[0].outline.field == 2002);
  CHECK(r->strip[0].outline.value.text.len == 1 && r->strip[0].outline.ancestors_len == 0);
  CHECK(memcmp(r->strip[0].outline.value.text.data, "b", 1) == 0);
  CHECK(!r->strip[1].has_outline);
  /* The source now places a Grand Total after, and shows a declared
   * aggregate with a parameter: the Grid locates the Grand Total by its
   * Key and pins it at the trailing edge. */
  BinnacleViewSpec totalled = effect.view;
  BinnacleText param = {(const uint8_t *)"0.9", 3};
  BinnacleAggregateValue percentile = {.has_field = 1, .field = 2002,
                                       .aggregate = {(const uint8_t *)"percentile", 10},
                                       .params = &param, .params_len = 1};
  totalled.grouping.values = &percentile;
  totalled.grouping.values_len = 1;
  totalled.grouping.axes[BINNACLE_ROWS].grand_total = BINNACLE_PLACEMENT_AFTER;
  CHECK(binnacle_grid_view_changed(grid, &totalled, NULL));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_LOCATE && effect.key == BINNACLE_GRAND_TOTAL_KEY);
  CHECK(BINNACLE_VALUES_FIELD != BINNACLE_GRAND_TOTAL_KEY);
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_PINS_CHANGED && effect.edge == BINNACLE_EDGE_TRAILING);
  CHECK(effect.keys_len == 1 && effect.keys[0] == BINNACLE_GRAND_TOTAL_KEY);
  CHECK(binnacle_grid_located(grid, BINNACLE_ROWS, BINNACLE_GRAND_TOTAL_KEY,
                              BINNACLE_LOCATED_GONE, 0));
  /* Ungrouping makes the table plain again, and unpins the Grand Total. */
  CHECK(binnacle_grid_ungroup(grid, BINNACLE_ROWS));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_UNGROUP && !effect.view.has_grouping);
  CHECK(binnacle_grid_view_applied(grid, &effect.view, NULL));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_PINS_CHANGED && effect.keys_len == 0);

  /* A native slicer on the plain table asks for a grouping with no levels
   * that carries the text as written; a source that drops it leaves it
   * listed, not applied. */
  BinnacleCondition native = {.key = 2002, .kind = BINNACLE_CONDITION_NATIVE,
                              .text = {.text = {(const uint8_t *)"year = 2024", 11}}};
  CHECK(!binnacle_grid_add_slicer(grid, &(BinnacleCondition){.key = 2002, .kind = 5}));
  CHECK(binnacle_grid_add_slicer(grid, &native));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_ADD_SLICER && effect.edit.key == 2002);
  CHECK(effect.view.has_grouping && effect.view.grouping.slicers_len == 1);
  CHECK(effect.view.grouping.axes[BINNACLE_ROWS].levels_len == 0);
  CHECK(effect.view.grouping.slicers[0].kind == BINNACLE_CONDITION_NATIVE);
  CHECK(memcmp(effect.view.grouping.slicers[0].text.text.data, "year = 2024", 11) == 0);
  effect.view.has_grouping = 0;
  CHECK(binnacle_grid_view_applied(grid, &effect.view, NULL));
  frame(grid, &snap);
  CHECK(snap.slicers_len == 1 && snap.slicers[0].state == BINNACLE_CONDITION_NOT_APPLIED);
  CHECK(binnacle_grid_remove_slicer(grid, &native));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_REMOVE_SLICER && !effect.view.has_grouping);
  CHECK(binnacle_grid_view_applied(grid, &effect.view, NULL));

  /* Find: the host declares it, sets a query, steps, and answers the step;
   * the match takes focus, and closing find drops it. */
  CHECK(!binnacle_grid_set_find_support(grid, BINNACLE_FIND_REGEX));
  CHECK(binnacle_grid_set_find_support(grid, BINNACLE_FIND));
  BinnacleFindQuery find = {.query = {.text = {(const uint8_t *)"7", 1}}, .by = BINNACLE_COLS};
  CHECK(!binnacle_grid_set_find(grid, &(BinnacleFindQuery){.query = find.query, .by = 2}));
  CHECK(binnacle_grid_set_find(grid, &find));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_FIND_REQUESTED && effect.has_find);
  CHECK(effect.find.by == BINNACLE_COLS && effect.find.query.text.len == 1);
  /* Focus a cell of the top row in column 2001 to step from. */
  click(grid, 14, 1.5f, 0);
  drain(grid, &focus);
  frame(grid, &snap);
  CHECK(r->focus.has_key && c->focus.key == 2001);
  CHECK(binnacle_grid_find_previous(grid));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_FIND_STEP_REQUESTED);
  CHECK(effect.direction == BINNACLE_FIND_PREVIOUS && effect.has_from);
  CHECK(effect.from.row == r->focus.key && effect.from.col == 2001);
  CHECK(memcmp(effect.find.query.text.data, "7", 1) == 0);
  /* A row in the Window, so no Locate is needed. */
  BinnacleFindMatch found = {.row = r->strip[2].key, .col = 2001};
  CHECK(binnacle_grid_find_stepped(grid, &found));
  drain(grid, &focus);
  frame(grid, &snap);
  CHECK(snap.find_support == BINNACLE_FIND && snap.has_find && snap.find.by == BINNACLE_COLS);
  CHECK(snap.has_find_match && snap.find_match.row == found.row && snap.find_match.col == 2001);
  CHECK(r->focus.key == found.row && c->focus.key == 2001);
  /* A match within no groups lands at once, as binnacle_grid_find_stepped. */
  CHECK(binnacle_grid_find_next(grid));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_FIND_STEP_REQUESTED);
  CHECK(!binnacle_grid_find_stepped_within(grid, NULL, NULL, 0, NULL, 0));
  found.row = r->strip[3].key;
  CHECK(binnacle_grid_find_stepped_within(grid, &found, NULL, 0, NULL, 0));
  drain(grid, &focus);
  frame(grid, &snap);
  CHECK(snap.has_find_match && snap.find_match.row == found.row);
  CHECK(r->focus.key == found.row && c->focus.key == 2001);
  CHECK(binnacle_grid_set_find(grid, NULL));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_FIND_REQUESTED && !effect.has_find);
  frame(grid, &snap);
  CHECK(!snap.has_find && !snap.has_find_match);

  /* Settings, sizes and content sizes. */
  CHECK(binnacle_grid_set_selection_unit(grid, BINNACLE_SELECT_COLS));
  CHECK(binnacle_grid_set_selection_unit(grid, BINNACLE_SELECT_ROWS));
  CHECK(!binnacle_grid_set_selection_unit(grid, 3));
  CHECK(binnacle_grid_set_header_click(grid, BINNACLE_COLS, BINNACLE_HEADER_CLICK_NONE));
  CHECK(!binnacle_grid_set_header_click(grid, BINNACLE_COLS, 3));
  CHECK(binnacle_grid_set_drag_threshold(grid, 2));
  CHECK(binnacle_grid_scroll_to(grid, BINNACLE_ROWS, 0));
  CHECK(binnacle_grid_set_size(grid, BINNACLE_COLS, 2000, 12));
  frame(grid, &snap);
  CHECK(c->strip[0].size == 12 && c->strip[1].offset == 17);
  click(grid, 6, 1.5f, 0);
  frame(grid, &snap);
  CHECK(snap.has_selection_range && c->selection_runs_len == 1 && c->selection_runs[0].start == 0);
  CHECK(c->selection_runs[0].end == 10 && r->selection_runs_len == 1 && r->selection_runs[0].end == 1);
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
  /* Tiers and forms only show while the rows are grouped in header form. */
  const float tiers[] = {4.0f, 6.0f};
  CHECK(binnacle_grid_set_tiers(grid, BINNACLE_ROWS, tiers, 2));
  CHECK(!binnacle_grid_set_tiers(grid, 2, tiers, 2));
  CHECK(binnacle_grid_set_form(grid, BINNACLE_ROWS, BINNACLE_FORM_HEADER));
  CHECK(!binnacle_grid_set_form(grid, BINNACLE_ROWS, 2));
  /* A level name in the corner sorts by its field as a header click does. */
  CHECK(binnacle_grid_cycle_sort(grid, BINNACLE_ROWS, 1, 0));
  CHECK(!binnacle_grid_cycle_sort(grid, BINNACLE_ROWS, 1, 2));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_VIEW_REQUESTED);
  CHECK(effect.edit.kind == BINNACLE_VIEW_EDIT_CYCLE_SORT && effect.edit.key == 1);
  CHECK(!binnacle_grid_poll_effect(grid, &effect));
  frame(grid, &snap);
  CHECK(snap.mode == BINNACLE_MODE_IDLE);
  CHECK(c->header == 0 && c->strip[0].offset == 0);
  /* Ungrouped, the corner is one cell naming no level. */
  CHECK(r->corner_tiers_len == 1 && c->corner_tiers_len == 1);
  CHECK(r->corner_tiers[0].levels_len == 0 && c->corner_levels_len == 0);
  CHECK(snap.damage & BINNACLE_DAMAGE_LAYOUT);

  /* Dragging column 1's header into the empty leading pin zone pins it. */
  CHECK(binnacle_grid_set_pin_zone(grid, BINNACLE_COLS, 3));
  CHECK(!binnacle_grid_set_pin_zone(grid, 2, 3));
  frame(grid, &snap);
  uint64_t dragged = c->strip[1].key;
  float grab = c->strip[1].offset + 2;
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_DOWN, grab, 0.5f, 0)));
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_MOVE, 1, 0.5f, 0)));
  frame(grid, &snap);
  CHECK(snap.mode == BINNACLE_MODE_MOVING_HEADERS && snap.has_drop);
  CHECK(snap.drop_axis == BINNACLE_COLS && snap.drop_target == BINNACLE_DROP_PIN);
  CHECK(snap.drop_refusal == BINNACLE_MOVE_REFUSAL_OTHER);
  CHECK(snap.drop_edge == BINNACLE_EDGE_LEADING && !snap.has_drop_before && !snap.drop_reorder);
  CHECK(snap.pin_zones_len == 2 && snap.pin_zones[0].hot && !snap.pin_zones[1].hot);
  CHECK(snap.pin_zones[0].edge == BINNACLE_EDGE_LEADING && snap.pin_zones[0].size == 3);
  CHECK(binnacle_grid_pointer(grid, at(BINNACLE_POINTER_UP, 1, 0.5f, 0)));
  frame(grid, &snap);
  CHECK(!snap.has_drop && snap.pin_zones_len == 0);
  CHECK(c->pinned_len == 1 && c->strip[0].key == dragged);
  /* The drop reports the pins it made. */
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_PINS_CHANGED && effect.edge == BINNACLE_EDGE_LEADING);
  CHECK(effect.keys_len == 1 && effect.keys[0] == dragged);

  /* Restoring a saved layout: column 2003 and a column the source lacks
   * pinned right (the missing one sleeps, after its LOCATE), and a size for
   * each; both pins and both sizes are listed back. */
  uint64_t pins[] = {2003, 2999};
  CHECK(binnacle_grid_set_pins(grid, BINNACLE_COLS, BINNACLE_EDGE_TRAILING, pins, 2));
  CHECK(binnacle_grid_set_size(grid, BINNACLE_COLS, 2003, 4));
  CHECK(binnacle_grid_set_size(grid, BINNACLE_COLS, 2999, 5));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_LOCATE && effect.key == 2999);
  CHECK(binnacle_grid_located(grid, BINNACLE_COLS, 2999, BINNACLE_LOCATED_GONE, 0));
  CHECK(binnacle_grid_poll_effect(grid, &effect));
  CHECK(effect.kind == BINNACLE_EFFECT_PINS_CHANGED && effect.edge == BINNACLE_EDGE_TRAILING);
  CHECK(effect.keys_len == 2 && effect.keys[0] == 2003 && effect.keys[1] == 2999);
  frame(grid, &snap);
  CHECK(c->trailing_len == 1 && c->trailing_pins_len == 2 && c->trailing_pins[1] == 2999);
  BinnacleSizeOverrides overrides;
  CHECK(binnacle_grid_size_overrides(grid, BINNACLE_COLS, &overrides));
  /* In Key order, after any sizes set earlier. */
  size_t last = overrides.len - 1;
  CHECK(overrides.len >= 2 && overrides.keys[last] == 2999 && overrides.sizes[last] == 5);
  CHECK(overrides.keys[last - 1] == 2003 && overrides.sizes[last - 1] == 4);

  binnacle_grid_free(grid);
  binnacle_grid_free(NULL);
  group_layer();
  by_key();
  printf("ok\n");
  return 0;
}
