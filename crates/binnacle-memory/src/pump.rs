use binnacle_core::{
    Axis, ChangeNotice, Distinct, DistinctQuery, Effect, Grid, Key, Located, Position, Stamped,
    Support, TextSupport, Totals, ViewSpec,
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

    /// Moves the items with `keys` on `axis` to before Position `to`.
    /// Returns the Change Notice the axis needs, or `None` to refuse (the
    /// default: the Table Source moves nothing).
    fn move_items(&mut self, _axis: Axis, _keys: &[Key], _to: Position) -> Option<ChangeNotice> {
        None
    }
}

/// Drains the Grid's Effects until the queue is empty, answering those the
/// Table Source handles: `ViewRequested` with `view_applied` and the axes'
/// Change Notices (or `view_applied` of the previous spec to reject it), and
/// `Locate` with `located`, and `MoveRequested` with the axis's Change
/// Notice if the source moves the items. Answers can queue more Effects, which the same
/// loop drains. `seen` gets every Effect drained, in order, after the pump
/// has answered it.
pub fn pump(
    grid: &mut Grid,
    source: &mut (impl PumpSource + ?Sized),
    mut seen: impl FnMut(&mut Grid, Stamped),
) {
    while let Some(stamped) = grid.poll_effect() {
        match &stamped.effect {
            Effect::ViewRequested { spec, .. } => {
                let viewed = source.view(spec);
                grid.view_applied(viewed.applied, viewed.totals);
                for (axis, notice) in viewed.notices {
                    grid.notify(axis, notice);
                }
            }
            &Effect::Locate { axis, key } => {
                let answer = source.locate(axis, key);
                grid.located(axis, key, answer);
            }
            Effect::MoveRequested { axis, keys, to } => {
                if let Some(notice) = source.move_items(*axis, keys, *to) {
                    grid.notify(*axis, notice);
                }
            }
            _ => {}
        }
        seen(grid, stamped);
    }
}
