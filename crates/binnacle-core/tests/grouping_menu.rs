//! Seam 1 for the grouping menu's View Edits: removing and moving levels,
//! adding and removing values, and grouping either axis by a field, each
//! read from the View Spec the Grid requests.

use binnacle_core::{
    Aggregate, AggregateValue, Axis, AxisConfig, Effect, Grid, Grouping, Key, Level, Placement,
    Support, Totals, ViewEdit, ViewSpec,
};
use hegel::TestCase;
use hegel::generators as gs;

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const WEIGHT: Key = Key(3);
const QUARTER: Key = Key(5);

/// A Grid whose Table Source groups the rows and pivots by the region,
/// port and quarter, and sums the weight; it applies `spec`.
fn grid(spec: ViewSpec) -> Grid {
    let mut grid = Grid::new(AxisConfig::new(10, 1.0), AxisConfig::new(6, 5.0));
    let field = Support::default().with_group(true).with_pivot(true);
    for f in [REGION, PORT, QUARTER] {
        grid.set_support(Axis::Rows, f, field);
    }
    grid.set_aggregates(Axis::Rows, WEIGHT, Aggregate::numbers());
    grid.request_view(spec.clone());
    grid.view_applied(spec, Totals::unknown());
    while grid.poll_effect().is_some() {}
    grid
}

/// The View Spec `edit` asks for, with the View Edit it names, if any.
fn asked(grid: &mut Grid, edit: impl FnOnce(&mut Grid)) -> Option<(ViewSpec, ViewEdit)> {
    edit(grid);
    let mut asked = None;
    while let Some(e) = grid.poll_effect() {
        if let Effect::ViewRequested { spec, edit } = e.effect {
            asked = Some((spec, edit));
        }
    }
    asked
}

fn sum() -> AggregateValue {
    AggregateValue::of(WEIGHT, "sum")
}

/// The grouped demo's shape: rows by region, then port, Subtotals after,
/// the sum of the weights and the count.
fn grouped() -> Grouping {
    let mut g = Grouping::rows_by(REGION);
    g.row_levels = [REGION, PORT]
        .map(|field| Level::Field {
            field,
            subtotal: Placement::After,
        })
        .to_vec();
    g.values = vec![sum(), AggregateValue::count()];
    g
}

fn spec(g: Grouping) -> ViewSpec {
    ViewSpec::default().with_grouping(Some(g))
}

fn grouping(asked: Option<(ViewSpec, ViewEdit)>) -> Option<Grouping> {
    asked.expect("a View Spec asked for").0.grouping
}

fn fields(levels: &[Level]) -> Vec<Option<Key>> {
    levels.iter().map(Level::field_key).collect()
}

#[test]
fn removing_a_level_drops_it_and_names_the_edit() {
    let mut grid = grid(spec(grouped()));
    let (spec, edit) = asked(&mut grid, |g| g.remove_level(Axis::Rows, Some(REGION))).unwrap();
    let g = spec.grouping.unwrap();
    assert_eq!(fields(&g.row_levels), vec![Some(PORT)]);
    assert_eq!(
        edit,
        ViewEdit::RemoveLevel {
            axis: Axis::Rows,
            field: Some(REGION),
        }
    );
}

#[test]
fn removing_the_last_level_makes_the_table_plain() {
    let mut g = grouped();
    g.row_levels.truncate(1);
    let mut grid = grid(spec(g));
    assert_eq!(
        grouping(asked(&mut grid, |g| g.remove_level(Axis::Rows, Some(REGION)))),
        None
    );
}

#[test]
fn removing_a_level_not_there_or_the_values_level_asks_nothing() {
    let mut grid = grid(spec(grouped()));
    assert_eq!(
        asked(&mut grid, |g| g.remove_level(Axis::Cols, Some(REGION))),
        None
    );
    assert_eq!(
        asked(&mut grid, |g| g.remove_level(Axis::Rows, Some(QUARTER))),
        None
    );
    let mut pivot = grouped();
    pivot.col_levels = vec![Level::field(QUARTER), Level::Values];
    let mut grid = self::grid(spec(pivot));
    assert_eq!(asked(&mut grid, |g| g.remove_level(Axis::Cols, None)), None);
}

