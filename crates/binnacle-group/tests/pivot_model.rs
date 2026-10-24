//! The grouping layer on a pivot, driven directly, sans-IO, against a naive
//! model that groups the whole table both ways in memory: rows and columns
//! from the data, the values pseudo-level at any depth of either axis,
//! Subtotals and Grand Totals placed anywhere, either form, and every
//! cell's figure. A fake backend answers each batch out of order, some in
//! parts and some late.

use binnacle_core::{
    AggregateValue, Axis, ChangeNotice, Depth, Expand, Form, Grouping, ItemKind, Key, Level,
    Located, OpenState, Outline, Placement, VALUES_FIELD, Value, ViewSpec,
};
use binnacle_group::{Answer, Batch, Body, GroupRow, GroupingLayer, Path, Want};
use hegel::TestCase;
use hegel::generators as gs;

const FIELDS: [Key; 3] = [Key(10), Key(11), Key(12)];
const AMOUNT: Key = Key(13);

/// A row: its value in each field, and its amount.
type Row = ([Value; 3], u64);

#[hegel::composite]
fn value(tc: &TestCase) -> Value {
    let text = tc.draw(gs::sampled_from(vec!["a", "b", "c", ""]));
    match text {
        "" => Value::Blank,
        t => Value::Text(t.into()),
    }
}

#[hegel::composite]
fn rows(tc: &TestCase) -> Vec<Row> {
    let values = tc.draw(gs::vecs(hegel::tuples!(value(), value(), value())).max_size(60));
    let amounts = tc.draw(
        gs::vecs(gs::integers::<u64>().max_value(9))
            .min_size(values.len())
            .max_size(values.len()),
    );
    (values.into_iter().zip(amounts))
        .map(|((a, b, c), n)| ([a, b, c], n))
        .collect()
}

fn all_values() -> Vec<AggregateValue> {
    vec![AggregateValue::count(), AggregateValue::of(AMOUNT, "sum")]
}

/// The fake backend's aggregates of `rows`, one per value of `values`.
fn aggregates(rows: &[&Row], values: &[AggregateValue]) -> Vec<u64> {
    (values.iter())
        .map(|v| match v.aggregate.as_str() {
            "count" => rows.len() as u64,
            _ => rows.iter().map(|r| r.1).sum(),
        })
        .collect()
}

#[hegel::composite]
fn placement(tc: &TestCase) -> Placement {
    tc.draw(gs::sampled_from(vec![
        Placement::Before,
        Placement::After,
        Placement::Off,
    ]))
}

/// A pivot: each field a row level, a column level or neither, in any
/// order; one value or two; the values pseudo-level anywhere on either axis
/// (or left out, which the layer puts innermost on the columns when there
/// are two values); placements and forms of every kind; rows open to any
/// depth (or all), columns all open.
#[hegel::composite]
fn pivot(tc: &TestCase) -> Grouping {
    let order = tc.draw(gs::permutations(vec![0usize, 1, 2]));
    let mut grouping = Grouping::default();
    for f in order {
        let level = Level::Field {
            field: FIELDS[f],
            subtotal: tc.draw(placement()),
        };
        match tc.draw(gs::integers::<u8>().max_value(2)) {
            0 => grouping.row_levels.push(level),
            1 => grouping.col_levels.push(level),
            _ => {}
        }
    }
    grouping.values = match tc.draw(gs::booleans()) {
        true => all_values(),
        false => vec![all_values()[tc.draw(gs::integers::<usize>().max_value(1))].clone()],
    };
    if tc.draw(gs::booleans()) {
        let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
        let levels = grouping.levels_mut(axis);
        let at = tc.draw(gs::integers::<usize>().max_value(levels.len()));
        levels.insert(at, Level::Values);
    }
    if !grouping.pivot() {
        grouping.col_levels.push(Level::Values);
    }
    for axis in Axis::ALL {
        let g = grouping.axis_mut(axis);
        g.grand_total = tc.draw(placement());
        g.form = match tc.draw(gs::booleans()) {
            true => Form::Header,
            false => Form::Outline,
        };
    }
    let depth = tc.draw(gs::integers::<u32>().max_value(4));
    grouping.rows.expand = Expand::to(match depth {
        4 => Depth::All,
        n => Depth::Levels(n),
    });
    grouping
}

