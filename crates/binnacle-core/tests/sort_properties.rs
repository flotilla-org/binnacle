//! Seam 1 properties for sorting and filtering: a real Grid over an
//! In-Memory Table Source of random records, driven through the frame loop
//! and the shared pump with clicks, focus moves, header clicks, sorts and
//! filters made behind the Grid's back and bare `Reordered` notices.

use binnacle_core::{
    Action, Axis, AxisConfig, CellRange, ChangeNotice, Edge, Effect, FocusEnd, FocusStep, Grid,
    Key, Located, Mode, Modifiers, PointerEvent, ScrollAnchor, SelectionUnit, SortColumn, SortSpec,
    Support, Totals, ViewSpec, Viewport,
};
use binnacle_testkit::{Record, RecordTable, Scenario, ScenarioSource};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const ROW_HEADER: f32 = 3.0;
const COL_WIDTH: f32 = 4.0;

/// A table of records and how it is laid out.
#[derive(Clone, Debug)]
struct Table {
    rows: Vec<Record>,
    cols: usize,
    /// Columns the source has sort keys for.
    sortable: Vec<u64>,
    /// Columns the Grid is told it may not request sorts by.
    unsortable: Vec<u64>,
    gap: f32,
    row_size: f32,
    viewport: Viewport,
    follow: bool,
    unit: SelectionUnit,
}

#[hegel::composite]
fn table(tc: &TestCase) -> Table {
    let cols = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    // Few distinct values and some nulls, so sorts tie and nulls matter.
    let value = gs::optional(gs::integers::<i64>().min_value(-3).max_value(3));
    let record = gs::vecs(value).min_size(cols).max_size(cols);
    let rows = tc.draw(gs::vecs(record).max_size(30));
    let column = || gs::integers::<u64>().max_value(cols as u64 - 1);
    let sortable = tc.draw(gs::vecs(column()).max_size(cols));
    let unsortable = tc.draw(gs::vecs(column()).max_size(2));
    let gap = tc.draw(gs::sampled_from(vec![0.0f32, 1.0]));
    let row_size = tc.draw(gs::floats::<f32>().min_value(0.25).max_value(3.0));
    // Mostly a viewport with a body past both header bands, where pointer
    // input lands on cells; sometimes any size down to nothing.
    let length = |body: f32, max: f32| {
        hegel::one_of!(
            gs::floats::<f32>().min_value(body).max_value(max),
            gs::floats::<f32>().min_value(body).max_value(max),
            gs::floats::<f32>().min_value(0.0).max_value(max),
        )
    };
    let width = tc.draw(length(ROW_HEADER + COL_WIDTH, 30.0));
    let height = tc.draw(length(2.0, 12.0));
    let follow = tc.draw(gs::booleans());
    let unit = tc.draw(gs::sampled_from(vec![
        SelectionUnit::Cells,
        SelectionUnit::Rows,
    ]));
    Table {
        follow,
        unit,
        rows,
        cols,
        sortable,
        unsortable,
        gap,
        row_size,
        viewport: Viewport::new(width, height),
    }
}

#[derive(Clone, Debug)]
enum Input {
    Action(Action),
    /// A press and release at fractions of the viewport.
    Click(f32, f32, Modifiers),
    /// A press, and a move to a second point, left held.
    Drag([f32; 2], [f32; 2]),
    Release,
    /// A click on the header of the column with Key `c`, if it is in the
    /// Strip.
    Header(u64, bool),
    /// The host sorts the source itself and tells the Grid.
    Sort(SortSpec),
    /// The host filters the source (or clears the filter) and tells the
    /// Grid.
    Filter(Option<Keep>),
    Notice(Axis),
}

#[hegel::composite]
fn axis(tc: &TestCase) -> Axis {
    tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]))
}

#[hegel::composite]
fn fraction(tc: &TestCase) -> f32 {
    tc.draw(gs::floats::<f32>().min_value(0.0).max_value(0.999))
}

#[hegel::composite]
fn move_focus(tc: &TestCase) -> Action {
    let by = tc.draw(gs::sampled_from(vec![
        FocusStep::Item,
        FocusStep::Item,
        FocusStep::Page,
        FocusStep::Edge,
    ]));
    Action::MoveFocus {
        axis: tc.draw(axis()),
        by,
        forward: tc.draw(gs::booleans()),
        extend: tc.draw(gs::weighted_booleans(0.3)),
    }
}

