//! Seam 1 state machine: a real Grid against an honest model Table Source,
//! with every kind of input mixed in any order. Change Notices, `located`
//! answers (some held back and answered late), refilters, sorts, quick
//! filters and column conditions (answered by a sort or a refilter), header
//! moves, edits,
//! pointer gestures (band edge drags among them), Actions, Size Overrides,
//! `set_header`, `set_gap` and `set_focus` (with Keys the source has and Keys
//! it never had). Every frame's Snapshot is checked
//! against the model.
//!
//! Ported from the seeded chaos driver of the correctness review, which
//! found the Cancel bugs pinned in `positional.rs`.

use std::collections::{BTreeSet, VecDeque};

use binnacle_core::{
    Action, Axis, AxisConfig, Button, ChangeNotice, Condition, ConditionState, EditEnd, Effect, FocusStep, Grid, HeaderClick,
    Key, Located, Mode, Modifiers, PointerEvent, PointerKind, ScrollAnchor, Searched,
    SelectionUnit, TextQuery, Totals, ViewSpec, Viewport,
};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// The Grid and the Table Source it is told about: `keys[axis]` holds the
/// source's Keys in Position order. Every Change Notice sent first changes
/// the model the same way, and `Locate` is answered from the model as it is
/// when the answer is given.
struct Host {
    grid: Grid,
    keys: [Vec<u64>; 2],
    /// Per axis, the Keys a filter leaves out, which a later `Refiltered`
    /// may bring back.
    hidden: [Vec<u64>; 2],
    /// Row Keys a `Refiltered` took out since the last `Reset`: once every
    /// `Locate` is answered, none of them is picked.
    refiltered_out: BTreeSet<u64>,
    /// The next fresh Key; row and column Keys never collide with each other
    /// or with the initial ones.
    next: u64,
    /// Effects drained but not answered yet.
    pending: VecDeque<Effect>,
    viewport: Viewport,
    /// Each axis's band thickness and Gap as of the last frame.
    headers: [f32; 2],
    gaps: [f32; 2],
}

fn axis() -> impl hegel::PrintableGenerator<Axis> {
    gs::sampled_from(vec![Axis::Rows, Axis::Cols])
}

impl Host {
    fn fresh(&mut self, n: u64) -> Vec<u64> {
        let keys = (self.next..self.next + n).collect();
        self.next += n;
        keys
    }

    /// A Position on `axis`, up to and including the count.
    fn position(&self, tc: &TestCase, axis: Axis) -> u64 {
        let len = self.keys[axis.index()].len() as u64;
        tc.draw(gs::integers::<u64>().max_value(len))
    }

    /// A Key the source has on `axis`, or one it never had.
    fn key(&self, tc: &TestCase, axis: Axis) -> Key {
        let keys = &self.keys[axis.index()];
        if !keys.is_empty() && tc.draw(gs::weighted_booleans(0.7)) {
            Key(*tc.draw(gs::sampled_from(keys.iter().collect::<Vec<_>>())))
        } else {
            Key(tc.draw(gs::integers::<u64>().max_value(self.next + 100)))
        }
    }

    /// A point along a viewport `length`: mostly a unit cell's middle (so
    /// presses land inside items, headers and borders alike), else anywhere.
    fn coord(&self, tc: &TestCase, length: f32) -> f32 {
        if tc.draw(gs::weighted_booleans(0.8)) {
            tc.draw(gs::integers::<u32>().max_value(length as u32)) as f32 + 0.5
        } else {
            tc.draw(gs::floats::<f32>().min_value(0.0).max_value(length))
        }
    }