/// An item as the model and the test describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Group(Path),
    Subtotal(Path),
    /// The Subtotal of a group above the values pseudo-level for one
    /// value: the group's path, then the value's step.
    ValueSubtotal(Path),
    GrandTotal,
    ValueTotal(String),
}

#[derive(Clone, Debug, PartialEq)]
struct Described {
    item: Item,
    kind: ItemKind,
    depth: u32,
    open: OpenState,
    ancestors: Vec<Path>,
    group: Option<(Key, Value)>,
}

fn prefixes(path: &Path) -> Vec<Path> {
    (1..=path.len()).map(|d| path[..d].to_vec()).collect()
}

/// The field a path step names, and whether a row is under it.
fn fits(row: &Row, path: &Path) -> bool {
    path.iter()
        .all(|(f, v)| match FIELDS.iter().position(|x| x == f) {
            Some(i) => row.0[i] == *v,
            None => true,
        })
}

/// The values under `step` among `rows`: a field's in order of first row,
/// or each value's identity.
fn steps_under(rows: &[&Row], step: Option<Key>, values: &[String]) -> Vec<(Key, Value)> {
    match step {
        None => (values.iter())
            .map(|v| (VALUES_FIELD, Value::Text(v.clone())))
            .collect(),
        Some(f) => {
            let i = FIELDS.iter().position(|x| *x == f).unwrap();
            let mut order: Vec<(Key, Value)> = Vec::new();
            for row in rows {
                if !order.iter().any(|(_, v)| *v == row.0[i]) {
                    order.push((f, row.0[i].clone()));
                }
            }
            order
        }
    }
}

/// The naive model of one axis of the pivot `g`: every group in order of
/// its first row (each value in turn at the values pseudo-level), an open
/// group's Group Item (in outline form) and Subtotals around what is under
/// it, a closed one alone (in header form, its Subtotal), the innermost
/// groups items that can't open, and the Grand Total (one per value where
/// the values are on the axis) where it goes. A group above the values
/// pseudo-level has one Subtotal per value, and closed shows them after its
/// Group Item in outline form too. The groups at `except` are in the
/// opposite state to the one their level's depth gives them.
fn model(rows: &[Row], g: &Grouping, axis: Axis, except: &[Path]) -> Vec<Described> {
    let steps: Vec<Option<Key>> = g.levels(axis).iter().map(Level::field_key).collect();
    let subtotals: Vec<Placement> = (g.levels(axis).iter())
        .map(|l| match l {
            Level::Field { subtotal, .. } => *subtotal,
            _ => Placement::Off,
        })
        .collect();
    let values: Vec<String> = g.values.iter().map(|v| v.identity()).collect();
    let ax = g.axis(axis);
    let header = ax.form == Form::Header;
    #[allow(clippy::too_many_arguments)]
    fn walk(
        rows: &[&Row],
        path: Path,
        steps: &[Option<Key>],
        subtotals: &[Placement],
        values: &[String],
        header: bool,
        expand: &Expand,
        except: &[Path],
        out: &mut Vec<Described>,
    ) {
        let depth = path.len();
        let Some(&step) = steps.get(depth) else {
            return;
        };
        for (field, value) in steps_under(rows, step, values) {
            let mut child = path.clone();
            child.push((field, value.clone()));
            let under: Vec<&Row> = rows.iter().copied().filter(|r| fits(r, &child)).collect();
            let children = match steps.get(depth + 1) {
                None => 0,
                Some(&next) => steps_under(&under, next, values).len(),
            };
            let open = children > 0 && expand.opens_level(depth as u32) != except.contains(&child);
            let group = Some((field, value.clone()));
            let split = !values.is_empty() && steps[depth + 1..].contains(&None);
            let subtotal = |item, group| Described {
                item,
                kind: ItemKind::Subtotal,
                depth: depth as u32 + 1,
                open: OpenState::CantOpen,
                ancestors: prefixes(&child),
                group,
            };
            let totals: Vec<Described> = match split {
                false => vec![subtotal(Item::Subtotal(child.clone()), group.clone())],
                true => (values.iter())
                    .map(|v| {
                        let step = (VALUES_FIELD, Value::Text(v.clone()));
                        let mut path = child.clone();
                        path.push(step.clone());
                        subtotal(Item::ValueSubtotal(path), Some(step))
                    })
                    .collect(),
            };
            if header && children > 0 && !open {
                out.extend(totals);
                continue;
            }
            if !header || children == 0 {
                out.push(Described {
                    item: Item::Group(child.clone()),
                    kind: ItemKind::Group,
                    depth: depth as u32,
                    open: match (children, open) {
                        (0, _) => OpenState::CantOpen,
                        (_, true) => OpenState::Open,
                        (_, false) => OpenState::Closed,
                    },
                    ancestors: prefixes(&path),
                    group,
                });
            }
            if !open {
                if split && children > 0 {
                    out.extend(totals);
                }
                continue;
            }
            if subtotals[depth] == Placement::Before {
                out.extend(totals.clone());
            }
            walk(
                &under, child, steps, subtotals, values, header, expand, except, out,
            );
            if subtotals[depth] == Placement::After {
                out.extend(totals);
            }
        }
    }
    let all: Vec<&Row> = rows.iter().collect();
    let mut out = Vec::new();
    walk(
        &all,
        Vec::new(),
        &steps,
        &subtotals,
        &values,
        header,
        &ax.expand,
        except,
        &mut out,
    );
    let placement = match steps.as_slice() {
        [] => Placement::Before,
        [None] => Placement::Off,
        _ => ax.grand_total,
    };
    let totals: Vec<Described> = match steps.contains(&None) {
        true => (values.iter())
            .map(|v| Described {
                item: Item::ValueTotal(v.clone()),
                kind: ItemKind::GrandTotal,
                depth: 0,
                open: OpenState::CantOpen,
                ancestors: Vec::new(),
                group: Some((VALUES_FIELD, Value::Text(v.clone()))),
            })
            .collect(),
        false => vec![Described {
            item: Item::GrandTotal,
            kind: ItemKind::GrandTotal,
            depth: 0,
            open: OpenState::CantOpen,
            ancestors: Vec::new(),
            group: None,
        }],
    };
    match placement {
        Placement::Before => totals.into_iter().chain(out).collect(),
        Placement::After => out.into_iter().chain(totals).collect(),
        Placement::Off => out,
    }
}