#[test]
fn moving_a_row_level_to_the_columns_makes_a_pivot_with_the_values_level_and_back() {
    let mut grid = grid(spec(grouped()));
    let (to, edit) = asked(&mut grid, |g| g.move_level(Axis::Rows, Some(PORT))).unwrap();
    assert_eq!(
        edit,
        ViewEdit::MoveLevel {
            axis: Axis::Rows,
            field: Some(PORT),
        }
    );
    let g = to.grouping.clone().unwrap();
    assert!(g.pivot());
    assert_eq!(fields(&g.row_levels), vec![Some(REGION)]);
    assert_eq!(
        fields(&g.col_levels),
        vec![Some(PORT), None],
        "two values: the values level joins the columns, innermost"
    );
    assert_eq!(
        g.col_levels[0],
        Level::Field {
            field: PORT,
            subtotal: Placement::After
        },
        "the level keeps its Subtotal"
    );
    let mut grid = self::grid(to);
    let back = grouping(asked(&mut grid, |g| g.move_level(Axis::Cols, Some(PORT)))).unwrap();
    assert!(
        !back.pivot(),
        "the values level leaves with the last column field"
    );
    assert_eq!(fields(&back.row_levels), vec![Some(REGION), Some(PORT)]);
    assert_eq!(back.col_levels, vec![]);
}

#[test]
fn the_values_level_moves_to_the_other_axis() {
    let mut g = grouped();
    g.col_levels = vec![Level::field(QUARTER), Level::Values];
    let mut grid = grid(spec(g));
    let g = grouping(asked(&mut grid, |g| g.move_level(Axis::Cols, None))).unwrap();
    assert_eq!(fields(&g.row_levels), vec![Some(REGION), Some(PORT), None]);
    assert_eq!(fields(&g.col_levels), vec![Some(QUARTER)]);
}

#[test]
fn a_level_moves_only_where_the_source_groups_that_axis_by_its_field() {
    let mut grid = grid(spec(grouped()));
    grid.set_support(Axis::Rows, PORT, Support::default().with_group(true));
    assert_eq!(
        asked(&mut grid, |g| g.move_level(Axis::Rows, Some(PORT))),
        None
    );
    assert_eq!(
        asked(&mut grid, |g| g.move_level(Axis::Cols, Some(PORT))),
        None
    );
}

#[test]
fn grouping_the_columns_by_a_field_needs_pivot_support_and_adds_a_column_level() {
    let mut grid = grid(spec(grouped()));
    let (to, edit) = asked(&mut grid, |g| g.group_by(Axis::Cols, QUARTER)).unwrap();
    assert_eq!(
        edit,
        ViewEdit::Group {
            axis: Axis::Cols,
            field: QUARTER,
        }
    );
    let g = to.grouping.unwrap();
    assert_eq!(fields(&g.col_levels), vec![Some(QUARTER), None]);
    let mut grid = self::grid(spec(grouped()));
    grid.set_support(Axis::Rows, QUARTER, Support::default().with_group(true));
    assert_eq!(asked(&mut grid, |g| g.group_by(Axis::Cols, QUARTER)), None);
}

#[test]
fn a_field_groups_one_level_at_most() {
    let mut grid = grid(spec(grouped()));
    assert_eq!(asked(&mut grid, |g| g.group_by(Axis::Cols, PORT)), None);
}

#[test]
fn adding_a_value_takes_an_aggregate_the_source_offers() {
    let mut g = grouped();
    g.values = vec![AggregateValue::count()];
    let mut grid = grid(spec(g));
    let average = AggregateValue::of(WEIGHT, "average");
    let (to, edit) = asked(&mut grid, |g| g.add_value(average.clone())).unwrap();
    assert_eq!(
        edit,
        ViewEdit::AddValue {
            value: average.clone()
        }
    );
    let g = to.grouping.unwrap();
    assert_eq!(g.values, vec![AggregateValue::count(), average.clone()]);
    assert_eq!(g.col_levels, vec![], "no pivot: no values level");
    let median = AggregateValue::of(WEIGHT, "median");
    assert_eq!(
        asked(&mut grid, |g| g.add_value(median)),
        None,
        "not offered"
    );
    let port = AggregateValue::of(PORT, "sum");
    assert_eq!(
        asked(&mut grid, |g| g.add_value(port)),
        None,
        "nothing over the port"
    );
}

#[test]
fn adding_a_value_already_there_or_to_a_plain_table_asks_nothing() {
    let mut grid = grid(spec(grouped()));
    assert_eq!(asked(&mut grid, |g| g.add_value(sum())), None);
    let mut grid = self::grid(ViewSpec::default());
    assert_eq!(asked(&mut grid, |g| g.add_value(sum())), None);
}