    fn pointer_event(&mut self, tc: &TestCase, kind: PointerKind) {
        let x = self.coord(tc, self.viewport.width);
        let y = self.coord(tc, self.viewport.height);
        let modifiers = Modifiers {
            shift: tc.draw(gs::weighted_booleans(0.2)),
            ctrl: tc.draw(gs::weighted_booleans(0.2)),
            alt: false,
            meta: false,
        };
        self.grid.pointer(PointerEvent {
            kind,
            x,
            y,
            button: Button::Primary,
            modifiers,
            clicks: tc.draw(gs::integers::<u32>().min_value(1).max_value(2)),
        });
    }

    /// A host frame: fills the requested Windows from the model, then checks
    /// the Snapshot against it.
    fn frame_and_check(&mut self) {
        let requests: Vec<_> = self.grid.begin_frame(self.viewport).into_iter().collect();
        for (axis, range) in requests {
            let keys = &self.keys[axis.index()];
            let end = (range.end as usize).min(keys.len());
            let start = (range.start as usize).min(end);
            let window: Vec<Key> = keys[start..end].iter().map(|&k| Key(k)).collect();
            self.grid.set_window(axis, range.start, &window);
        }
        let snap = self.grid.snapshot();
        // The chips list the applied conditions, each Applied or Removing,
        // and the headers mark the columns they read.
        let applied = snap.applied_view().rows.filter.conditions.conditions();
        let listed = (snap.conditions(Axis::Rows).into_iter())
            .filter(|(_, state)| *state != ConditionState::Adding)
            .count();
        assert_eq!(listed, applied.len(), "conditions listed as applied");
        for h in snap.headers(Axis::Cols) {
            if let Some(key) = h.key {
                let filtered = applied.iter().any(|c| c.key == key);
                assert_eq!(h.filter.filtered, filtered, "filter mark on column {key:?}");
            }
        }
        self.headers = Axis::ALL.map(|a| snap.header(a));
        self.gaps = Axis::ALL.map(|a| snap.gap(a));
        for axis in Axis::ALL {
            let header = snap.header(axis);
            assert!(
                header.is_finite() && header >= 0.0,
                "band {header} on {axis:?}"
            );
            // Items start where the band ends, less the anchor's offset (or
            // a Gap after it, when the anchor is in the Gap).
            if let Some(first) = snap.strip(axis).first() {
                assert!(
                    first.offset <= header + snap.gap(axis) && first.end() > header,
                    "{first:?} does not start the body after the band {header} on {axis:?}"
                );
            }
            let keys = &self.keys[axis.index()];
            let count = keys.len() as u64;
            assert_eq!(snap.count(axis), count, "count on {axis:?}");
            for item in snap.strip(axis) {
                if let Some(k) = item.key {
                    assert_eq!(
                        Some(&k.0),
                        keys.get(item.position as usize),
                        "Strip Key on {axis:?}: {item:?}"
                    );
                }
            }
            if let Some(f) = snap.focus(axis) {
                assert!(f.position < count, "focus on {axis:?} {f:?}, count {count}");
                if !snap.locating(axis)
                    && let Some(k) = f.key
                {
                    assert_eq!(
                        Some(&k.0),
                        keys.get(f.position as usize),
                        "focus on {axis:?} {f:?} is not locating, but is not where its Key is"
                    );
                }
            }
            if let Some(range) = snap.selection_range() {
                let span = range.span(axis);
                assert!(
                    span.start <= span.end && span.end <= count,
                    "range {range:?}, count {count} on {axis:?}"
                );
            }
        }
        assert_eq!(
            snap.focus(Axis::Rows).is_some(),
            snap.focus(Axis::Cols).is_some(),
            "focus placed on one axis only"
        );
        // Every other accessor, for panics.
        let _ = snap.selection_rects();
        let _ = snap.headers(Axis::Rows).count() + snap.headers(Axis::Cols).count();
        let _ = (snap.focus_rect(), snap.editor_rect());
        let _ = snap.drop_indicator();
        match (snap.mode(), snap.resize_guide()) {
            (
                Mode::ResizingBand {
                    axis,
                    start_thickness,
                },
                guide,
            ) => {
                let h = snap.header(axis);
                assert!(
                    h == start_thickness || (1.0..=1_048_576.0).contains(&h),
                    "a band drag left {h} on {axis:?}"
                );
                assert!(snap.capturing());
                let guide = guide.expect("no guide while resizing a band");
                assert_eq!((guide.axis, guide.key, guide.offset), (axis, None, h));
            }
            (_, Some(guide)) => assert!(guide.key.is_some(), "band guide while not resizing one"),
            _ => {}
        }
    }