/// The path of the item, or none for a Grand Total, and its value if it
/// shows one alone.
fn item_path(item: &Item) -> (Path, Option<String>) {
    let value_of = |p: &Path| {
        p.iter().find_map(|(f, v)| match (f, v) {
            (&VALUES_FIELD, Value::Text(t)) => Some(t.clone()),
            _ => None,
        })
    };
    match item {
        Item::Group(p) | Item::Subtotal(p) | Item::ValueSubtotal(p) => (p.clone(), value_of(p)),
        Item::GrandTotal => (Vec::new(), None),
        Item::ValueTotal(v) => (Vec::new(), Some(v.clone())),
    }
}

/// The model's figure for the cell of `row` and `col`: the value either
/// shows aggregated over the rows under both paths. Where neither shows
/// one, the only value, unless an axis splits the values: a group above
/// them has no figure of its own. A cell under no rows has none, unless a
/// Grand Total heads either axis.
fn cell(rows: &[Row], g: &Grouping, row: &Item, col: &Item) -> Option<u64> {
    let (rp, rv) = item_path(row);
    let (cp, cv) = item_path(col);
    let split = g.splits_values(Axis::Rows) || g.splits_values(Axis::Cols);
    let index = match rv.or(cv) {
        Some(v) => g.value_of(&v).unwrap().0,
        None if split => return None,
        None => 0,
    };
    let under: Vec<&Row> = (rows.iter())
        .filter(|r| fits(r, &rp) && fits(r, &cp))
        .collect();
    let data = |p: &Path| p.iter().any(|(f, _)| *f != VALUES_FIELD);
    if under.is_empty() && data(&rp) && data(&cp) {
        return None;
    }
    Some(aggregates(&under, &g.values)[index])
}

/// A fake backend over `rows`, which answers in any order and in parts,
/// and holds some answers back for a later batch.
struct Fake {
    rows: Vec<Row>,
    late: Vec<Answer<u64>>,
    /// The column groups it gave before an edit: it answers their figures
    /// still, as of no rows where none are left.
    columns: Vec<Path>,
}

