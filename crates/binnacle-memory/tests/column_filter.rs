//! The column filter's shared rules through their public seam: ticks on the
//! checklist write the exact condition each change needs, the conditions the
//! Grid is asked for are the ones the rules return, and the comparison row
//! and the chips name what they write.

use binnacle_core::{
    Axis, AxisConfig, Compared, Comparison, Condition, Grid, Key, Scalar, Support, TextQuery,
    TextSupport, Value,
};
use binnacle_memory::column_filter::{Column, Shown, Tick, chip_text, compare};
use hegel::TestCase;
use hegel::generators as gs;

const PORT: Key = Key(1);

fn grid() -> Grid {
    let mut grid = Grid::new(AxisConfig::new(5, 1.0), AxisConfig::new(3, 1.0));
    let support = Support::default()
        .with_text(Some(TextSupport::new(true)))
        .with_one_of(true);
    grid.set_support(Axis::Rows, PORT, support);
    grid
}

/// The conditions on the rows the Grid was last asked for.
fn requested(grid: &mut Grid) -> Vec<Condition> {
    let snapshot = grid.snapshot();
    let view = snapshot.requested_view().unwrap_or(snapshot.applied_view());
    view.rows.filter.conditions.conditions().to_vec()
}

fn values() -> Vec<Value> {
    ["Aberdeen", "Bergen", "Cádiz"]
        .into_iter()
        .map(|p| Value::of(Some(p)))
        .chain([Value::Blank])
        .collect()
}

fn whole(values: &[Value]) -> Shown<'_> {
    Shown {
        values,
        partial: false,
        search: "",
    }
}

#[test]
fn unticking_from_all_writes_not_one_of_and_ticking_it_back_removes_it() {
    let mut grid = grid();
    let values = values();
    let column = Column::of(&[], Axis::Rows, PORT);
    let column = column.tick(&mut grid, &values[1], false, &whole(&values));
    let not_bergen = Condition::one_of(PORT, vec![values[1].clone()]).not(true);
    assert_eq!(requested(&mut grid), vec![not_bergen]);
    assert!(!column.ticked(&values[1]) && column.ticked(&values[0]));
    let column = column.tick(&mut grid, &values[1], true, &whole(&values));
    assert_eq!(requested(&mut grid), vec![]);
    assert_eq!(column.one_of, None);
}

#[test]
fn unticking_the_head_writes_one_of_nothing_and_later_ticks_list_the_ticked() {
    let mut grid = grid();
    let values = values();
    let column = Column::of(&[], Axis::Rows, PORT);
    let column = column.tick_head(&mut grid, false, &whole(&values));
    assert_eq!(requested(&mut grid), vec![Condition::one_of(PORT, vec![])]);
    let column = column.tick(&mut grid, &Value::Blank, true, &whole(&values));
    assert_eq!(
        requested(&mut grid),
        vec![Condition::one_of(PORT, vec![Value::Blank])]
    );
    assert!(!column.head_ticked(&whole(&values)));
}

#[test]
fn ticking_every_value_back_from_none_removes_the_one_of() {
    let mut grid = grid();
    let values = values();
    let shown = whole(&values);
    let mut column = Column::of(&[], Axis::Rows, PORT).tick_head(&mut grid, false, &shown);
    for v in &values {
        column = column.tick(&mut grid, v, true, &shown);
    }
    assert_eq!(requested(&mut grid), vec![]);
    assert!(column.head_ticked(&shown));
}

#[test]
fn select_all_matching_a_search_writes_a_text_condition_and_drops_the_one_of() {
    let mut grid = grid();
    let values = values();
    let column = Column::of(&[], Axis::Rows, PORT);
    let column = column.tick(&mut grid, &values[0], false, &whole(&values));
    let found = [values[1].clone()];
    let searched = Shown {
        values: &found,
        partial: false,
        search: "berg",
    };
    let column = column.tick_head(&mut grid, true, &searched);
    assert_eq!(
        requested(&mut grid),
        vec![Condition::text(PORT, TextQuery::new("berg"))]
    );
    assert!(column.head_ticked(&searched));
    // The Text condition rejects the others: they can't be ticked.
    assert!(column.excluded(&values[0]) && column.excluded(&Value::Blank));
}

