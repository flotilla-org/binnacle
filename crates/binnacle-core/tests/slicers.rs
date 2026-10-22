//! Seam 1 scenarios for slicers and grouping support: adding, replacing and
//! removing a slicer is a View Edit on the grouping, slicers outlive the
//! levels, and the parts of a grouping the Table Source leaves out (levels,
//! Subtotals, values, slicers) stay listed, not applied, so restoring one it
//! can't apply never fails.

use binnacle_core::{
    Aggregate, AggregateValue, Axis, AxisConfig, Comparison, Condition, ConditionState, Effect,
    Grid, Grouping, Key, Level, Placement, Placements, Scalar, Support, Totals, ViewEdit, ViewSpec,
    Viewport,
};

const V: Viewport = Viewport {
    width: 40.0,
    height: 10.0,
};

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const WEIGHT: Key = Key(3);
const YEAR: Key = Key(4);

fn grid() -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0),
    );
    grid.begin_frame(V);
    grid
}

fn heavy() -> Condition {
    Condition::compare(WEIGHT, Comparison::Greater(Scalar::Number(100.0)))
}

fn native() -> Condition {
    Condition::native(YEAR, "year = 2024")
}

/// The View Specs and edits requested since the last call.
fn requests(grid: &mut Grid) -> Vec<(ViewSpec, ViewEdit)> {
    std::iter::from_fn(|| grid.poll_effect())
        .filter_map(|e| match e.effect {
            Effect::ViewRequested { spec, edit } => Some((spec, edit)),
            _ => None,
        })
        .collect()
}

/// Answers every request with what it asked for.
fn apply_all(grid: &mut Grid) {
    for (spec, _) in requests(grid) {
        grid.view_applied(spec, Totals::unknown());
    }
}

fn grouped(levels: &[Key]) -> Grouping {
    let mut grouping = Grouping::rows_by(levels[0]);
    grouping.row_levels = levels.iter().map(|&f| Level::field(f)).collect();
    grouping
}

#[test]
fn a_slicer_on_a_plain_table_asks_for_a_grouping_with_no_levels() {
    let mut grid = grid();
    grid.add_slicer(heavy());
    let mut want = Grouping::default();
    want.values = vec![AggregateValue::count()];
    want.slicers = vec![heavy()];
    assert_eq!(
        requests(&mut grid),
        vec![(
            ViewSpec::default().with_grouping(Some(want)),
            ViewEdit::AddSlicer { condition: heavy() }
        )]
    );
}

#[test]
fn removing_the_last_slicer_with_no_levels_makes_the_table_plain_again() {
    let mut grid = grid();
    grid.add_slicer(heavy());
    apply_all(&mut grid);
    grid.remove_slicer(&heavy());
    let asked = requests(&mut grid);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].0, ViewSpec::default());
    assert_eq!(asked[0].1, ViewEdit::RemoveSlicer { condition: heavy() });
}

#[test]
fn slicers_outlive_ungrouping_and_the_levels_outlive_the_slicers() {
    let mut grid = grid();
    grid.set_support(Axis::Rows, REGION, Support::default().with_group(true));
    grid.group_by(Axis::Rows, REGION);
    grid.add_slicer(heavy());
    apply_all(&mut grid);
    grid.ungroup(Axis::Rows);
    let (spec, _) = requests(&mut grid).pop().unwrap();
    let grouping = spec.grouping.expect("the slicer keeps a grouping");
    assert!(grouping.row_levels.is_empty());
    assert_eq!(grouping.slicers, vec![heavy()]);

    let mut grid = self::grid();
    grid.set_support(Axis::Rows, REGION, Support::default().with_group(true));
    grid.group_by(Axis::Rows, REGION);
    grid.add_slicer(heavy());
    apply_all(&mut grid);
    grid.remove_slicer(&heavy());
    let (spec, _) = requests(&mut grid).pop().unwrap();
    assert_eq!(spec.grouping, Some(Grouping::rows_by(REGION)));
}

#[test]
fn replacing_a_slicer_keeps_its_place_and_asks_nothing_for_the_same() {
    let mut grid = grid();
    grid.add_slicer(native());
    grid.add_slicer(heavy());
    apply_all(&mut grid);
    let light = Condition::compare(WEIGHT, Comparison::AtMost(Scalar::Number(10.0)));
    grid.replace_slicer(&native(), native());
    assert_eq!(requests(&mut grid), vec![], "the same slicer");
    grid.replace_slicer(&native(), light.clone());
    let (spec, edit) = requests(&mut grid).pop().unwrap();
    assert_eq!(spec.grouping.unwrap().slicers, vec![light.clone(), heavy()]);
    assert_eq!(
        edit,
        ViewEdit::ReplaceSlicer {
            from: native(),
            to: light
        }
    );
    grid.add_slicer(Condition::native(YEAR, ""));
    assert_eq!(requests(&mut grid), vec![], "an empty native slicer");
}

#[test]
fn slicers_list_as_applied_adding_removing_and_not_applied() {
    let mut grid = grid();
    grid.add_slicer(heavy());
    grid.begin_frame(V);
    assert_eq!(
        grid.snapshot().slicers(),
        vec![(heavy(), ConditionState::Adding)]
    );
    apply_all(&mut grid);
    grid.begin_frame(V);
    assert_eq!(
        grid.snapshot().slicers(),
        vec![(heavy(), ConditionState::Applied)]
    );

    grid.replace_slicer(&heavy(), native());
    let asked = requests(&mut grid);
    grid.begin_frame(V);
    assert_eq!(
        grid.snapshot().slicers(),
        vec![
            (native(), ConditionState::Adding),
            (heavy(), ConditionState::Removing)
        ]
    );
    // The source can't slice natively: it drops the slicer, and with it the
    // grouping, as there is nothing else left.
    for _ in asked {
        grid.view_applied(ViewSpec::default(), Totals::unknown());
    }
    grid.begin_frame(V);
    assert_eq!(
        grid.snapshot().slicers(),
        vec![(native(), ConditionState::NotApplied)]
    );
    grid.remove_slicer(&native());
    apply_all(&mut grid);
    grid.begin_frame(V);
    assert_eq!(grid.snapshot().slicers(), vec![]);
    assert_eq!(grid.snapshot().requested_view(), None);
}