impl Fake {
    /// The groups of the next of `fields` under `path`, in order of first
    /// row, each with its children (none past the innermost), aggregates
    /// and, for row groups, its cells under every column group.
    fn groups(
        &self,
        batch: &Batch,
        path: &Path,
        fields: &[Key],
        cells: bool,
    ) -> Vec<GroupRow<u64>> {
        let values = &batch.scope.values;
        let under: Vec<&Row> = self.rows.iter().filter(|r| fits(r, path)).collect();
        let Some(&field) = fields.get(path.len()) else {
            return Vec::new();
        };
        steps_under(&under, Some(field), &[])
            .into_iter()
            .map(|step| {
                let mut child = path.clone();
                child.push(step.clone());
                let rows: Vec<&Row> = under.iter().copied().filter(|r| fits(r, &child)).collect();
                let children = match fields.get(child.len()) {
                    Some(&next) => steps_under(&rows, Some(next), &[]).len() as u64,
                    None => rows.len() as u64,
                };
                let mut row = GroupRow::new(step.1, children, aggregates(&rows, values));
                if cells {
                    row.cells = column_paths(&rows, &batch.scope.columns)
                        .into_iter()
                        .map(|c| {
                            let under: Vec<&Row> =
                                rows.iter().copied().filter(|r| fits(r, &c)).collect();
                            (c, aggregates(&under, values))
                        })
                        .collect();
                }
                row
            })
            .collect()
    }

    fn answer(&mut self, tc: &TestCase, batch: &Batch) -> Vec<Answer<u64>> {
        let scope = &batch.scope;
        let mut answers = Vec::new();
        for request in &batch.requests {
            let body = match request.want {
                Want::Groups | Want::Columns => {
                    let columns = request.want == Want::Columns;
                    let fields = match columns {
                        true => &scope.columns,
                        false => &scope.levels,
                    };
                    let all = self.groups(batch, &request.path, fields, !columns);
                    let total = all.len() as u64;
                    let (s, e) = (request.range.start as usize, request.range.end as usize);
                    let e = e.min(all.len());
                    let groups = all[s.min(e)..e].to_vec();
                    match columns {
                        true => Body::Columns { total, groups },
                        false => Body::Groups { total, groups },
                    }
                }
                Want::Total => {
                    let under: Vec<&Row> = self
                        .rows
                        .iter()
                        .filter(|r| fits(r, &request.path))
                        .collect();
                    let mut columns = column_paths(&under, &scope.columns);
                    if request.path.is_empty() {
                        let gone = self.columns.iter().filter(|c| !columns.contains(c));
                        columns.extend(gone.cloned().collect::<Vec<_>>());
                    }
                    let cells = (columns.into_iter())
                        .map(|c| {
                            let rows: Vec<&Row> =
                                under.iter().copied().filter(|r| fits(r, &c)).collect();
                            (c, aggregates(&rows, &scope.values))
                        })
                        .collect();
                    Body::Total {
                        aggregates: aggregates(&under, &scope.values),
                        cells,
                    }
                }
                Want::Leaves | Want::Locate(_) => panic!("a pivot asks for no Leaves"),
            };
            answers.push(Answer {
                stamp: batch.stamp,
                id: Some(request.id),
                path: request.path.clone(),
                start: request.range.start,
                body,
            });
        }
        let order = tc.draw(gs::permutations((0..answers.len()).collect::<Vec<_>>()));
        let mut answers: Vec<Option<Answer<u64>>> = answers.into_iter().map(Some).collect();
        let mut out = std::mem::take(&mut self.late);
        for i in order {
            let answer = answers[i].take().unwrap();
            match tc.draw(gs::weighted_booleans(0.2)) {
                true => self.late.push(answer),
                false => out.push(answer),
            }
        }
        out
    }
}

/// Every column group's path among `rows`, at every column level.
fn column_paths(rows: &[&Row], columns: &[Key]) -> Vec<Path> {
    let mut paths: Vec<Path> = Vec::new();
    for row in rows {
        let mut path = Vec::new();
        for &f in columns {
            let i = FIELDS.iter().position(|x| *x == f).unwrap();
            path.push((f, row.0[i].clone()));
            if !paths.contains(&path) {
                paths.push(path.clone());
            }
        }
    }
    paths
}

fn drive(tc: &TestCase, layer: &mut GroupingLayer<u64>, fake: &mut Fake) {
    for _ in 0..10_000 {
        let batch = layer.poll_batch();
        if batch.is_none() && fake.late.is_empty() {
            return;
        }
        let answers = match &batch {
            Some(batch) => fake.answer(tc, batch),
            None => std::mem::take(&mut fake.late),
        };
        for answer in answers {
            layer.answer(answer);
        }
    }
    panic!("the layer never settled");
}