    fn answer(&mut self, tc: &TestCase, effect: Effect) {
        match effect {
            Effect::Locate { axis, key } => {
                let answer = match self.keys[axis.index()].iter().position(|&k| k == key.0) {
                    Some(p) => Located::Found(p as u64),
                    None => Located::Gone { suggest: None },
                };
                tc.note(&format!("located({axis:?}, {key:?}, {answer:?})"));
                self.grid.located(axis, key, answer);
            }
            Effect::ViewRequested { spec, .. } => {
                let total = (self.keys[0].len() + self.hidden[0].len()) as u64;
                let totals =
                    Totals::unknown().with(Axis::Rows, tc.draw(gs::booleans()).then_some(total));
                self.grid.view_applied(spec, totals);
                if tc.draw(gs::booleans()) {
                    // Any rows will do for a filter: the Grid never sees values.
                    self.refilter_axis(tc, Axis::Rows);
                } else {
                    // Any order will do for a sort.
                    let rows = std::mem::take(&mut self.keys[0]);
                    self.keys[0] = tc.draw(gs::permutations(rows));
                    tc.note(&format!("sorted: rows {:?}", self.keys[0]));
                    self.grid.notify(Axis::Rows, ChangeNotice::Reordered);
                }
            }
            Effect::MoveRequested { axis, keys, to } => {
                let order = &mut self.keys[axis.index()];
                let to = (to as usize).min(order.len());
                let stays = |k: &&u64| !keys.contains(&Key(**k));
                let before: Vec<u64> = order[..to].iter().filter(stays).copied().collect();
                let after: Vec<u64> = order[to..].iter().filter(stays).copied().collect();
                let moved: Vec<u64> = keys
                    .iter()
                    .map(|k| k.0)
                    .filter(|k| order.contains(k))
                    .collect();
                *order = before.into_iter().chain(moved).chain(after).collect();
                tc.note(&format!("moved: {axis:?} {order:?}"));
                self.grid.notify(axis, ChangeNotice::Reordered);
            }
            // The host fits the band to content it measured.
            Effect::BandAutoSizeRequested { axis } => {
                let fit = tc.draw(gs::sampled_from(vec![0.0, 1.0, 2.0, 4.5]));
                tc.note(&format!("set_header({axis:?}, {fit})"));
                self.grid.set_header(axis, fit);
            }
            // The host usually opens its editor when asked.
            Effect::EditRequested { row, col } | Effect::Activated { row, col }
                if tc.draw(gs::weighted_booleans(0.7)) =>
            {
                tc.note(&format!("edit_started({row:?}, {col:?})"));
                self.grid.edit_started(row, col);
            }
            _ => {}
        }
    }
}

impl Host {
    fn insert(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let at = self.position(tc, axis);
        let n = tc.draw(gs::integers::<u64>().min_value(1).max_value(3));
        let fresh = self.fresh(n);
        self.keys[axis.index()].splice(at as usize..at as usize, fresh);
        self.grid.notify(axis, ChangeNotice::Inserted { at, n });
    }

    fn remove(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let len = self.keys[axis.index()].len() as u64;
        // An empty axis has nothing to remove (an assumption would end the
        // whole test case).
        if len == 0 {
            return;
        }
        let at = tc.draw(gs::integers::<u64>().max_value(len - 1));
        let n = tc.draw(gs::integers::<u64>().min_value(1).max_value(len - at));
        self.keys[axis.index()].drain(at as usize..(at + n) as usize);
        self.grid.notify(axis, ChangeNotice::Removed { at, n });
    }

