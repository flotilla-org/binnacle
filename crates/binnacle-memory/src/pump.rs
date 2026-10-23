use binnacle_core::{
    Axis, ChangeNotice, Distinct, DistinctQuery, Effect, FindDirection, FindMatch, FindQuery, Grid,
    Key, Located, Position, Stamped, Support, TextSupport, Totals, ViewSpec,
};

/// A Table Source's answer to a requested View Spec: the spec it now applies
/// (the request, a part of it, or the previous spec to reject it), each
/// axis's unfiltered count where known, and the Change Notices the axes
/// need.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Viewed {
    pub applied: ViewSpec,
    pub totals: Totals,
    pub notices: Vec<(Axis, ChangeNotice)>,
}

/// A Table Source's answer to a move it made: the Change Notices the axes
/// need, and the View Spec it applies now with each axis's unfiltered count
/// if the move changed it (a sort it cleared to keep the moved order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moved {
    pub notices: Vec<(Axis, ChangeNotice)>,
    pub view: Option<(ViewSpec, Totals)>,
}

/// A Table Source's answer to a find step: the match stepped to, its place
/// among every match in the Find Query's order (from 0) if known, and
/// whether the step went past the end (or the start) and wrapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FindStep {
    pub at: FindMatch,
    pub index: Option<u64>,
    pub wrapped: bool,
}

/// How many cells match a Find Query: `total`, or at least `total` while
/// `partial`, of which `hidden` lie in collapsed groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindCount {
    pub total: u64,
    pub partial: bool,
    pub hidden: u64,
}

/// What [`pump`] asks of a Table Source.
pub trait PumpSource {
    /// Presents the table as `spec` asks, as far as it can. The default
    /// applies nothing and keeps the base view.
    fn view(&mut self, _spec: &ViewSpec) -> Viewed {
        Viewed::default()
    }

    /// How `axis` may be viewed by `key`, an item of the other axis, for the
    /// host to declare with [`Grid::set_support`]. The default supports
    /// everything the Grid asks for.
    fn support(&mut self, _axis: Axis, _key: Key) -> Support {
        Support::default()
    }

    /// Whether `axis` has a quick filter and how it matches, for the host to
    /// declare with [`Grid::set_quick_filter_support`]. The default has none.
    fn quick_filter(&mut self, _axis: Axis) -> Option<TextSupport> {
        None
    }

    /// Whether the source hides items of `axis` named in the View Spec's
    /// hidden Keys, for the host to declare with [`Grid::set_hide_support`].
    /// The default hides none.
    fn hide_support(&mut self, _axis: Axis) -> bool {
        false
    }

    /// The most hidden Keys the source takes on `axis`, if fewer than the
    /// Grid's own cap, for the host to declare with [`Grid::set_hide_cap`].
    fn hide_cap(&mut self, _axis: Axis) -> Option<usize> {
        None
    }

    /// The distinct values `key`, an item of the other axis, holds across
    /// the items of `axis`, as `query` asks, for a checklist; `None` if the
    /// source can't list them (the default), so OneOf conditions are not
    /// offered.
    fn distinct(&mut self, _axis: Axis, _key: Key, _query: &DistinctQuery) -> Option<Distinct> {
        None
    }

    /// Where `key` sits on `axis` now. The default knows no Keys and answers
    /// `Gone`.
    fn locate(&mut self, _axis: Axis, _key: Key) -> Located {
        Located::Gone { suggest: None }
    }

    /// Whether the source finds, and how it matches, for the host to declare
    /// with [`Grid::set_find_support`]. The default does not find.
    fn find_support(&mut self) -> Option<TextSupport> {
        None
    }

    /// The next (or previous) match of `query` after `from` in the view's
    /// order, wrapping at the end, or `None` if nothing matches. The
    /// default finds nothing.
    fn find_next(
        &mut self,
        _query: &FindQuery,
        _from: Option<FindMatch>,
        _direction: FindDirection,
    ) -> Option<FindStep> {
        None
    }

    /// The Group Item Keys above the item with `key` on `axis`, outermost
    /// first, whether its groups are open or not; empty on an axis that is
    /// not grouped (the default). [`pump`] hands them to
    /// [`Grid::find_stepped_within`] with each find step, so that a match in
    /// collapsed groups opens them.
    fn ancestors(&mut self, _axis: Axis, _key: Key) -> Vec<Key> {
        Vec::new()
    }