/// The layer's item as the model describes it, its paths found through
/// the layer's summaries.
fn describe(layer: &GroupingLayer<u64>, g: &Grouping, key: Key, outline: &Outline) -> Described {
    let path = |k: Key| layer.summary(k).map(|s| s.path.clone()).unwrap();
    let item = match (outline.kind, &outline.group) {
        (ItemKind::Group, _) => Item::Group(path(key)),
        // A value's Subtotal sits a level above its path's end.
        (ItemKind::Subtotal, _) if path(key).len() == outline.depth as usize + 1 => {
            Item::ValueSubtotal(path(key))
        }
        (ItemKind::Subtotal, _) => Item::Subtotal(path(key)),
        (ItemKind::GrandTotal, Some(group)) => {
            let Value::Text(id) = &group.value else {
                panic!("a value's Grand Total names it")
            };
            assert!(g.value_of(id).is_some());
            Item::ValueTotal(id.clone())
        }
        (ItemKind::GrandTotal, None) => Item::GrandTotal,
        (ItemKind::Leaf, _) => panic!("a pivot shows no Leaves"),
    };
    Described {
        item,
        kind: outline.kind,
        depth: outline.depth,
        open: outline.open,
        ancestors: outline.ancestors.iter().map(|&k| path(k)).collect(),
        group: outline.group.as_ref().map(|g| (g.field, g.value.clone())),
    }
}

/// The grouping as the layer applies it: with two values and no values
/// pseudo-level, the values go innermost on the columns.
fn applied(mut g: Grouping) -> Grouping {
    if g.values.len() > 1 && !g.splits_values(Axis::Rows) && !g.splits_values(Axis::Cols) {
        g.col_levels.push(Level::Values);
    }
    g
}

/// A pivot of drawn rows, settled: the rows, the grouping as applied, the
/// layer, and the Change Notices its last View Spec came with.
struct Settled {
    rows: Vec<Row>,
    grouping: Grouping,
    layer: GroupingLayer<u64>,
    notices: Vec<(Axis, ChangeNotice)>,
}

fn settle(tc: &TestCase) -> Settled {
    let grouping = tc.draw(pivot());
    settle_with(tc, grouping)
}

fn settle_with(tc: &TestCase, grouping: Grouping) -> Settled {
    let rows = tc.draw(rows());
    let mut fake = Fake {
        rows: rows.clone(),
        late: Vec::new(),
        columns: Vec::new(),
    };
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(ViewSpec::default().with_grouping(Some(grouping.clone())));
    drive(tc, &mut layer, &mut fake);
    let grouping = applied(grouping);
    let mut last = None;
    while let Some(v) = layer.poll_viewed() {
        last = Some(v);
    }
    let last = last.expect("the layer settles");
    assert_eq!(last.applied.grouping.as_ref(), Some(&grouping));
    Settled {
        rows,
        grouping,
        layer,
        notices: last.notices,
    }
}

/// Each axis's items as the model describes them, with their Keys.
fn items(s: &mut Settled) -> [Vec<(Key, Described)>; 2] {
    Axis::ALL.map(|axis| {
        let n = s.layer.count(axis).unwrap();
        let got = s.layer.window(axis, 0..n);
        assert_eq!(
            got.len() as u64,
            n,
            "a pivot's groups are all known once settled"
        );
        (got.iter())
            .map(|(k, o)| (*k, describe(&s.layer, &s.grouping, *k, o)))
            .collect()
    })
}

/// Each of `items` on `axis` is found where it sits.
fn assert_located(layer: &mut GroupingLayer<u64>, axis: Axis, items: &[(Key, Described)]) {
    for (at, (key, d)) in items.iter().enumerate() {
        assert_eq!(
            layer.locate(axis, *key),
            Some(Located::Found(at as u64)),
            "{axis:?} {:?}",
            d.item
        );
    }
}

#[hegel::test(test_cases = 300)]
fn a_pivot_shows_the_items_a_whole_table_pivot_shows(tc: TestCase) {
    let mut s = settle(&tc);
    for (axis, got) in Axis::ALL.into_iter().zip(items(&mut s)) {
        let got: Vec<Described> = got.into_iter().map(|(_, d)| d).collect();
        assert_eq!(got, model(&s.rows, &s.grouping, axis, &[]), "{axis:?}");
    }
}