#[hegel::composite]
fn spec(tc: &TestCase, cols: usize) -> SortSpec {
    let column = gs::integers::<u64>().max_value(cols as u64 - 1);
    let keys = tc.draw(gs::vecs(column).max_size(3));
    let mut columns: Vec<SortColumn> = Vec::new();
    for k in keys {
        if columns.iter().all(|c| c.key != Key(k)) {
            let descending = tc.draw(gs::booleans());
            columns.push(if descending {
                SortColumn::descending(Key(k))
            } else {
                SortColumn::ascending(Key(k))
            });
        }
    }
    SortSpec::new(columns)
}

#[hegel::composite]
fn header(tc: &TestCase, cols: usize) -> Input {
    let c = tc.draw(gs::integers::<u64>().max_value(cols as u64 - 1));
    Input::Header(c, tc.draw(gs::weighted_booleans(0.3)))
}

/// A filter predicate: keep records whose value in `col` is at least `min`,
/// and nulls if `nulls`.
#[derive(Clone, Copy, Debug)]
struct Keep {
    col: usize,
    min: i64,
    nulls: bool,
}

impl Keep {
    fn keeps(&self, r: &Record) -> bool {
        r[self.col].map_or(self.nulls, |v| v >= self.min)
    }
}

/// A filter over columns `0..cols`, or none to clear it. Values run -3..=3,
/// so thresholds keep anything from every record to none.
#[hegel::composite]
fn filter(tc: &TestCase, cols: usize) -> Input {
    let keep = hegel::tuples!(
        gs::integers::<usize>().max_value(cols - 1),
        gs::integers::<i64>().min_value(-4).max_value(4),
        gs::booleans()
    )
    .map(|(col, min, nulls)| Keep { col, min, nulls });
    Input::Filter(tc.draw(gs::optional(keep)))
}

/// Anything a user or host might do.
#[hegel::composite]
fn user_input(tc: &TestCase, cols: usize) -> Input {
    let modifiers = gs::sampled_from(vec![Modifiers::NONE, Modifiers::SHIFT, Modifiers::CTRL]);
    tc.draw(hegel::one_of!(
        move_focus().map(Input::Action),
        move_focus().map(Input::Action),
        hegel::tuples!(fraction(), fraction(), modifiers).map(|(x, y, m)| Input::Click(x, y, m)),
        hegel::tuples!(fraction(), fraction(), fraction(), fraction())
            .map(|(a, b, c, d)| Input::Drag([a, b], [c, d])),
        gs::sampled_from(vec![
            Input::Release,
            Input::Action(Action::SelectAll),
            Input::Action(Action::SelectColumn),
            Input::Action(Action::Cancel),
        ]),
        hegel::tuples!(axis(), gs::integers::<i64>().min_value(-5).max_value(5))
            .map(|(axis, items)| Input::Action(Action::ScrollBy { axis, items })),
        header(cols),
        spec(cols).map(Input::Sort),
        filter(cols),
        axis().map(Input::Notice),
    ))
}

/// Only what reorders: header clicks, sorts behind the Grid, bare notices.
#[hegel::composite]
fn sort_input(tc: &TestCase, cols: usize) -> Input {
    tc.draw(hegel::one_of!(
        header(cols),
        spec(cols).map(Input::Sort),
        axis().map(Input::Notice),
    ))
}

/// What a check needs from a Snapshot, owned so the source can be queried
/// beside it.
#[derive(Clone, Debug, PartialEq)]
struct View {
    focus: Option<[FocusEnd; 2]>,
    locating: [bool; 2],
    range: Option<CellRange>,
    picks: Vec<Key>,
    anchors: [ScrollAnchor; 2],
    counts: [u64; 2],
    mode: Mode,
    applied: ViewSpec,
    requested: Option<ViewSpec>,
    /// Each visible column's Key, offset and end.
    cols: Vec<(Option<Key>, f32, f32)>,
}

struct Host {
    s: Scenario<RecordTable>,
    table: Table,
    view: View,
    /// Effects drained by the last frame's pump.
    drained: Vec<Effect>,
}