    fn reorder(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let keys = std::mem::take(&mut self.keys[axis.index()]);
        self.keys[axis.index()] = tc.draw(gs::permutations(keys));
        self.grid.notify(axis, ChangeNotice::Reordered);
    }

    /// Some Keys stay, some leave and some arrive (a filter changing, rows
    /// replaced); now and then everything is reordered too.
    fn replace(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let mut keys = tc.draw(gs::subsequences(self.keys[axis.index()].clone()));
        for _ in 0..tc.draw(gs::integers::<u64>().max_value(3)) {
            let at = tc.draw(gs::integers::<usize>().max_value(keys.len()));
            let fresh = self.fresh(1);
            keys.insert(at, fresh[0]);
        }
        if tc.draw(gs::weighted_booleans(0.2)) {
            keys = tc.draw(gs::permutations(keys));
        }
        self.keys[axis.index()] = keys;
        let count = self.keys[axis.index()].len() as u64;
        if axis == Axis::Rows {
            self.refiltered_out.clear();
        }
        self.grid.notify(axis, ChangeNotice::Reset(count));
    }

    /// A filter changed: some Keys leave, some hidden ones come back, and
    /// now and then the order changes too.
    fn refilter(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        self.refilter_axis(tc, axis);
    }

    /// Keeps a random part of `axis`'s Keys, hidden ones included, and
    /// sends `Refiltered`.
    fn refilter_axis(&mut self, tc: &TestCase, axis: Axis) {
        let i = axis.index();
        let pool: Vec<u64> = self.keys[i]
            .iter()
            .chain(&self.hidden[i])
            .copied()
            .collect();
        let mut keys = tc.draw(gs::subsequences(pool.clone()));
        if tc.draw(gs::weighted_booleans(0.2)) {
            keys = tc.draw(gs::permutations(keys));
        }
        self.hidden[i] = pool.into_iter().filter(|k| !keys.contains(k)).collect();
        if axis == Axis::Rows {
            self.refiltered_out.retain(|k| !keys.contains(k));
            self.refiltered_out.extend(&self.hidden[i]);
        }
        self.keys[i] = keys;
        let count = self.keys[i].len() as u64;
        tc.note(&format!("refiltered: {axis:?} {:?}", self.keys[i]));
        self.grid.notify(axis, ChangeNotice::Refiltered(count));
    }

    /// Answers every Effect, and those the answers bring, until none is
    /// left: every pick a `Refiltered` took out is then gone.
    fn answer_all(&mut self, tc: &TestCase) {
        for _ in 0..20 {
            while let Some(stamped) = self.grid.poll_effect() {
                self.pending.push_back(stamped.effect);
            }
            let Some(effect) = self.pending.pop_front() else {
                break;
            };
            self.answer(tc, effect);
        }
        if !self.pending.is_empty() {
            return;
        }
        self.frame_and_check();
        let picked = self.grid.snapshot().picked().to_vec();
        for key in picked {
            assert!(
                !self.refiltered_out.contains(&key.0),
                "{key:?} is still picked after a Refiltered took it out"
            );
        }
    }

    fn edit_content(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let at = self.position(tc, axis);
        let n = tc.draw(gs::integers::<u64>().max_value(3));
        self.grid
            .notify(axis, ChangeNotice::ContentChanged(at..at + n));
    }

