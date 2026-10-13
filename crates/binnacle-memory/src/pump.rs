use binnacle_core::{Axis, ChangeNotice, Effect, Grid, Key, Located, Position, SortSpec, Stamped};

/// What [`pump`] asks of a Table Source.
pub trait PumpSource {
    /// Sorts the rows by `spec`. Returns the Change Notice the rows need, or
    /// the Sort Spec still in force to reject the request (the default: the
    /// Table Source cannot sort and keeps base order).
    fn sort(&mut self, _spec: &SortSpec) -> Result<ChangeNotice, SortSpec> {
        Err(SortSpec::unsorted())
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
/// Table Source handles: `SortRequested` with `sort_applied` and the rows'
/// Change Notice (or `sort_applied` of the previous spec to reject it), and
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
            Effect::SortRequested { spec } => match source.sort(spec) {
                Ok(notice) => {
                    grid.sort_applied(spec.clone());
                    grid.notify(Axis::Rows, notice);
                }
                Err(previous) => grid.sort_applied(previous),
            },
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