impl Host {
    fn new(table: Table) -> Host {
        let mut grid = Grid::new(
            AxisConfig::new(table.rows.len() as u64, table.row_size).header(1.0),
            AxisConfig::new(table.cols as u64, COL_WIDTH)
                .header(ROW_HEADER)
                .gap(table.gap),
        );
        // Column headers keep their default click, Sort, which sort steps use.
        grid.set_follow_focus_on_reorder(table.follow);
        grid.set_selection_unit(table.unit);
        for &c in &table.unsortable {
            grid.set_support(Axis::Rows, Key(c), Support::default().with_sort(false));
        }
        let source = RecordTable::new(table.rows.clone(), table.cols, table.sortable.clone());
        let mut host = Host {
            s: Scenario::new(grid, source),
            table,
            view: View {
                focus: None,
                locating: [false; 2],
                range: None,
                picks: Vec::new(),
                anchors: [ScrollAnchor::default(); 2],
                counts: [0; 2],
                mode: Mode::Idle,
                applied: ViewSpec::default(),
                requested: None,
                cols: Vec::new(),
            },
            drained: Vec::new(),
        };
        host.frame();
        host
    }

    fn frame(&mut self) {
        self.drained = self.s.effects();
        let snap = self.s.frame(self.table.viewport);
        let focus = snap.focus(Axis::Rows).zip(snap.focus(Axis::Cols));
        self.view = View {
            focus: focus.map(|(r, c)| [r, c]),
            locating: Axis::ALL.map(|a| snap.locating(a)),
            range: snap.selection_range().cloned(),
            picks: snap.picked(Axis::Rows).to_vec(),
            anchors: Axis::ALL.map(|a| snap.scroll_anchor(a)),
            counts: Axis::ALL.map(|a| snap.count(a)),
            mode: snap.mode(),
            applied: snap.applied_view().clone(),
            requested: snap.requested_view().cloned(),
            cols: snap
                .cols()
                .iter()
                .map(|c| (c.key, c.offset, c.end()))
                .collect(),
        };
    }

    /// Pushes `input`; returns the axes it sent `Reordered` for.
    fn input(&mut self, input: Input) -> Vec<Axis> {
        let Viewport { width, height } = self.table.viewport;
        let click = |s: &mut Scenario<RecordTable>, x: f32, y: f32, m: Modifiers| {
            s.input(|g| {
                g.pointer(PointerEvent::down(x, y).modifiers(m));
                g.pointer(PointerEvent::up(x, y).modifiers(m));
            })
        };
        match input {
            Input::Action(a) => self.s.input(|g| g.action(a)),
            Input::Click(x, y, m) => click(&mut self.s, x * width, y * height, m),
            Input::Drag([x0, y0], [x1, y1]) => self.s.input(|g| {
                g.pointer(PointerEvent::down(x0 * width, y0 * height));
                g.pointer(PointerEvent::moved(x1 * width, y1 * height));
            }),
            Input::Release => self.s.input(|g| g.pointer(PointerEvent::up(0.0, 0.0))),
            Input::Header(c, shift) => {
                // By Key: a column move leaves Key `c` at another Position.
                let at = self.view.cols.iter().find(|&&(k, _, _)| k == Some(Key(c)));
                if let Some(&(_, offset, end)) = at {
                    let m = if shift {
                        Modifiers::SHIFT
                    } else {
                        Modifiers::NONE
                    };
                    // The middle of the header's visible part.
                    let x = (offset.max(ROW_HEADER) + end.min(width)) / 2.0;
                    click(&mut self.s, x, 0.5, m);
                }
            }
            Input::Sort(sort) => {
                let spec = ViewSpec::default().with_sort(Axis::Rows, sort);
                if let Ok(notices) = self.s.source.source.view(&self.s.source.rows, &spec) {
                    // A sort leaving the rows in order reorders nothing.
                    let reordered = (notices.iter())
                        .filter(|(_, n)| *n == ChangeNotice::Reordered)
                        .map(|&(axis, _)| axis)
                        .collect();
                    self.s.input(|g| {
                        g.view_applied(spec, Totals::unknown());
                        notices.into_iter().for_each(|(axis, n)| g.notify(axis, n));
                    });
                    return reordered;
                }
            }
            Input::Filter(keep) => {
                let notice = match keep {
                    Some(keep) => self.s.source.filter(move |r| keep.keeps(r)),
                    None => self.s.source.source.clear_filter(&self.s.source.rows),
                };
                self.s.input(|g| g.notify(Axis::Rows, notice));
            }
            Input::Notice(axis) => {
                self.s.input(|g| g.notify(axis, ChangeNotice::Reordered));
                return vec![axis];
            }
        }
        Vec::new()
    }

    /// The Position the source has `key` at on `axis`.
    fn position(&mut self, axis: Axis, key: Key) -> u64 {
        match self.s.source.source.locate(axis, key) {
            Located::Found(p) => p,
            gone => panic!("{key:?} is {gone:?}"),
        }
    }
}