    fn keyboard_action(&mut self, tc: &TestCase) {
        let move_focus = hegel::tuples!(
            axis(),
            gs::sampled_from(vec![FocusStep::Item, FocusStep::Page, FocusStep::Edge]),
            gs::booleans(),
            gs::weighted_booleans(0.3)
        )
        .map(|(axis, by, forward, extend)| Action::MoveFocus {
            axis,
            by,
            forward,
            extend,
        });
        let scroll = hegel::tuples!(
            gs::floats::<f32>().min_value(-5.0).max_value(5.0),
            gs::floats::<f32>().min_value(-5.0).max_value(5.0)
        )
        .map(|(dx, dy)| Action::Scroll { dx, dy });
        let scroll_by = hegel::tuples!(axis(), gs::integers::<i64>().min_value(-3).max_value(3))
            .map(|(axis, items)| Action::ScrollBy { axis, items });
        let action = tc.draw(hegel::one_of!(
            move_focus,
            scroll,
            scroll_by,
            gs::sampled_from(vec![
                Action::SelectAll,
                Action::SelectColumn,
                Action::Cancel,
                Action::BeginEdit,
                Action::Activate,
                Action::TypeText("x".into()),
            ]),
        ));
        self.grid.action(action);
    }

    fn focus_keys(&mut self, tc: &TestCase) {
        let row = self.key(tc, Axis::Rows);
        let col = self.key(tc, Axis::Cols);
        self.grid.set_focus(row, col);
    }

    /// A press inside the last unit of a band, where its edge is when the
    /// axis has a Gap or the Grid a border grab, halfway across the other band.
    fn band_press(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let along = self.headers[axis.index()] - 0.25;
        let across = self.headers[axis.other().index()] / 2.0;
        let (x, y) = match axis {
            Axis::Rows => (across, along),
            Axis::Cols => (along, across),
        };
        let clicks = tc.draw(gs::integers::<u32>().min_value(1).max_value(2));
        self.grid.pointer(PointerEvent::down(x, y).clicks(clicks));
    }

    /// `set_header` takes a thickness the Snapshot then shows, 0 included;
    /// one that is not finite and non-negative changes nothing.
    fn band_thickness(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let thickness = tc.draw(hegel::one_of!(
            gs::sampled_from(vec![0.0f32, 1.0, 2.0, -1.0, f32::NAN]),
            gs::floats::<f32>().min_value(0.0).max_value(8.0)
        ));
        self.frame_and_check();
        let before = self.headers[axis.index()];
        self.grid.set_header(axis, thickness);
        self.frame_and_check();
        let expected = if thickness.is_finite() && thickness >= 0.0 {
            thickness
        } else {
            before
        };
        assert_eq!(
            self.headers[axis.index()],
            expected,
            "set_header({thickness})"
        );
    }

    /// `set_gap` changes the Gap and never scrolls forward or across: the
    /// anchor stays, or the end clamp or a shrunk Gap pulls it back.
    fn gap(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let gap = tc.draw(gs::sampled_from(vec![0.0f32, 0.5, 1.0, 2.0]));
        let anchors = |grid: &mut Grid| -> [ScrollAnchor; 2] {
            let snap = grid.snapshot();
            Axis::ALL.map(|a| snap.scroll_anchor(a))
        };
        self.frame_and_check();
        let before = anchors(&mut self.grid);
        self.grid.set_gap(axis, gap);
        self.frame_and_check();
        let after = anchors(&mut self.grid);
        assert_eq!(self.gaps[axis.index()], gap);
        let (b, a) = (before[axis.index()], after[axis.index()]);
        assert!(
            (a.position, a.offset) <= (b.position, b.offset),
            "set_gap({gap}) moved {b:?} forward to {a:?}"
        );
        let other = axis.other().index();
        assert_eq!(
            after[other], before[other],
            "set_gap scrolled the other axis"
        );
    }

    fn resize(&mut self, tc: &TestCase) {
        let axis = tc.draw(axis());
        let key = self.key(tc, axis);
        let size = tc.draw(gs::floats::<f32>().min_value(0.5).max_value(5.5));
        self.grid.set_size(axis, key, size);
    }
}