#[test]
fn a_grouping_applied_in_part_lists_the_rest_not_applied() {
    let mut grid = grid();
    let mut wanted = grouped(&[REGION, PORT]);
    wanted.row_levels[1] = Level::Field {
        field: PORT,
        subtotal: Placement::Before,
    };
    wanted.values = vec![
        AggregateValue::of(WEIGHT, "sum"),
        AggregateValue::of(WEIGHT, "median"),
    ];
    wanted.slicers = vec![heavy(), native()];
    // Restoring a layout: the whole spec at once.
    grid.request_view(ViewSpec::default().with_grouping(Some(wanted.clone())));
    let _ = requests(&mut grid);
    grid.begin_frame(V);
    let snap = grid.snapshot();
    assert!(
        snap.levels(Axis::Rows)
            .iter()
            .all(|(_, s)| *s == ConditionState::Adding)
    );

    // The source groups by both but can't place the port's Subtotal before,
    // has no median and no native slicers.
    let mut applied = grouped(&[REGION, PORT]);
    applied.values = vec![AggregateValue::of(WEIGHT, "sum")];
    applied.slicers = vec![heavy()];
    grid.view_applied(
        ViewSpec::default().with_grouping(Some(applied)),
        Totals::unknown(),
    );
    grid.begin_frame(V);
    let snap = grid.snapshot();
    assert_eq!(
        snap.levels(Axis::Rows),
        vec![
            (Level::field(REGION), ConditionState::Applied),
            (wanted.row_levels[1].clone(), ConditionState::NotApplied),
        ],
        "the port level is listed once, its Subtotal not applied"
    );
    assert_eq!(snap.levels(Axis::Cols), vec![]);
    assert_eq!(
        snap.values(),
        vec![
            (AggregateValue::of(WEIGHT, "sum"), ConditionState::Applied),
            (
                AggregateValue::of(WEIGHT, "median"),
                ConditionState::NotApplied
            ),
        ]
    );
    assert_eq!(
        snap.slicers(),
        vec![
            (heavy(), ConditionState::Applied),
            (native(), ConditionState::NotApplied)
        ]
    );
}

#[test]
fn a_grouping_the_source_cant_apply_at_all_stays_listed_until_ungrouped() {
    let mut grid = grid();
    let mut wanted = grouped(&[YEAR]);
    wanted.slicers = vec![native()];
    grid.request_view(ViewSpec::default().with_grouping(Some(wanted)));
    let _ = requests(&mut grid);
    grid.view_applied(ViewSpec::default(), Totals::unknown());
    grid.begin_frame(V);
    let snap = grid.snapshot();
    assert_eq!(snap.applied_view(), &ViewSpec::default());
    assert_eq!(
        snap.levels(Axis::Rows),
        vec![(Level::field(YEAR), ConditionState::NotApplied)]
    );
    assert_eq!(snap.slicers(), vec![(native(), ConditionState::NotApplied)]);

    // Ungrouping and removing the slicer clear what was not applied.
    grid.ungroup(Axis::Rows);
    grid.remove_slicer(&native());
    let asked = requests(&mut grid);
    assert_eq!(asked.last().unwrap().0, ViewSpec::default());
    for _ in asked {
        grid.view_applied(ViewSpec::default(), Totals::unknown());
    }
    grid.begin_frame(V);
    let snap = grid.snapshot();
    assert_eq!(snap.levels(Axis::Rows), vec![]);
    assert_eq!(snap.slicers(), vec![]);
    assert_eq!(snap.requested_view(), None);
}

#[test]
fn per_field_support_and_aggregates_reach_the_snapshot() {
    let mut grid = grid();
    let support = Support::default()
        .with_group(true)
        .with_pivot(true)
        .with_native(true)
        .with_subtotals(Placements {
            before: false,
            after: true,
        });
    grid.set_support(Axis::Rows, WEIGHT, support);
    let custom = Aggregate::new("percentile").with_params(vec!["p".into()]);
    let mut offered = Aggregate::numbers();
    offered.push(custom.clone());
    grid.set_aggregates(Axis::Rows, WEIGHT, offered.clone());
    grid.begin_frame(V);
    let snap = grid.snapshot();
    assert_eq!(snap.support(Axis::Rows, WEIGHT), support);
    assert!(
        snap.support(Axis::Rows, WEIGHT)
            .subtotals
            .allows(Placement::After)
    );
    assert!(
        !snap
            .support(Axis::Rows, WEIGHT)
            .subtotals
            .allows(Placement::Before)
    );
    assert!(
        snap.support(Axis::Rows, WEIGHT)
            .subtotals
            .allows(Placement::Off)
    );
    assert_eq!(snap.aggregates(Axis::Rows, WEIGHT), offered.as_slice());
    assert_eq!(snap.aggregates(Axis::Rows, PORT), &[]);
    assert_eq!(snap.aggregates(Axis::Cols, WEIGHT), &[]);

    grid.set_aggregates(Axis::Rows, WEIGHT, Vec::new());
    grid.begin_frame(V);
    assert_eq!(grid.snapshot().aggregates(Axis::Rows, WEIGHT), &[]);
}