/// What one step of [`runs`] did.
struct Step {
    /// The View before the step.
    before: View,
    /// The step held only header clicks, sorts and notices.
    sorting: bool,
    inputs: Vec<Input>,
    /// The axes the step sent `Reordered` for itself.
    reordered: Vec<Axis>,
}

/// Draws a table, then steps of user inputs or of sort-only inputs, each
/// followed by a frame; `check` sees the Host after every frame with the
/// step.
fn runs(tc: &TestCase, mut check: impl FnMut(&mut Host, &Step)) {
    let table = tc.draw(table());
    let cols = table.cols;
    let mut host = Host::new(table);
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(8));
    for _ in 0..steps {
        let kind = tc.draw(gs::integers::<u8>().max_value(4));
        let sorting = kind < 2;
        let inputs = if sorting {
            tc.draw(gs::vecs(sort_input(cols)).min_size(1).max_size(3))
        } else if kind == 2 {
            // Often pick and focus a row first, so the filter may drop a
            // picked, focused row.
            if tc.draw(gs::booleans()) {
                let (x, y) = (tc.draw(fraction()), tc.draw(fraction()));
                host.input(Input::Click(x, y, Modifiers::CTRL));
                host.frame();
            }
            vec![tc.draw(filter(cols))]
        } else {
            let mut inputs = tc.draw(gs::vecs(user_input(cols)).max_size(5));
            // Often leave a drag in progress for the next step to meet.
            if tc.draw(gs::weighted_booleans(0.3)) {
                let at = [0; 4].map(|_| tc.draw(fraction()));
                inputs.push(Input::Drag([at[0], at[1]], [at[2], at[3]]));
            }
            inputs
        };
        let before = host.view.clone();
        let mut reordered = Vec::new();
        for input in inputs.clone() {
            reordered.extend(host.input(input));
        }
        host.frame();
        let step = Step {
            before,
            sorting,
            inputs,
            reordered,
        };
        check(&mut host, &step);
    }
}

#[hegel::test(test_cases = 1000)]
fn the_focused_position_always_holds_the_focused_key(tc: TestCase) {
    runs(&tc, |host, _| {
        let Some(focus) = host.view.focus else { return };
        for axis in Axis::ALL {
            let end = focus[axis.index()];
            assert!(end.position < host.view.counts[axis.index()]);
            if let Some(key) = end.key {
                assert_eq!(host.position(axis, key), end.position, "{axis:?}");
            }
        }
    });
}

#[hegel::test(test_cases = 1000)]
fn focus_and_picks_keep_their_keys_across_any_sort(tc: TestCase) {
    runs(&tc, |host, step| {
        let before = &step.before;
        // A drag that a notice cancels restores the focus and picks from
        // before its press.
        if !step.sorting || before.mode == Mode::DraggingSelection {
            return;
        }
        // A Key the Grid did not know yet may be completed by the Window (a
        // follow-focus scroll fetches it); a known one must stay.
        assert_eq!(host.view.focus.is_some(), before.focus.is_some());
        if let (Some(now), Some(then)) = (host.view.focus, before.focus) {
            for (now, then) in now.iter().zip(then) {
                if then.key.is_some() {
                    assert_eq!(now.key, then.key);
                }
            }
        }
        assert_eq!(host.view.picks, before.picks);
    });
}

#[hegel::test(test_cases = 1000)]
fn a_reorder_drops_the_range_and_any_drag_and_leaves_the_anchor(tc: TestCase) {
    runs(&tc, |host, step| {
        // Later user input may select again; a sort-only step must not.
        if !step.sorting {
            return;
        }
        for &axis in &step.reordered {
            let i = axis.index();
            if let Some(range) = &host.view.range {
                let all = 0..host.view.counts[i];
                assert_eq!(range.span(axis), [all], "{axis:?}");
            }
            assert_ne!(host.view.mode, Mode::DraggingSelection);
        }
        if !host.table.follow {
            for axis in Axis::ALL {
                anchor_kept_or_stepped_off_a_pin(host, axis, step.before.anchors[axis.index()]);
            }
        }
    });
}