/// Rule weights roughly follow the review's chaos driver (a fifth of the
/// steps frames, a fifth pointer events, a quarter or more Change Notices),
/// with more reorders, Resets and `Cancel`s, and fewer releases.
#[hegel::state_machine]
impl Host {
    /// A host frame.
    #[rule(weight = 4.0)]
    fn frame(&mut self, _: TestCase) {
        self.frame_and_check();
    }

    /// A frame here too, so the model is checked (at least on the final
    /// state) even when the engine leaves the frame rule out of a test case.
    #[invariant]
    fn a_frame_agrees_with_the_model(&mut self, _: TestCase) {
        self.frame_and_check();
    }

    #[rule]
    fn inserted(&mut self, tc: TestCase) {
        self.insert(&tc);
    }

    #[rule]
    fn removed(&mut self, tc: TestCase) {
        self.remove(&tc);
    }

    #[rule(weight = 2.0)]
    fn reordered(&mut self, tc: TestCase) {
        self.reorder(&tc);
    }

    #[rule(weight = 2.0)]
    fn reset(&mut self, tc: TestCase) {
        self.replace(&tc);
    }

    #[rule(weight = 2.0)]
    fn refiltered(&mut self, tc: TestCase) {
        self.refilter(&tc);
    }

    /// Answers everything waiting, then checks the picks.
    #[rule]
    fn settle(&mut self, tc: TestCase) {
        self.answer_all(&tc);
    }

    #[rule]
    fn content_changed(&mut self, tc: TestCase) {
        self.edit_content(&tc);
    }

    /// Drains the Effect queue, then answers what is waiting in order, but
    /// may stop early and leave the rest for a later drain.
    #[rule(weight = 2.0)]
    fn drain(&mut self, tc: TestCase) {
        while let Some(stamped) = self.grid.poll_effect() {
            self.pending.push_back(stamped.effect);
        }
        while !self.pending.is_empty() && tc.draw(gs::weighted_booleans(0.8)) {
            let effect = self.pending.pop_front().expect("not empty");
            self.answer(&tc, effect);
        }
    }

    /// Releases are rarer than presses, so a press is often held through
    /// notices, answers and `Cancel`.
    #[rule(weight = 2.0)]
    fn press(&mut self, tc: TestCase) {
        self.pointer_event(&tc, PointerKind::Down);
    }

    #[rule(weight = 1.5)]
    fn drag(&mut self, tc: TestCase) {
        self.pointer_event(&tc, PointerKind::Move);
    }

    #[rule(weight = 0.5)]
    fn release(&mut self, tc: TestCase) {
        self.pointer_event(&tc, PointerKind::Up);
    }

    #[rule]
    fn action(&mut self, tc: TestCase) {
        self.keyboard_action(&tc);
    }

    /// Escape, which matters most while a press is held.
    #[rule]
    fn cancel(&mut self, _: TestCase) {
        self.grid.action(Action::Cancel);
    }

    #[rule]
    fn set_focus(&mut self, tc: TestCase) {
        self.focus_keys(&tc);
    }

    #[rule]
    fn edit_ended(&mut self, _: TestCase) {
        self.grid.edit_ended(EditEnd::Commit);
    }

    #[rule]
    fn set_size(&mut self, tc: TestCase) {
        self.resize(&tc);
    }

    /// A press on a band's edge; the drag rule then moves it.
    #[rule]
    fn press_band_edge(&mut self, tc: TestCase) {
        self.band_press(&tc);
    }

    #[rule(weight = 0.5)]
    fn set_header(&mut self, tc: TestCase) {
        self.band_thickness(&tc);
    }

    /// The quick filter set, changed or cleared, searching the shown
    /// columns or one of them.
    #[rule(weight = 0.5)]
    fn quick_filter(&mut self, tc: TestCase) {
        let text = tc.draw(gs::sampled_from(vec!["", "a", "ab"]));
        let mut query = TextQuery::new(text);
        if tc.draw(gs::booleans()) {
            query = query.searching(Searched::Keys(vec![self.key(&tc, Axis::Cols)]));
        }
        tc.note(&format!("set_quick_filter({query:?})"));
        self.grid.set_quick_filter(Axis::Rows, Some(query));
    }

