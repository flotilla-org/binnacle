use binnacle_core::{
    Axis, Distinct, DistinctQuery, FindDirection, FindMatch, FindQuery, Key, Located, Position,
    Searched, Support, TextSupport, Totals, ViewSpec,
};

use crate::{FindStep, Moved, PumpSource, Viewed};

/// A Table Source on its side: the Grid's rows are the inner source's
/// columns and the Grid's columns its rows. Every request is passed on about
/// the other axis and every answer swapped back, so a row-major source (the
/// In-Memory Table Source over records, say) serves a column-major table,
/// each record a column. It adds nothing: what the inner source does along
/// one axis only, the transposed table does along the other only.
///
/// It is a pure axis swap and must stay one: it is not how a source supports
/// its columns. The In-Memory Table Source supports sort, filter, hide and
/// move on the column axis natively, tested against its own model; turning
/// it only exercises those paths from the other side.
///
/// A View Spec's Text Queries and Conditions name items of the other axis,
/// which turning leaves on the same side, so they pass on as they are. A
/// Find Query names columns, which the inner source has as rows: one naming
/// the items it searches finds nothing (see [`transpose_find`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transposed<S>(pub S);

impl<S> Transposed<S> {
    pub fn inner(&self) -> &S {
        &self.0
    }

    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.0
    }

    pub fn into_inner(self) -> S {
        self.0
    }
}

/// `spec` with its axes' parts swapped.
pub fn transpose_spec(spec: &ViewSpec) -> ViewSpec {
    ViewSpec {
        rows: spec.cols.clone(),
        cols: spec.rows.clone(),
    }
}

/// `totals` with its axes swapped.
pub fn transpose_totals(totals: Totals) -> Totals {
    Totals {
        rows: totals.cols,
        cols: totals.rows,
    }
}

/// `query` for the inner source of a [`Transposed`]: stepping along the
/// other axis first. `None` if it names the columns it searches, since the
/// inner source can search only its own columns, which are the rows here.
pub fn transpose_find(query: &FindQuery) -> Option<FindQuery> {
    match query.query.searches {
        Searched::Shown => Some(query.clone().by(query.by.other())),
        Searched::Keys(_) => None,
    }
}

/// `m` with its row and column swapped.
pub fn transpose_match(m: FindMatch) -> FindMatch {
    FindMatch::new(m.col, m.row)
}

impl<S: PumpSource> PumpSource for Transposed<S> {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        let viewed = self.0.view(&transpose_spec(spec));
        Viewed {
            applied: transpose_spec(&viewed.applied),
            totals: transpose_totals(viewed.totals),
            notices: (viewed.notices.into_iter())
                .map(|(axis, notice)| (axis.other(), notice))
                .collect(),
        }
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.0.support(axis.other(), key)
    }

    fn quick_filter(&mut self, axis: Axis) -> Option<TextSupport> {
        self.0.quick_filter(axis.other())
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        self.0.distinct(axis.other(), key, query)
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.0.locate(axis.other(), key)
    }

    fn find_support(&mut self) -> Option<TextSupport> {
        self.0.find_support()
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        let from = from.map(transpose_match);
        let step = self.0.find_next(&transpose_find(query)?, from, direction)?;
        Some(FindStep {
            at: transpose_match(step.at),
            ..step
        })
    }

    fn movable(&mut self, axis: Axis) -> Option<bool> {
        self.0.movable(axis.other())
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Result<Moved, String> {
        let moved = self.0.move_items(axis.other(), keys, to)?;
        Ok(Moved {
            notices: (moved.notices.into_iter())
                .map(|(axis, notice)| (axis.other(), notice))
                .collect(),
            view: (moved.view)
                .map(|(spec, totals)| (transpose_spec(&spec), transpose_totals(totals))),
        })
    }
}