/// Ticks and unticks of values and the head, on a whole unsearched list,
/// leave each value ticked as the last change to it (or the head) says, and
/// the Grid is asked for exactly the conditions the rules return.
#[hegel::test(test_cases = 500)]
fn ticks_on_a_whole_list_follow_a_model_of_ticked_values(tc: TestCase) {
    let mut grid = grid();
    let values = values();
    let shown = whole(&values);
    let mut column = Column::of(&[], Axis::Rows, PORT);
    let mut model = vec![true; values.len()];
    let steps = tc.draw(gs::integers::<usize>().max_value(12));
    for _ in 0..steps {
        let on = tc.draw(gs::booleans());
        match tc.draw(gs::integers::<usize>().max_value(values.len())) {
            i if i == values.len() => {
                column = column.tick_head(&mut grid, on, &shown);
                model.fill(on);
            }
            i => {
                column = column.tick(&mut grid, &values[i], on, &shown);
                model[i] = on;
            }
        }
        let ticked: Vec<bool> = values.iter().map(|v| column.ticked(v)).collect();
        assert_eq!(ticked, model);
        assert_eq!(column.head_ticked(&shown), model.iter().all(|t| *t));
        assert_eq!(requested(&mut grid), column.all);
        // Everything ticked filters nothing, so nothing is asked for.
        if model.iter().all(|t| *t) {
            assert_eq!(column.all, vec![]);
        }
    }
}

#[test]
fn ticks_read_each_value_as_ticked_and_excluded_do() {
    let values = values();
    let conditions = [
        Condition::text(PORT, TextQuery::new("E")),
        Condition::one_of(PORT, vec![values[1].clone()]).not(true),
    ];
    let column = Column::of(&conditions, Axis::Rows, PORT);
    let each: Vec<Tick> = (values.iter())
        .map(|v| Tick {
            ticked: column.ticked(v),
            excluded: column.excluded(v),
        })
        .collect();
    assert_eq!(column.ticks(&values), each);
    // Aberdeen is kept; Bergen contains "e" but is unticked; Cádiz and the
    // blank don't contain it.
    let ticked: Vec<(bool, bool)> = each.iter().map(|t| (t.ticked, t.excluded)).collect();
    assert_eq!(
        ticked,
        [(true, false), (false, false), (false, true), (false, true)]
    );
}

#[test]
fn the_comparison_row_reads_back_what_it_writes() {
    let between = compare::comparison(
        compare::BETWEEN,
        Compared::Dates,
        "2024-04-01",
        "2024-06-30",
    )
    .expect("two dates");
    assert_eq!(
        between,
        Comparison::Between(Scalar::date(2024, 4, 1), Scalar::date(2024, 6, 30))
    );
    assert_eq!(
        compare::fields(&between),
        (compare::BETWEEN, "2024-04-01".into(), "2024-06-30".into())
    );
    assert_eq!(compare::comparison(5, Compared::Numbers, "x", ""), None);
    assert_eq!(compare::comparison(0, Compared::Numbers, "1", ""), None);
}

#[test]
fn chips_name_the_condition_on_their_column() {
    let not_bergen =
        Condition::one_of(PORT, vec![Value::of(Some("Bergen")), Value::Blank]).not(true);
    assert_eq!(chip_text("port", &not_bergen), "port: not Bergen, (Blanks)");
    let over = Condition::compare(Key(2), Comparison::Greater(Scalar::Number(100.0)));
    assert_eq!(chip_text("weight", &over), "weight: > 100");
    let pattern = Condition::text(PORT, TextQuery::new("^B").regex(true));
    assert_eq!(chip_text("port", &pattern), "port: /^B/");
    let cased = Condition::text(PORT, TextQuery::new("Ber").case_sensitive(true)).not(true);
    assert_eq!(chip_text("port", &cased), "port: not Ber Aa");
    let native = Condition::native(PORT, "port ~ 'B%'");
    assert_eq!(chip_text("port", &native), "port: ‹port ~ 'B%'›");
}