/// The Scroll Anchor after a reorder, given the one before, `then`: it stays,
/// unless the reorder moved a pinned item onto its Position. A pinned item
/// takes no anchor, so the anchor then goes to the next unpinned item, at
/// its start, or as far towards it as the end allows: the view is then at
/// its end (the thumb at its end, or everything fitting with the anchor at
/// the start, or with no room to scroll in, the anchor on the last unpinned
/// item). With every item pinned nothing scrolls, so the anchor rests at the
/// start, Position 0 offset 0, as on an empty axis.
fn anchor_kept_or_stepped_off_a_pin(host: &mut Host, axis: Axis, then: ScrollAnchor) {
    let now = host.view.anchors[axis.index()];
    let count = host.view.counts[axis.index()];
    let pins: Vec<Key> = Edge::ALL
        .iter()
        .flat_map(|&edge| host.s.grid.pins(axis, edge).collect::<Vec<_>>())
        .collect();
    let keys = host.s.source.window(axis, 0..count);
    let pinned: Vec<bool> = keys.iter().map(|k| pins.contains(k)).collect();
    if !pinned.get(then.position as usize).copied().unwrap_or(false) {
        assert_eq!(now, then, "{axis:?}");
        return;
    }
    if pinned.iter().all(|&p| p) {
        assert_eq!(now, ScrollAnchor::default(), "{axis:?}: every item pinned");
        return;
    }
    let unpinned = |p: &u64| !pinned[*p as usize];
    let start = |position| ScrollAnchor {
        position,
        offset: 0.0,
    };
    let next = (then.position + 1..count).find(unpinned).map(start);
    if next == Some(now) {
        return;
    }
    let last = (0..count).rev().find(unpinned).map(start);
    let snap = host.s.grid.snapshot();
    let thumb = snap.thumb(axis);
    let at_end = thumb.fraction == 1.0
        || (thumb.size >= 1.0 && now == ScrollAnchor::default())
        || (snap.scrolling(axis).is_empty() && Some(now) == last);
    let short = next.is_none_or(|n| (now.position, now.offset) < (n.position, n.offset));
    assert!(
        at_end && short && unpinned(&now.position),
        "{axis:?}: a pin moved onto {then:?}; next unpinned {next:?}, now {now:?}"
    );
}

/// The seed that found the pinned case: a drag up from row 0's header pins
/// it, the view scrolls to row 1, and a sort puts the pinned row at
/// Position 1. With no unpinned row after it and no room to scroll in, the
/// anchor goes to row 0, the last unpinned one.
#[test]
fn a_sort_moving_a_pin_onto_the_anchor_moves_the_anchor_off_it() {
    let mut host = Host::new(Table {
        rows: vec![vec![None, None], vec![Some(0), None]],
        cols: 2,
        sortable: vec![0],
        unsortable: vec![],
        gap: 0.0,
        row_size: 1.0,
        viewport: Viewport::new(7.0, 2.0),
        follow: false,
        unit: SelectionUnit::Cells,
    });
    host.input(Input::Drag([0.0, 0.5], [0.0, 0.0]));
    host.input(Input::Release);
    host.frame();
    let then = host.view.anchors[Axis::Rows.index()];
    assert_eq!(
        then,
        ScrollAnchor {
            position: 1,
            offset: 0.0
        }
    );
    host.input(Input::Header(0, false));
    host.frame();
    assert_eq!(
        host.view.anchors[Axis::Rows.index()],
        ScrollAnchor::default()
    );
    anchor_kept_or_stepped_off_a_pin(&mut host, Axis::Rows, then);
}

/// The seed of #256: a drag left from the only column's header pins it, so
/// no column is left unpinned and nothing scrolls. A sort then leaves the
/// anchor at the start, Position 0, with no unpinned item to step to.
#[test]
fn with_every_item_pinned_a_sort_leaves_the_anchor_at_the_start() {
    let mut host = Host::new(Table {
        rows: vec![],
        cols: 1,
        sortable: vec![],
        unsortable: vec![],
        gap: 0.0,
        row_size: 1.0,
        viewport: Viewport::new(7.0, 2.0),
        follow: false,
        unit: SelectionUnit::Cells,
    });
    host.input(Input::Drag([0.5, 0.0], [0.0, 0.0]));
    host.input(Input::Release);
    host.frame();
    assert_eq!(host.s.grid.pins(Axis::Cols, Edge::Leading).count(), 1);
    let then = host.view.anchors[Axis::Cols.index()];
    host.input(Input::Sort(SortSpec::unsorted()));
    host.frame();
    assert_eq!(
        host.view.anchors[Axis::Cols.index()],
        ScrollAnchor::default()
    );
    anchor_kept_or_stepped_off_a_pin(&mut host, Axis::Cols, then);
}