    /// Whether the Key `key` may still name an item of `axis`: `false` for
    /// one the source retired for good (a path Key whose path no longer fits
    /// the levels), which [`pump`] has the Grid [`Grid::forget`] after each
    /// View Spec applied. The default retires none.
    fn fits(&mut self, _axis: Axis, _key: Key) -> bool {
        true
    }

    /// Whether the items of `axis` can move now, which [`pump`] declares
    /// with [`Grid::set_movable`] before it drains and after each answer
    /// (so a source can withdraw moves while sorted, say). The default,
    /// `None`, leaves what the host declared.
    fn movable(&mut self, _axis: Axis) -> Option<bool> {
        None
    }

    /// Moves the items with `keys` on `axis` to before Position `to`.
    /// Returns what the move changed, or a short reason to refuse it, for
    /// the user (the default: the Table Source moves nothing).
    fn move_items(&mut self, _axis: Axis, _keys: &[Key], _to: Position) -> Result<Moved, String> {
        Err("These can't be moved".into())
    }
}

/// Has `grid` forget the Keys it holds state on that `source` retired.
pub fn forget_retired(grid: &mut Grid, source: &mut (impl PumpSource + ?Sized)) {
    for axis in Axis::ALL {
        let held = grid.held_keys(axis);
        let retired: Vec<Key> = held
            .into_iter()
            .filter(|&k| !source.fits(axis, k))
            .collect();
        grid.forget(axis, &retired);
    }
}

/// Declares to `grid` what `source` says of each axis's movability.
fn declare_movable(grid: &mut Grid, source: &mut (impl PumpSource + ?Sized)) {
    for axis in Axis::ALL {
        if let Some(movable) = source.movable(axis) {
            grid.set_movable(axis, movable);
        }
    }
}

/// Answers a `MoveRequested` from `source`: the View Spec the move left in
/// force, the Change Notices, then [`Grid::move_answered`].
pub fn answer_move(
    grid: &mut Grid,
    source: &mut (impl PumpSource + ?Sized),
    axis: Axis,
    keys: &[Key],
    to: Position,
) {
    match source.move_items(axis, keys, to) {
        Ok(moved) => {
            if let Some((spec, totals)) = moved.view {
                grid.view_changed(spec, totals);
            }
            for (axis, notice) in moved.notices {
                grid.notify(axis, notice);
            }
            grid.move_answered(None);
        }
        Err(reason) => grid.move_answered(Some(reason)),
    }
}

/// Drains the Grid's Effects until the queue is empty, answering those the
/// Table Source handles: `ViewRequested` with `view_applied` and the axes'
/// Change Notices (or `view_applied` of the previous spec to reject it), and
/// `Locate` with `located`, `MoveRequested` as [`answer_move`] does, and
/// `FindStepRequested` with `find_stepped` (`find_stepped_within` where
/// the match has ancestors). Answers can queue more Effects,
/// which the same loop drains. Movability is declared from
/// [`PumpSource::movable`] before draining and after each answer. `seen`
/// gets every Effect drained, in order, after the pump has answered it.
pub fn pump(
    grid: &mut Grid,
    source: &mut (impl PumpSource + ?Sized),
    mut seen: impl FnMut(&mut Grid, Stamped),
) {
    declare_movable(grid, source);
    while let Some(stamped) = grid.poll_effect() {
        match &stamped.effect {
            Effect::ViewRequested { spec, .. } => {
                let viewed = source.view(spec);
                grid.view_applied(viewed.applied, viewed.totals);
                for (axis, notice) in viewed.notices {
                    grid.notify(axis, notice);
                }
                forget_retired(grid, source);
            }
            &Effect::Locate { axis, key } => {
                let answer = source.locate(axis, key);
                grid.located(axis, key, answer);
            }
            Effect::MoveRequested { axis, keys, to } => answer_move(grid, source, *axis, keys, *to),
            Effect::FindStepRequested {
                query,
                from,
                direction,
            } => match source.find_next(query, *from, *direction) {
                Some(step) => {
                    let rows = source.ancestors(Axis::Rows, step.at.row);
                    let cols = source.ancestors(Axis::Cols, step.at.col);
                    match rows.is_empty() && cols.is_empty() {
                        true => grid.find_stepped(Some(step.at)),
                        false => grid.find_stepped_within(step.at, [&rows, &cols]),
                    }
                }
                None => grid.find_stepped(None),
            },
            _ => {}
        }
        if matches!(
            stamped.effect,
            Effect::ViewRequested { .. } | Effect::MoveRequested { .. }
        ) {
            declare_movable(grid, source);
        }
        seen(grid, stamped);
    }
}