/// Rows closed above the values pseudo-level show a Subtotal of each value,
/// after the Group Item in outline form, and no figure on the Group Item.
#[hegel::test(test_cases = 200)]
fn rows_closed_above_the_values_show_a_subtotal_of_each(tc: TestCase) {
    let mut grouping = tc.draw(pivot());
    for axis in Axis::ALL {
        grouping.levels_mut(axis).retain(|l| *l != Level::Values);
    }
    tc.assume(!grouping.row_levels.is_empty());
    let at = tc.draw(
        gs::integers::<usize>()
            .min_value(1)
            .max_value(grouping.row_levels.len()),
    );
    grouping.row_levels.insert(at, Level::Values);
    let depth = tc.draw(gs::integers::<u32>().max_value(at as u32 - 1));
    grouping.rows.expand = Expand::to(Depth::Levels(depth));
    let mut s = settle_with(&tc, grouping);
    let [rows, cols] = items(&mut s);
    let got: Vec<Described> = rows.iter().map(|(_, d)| d.clone()).collect();
    assert_eq!(got, model(&s.rows, &s.grouping, Axis::Rows, &[]));
    for (rk, r) in &rows {
        for (ck, c) in &cols {
            let want = cell(&s.rows, &s.grouping, &r.item, &c.item);
            assert_eq!(s.layer.cell(*rk, *ck).copied(), want);
        }
    }
}

#[hegel::test(test_cases = 200)]
fn each_item_of_a_pivot_is_found_where_it_sits(tc: TestCase) {
    let mut s = settle(&tc);
    for (axis, got) in Axis::ALL.into_iter().zip(items(&mut s)) {
        assert_located(&mut s.layer, axis, &got);
    }
}

#[hegel::test(test_cases = 100)]
fn a_pivot_refilters_both_axes(tc: TestCase) {
    let s = settle(&tc);
    let want = Axis::ALL.map(|axis| model(&s.rows, &s.grouping, axis, &[]).len() as u64);
    assert_eq!(
        s.notices,
        vec![
            (Axis::Rows, ChangeNotice::Refiltered(want[0])),
            (Axis::Cols, ChangeNotice::Refiltered(want[1])),
        ],
        "both axes come from the data, each refiltered"
    );
}

#[hegel::test(test_cases = 300)]
fn each_cell_of_a_pivot_is_the_figure_where_its_row_and_column_cross(tc: TestCase) {
    let mut s = settle(&tc);
    let [rows, cols] = items(&mut s);
    for (rk, r) in &rows {
        for (ck, c) in &cols {
            assert_eq!(
                s.layer.cell(*rk, *ck).copied(),
                cell(&s.rows, &s.grouping, &r.item, &c.item),
                "the cell of {:?} and {:?}",
                r.item,
                c.item
            );
        }
    }
}

#[hegel::test(test_cases = 100)]
fn a_pivot_column_keeps_its_key_while_its_value_leaves_and_returns(tc: TestCase) {
    let rows = tc.draw(rows());
    let grouping = tc.draw(pivot());
    let mut fake = Fake {
        rows: rows.clone(),
        late: Vec::new(),
        columns: Vec::new(),
    };
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(ViewSpec::default().with_grouping(Some(grouping)));
    let columns = |layer: &mut GroupingLayer<u64>, fake: &mut Fake| {
        drive(&tc, layer, fake);
        while layer.poll_viewed().is_some() {}
        let n = layer.count(Axis::Cols).unwrap();
        layer.window(Axis::Cols, 0..n)
    };
    let before = columns(&mut layer, &mut fake);
    // Some rows leave the data and come back.
    let keep = tc.draw(
        gs::vecs(gs::booleans())
            .min_size(rows.len())
            .max_size(rows.len()),
    );
    fake.rows = (rows.iter().zip(&keep))
        .filter(|(_, k)| **k)
        .map(|(r, _)| r.clone())
        .collect();
    layer.refresh();
    let during = columns(&mut layer, &mut fake);
    for (key, outline) in &during {
        if let Some((_, was)) = before.iter().find(|(k, _)| k == key) {
            assert_eq!(was.group, outline.group, "one Key, one column");
        }
    }
    fake.rows = rows;
    layer.refresh();
    let after = columns(&mut layer, &mut fake);
    assert_eq!(after, before, "every column back under its Key");
}