#[hegel::test(test_cases = 1000)]
fn the_pump_answers_everything_it_drains(tc: TestCase) {
    runs(&tc, |host, _| {
        assert_eq!(host.view.requested, None);
        assert_eq!(host.view.locating, [false; 2]);
        assert_eq!(host.view.applied, host.s.source.source.view_spec());
    });
}

#[hegel::test(test_cases = 1000)]
fn clicking_the_header_of_a_column_marked_unsortable_requests_nothing(tc: TestCase) {
    runs(&tc, |host, step| {
        let unsortable = |i: &Input| match i {
            Input::Header(c, _) => host.table.unsortable.contains(c),
            _ => false,
        };
        if !step.inputs.is_empty() && step.inputs.iter().all(unsortable) {
            let requests = host
                .drained
                .iter()
                .filter(|e| matches!(e, Effect::ViewRequested { .. }));
            assert_eq!(requests.count(), 0);
        }
    });
}

/// Focus moves made while `located` is pending end where the same moves made
/// after it would: two Grids over the same table, one moving before the
/// pump answers `Locate`, one after.
#[hegel::test(test_cases = 1000)]
fn buffered_focus_moves_land_where_moves_after_located_would(tc: TestCase) {
    let table = tc.draw(table());
    let cols = table.cols;
    let mut prefix = tc.draw(gs::vecs(user_input(cols)).max_size(4));
    // Usually place focus, so there is a Key to locate.
    if tc.draw(gs::weighted_booleans(0.9)) {
        prefix.push(Input::Click(
            tc.draw(fraction()),
            tc.draw(fraction()),
            Modifiers::NONE,
        ));
        prefix.push(Input::Action(Action::MoveFocus {
            axis: Axis::Rows,
            by: FocusStep::Item,
            forward: true,
            extend: false,
        }));
    }
    let sort = tc.draw(spec(cols));
    let moves = tc.draw(gs::vecs(move_focus()).min_size(1).max_size(6));
    let [mut early, mut late] = [0, 1].map(|_| Host::new(table.clone()));
    for host in [&mut early, &mut late] {
        for input in prefix.clone() {
            host.input(input);
        }
        host.frame();
        // A press left held would make the moves stretch a drag.
        host.input(Input::Release);
        // The release may drop dragged column headers: let the source move
        // them now, so the sort is the only reorder the moves can precede.
        host.frame();
        host.input(Input::Sort(sort.clone()));
    }
    for m in &moves {
        early.input(Input::Action(m.clone()));
    }
    early.frame();
    late.frame();
    for m in &moves {
        late.input(Input::Action(m.clone()));
    }
    late.frame();
    assert_eq!(early.view.focus, late.view.focus);
    assert_eq!(early.view.range, late.view.range);
    assert_eq!(early.view.picks, late.view.picks);
}

#[hegel::test(test_cases = 1000)]
fn a_filtered_out_focus_moves_to_the_suggestion_and_hidden_picks_go(tc: TestCase) {
    runs(&tc, |host, step| {
        let ([Input::Filter(_)], Some(before)) = (step.inputs.as_slice(), step.before.focus) else {
            return;
        };
        // A drag the notice cancels restores the focus from before its press.
        if step.before.mode == Mode::DraggingSelection {
            return;
        }
        let Some(key) = before[0].key else { return };
        let after = host.view.focus;
        // Picks of rows the filter hides are dropped, whatever focus does.
        let mut picks = step.before.picks.clone();
        picks.retain(|&k| {
            matches!(
                host.s.source.source.locate(Axis::Rows, k),
                Located::Found(_)
            )
        });
        match host.s.source.source.locate(Axis::Rows, key) {
            Located::Found(p) => assert_eq!(
                after.map(|f| f[0]),
                Some(FocusEnd {
                    position: p,
                    key: Some(key)
                })
            ),
            Located::Gone { suggest: None } => {
                assert_eq!(host.view.counts[0], 0);
                assert_eq!(after, None);
                return;
            }
            Located::Gone { suggest: Some(p) } => {
                // The suggestion is where the Key would sit: past the last
                // item when none follows, so focus goes to the last.
                let p = p.min(host.view.counts[0] - 1);
                let end = after.expect("focus stays placed")[0];
                assert_eq!(end.position, p);
                // The suggested item's Key is known once a Window holds it.
                let to = host.s.source.source.window(Axis::Rows, p..p + 1);
                assert!(end.key.is_none_or(|k| Some(&k) == to.first()), "{end:?}");
            }
        }
        assert_eq!(after.map(|f| f[1]), Some(before[1]));
        assert_eq!(host.view.picks, picks);
    });
}