    /// A Text condition added on a column, or the first one listed removed.
    #[rule(weight = 0.5)]
    fn column_condition(&mut self, tc: TestCase) {
        let listed = self.grid.snapshot().conditions(Axis::Rows);
        if tc.draw(gs::booleans()) && !listed.is_empty() {
            let condition = listed[0].0.clone();
            tc.note(&format!("remove_condition({condition:?})"));
            self.grid.remove_condition(Axis::Rows, &condition);
            return;
        }
        let text = tc.draw(gs::sampled_from(vec!["", "a", "ab"]));
        let key = self.key(&tc, Axis::Cols);
        let condition = Condition::text(key, TextQuery::new(text)).not(tc.draw(gs::booleans()));
        tc.note(&format!("add_condition({condition:?})"));
        self.grid.add_condition(Axis::Rows, condition);
    }

    /// The host restores the base view.
    #[rule(weight = 0.25)]
    fn request_view(&mut self, _: TestCase) {
        self.grid.request_view(ViewSpec::default());
    }

    #[rule(weight = 0.5)]
    fn set_gap(&mut self, tc: TestCase) {
        self.gap(&tc);
    }
}

/// 5000 test cases, or `HEGEL_TEST_CASES` to soak: the bugs this found need
/// a press held through two notices, an answer and `Cancel`, and turn up in
/// about one run in three at the default.
fn settings() -> hegel::Settings {
    let cases = std::env::var("HEGEL_TEST_CASES").ok();
    hegel::Settings::new().test_cases(cases.and_then(|n| n.parse().ok()).unwrap_or(5000))
}

#[hegel::test(settings())]
fn the_grid_agrees_with_an_honest_source_through_any_input(tc: TestCase) {
    // Mostly few rows, so Keys collide often; sometimes more than a Window.
    let rows = tc.draw(hegel::one_of!(
        gs::integers::<u64>().max_value(8),
        gs::integers::<u64>().max_value(40)
    ));
    let cols = tc.draw(gs::integers::<u64>().max_value(5));
    // Geometry is fixed apart from the Gap: what is checked here is keyed
    // state, and `properties.rs` draws sizes, headers and counts in full.
    let gap = tc.draw(gs::sampled_from(vec![0.0, 1.0]));
    let mut grid = Grid::new(
        AxisConfig::new(rows, 1.0).header(1.0).gap(gap),
        AxisConfig::new(cols, 3.0).header(2.0).gap(gap),
    );
    if tc.draw(gs::booleans()) {
        grid.set_selection_unit(SelectionUnit::Rows);
    }
    for axis in Axis::ALL {
        grid.set_header_click(
            axis,
            tc.draw(gs::sampled_from(vec![
                HeaderClick::Sort,
                HeaderClick::Select,
                HeaderClick::None,
            ])),
        );
    }
    grid.set_follow_focus_on_reorder(tc.draw(gs::booleans()));
    if tc.draw(gs::booleans()) {
        grid.set_border_grab(0.5);
    }
    let viewport = Viewport::new(
        tc.draw(gs::floats::<f32>().min_value(4.0).max_value(20.0)),
        tc.draw(gs::floats::<f32>().min_value(3.0).max_value(13.0)),
    );
    let host = Host {
        grid,
        keys: [(0..rows).collect(), (100..100 + cols).collect()],
        hidden: [Vec::new(), Vec::new()],
        refiltered_out: BTreeSet::new(),
        next: 1000,
        pending: VecDeque::new(),
        viewport,
        headers: [1.0, 2.0],
        gaps: [gap; 2],
    };
    hegel::stateful::machine(host).steps(300).run(tc);
}