#[test]
fn a_second_value_on_a_pivot_brings_the_values_level_and_the_last_but_one_takes_it_away() {
    let mut g = grouped();
    g.col_levels = vec![Level::field(QUARTER)];
    g.values = vec![sum()];
    let mut grid = grid(spec(g));
    let g = grouping(asked(&mut grid, |g| g.add_value(AggregateValue::count()))).unwrap();
    assert_eq!(fields(&g.col_levels), vec![Some(QUARTER), None]);
    let mut grid = self::grid(spec(g));
    let (to, edit) = asked(&mut grid, |g| g.remove_value(&AggregateValue::count())).unwrap();
    assert_eq!(
        edit,
        ViewEdit::RemoveValue {
            value: AggregateValue::count()
        }
    );
    let g = to.grouping.unwrap();
    assert_eq!(g.values, vec![sum()]);
    assert_eq!(
        fields(&g.col_levels),
        vec![Some(QUARTER)],
        "one value: a caption"
    );
}

#[test]
fn the_last_value_stays() {
    let mut g = grouped();
    g.values = vec![sum()];
    let mut grid = grid(spec(g));
    assert_eq!(asked(&mut grid, |g| g.remove_value(&sum())), None);
    assert_eq!(
        asked(&mut grid, |g| g.remove_value(&AggregateValue::count())),
        None,
        "not a value"
    );
}

#[test]
fn edits_build_on_the_latest_request() {
    let mut grid = grid(spec(grouped()));
    let _ = asked(&mut grid, |g| g.move_level(Axis::Rows, Some(PORT)));
    let g = grouping(asked(&mut grid, |g| g.move_level(Axis::Rows, Some(REGION)))).unwrap();
    assert_eq!(g.row_levels, vec![]);
    assert_eq!(
        fields(&g.col_levels),
        vec![Some(PORT), Some(REGION), None],
        "a field level joins inside the others, the values level staying innermost"
    );
}

/// A grouping menu action, as a user picks it.
#[derive(Clone, Debug)]
enum Pick {
    Group(Axis, Key),
    Remove(Axis, Option<Key>),
    Move(Axis, Option<Key>),
    AddValue(AggregateValue),
    RemoveValue(AggregateValue),
}

/// What the menu leaves the levels and values as, kept the plain way: the
/// field levels of each axis in order, and the values; and the fields each
/// axis may be grouped by, as the source declares them.
#[derive(Clone, Debug, Default, PartialEq)]
struct Model {
    fields: [Vec<Key>; 2],
    values: Vec<AggregateValue>,
    groups: [Vec<Key>; 2],
}

impl Model {
    fn of(spec: &ViewSpec) -> Model {
        let Some(g) = &spec.grouping else {
            return Model::default();
        };
        let keys = |a| g.levels(a).iter().filter_map(Level::field_key).collect();
        Model {
            fields: [keys(Axis::Rows), keys(Axis::Cols)],
            values: g.values.clone(),
            groups: Default::default(),
        }
    }

    fn apply(&mut self, pick: &Pick) {
        let used = |m: &Model, f| m.fields.iter().flatten().any(|&k| k == f);
        let grouped = self.fields.iter().any(|f| !f.is_empty());
        let groups = |axis: Axis, f: &Key| self.groups[axis.index()].contains(f);
        // A count of items, or an aggregate the source declares over WEIGHT.
        let offered = |v: &AggregateValue| {
            *v == AggregateValue::count()
                || (v.field == Some(WEIGHT)
                    && v.params.is_empty()
                    && [
                        "count",
                        "distinct count",
                        "sum",
                        "average",
                        "minimum",
                        "maximum",
                    ]
                    .contains(&v.aggregate.as_str()))
        };
        match pick {
            Pick::Group(axis, f) if !used(self, *f) && groups(*axis, f) => {
                if !grouped {
                    self.values = vec![AggregateValue::count()];
                }
                self.fields[axis.index()].push(*f);
            }
            Pick::Remove(axis, Some(f)) if self.fields[axis.index()].contains(f) => {
                self.fields[axis.index()].retain(|k| k != f);
                if self.fields.iter().all(Vec::is_empty) {
                    self.values.clear();
                }
            }
            Pick::Move(axis, Some(f))
                if self.fields[axis.index()].contains(f) && groups(axis.other(), f) =>
            {
                self.fields[axis.index()].retain(|k| k != f);
                self.fields[axis.other().index()].push(*f);
            }
            Pick::AddValue(v) if grouped && offered(v) && !self.values.contains(v) => {
                self.values.push(v.clone())
            }
            Pick::RemoveValue(v) if self.values.len() > 1 => self.values.retain(|x| x != v),
            _ => {}
        }
    }
}