/// The column groups the user can toggle: the Group Items that can open
/// and the groups whose spans hold columns, with their paths.
fn column_groups(layer: &mut GroupingLayer<u64>) -> Vec<(Key, Path)> {
    let n = layer.count(Axis::Cols).unwrap();
    let mut keys: Vec<Key> = Vec::new();
    for (key, outline) in layer.window(Axis::Cols, 0..n) {
        if outline.kind == ItemKind::Group && outline.open != OpenState::CantOpen {
            keys.push(key);
        }
        keys.extend(outline.ancestors);
    }
    keys.sort();
    keys.dedup();
    (keys.into_iter())
        .map(|k| (k, layer.summary(k).unwrap().path.clone()))
        .collect()
}

/// Opening and closing column groups, one at a time or to a depth, shows
/// the columns the model shows, and the notices take the columns shown
/// before to those shown after: `Collapsed` or `Expanded` when one group
/// the user sees opens or closes, `Refiltered` only for other changes. The
/// rows stay as they are.
#[hegel::test(test_cases = 300)]
fn opening_and_closing_column_groups_says_where_with_collapsed_or_expanded(tc: TestCase) {
    let mut s = settle(&tc);
    let mut fake = Fake {
        rows: s.rows.clone(),
        late: Vec::new(),
        columns: Vec::new(),
    };
    let levels = s.grouping.col_levels.len() as u32;
    let mut except: Vec<Path> = Vec::new();
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(6));
    for _ in 0..steps {
        let known = column_groups(&mut s.layer);
        let [rows_before, cols_before] = items(&mut s);
        let before: Vec<Item> = cols_before.iter().map(|(_, d)| d.item.clone()).collect();
        let expand = &mut s.grouping.cols.expand;
        let mut toggled = None;
        match known.is_empty() || tc.draw(gs::weighted_booleans(0.25)) {
            true => {
                let n = tc.draw(gs::integers::<u32>().max_value(levels + 1));
                *expand = Expand::to(match n > levels {
                    true => Depth::All,
                    false => Depth::Levels(n),
                });
                except.clear();
            }
            false => {
                let (key, path) = tc.draw(gs::sampled_from(known));
                let level = path.len() as u32 - 1;
                let open = expand.opens(level, key);
                *expand = expand.clone().with(level, key, !open);
                match except.iter().position(|p| *p == path) {
                    Some(i) => drop(except.remove(i)),
                    None => except.push(path.clone()),
                }
                toggled = Some(path);
            }
        }
        let spec = ViewSpec::default().with_grouping(Some(s.grouping.clone()));
        s.layer.request_view(spec);
        drive(&tc, &mut s.layer, &mut fake);
        let mut notices = Vec::new();
        while let Some(v) = s.layer.poll_viewed() {
            notices.extend(v.notices);
        }
        let got = items(&mut s);
        assert_eq!(got[0], rows_before, "the rows stay as they are");
        assert_located(&mut s.layer, Axis::Cols, &got[1]);
        let after: Vec<Item> = got[1].iter().map(|(_, d)| d.item.clone()).collect();
        let want = model(&s.rows, &s.grouping, Axis::Cols, &except);
        let described: Vec<Described> = got[1].iter().map(|(_, d)| d.clone()).collect();
        assert_eq!(described, want);
        // Where a closed group shows Subtotals (in header form, or above
        // the values pseudo-level), they stand in for what it hid only if
        // placed while open too.
        let header = s.grouping.cols.form == Form::Header;
        let seen = toggled.is_some_and(|p| {
            let levels = &s.grouping.col_levels;
            let stands_in = header || levels[p.len()..].contains(&Level::Values);
            match stands_in {
                false => before.contains(&Item::Group(p)),
                true => {
                    let placed = match &levels[p.len() - 1] {
                        Level::Field { subtotal, .. } => *subtotal != Placement::Off,
                        _ => false,
                    };
                    placed && before.iter().any(|i| item_path(i).0.starts_with(&p))
                }
            }
        });
        assert!(notices.iter().all(|(a, _)| *a == Axis::Cols), "{notices:?}");
        let notices: Vec<ChangeNotice> = notices.into_iter().map(|(_, n)| n).collect();
        match notices.as_slice() {
            [] => assert_eq!(before, after, "no notice, nothing moved"),
            [ChangeNotice::Collapsed { at, n }] => {
                let mut was = before.clone();
                was.drain(*at as usize..(at + n) as usize);
                assert_eq!(was, after);
            }
            [ChangeNotice::Expanded { at, n }] => {
                let mut now = after.clone();
                now.drain(*at as usize..(at + n) as usize);
                assert_eq!(before, now);
            }
            [ChangeNotice::Refiltered(n)] if !seen => assert_eq!(*n, after.len() as u64),
            other => panic!("unexpected notices {other:?}"),
        }
    }
}

