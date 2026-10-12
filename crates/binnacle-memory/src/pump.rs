use binnacle_core::{Axis, ChangeNotice, Effect, Grid, Key, Located, SortSpec, Stamped};

/// What [`pump`] asks of a Table Source.
pub trait PumpSource {
    /// Sorts the rows by `spec`. Returns the Change Notice the rows need, or
    /// the Sort Spec still in force to reject the request.
    fn sort(&mut self, spec: &SortSpec) -> Result<ChangeNotice, SortSpec>;

    /// Where `key` sits on `axis` now.
    fn locate(&mut self, axis: Axis, key: Key) -> Located;
}

/// Drains the Grid's Effects until the queue is empty, answering those the
/// Table Source handles: `SortRequested` with `sort_applied` and the rows'
/// Change Notice (or `sort_applied` of the previous spec to reject it), and
/// `Locate` with `located`. Answers can queue more Effects, which the same
/// loop drains. `seen` gets every Effect drained, in order, after the pump
/// has answered it.
pub fn pump(
    grid: &mut Grid,
    source: &mut impl PumpSource,
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
            _ => {}
        }
        seen(grid, stamped);
    }
}