#[hegel::composite]
fn pick(tc: &TestCase) -> Pick {
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let field = tc.draw(gs::sampled_from(vec![REGION, PORT, QUARTER, WEIGHT]));
    let level = tc.draw(gs::sampled_from(vec![
        Some(REGION),
        Some(PORT),
        Some(QUARTER),
        Some(WEIGHT),
        None,
    ]));
    // Offered values, and some the source doesn't offer: over a field it
    // declares no aggregates for, with parameters it doesn't take, or an
    // aggregate it doesn't name.
    let value = tc.draw(gs::sampled_from(vec![
        AggregateValue::count(),
        sum(),
        AggregateValue::of(WEIGHT, "average"),
        AggregateValue::of(REGION, "sum"),
        sum().with_params(vec!["0.9".into()]),
        AggregateValue::of(WEIGHT, "median"),
    ]));
    match tc.draw(gs::integers::<u8>().max_value(4)) {
        0 => Pick::Group(axis, field),
        1 => Pick::Remove(axis, level),
        2 => Pick::Move(axis, level),
        3 => Pick::AddValue(value),
        _ => Pick::RemoveValue(value),
    }
}

/// Through any picks from the grouping menu, each answered whole, the
/// levels and values change as a plain model of them says, and the
/// grouping stays well formed: a field groups one level at most, the
/// values are there and distinct, and the values pseudo-level shows
/// exactly where several values meet a pivot, placed once.
#[hegel::test(test_cases = 500)]
fn menu_picks_change_levels_and_values_as_the_model_says(tc: TestCase) {
    let mut grid = grid(ViewSpec::default());
    let mut model = Model::default();
    // What each field may group, drawn: the rows by it, the columns by it.
    for f in [REGION, PORT, QUARTER, WEIGHT] {
        let (group, pivot) = (tc.draw(gs::booleans()), tc.draw(gs::booleans()));
        grid.set_support(
            Axis::Rows,
            f,
            Support::default().with_group(group).with_pivot(pivot),
        );
        for (on, axis) in [(group, Axis::Rows), (pivot, Axis::Cols)] {
            if on {
                model.groups[axis.index()].push(f);
            }
        }
    }
    let picks = tc.draw(gs::vecs(pick()).min_size(1).max_size(16));
    for p in &picks {
        let asked = asked(&mut grid, |g| match p {
            Pick::Group(axis, f) => g.group_by(*axis, *f),
            Pick::Remove(axis, f) => g.remove_level(*axis, *f),
            Pick::Move(axis, f) => g.move_level(*axis, *f),
            Pick::AddValue(v) => g.add_value(v.clone()),
            Pick::RemoveValue(v) => g.remove_value(v),
        });
        model.apply(p);
        if let Some((spec, _)) = asked {
            grid.view_applied(spec, Totals::unknown());
        }
        let applied = grid.snapshot().applied_view().clone();
        let seen = Model::of(&applied);
        assert_eq!(
            (&seen.fields, &seen.values),
            (&model.fields, &model.values),
            "after {p:?}"
        );
        let Some(g) = applied.grouping else {
            continue;
        };
        assert!(!g.row_levels.is_empty() || !g.col_levels.is_empty());
        assert!(!g.values.is_empty());
        let distinct = |v: &[AggregateValue]| (0..v.len()).all(|i| !v[i + 1..].contains(&v[i]));
        assert!(distinct(&g.values));
        let all: Vec<&Level> = g.row_levels.iter().chain(&g.col_levels).collect();
        let values_levels = all.iter().filter(|l| ***l == Level::Values).count();
        let column_fields = g.col_levels.iter().any(|l| l.field_key().is_some());
        let rows_hold_values = g.row_levels.contains(&Level::Values);
        let wanted = g.values.len() > 1 && (column_fields || rows_hold_values);
        assert_eq!(values_levels, usize::from(wanted), "values level in {g:?}");
        if g.col_levels.contains(&Level::Values) {
            assert!(
                column_fields,
                "a values level on the columns only beside a field"
            );
            assert_eq!(g.col_levels.last(), Some(&Level::Values), "innermost");
        }
    }
}