/// Edits to the rows under a settled pivot, each refiguring it, move no item
/// on either axis: every figure, each cell included, is pending until the
/// backend answers, then each cell is the figure where its row and column
/// cross over the rows as they are now, each row whose cells changed drawn
/// anew.
#[hegel::test(test_cases = 300)]
fn an_edit_refigures_every_cell_of_a_pivot_in_place(tc: TestCase) {
    let mut s = settle(&tc);
    tc.assume(!s.rows.is_empty());
    let fields: Vec<Key> = (s.grouping.col_levels.iter())
        .filter_map(|l| l.field_key())
        .collect();
    let mut fake = Fake {
        rows: s.rows.clone(),
        late: Vec::new(),
        columns: column_paths(&s.rows.iter().collect::<Vec<_>>(), &fields),
    };
    let before = items(&mut s);
    // Each row's cells, as the layer has them.
    let figures = |s: &Settled| -> Vec<Vec<Option<u64>>> {
        (before[0].iter())
            .map(|(rk, _)| {
                (before[1].iter())
                    .map(|(ck, _)| s.layer.cell(*rk, *ck).copied())
                    .collect()
            })
            .collect()
    };
    let was = figures(&s);

    let mut redrawn = Vec::new();
    for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(3)) {
        let row = tc.draw(gs::integers::<usize>().max_value(s.rows.len() - 1));
        match tc.draw(gs::integers::<usize>().max_value(3)) {
            3 => s.rows[row].1 = tc.draw(gs::integers::<u64>().max_value(9)),
            f => s.rows[row].0[f] = tc.draw(value()),
        }
        fake.rows = s.rows.clone();
        s.layer.refigure();
        while let Some((axis, notice)) = s.layer.poll_change() {
            assert_eq!(axis, Axis::Rows);
            let ChangeNotice::ContentChanged(range) = notice else {
                panic!("only figures change: {notice:?}");
            };
            redrawn.push(range);
        }
        for (rk, r) in &before[0] {
            for (ck, c) in &before[1] {
                assert_eq!(
                    s.layer.cell(*rk, *ck),
                    None,
                    "the cell of {:?} and {:?} is pending",
                    r.item,
                    c.item
                );
            }
        }
        for (key, d) in before.iter().flatten() {
            let summary = s.layer.summary(*key).unwrap();
            assert!(summary.aggregates.is_empty(), "{:?} is pending", d.item);
        }
        // Some answers, or none, before the next edit.
        if tc.draw(gs::booleans())
            && let Some(batch) = s.layer.poll_batch()
        {
            for answer in fake.answer(&tc, &batch) {
                s.layer.answer(answer);
            }
        }
    }

    drive(&tc, &mut s.layer, &mut fake);
    while let Some((_, ChangeNotice::ContentChanged(range))) = s.layer.poll_change() {
        redrawn.push(range);
    }
    let after = items(&mut s);
    assert_eq!(after, before, "nothing moves");
    for (rk, r) in &after[0] {
        for (ck, c) in &after[1] {
            assert_eq!(
                s.layer.cell(*rk, *ck).copied(),
                cell(&s.rows, &s.grouping, &r.item, &c.item),
                "the cell of {:?} and {:?}",
                r.item,
                c.item
            );
        }
    }
    let now = figures(&s);
    for (at, (was, now)) in was.iter().zip(&now).enumerate() {
        let at = at as u64;
        if was != now {
            assert!(
                redrawn.iter().any(|r| r.contains(&at)),
                "the row at {at}, {:?}, is drawn anew",
                after[0][at as usize].1.item
            );
        }
    }
}
