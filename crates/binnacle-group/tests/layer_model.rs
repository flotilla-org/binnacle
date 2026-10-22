//! The grouping layer driven directly, sans-IO, against a naive model that
//! groups the whole table in memory, with Subtotals and the Grand Total
//! placed before, after or nowhere. A fake aggregate backend answers each
//! batch out of order, in parts, short (the rest asked for again), with
//! more than asked, jointly (several requests as one answer), some answers
//! late, and some after a newer View Spec has superseded them.

use std::collections::HashMap;
use std::ops::Range;

use binnacle_core::{
    AggregateValue, Axis, ChangeNotice, Comparison, Condition, Depth, Expand, FilterSpec,
    GRAND_TOTAL, Grouping, ItemKind, Key, Level, Located, OpenState, Outline, Placement, Scalar,
    SortColumn, SortSpec, Value, ViewSpec,
};
use binnacle_group::{
    Answer, Batch, Body, GroupRow, GroupingLayer, Path, Request, SUMMARY_KEY_BIT, Want, having,
};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const FIELDS: [Key; 2] = [Key(10), Key(11)];

/// A row of the table, in the order the backend shows them: its Leaf Key
/// and its value in each field.
type Row = (Key, [Value; 2]);

#[hegel::composite]
fn value(tc: &TestCase) -> Value {
    // Mostly a few values, so groups fill up; any text now and then.
    let text = tc.draw(hegel::one_of!(
        gs::sampled_from(vec!["a", "b", "c", ""]).map(String::from),
        gs::text().max_size(8)
    ));
    match text.as_str() {
        "" => Value::Blank,
        _ => Value::Text(text),
    }
}

#[hegel::composite]
fn rows(tc: &TestCase) -> Vec<Row> {
    let values = tc.draw(gs::vecs(hegel::tuples!(value(), value())).max_size(150));
    // Leaf Keys in any order, never in Position order.
    let mut keys: Vec<u64> = (0..values.len() as u64).map(|k| k * 7 + 3).collect();
    keys.reverse();
    keys.into_iter()
        .zip(values)
        .map(|(k, (a, b))| (Key(k), [a, b]))
        .collect()
}

/// Where each level's Subtotals and the Grand Total go.
#[derive(Clone, Debug, Default, PartialEq)]
struct Shape {
    subtotals: [Placement; 2],
    grand_total: Placement,
}

#[hegel::composite]
fn placement(tc: &TestCase) -> Placement {
    tc.draw(gs::sampled_from(vec![
        Placement::Before,
        Placement::After,
        Placement::Off,
    ]))
}

#[hegel::composite]
fn shape(tc: &TestCase) -> Shape {
    Shape {
        subtotals: [tc.draw(placement()), tc.draw(placement())],
        grand_total: tc.draw(placement()),
    }
}

/// The values every spec asks for: a count, a distinct count of the second
/// field (which no sum of the groups' gives), and an aggregate the backend
/// declares, with parameters it must get as they are.
fn values() -> Vec<AggregateValue> {
    vec![
        AggregateValue::count(),
        AggregateValue::of(FIELDS[1], "distinct count"),
        AggregateValue::of(FIELDS[0], "percentile").with_params(vec!["0.9".into(), "x; y".into()]),
    ]
}

fn spec(levels: usize, shape: &Shape) -> ViewSpec {
    opening(levels, shape, Expand::default())
}

fn opening(levels: usize, shape: &Shape, expand: Expand) -> ViewSpec {
    let mut grouping = Grouping::rows_by(FIELDS[0]);
    grouping.row_levels = (0..levels)
        .map(|d| Level::Field {
            field: FIELDS[d],
            subtotal: shape.subtotals[d],
        })
        .collect();
    grouping.values = values();
    grouping.rows.grand_total = shape.grand_total;
    grouping.rows.expand = expand;
    ViewSpec::default().with_grouping(Some(grouping))
}

/// What the fake backend aggregates `rows` to: their count, the number of
/// distinct values in the second field, and the percentile it declares,
/// which it computes as 7.
fn aggregates(rows: &[&Row]) -> Vec<u64> {
    let mut distinct: Vec<&Value> = rows.iter().map(|r| &r.1[1]).collect();
    distinct.sort_by_key(|v| format!("{v:?}"));
    distinct.dedup();
    vec![rows.len() as u64, distinct.len() as u64, 7]
}

/// An item as the model and the test describe it.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Group(Path),
    Subtotal(Path),
    GrandTotal,
    Leaf(Key),
}

/// An item with its outline, the ancestors as paths, and the aggregates
/// it carries, if a Summary Item.
#[derive(Clone, Debug, PartialEq)]
struct Described {
    item: Item,
    aggregates: Option<Vec<u64>>,
    kind: ItemKind,
    depth: u32,
    open: OpenState,
    ancestors: Vec<Path>,
    group: Option<(Key, Value)>,
    /// How many items a Group Item shows under it.
    span: u64,
    /// The groups of its ancestors, as the outline names them.
    path: Path,
}

fn path_of(row: &Row, depth: usize) -> Path {
    (0..depth).map(|d| (FIELDS[d], row.1[d].clone())).collect()
}

fn prefixes(path: &Path) -> Vec<Path> {
    (1..=path.len()).map(|d| path[..d].to_vec()).collect()
}

/// Which groups are open: those `depth` opens, except the groups at the
/// paths in `except`, which are the other way.
#[derive(Clone, Debug, Default)]
struct Opened {
    depth: Depth,
    except: Vec<Path>,
}

impl Opened {
    fn opens(&self, path: &Path) -> bool {
        let level = path.len() as u32 - 1;
        let default = match self.depth {
            Depth::All => true,
            Depth::Levels(n) => level < n,
        };
        default != self.except.contains(path)
    }
}

/// The naive model: every group in order of its first row, each open
/// Group Item followed by what is under it, with its Subtotal before or
/// after that, a closed one alone, and the Grand Total before or after it
/// all.
fn model(rows: &[Row], levels: usize, shape: &Shape, opened: &Opened) -> Vec<Described> {
    fn walk(
        rows: &[&Row],
        path: Path,
        levels: usize,
        shape: &Shape,
        opened: &Opened,
        out: &mut Vec<Described>,
    ) {
        let depth = path.len();
        if depth == levels {
            for row in rows {
                out.push(Described {
                    item: Item::Leaf(row.0),
                    aggregates: None,
                    kind: ItemKind::Leaf,
                    depth: depth as u32,
                    open: OpenState::CantOpen,
                    ancestors: prefixes(&path),
                    group: None,
                    span: 0,
                    path: path.clone(),
                });
            }
            return;
        }
        let mut order: Vec<Value> = Vec::new();
        for row in rows {
            if !order.contains(&row.1[depth]) {
                order.push(row.1[depth].clone());
            }
        }
        for value in order {
            let mut child = path.clone();
            child.push((FIELDS[depth], value.clone()));
            let under: Vec<&Row> = rows
                .iter()
                .filter(|r| r.1[depth] == value)
                .copied()
                .collect();
            let open = opened.opens(&child);
            out.push(Described {
                item: Item::Group(child.clone()),
                aggregates: Some(aggregates(&under)),
                kind: ItemKind::Group,
                depth: depth as u32,
                open: match open {
                    true => OpenState::Open,
                    false => OpenState::Closed,
                },
                ancestors: prefixes(&path),
                group: Some((FIELDS[depth], value.clone())),
                span: 0,
                path: path.clone(),
            });
            if !open {
                continue;
            }
            let at = out.len() - 1;
            let subtotal = Described {
                item: Item::Subtotal(child.clone()),
                aggregates: Some(aggregates(&under)),
                kind: ItemKind::Subtotal,
                depth: depth as u32 + 1,
                open: OpenState::CantOpen,
                ancestors: prefixes(&child),
                group: Some((FIELDS[depth], value.clone())),
                span: 0,
                path: child.clone(),
            };
            if shape.subtotals[depth] == Placement::Before {
                out.push(subtotal.clone());
            }
            walk(&under, child, levels, shape, opened, out);
            if shape.subtotals[depth] == Placement::After {
                out.push(subtotal);
            }
            out[at].span = (out.len() - at - 1) as u64;
        }
    }
    let all: Vec<&Row> = rows.iter().collect();
    let total = Described {
        item: Item::GrandTotal,
        aggregates: Some(aggregates(&all)),
        kind: ItemKind::GrandTotal,
        depth: 0,
        open: OpenState::CantOpen,
        ancestors: Vec::new(),
        group: None,
        span: 0,
        path: Vec::new(),
    };
    let mut out = Vec::new();
    if shape.grand_total == Placement::Before {
        out.push(total.clone());
    }
    walk(&all, Vec::new(), levels, shape, opened, &mut out);
    if shape.grand_total == Placement::After {
        out.push(total);
    }
    out
}

/// A fake backend over `rows`, worked out naively per request, which
/// mangles how it answers as `tc` draws.
struct Fake {
    rows: Vec<Row>,
    /// Answers held back, delivered at a later batch.
    late: Vec<Answer<u64>>,
    /// The groups whose groups or Leaves the layer asked for.
    asked: Vec<Path>,
}

impl Fake {
    fn new(rows: Vec<Row>) -> Self {
        Fake {
            rows,
            late: Vec::new(),
            asked: Vec::new(),
        }
    }

    fn under(&self, path: &Path) -> Vec<&Row> {
        (self.rows.iter())
            .filter(|r| path.iter().enumerate().all(|(d, (_, v))| r.1[d] == *v))
            .collect()
    }

    fn groups(&self, path: &Path, levels: usize) -> Vec<GroupRow<u64>> {
        let depth = path.len();
        // One pass over the group's rows: each value's rows in order of
        // their first.
        let mut order: Vec<(Value, Vec<&Row>)> = Vec::new();
        let mut at: HashMap<&Value, usize> = HashMap::new();
        for row in self.under(path) {
            let i = *at.entry(&row.1[depth]).or_insert_with(|| {
                order.push((row.1[depth].clone(), Vec::new()));
                order.len() - 1
            });
            order[i].1.push(row);
        }
        order
            .into_iter()
            .map(|(value, rows)| {
                let children = match depth + 1 < levels {
                    true => {
                        let mut next: Vec<&Value> = rows.iter().map(|r| &r.1[depth + 1]).collect();
                        next.sort_by_key(|v| format!("{v:?}"));
                        next.dedup();
                        next.len()
                    }
                    false => rows.len(),
                };
                GroupRow {
                    value,
                    children: children as u64,
                    aggregates: aggregates(&rows),
                }
            })
            .collect()
    }

    /// The whole answer to each request, split in parts as `tc` draws.
    fn answer(&mut self, tc: &TestCase, batch: &Batch) -> Vec<Answer<u64>> {
        assert_eq!(
            batch.scope.values,
            values(),
            "values reach the backend as asked"
        );
        let levels = batch.scope.levels.len();
        let mut answers = Vec::new();
        // Jointly: the requests for one group's groups or Leaves answered
        // as one, whole, from the first's start to the last's end, naming
        // only the first; the others are never named, the answer covering
        // them.
        let mut requests: Vec<(Request, bool)> = Vec::new();
        for request in &batch.requests {
            let joint = requests.iter_mut().find(|(r, _)| {
                r.path == request.path && r.want == request.want && r.want != Want::Total
            });
            match joint {
                Some((r, joint)) if tc.draw(gs::weighted_booleans(0.3)) => {
                    r.range =
                        r.range.start.min(request.range.start)..r.range.end.max(request.range.end);
                    *joint = true;
                }
                _ => requests.push((request.clone(), false)),
            }
        }
        for (request, joint) in &requests {
            if matches!(request.want, Want::Groups | Want::Leaves) {
                self.asked.push(request.path.clone());
            }
            let range = request.range.start as usize..request.range.end as usize;
            let parts: Vec<(u64, Body<u64>)> = match request.want {
                Want::Groups => {
                    let all = self.groups(&request.path, levels);
                    let total = all.len() as u64;
                    let end = range.end.min(all.len());
                    let start = range.start.min(end);
                    split(tc, start, all[start..end].to_vec())
                        .into_iter()
                        .map(|(at, groups)| (at, Body::Groups { total, groups }))
                        .collect()
                }
                Want::Leaves => {
                    let all: Vec<Key> = self.under(&request.path).iter().map(|r| r.0).collect();
                    let end = range.end.min(all.len());
                    let start = range.start.min(end);
                    split(tc, start, all[start..end].to_vec())
                        .into_iter()
                        .map(|(at, keys)| (at, Body::Leaves { keys }))
                        .collect()
                }
                Want::Total => {
                    let aggregates = aggregates(&self.under(&request.path));
                    answers.push(Answer {
                        stamp: batch.stamp,
                        id: Some(request.id),
                        path: request.path.clone(),
                        start: 0,
                        body: Body::Total { aggregates },
                    });
                    continue;
                }
                Want::Locate(key) => {
                    let row = self.rows.iter().find(|r| r.0 == key);
                    let (path, start) = match row {
                        Some(row) => {
                            let path = path_of(row, levels);
                            let at = self.under(&path).iter().position(|r| r.0 == key);
                            (path, at.unwrap_or(0) as u64)
                        }
                        None => (Vec::new(), 0),
                    };
                    let found = row.is_some();
                    answers.push(Answer {
                        stamp: batch.stamp,
                        id: Some(request.id),
                        path,
                        start,
                        body: Body::Located { key, found },
                    });
                    continue;
                }
            };
            let mut parts = parts;
            // A short answer: the first part alone, naming the request, so
            // the layer must ask again for the rest.
            if !joint && parts.len() > 1 && tc.draw(gs::weighted_booleans(0.3)) {
                parts.truncate(1);
            }
            let last = parts.len().saturating_sub(1);
            for (i, (start, body)) in parts.into_iter().enumerate() {
                answers.push(Answer {
                    stamp: batch.stamp,
                    // Every part but the last comes unasked.
                    id: (i == last).then_some(request.id),
                    path: request.path.clone(),
                    start,
                    body,
                });
            }
        }
        // More than asked: a group's whole Leaves, or the top groups.
        if tc.draw(gs::booleans()) {
            let path = match tc.draw(gs::booleans()) {
                true => self
                    .rows
                    .first()
                    .map(|r| path_of(r, levels))
                    .unwrap_or_default(),
                false => Vec::new(),
            };
            let body = match path.len() == levels {
                true => Body::Leaves {
                    keys: self.under(&path).iter().map(|r| r.0).collect(),
                },
                false => {
                    let groups = self.groups(&path, levels);
                    Body::Groups {
                        total: groups.len() as u64,
                        groups,
                    }
                }
            };
            answers.push(Answer {
                stamp: batch.stamp,
                id: None,
                path,
                start: 0,
                body,
            });
        }
        let order = tc.draw(gs::permutations((0..answers.len()).collect::<Vec<_>>()));
        let mut answers: Vec<Option<Answer<u64>>> = answers.into_iter().map(Some).collect();
        let mut out: Vec<Answer<u64>> = std::mem::take(&mut self.late);
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

/// `items` from index `start` in up to three runs.
fn split<T: Clone>(tc: &TestCase, start: usize, items: Vec<T>) -> Vec<(u64, Vec<T>)> {
    if items.len() < 2 || !tc.draw(gs::booleans()) {
        return vec![(start as u64, items)];
    }
    let cut = tc.draw(
        gs::integers::<usize>()
            .min_value(1)
            .max_value(items.len() - 1),
    );
    vec![
        (start as u64, items[..cut].to_vec()),
        ((start + cut) as u64, items[cut..].to_vec()),
    ]
}

/// Runs batches through `fake` until the layer asks for nothing and holds
/// no late answer.
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

/// Where the layer locates `key`, asking the backend as often as it needs
/// to (a few rounds at most).
fn locate(tc: &TestCase, layer: &mut GroupingLayer<u64>, fake: &mut Fake, key: Key) -> Located {
    for _ in 0..20 {
        if let Some(located) = layer.locate(Axis::Rows, key) {
            return located;
        }
        drive(tc, layer, fake);
    }
    panic!("{key:?} was never located");
}

/// The layer's item at a Key and outline, described as the model does.
fn describe(layer: &GroupingLayer<u64>, key: Key, outline: &Outline) -> Described {
    let path = |k: Key| layer.summary(k).map(|s| s.path.clone());
    let summary = layer.summary(key);
    if let Some(s) = summary {
        assert_eq!(s.kind, outline.kind, "the summary's kind is the outline's");
    }
    let item = match outline.kind {
        ItemKind::Leaf => Item::Leaf(key),
        ItemKind::Group => Item::Group(path(key).expect("a Group Item the layer knows")),
        ItemKind::Subtotal => Item::Subtotal(path(key).expect("a Subtotal the layer knows")),
        ItemKind::GrandTotal => Item::GrandTotal,
    };
    Described {
        item,
        aggregates: summary.map(|s| s.aggregates.to_vec()),
        kind: outline.kind,
        depth: outline.depth,
        open: outline.open,
        ancestors: outline
            .ancestors
            .iter()
            .map(|&k| path(k).unwrap())
            .collect(),
        group: outline.group.as_ref().map(|g| (g.field, g.value.clone())),
        span: outline.span,
        path: (outline.path.iter())
            .map(|g| (g.field, g.value.clone()))
            .collect(),
    }
}

#[hegel::test(test_cases = 300)]
fn the_layer_shows_what_a_whole_table_grouping_shows(tc: TestCase) {
    let rows = tc.draw(rows());
    let levels = tc.draw(gs::integers::<usize>().min_value(1).max_value(2));
    let shape = tc.draw(shape());
    let mut fake = Fake::new(rows.clone());
    let mut layer = GroupingLayer::<u64>::new();

    // A spec the backend answers late, superseded before it settles.
    if tc.draw(gs::booleans()) {
        layer.request_view(spec(3 - levels, &shape));
        let stale = layer.poll_batch().unwrap();
        let answers = fake.answer(&tc, &stale);
        fake.late.extend(answers);
    }
    layer.request_view(spec(levels, &shape));
    drive(&tc, &mut layer, &mut fake);

    let want = model(&rows, levels, &shape, &Opened::default());
    let mut viewed = Vec::new();
    while let Some(v) = layer.poll_viewed() {
        viewed.push(v);
    }
    let last = viewed.last().expect("the layer settles");
    assert_eq!(last.applied, spec(levels, &shape));
    assert_eq!(
        last.notices,
        vec![(Axis::Rows, ChangeNotice::Refiltered(want.len() as u64))]
    );
    assert_eq!(layer.count(Axis::Rows), Some(want.len() as u64));
    assert_eq!(layer.count(Axis::Cols), None, "the columns are not grouped");

    // Windows in chunks, each asking for what it lacks.
    let mut got: Vec<(Key, Outline)> = Vec::new();
    let chunk = tc.draw(gs::integers::<u64>().min_value(1).max_value(40));
    while (got.len() as u64) < want.len() as u64 {
        let start = got.len() as u64;
        let mut items = layer.window(Axis::Rows, start..start + chunk);
        // Short answers can take a few rounds.
        for _ in 0..20 {
            if !items.is_empty() {
                break;
            }
            drive(&tc, &mut layer, &mut fake);
            items = layer.window(Axis::Rows, start..start + chunk);
        }
        assert!(!items.is_empty(), "an answered window has items");
        got.extend(items);
    }
    let described: Vec<Described> = got.iter().map(|(k, o)| describe(&layer, *k, o)).collect();
    assert_eq!(described, want);

    // Keys are unique, Summary Item Keys carry the summary bit, and the
    // Grand Total's is the one the Grid pins.
    let mut seen = HashMap::new();
    for (i, (key, outline)) in got.iter().enumerate() {
        assert_eq!(seen.insert(*key, i), None, "Key {key:?} twice");
        assert_eq!(key.0 & SUMMARY_KEY_BIT != 0, outline.kind.is_summary());
        assert_eq!(*key == GRAND_TOTAL, outline.kind == ItemKind::GrandTotal);
        assert_eq!(outline.parent(), outline.ancestors.last().copied());
    }

    // Every Key is located where the window put it.
    for (i, (key, _)) in got.iter().enumerate() {
        let located = locate(&tc, &mut layer, &mut fake, *key);
        assert_eq!(located, Located::Found(i as u64), "{key:?}");
    }
}

#[hegel::test(test_cases = 200)]
fn a_leaf_is_located_before_its_page_is_fetched(tc: TestCase) {
    let rows = tc.draw(rows().filter(|r: &Vec<Row>| !r.is_empty()));
    let mut fake = Fake::new(rows.clone());
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(spec(1, &Shape::default()));
    drive(&tc, &mut layer, &mut fake);
    let want = model(&rows, 1, &Shape::default(), &Opened::default());
    let i = tc.draw(gs::integers::<usize>().max_value(rows.len() - 1));
    let key = rows[i].0;
    let at = want.iter().position(|d| d.item == Item::Leaf(key)).unwrap();
    let located = locate(&tc, &mut layer, &mut fake, key);
    assert_eq!(located, Located::Found(at as u64));
    let gone = locate(&tc, &mut layer, &mut fake, Key(1));
    assert_eq!(gone, Located::Gone { suggest: None }, "no row has Key 1");
}

#[hegel::test(test_cases = 100)]
fn summary_keys_stay_the_same_across_a_refresh_and_a_new_spec(tc: TestCase) {
    let rows = tc.draw(rows());
    let mut fake = Fake::new(rows);
    let mut layer = GroupingLayer::<u64>::new();
    let keys = |layer: &mut GroupingLayer<u64>, fake: &mut Fake| {
        drive(&tc, layer, fake);
        let count = layer.count(Axis::Rows).unwrap();
        let mut got = layer.window(Axis::Rows, 0..count);
        // Each Window fetches one group's Leaves more.
        for _ in 0..1000 {
            if got.len() as u64 == count {
                break;
            }
            drive(&tc, layer, fake);
            got = layer.window(Axis::Rows, 0..count);
        }
        got.into_iter().map(|(k, _)| k).collect::<Vec<_>>()
    };
    layer.request_view(spec(1, &Shape::default()));
    let first = keys(&mut layer, &mut fake);
    layer.refresh();
    assert_eq!(keys(&mut layer, &mut fake), first, "after a refresh");
    layer.request_view(spec(2, &Shape::default()));
    keys(&mut layer, &mut fake);
    layer.request_view(spec(1, &Shape::default()));
    assert_eq!(
        keys(&mut layer, &mut fake),
        first,
        "after grouping by two levels and back"
    );
}

#[hegel::test(test_cases = 100)]
fn slicers_and_the_filter_reach_the_backend_as_they_are(tc: TestCase) {
    let native = tc.draw(gs::text());
    let mut spec = spec(1, &Shape::default());
    let slicer = |tc: &TestCase| {
        let field = FIELDS[tc.draw(gs::integers::<usize>().max_value(FIELDS.len() - 1))];
        let condition = match tc.draw(gs::booleans()) {
            true => Condition::native(field, tc.draw(gs::text())),
            false => {
                let n = tc.draw(gs::floats::<f64>().allow_nan(false));
                Condition::compare(field, Comparison::AtLeast(Scalar::Number(n)))
            }
        };
        condition.not(tc.draw(gs::booleans()))
    };
    let count = tc.draw(gs::integers::<usize>().max_value(4));
    let slicers: Vec<Condition> = (0..count).map(|_| slicer(&tc)).collect();
    spec.grouping.as_mut().unwrap().slicers = slicers.clone();
    let filter = FilterSpec::default()
        .with_conditions(vec![Condition::native(FIELDS[0], native.clone() + " or not")].into());
    spec.rows.filter = filter.clone();
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(spec);
    let batch = layer.poll_batch().expect("a batch for the groups");
    assert_eq!(batch.scope.slicers, slicers);
    assert_eq!(batch.scope.filter, filter);
}

#[test]
fn only_comparisons_on_a_values_field_act_after_aggregating() {
    let values = values();
    let at_least = |key| Condition::compare(key, Comparison::AtLeast(Scalar::Number(2.0)));
    assert_eq!(having(&at_least(FIELDS[1]), &values), Some(1));
    assert_eq!(having(&at_least(FIELDS[0]), &values), Some(2));
    assert_eq!(
        having(&at_least(Key(99)), &values),
        None,
        "no value's field"
    );
    let text = Condition::one_of(FIELDS[1], vec![Value::Blank]);
    assert_eq!(having(&text, &values), None, "on the Leaves");
}

#[test]
fn a_plain_table_is_not_the_layers() {
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(ViewSpec::default());
    assert!(!layer.grouped(Axis::Rows));
    assert_eq!(layer.poll_batch(), None);
    assert_eq!(layer.count(Axis::Rows), None);
    assert_eq!(layer.window(Axis::Rows, 0..10), vec![]);
    let viewed = layer.poll_viewed().unwrap();
    assert_eq!(viewed.applied, ViewSpec::default());
    assert_eq!(viewed.notices, vec![]);
}

#[hegel::test(test_cases = 100)]
fn placing_subtotals_elsewhere_lays_out_what_is_known_again(tc: TestCase) {
    let rows = tc.draw(rows());
    let levels = tc.draw(gs::integers::<usize>().min_value(1).max_value(2));
    let (first, then) = (tc.draw(shape()), tc.draw(shape()));
    let mut fake = Fake::new(rows.clone());
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(spec(levels, &first));
    drive(&tc, &mut layer, &mut fake);
    while layer.poll_viewed().is_some() {}
    layer.request_view(spec(levels, &then));
    let want = model(&rows, levels, &then, &Opened::default());
    let viewed = layer
        .poll_viewed()
        .expect("laid out at once, the groups known");
    let notices = match spec(levels, &first) == spec(levels, &then) {
        true => vec![],
        false => vec![(Axis::Rows, ChangeNotice::Refiltered(want.len() as u64))],
    };
    assert_eq!(viewed.notices, notices);
    assert_eq!(layer.count(Axis::Rows), Some(want.len() as u64));
    // Only a Grand Total newly placed is asked for.
    if let Some(batch) = layer.poll_batch() {
        assert!(batch.requests.iter().all(|r| r.want == Want::Total));
    }
}

/// Every item the layer shows, fetched a Window at a time, described and
/// with the Group Items' Keys added to `known`.
fn everything(
    tc: &TestCase,
    layer: &mut GroupingLayer<u64>,
    fake: &mut Fake,
    known: &mut Vec<(Key, Path)>,
) -> Vec<Described> {
    let count = layer.count(Axis::Rows).expect("a settled layer counts");
    // Each Window described as it comes, while its pages are cached.
    let mut got: Vec<(Key, Described)> = Vec::new();
    for _ in 0..1000 {
        if got.len() as u64 >= count {
            break;
        }
        let start = got.len() as u64;
        let items = layer.window(Axis::Rows, start..count);
        if items.is_empty() {
            drive(tc, layer, fake);
        }
        got.extend(items.iter().map(|(k, o)| (*k, describe(layer, *k, o))));
    }
    assert_eq!(got.len() as u64, count, "every item is fetched");
    for (key, d) in &got {
        if let Item::Group(path) = &d.item
            && !known.iter().any(|(k, _)| k == key)
        {
            known.push((*key, path.clone()));
        }
    }
    got.into_iter().map(|(_, d)| d).collect()
}

/// The Change Notices since the last call, and the `expand` last applied.
fn news(layer: &mut GroupingLayer<u64>) -> (Vec<ChangeNotice>, Option<Expand>) {
    let mut notices = Vec::new();
    let mut expand = None;
    while let Some(viewed) = layer.poll_viewed() {
        notices.extend(viewed.notices.into_iter().map(|(_, n)| n));
        expand = viewed.applied.grouping.map(|g| g.rows.expand);
    }
    (notices, expand)
}

fn depth(tc: &TestCase, levels: usize) -> Depth {
    match tc.draw(gs::integers::<u32>().max_value(levels as u32 + 1)) {
        n if n > levels as u32 => Depth::All,
        n => Depth::Levels(n),
    }
}

/// What one step of [`toggling`] saw: the items before and after (as the
/// layer describes them and as the model has them), which group was
/// toggled if one was, the notices, the `expand` asked for and applied,
/// the groups the layer asked about and how much it held.
struct Step {
    before: Vec<Described>,
    after: Vec<Described>,
    want: Vec<Described>,
    toggled: Option<Path>,
    notices: Vec<ChangeNotice>,
    expand: Expand,
    applied: Option<Expand>,
    opened: Opened,
    asked: Vec<Path>,
    held: u64,
}

/// Groups opened and closed one at a time, or the depth set, over any
/// rows, levels and shape, each step settled and seen whole by `check`.
fn toggling(tc: &TestCase, mut check: impl FnMut(&Step)) {
    let rows = tc.draw(rows());
    let levels = tc.draw(gs::integers::<usize>().min_value(1).max_value(2));
    let shape = tc.draw(shape());
    let first = depth(tc, levels);
    let mut opened = Opened {
        depth: first,
        except: Vec::new(),
    };
    let mut expand = Expand::to(first);
    let mut fake = Fake::new(rows.clone());
    // A cache small enough to let pages go now and then.
    let cache = tc.draw(gs::sampled_from(vec![0, 5, 40, binnacle_group::CACHE]));
    let mut layer = GroupingLayer::<u64>::with_cache(cache);
    layer.request_view(opening(levels, &shape, expand.clone()));
    drive(tc, &mut layer, &mut fake);
    news(&mut layer);
    let mut known = Vec::new();
    let mut shown = everything(tc, &mut layer, &mut fake, &mut known);
    assert_eq!(shown, model(&rows, levels, &shape, &opened));

    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(8));
    for _ in 0..steps {
        let mut toggled = None;
        match tc.draw(gs::integers::<u8>().max_value(3)) {
            3 => {
                let to = depth(tc, levels);
                expand = Expand::to(to);
                opened = Opened {
                    depth: to,
                    except: Vec::new(),
                };
            }
            _ if !known.is_empty() => {
                let (key, path) = tc.draw(gs::sampled_from(known.clone()));
                let level = path.len() as u32 - 1;
                let open = expand.opens(level, key);
                expand = expand.with(level, key, !open);
                match opened.except.iter().position(|p| *p == path) {
                    Some(i) => drop(opened.except.remove(i)),
                    None => opened.except.push(path.clone()),
                }
                toggled = Some(path);
            }
            _ => continue,
        }
        fake.asked.clear();
        layer.request_view(opening(levels, &shape, expand.clone()));
        drive(tc, &mut layer, &mut fake);
        let (notices, applied) = news(&mut layer);
        let after = everything(tc, &mut layer, &mut fake, &mut known);
        let step = Step {
            before: shown,
            want: model(&rows, levels, &shape, &opened),
            after,
            toggled,
            notices,
            expand: expand.clone(),
            applied,
            opened: opened.clone(),
            asked: std::mem::take(&mut fake.asked),
            held: layer.held(),
        };
        check(&step);
        shown = step.after;
    }
}

/// Opening and closing groups shows what the model shows, with the spec
/// applied as asked.
#[hegel::test(test_cases = 300)]
fn opening_and_closing_groups_shows_what_the_model_shows(tc: TestCase) {
    toggling(&tc, |step| {
        assert_eq!(
            step.applied.as_ref(),
            Some(&step.expand),
            "the spec is applied"
        );
        assert_eq!(step.after, step.want);
    });
}

/// The notices take the items shown before to those shown after:
/// `Collapsed` or `Expanded` when one group the user sees opens or
/// closes, nothing when nothing moved, `Refiltered` (with the count) only
/// for other changes.
#[hegel::test(test_cases = 300)]
fn opening_or_closing_one_group_says_where_with_collapsed_or_expanded(tc: TestCase) {
    toggling(&tc, |step| {
        let items = |ds: &[Described]| ds.iter().map(|d| d.item.clone()).collect::<Vec<_>>();
        let (was, now) = (items(&step.before), items(&step.after));
        let seen = (step.toggled.clone()).is_some_and(|p| was.contains(&Item::Group(p)));
        match step.notices.as_slice() {
            [] => assert_eq!(was, now, "no notice, nothing moved"),
            [ChangeNotice::Collapsed { at, n }] => {
                let mut was = was.clone();
                was.drain(*at as usize..(at + n) as usize);
                assert_eq!(was, now);
            }
            [ChangeNotice::Expanded { at, n }] => {
                let mut now = now.clone();
                now.drain(*at as usize..(at + n) as usize);
                assert_eq!(was, now);
            }
            [ChangeNotice::Refiltered(n)] if !seen => assert_eq!(*n, now.len() as u64),
            other => panic!("unexpected notices {other:?}"),
        }
    });
}

/// The layer asks only for the groups and Leaves of open groups.
#[hegel::test(test_cases = 200)]
fn the_layer_asks_only_under_open_groups(tc: TestCase) {
    toggling(&tc, |step| {
        for path in &step.asked {
            assert!(
                prefixes(path).iter().all(|p| step.opened.opens(p)),
                "{path:?} asked for under a closed group"
            );
        }
    });
}

/// Once settled, the layer holds no more groups and Leaves than it shows:
/// its memory grows with the open groups, never with the rows.
#[hegel::test(test_cases = 200)]
fn the_layer_holds_no_more_than_it_shows(tc: TestCase) {
    toggling(&tc, |step| {
        let shown = (step.after.iter())
            .filter(|d| matches!(d.item, Item::Group(_) | Item::Leaf(_)))
            .count();
        assert!(
            step.held <= shown as u64,
            "holds {} for {shown} shown",
            step.held
        );
    });
}

/// Groups opened and closed keep their state when the data changes under
/// them, a group that leaves and comes back included, and new groups
/// follow the depth.
#[hegel::test(test_cases = 200)]
fn open_and_closed_groups_survive_a_change_of_data(tc: TestCase) {
    let rows = tc.draw(rows());
    let levels = tc.draw(gs::integers::<usize>().min_value(1).max_value(2));
    let shape = tc.draw(shape());
    let first = depth(&tc, levels);
    let mut opened = Opened {
        depth: first,
        except: Vec::new(),
    };
    let mut expand = Expand::to(first);
    let mut fake = Fake::new(rows.clone());
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(opening(levels, &shape, expand.clone()));
    drive(&tc, &mut layer, &mut fake);
    let mut known = Vec::new();
    everything(&tc, &mut layer, &mut fake, &mut known);
    if !known.is_empty() {
        let toggles = tc.draw(gs::vecs(gs::sampled_from(known.clone())).max_size(4));
        for (key, path) in toggles {
            let level = path.len() as u32 - 1;
            if !opened.except.contains(&path) {
                let open = expand.opens(level, key);
                expand = expand.with(level, key, !open);
                opened.except.push(path);
            }
        }
    }
    layer.request_view(opening(levels, &shape, expand.clone()));
    drive(&tc, &mut layer, &mut fake);

    let changed = tc.draw(self::rows());
    fake.rows = changed.clone();
    layer.refresh();
    drive(&tc, &mut layer, &mut fake);
    assert_eq!(
        everything(&tc, &mut layer, &mut fake, &mut Vec::new()),
        model(&changed, levels, &shape, &opened),
        "under the new data"
    );
    fake.rows = rows.clone();
    layer.refresh();
    drive(&tc, &mut layer, &mut fake);
    assert_eq!(
        everything(&tc, &mut layer, &mut fake, &mut Vec::new()),
        model(&rows, levels, &shape, &opened),
        "the data back as it was"
    );
}

/// A Group Item Key in `expand.except` is kept while its path fits the
/// levels, through re-sorts and filters, and pruned when the levels change
/// so that it no longer can; a Key the layer never issued is kept.
#[hegel::test(test_cases = 100)]
fn changing_the_levels_prunes_the_exceptions_that_no_longer_fit(tc: TestCase) {
    let rows = tc.draw(rows().filter(|r: &Vec<Row>| !r.is_empty()));
    let shape = Shape::default();
    let mut fake = Fake::new(rows.clone());
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(opening(2, &shape, Expand::default()));
    drive(&tc, &mut layer, &mut fake);
    let mut known = Vec::new();
    everything(&tc, &mut layer, &mut fake, &mut known);
    let stranger = Key(SUMMARY_KEY_BIT | 77_777);
    let mut except: Vec<Key> = tc
        .draw(gs::vecs(gs::sampled_from(known.clone())).unique(true))
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    except.push(stranger);
    let expand = Expand {
        depth: Depth::All,
        except: except.clone(),
    };
    let top = |k: &Key| known.iter().any(|(g, p)| g == k && p.len() == 1) || *k == stranger;

    let mut sorted = opening(2, &shape, expand.clone());
    sorted.rows.sort = SortSpec::new(vec![SortColumn::descending(FIELDS[1])]);
    layer.request_view(sorted);
    drive(&tc, &mut layer, &mut fake);
    assert_eq!(
        news(&mut layer).1,
        Some(expand.clone()),
        "a re-sort keeps them"
    );

    layer.request_view(opening(1, &shape, expand.clone()));
    drive(&tc, &mut layer, &mut fake);
    let pruned: Vec<Key> = except.iter().copied().filter(top).collect();
    assert_eq!(
        news(&mut layer).1.map(|e| e.except),
        Some(pruned.clone()),
        "one level keeps the top groups"
    );
}

/// How many groups or Leaves the layer asks for at once, as its requests
/// show.
const PAGE: u64 = 64;

/// Rows of `groups` distinct values in the first field, `per` rows each,
/// and any values in the second.
fn many(groups: usize, per: usize) -> Vec<Row> {
    (0..groups * per)
        .map(|i| {
            let first = Value::Text(format!("g{:04}", i % groups));
            let second = Value::Text(format!("h{}", i % 3));
            (Key(i as u64 * 5 + 1), [first, second])
        })
        .collect()
}

/// Closed groups past the first page are laid out before they are
/// fetched: the rows count at once, a Window stops at the first group not
/// yet answered (which the Grid draws pending), and asking again fills it
/// in as the model has it.
#[hegel::test(test_cases = 50)]
fn closed_groups_lay_out_before_they_are_fetched(tc: TestCase) {
    let groups = tc.draw(
        gs::integers::<usize>()
            .min_value(PAGE as usize + 1)
            .max_value(300),
    );
    let levels = tc.draw(gs::integers::<usize>().min_value(1).max_value(2));
    let shape = tc.draw(shape());
    let rows = many(groups, 2);
    let mut fake = Fake::new(rows.clone());
    let mut layer = GroupingLayer::<u64>::new();
    let closed = Expand::to(Depth::Levels(0));
    layer.request_view(opening(levels, &shape, closed));
    let batch = layer.poll_batch().expect("the top groups are asked for");
    for request in &batch.requests {
        assert!(request.range.end - request.range.start <= PAGE);
    }
    // Answered exactly as asked, once.
    for request in &batch.requests {
        let body = match request.want {
            Want::Total => Body::Total {
                aggregates: aggregates(&fake.under(&request.path)),
            },
            _ => {
                let all = fake.groups(&request.path, levels);
                let end = (request.range.end as usize).min(all.len());
                Body::Groups {
                    total: all.len() as u64,
                    groups: all[request.range.start as usize..end].to_vec(),
                }
            }
        };
        layer.answer(Answer {
            stamp: batch.stamp,
            id: Some(request.id),
            path: request.path.clone(),
            start: request.range.start,
            body,
        });
    }
    let want = model(
        &rows,
        levels,
        &shape,
        &Opened {
            depth: Depth::Levels(0),
            except: vec![],
        },
    );
    let viewed = layer
        .poll_viewed()
        .expect("laid out with one page of groups");
    assert_eq!(
        viewed.notices,
        vec![(Axis::Rows, ChangeNotice::Refiltered(want.len() as u64))]
    );
    let count = want.len() as u64;
    let top = u64::from(shape.grand_total == Placement::Before);
    let shown = layer.window(Axis::Rows, 0..count);
    assert_eq!(
        shown.len() as u64,
        top + PAGE,
        "a Window stops at the first group not fetched"
    );
    let got = everything(&tc, &mut layer, &mut fake, &mut Vec::new());
    assert_eq!(got, want);
}

/// The pages each item of `items` is fetched in, by its index: whether it
/// is a Group Item or a Leaf, its parent's path and the page's first
/// index, with the page's size.
fn pages(items: &[Described]) -> Vec<Option<((bool, Vec<Path>, u64), u64)>> {
    let mut index: HashMap<(bool, Vec<Path>), u64> = HashMap::new();
    let mut at = Vec::new();
    for d in items {
        let group = match d.item {
            Item::Group(_) => true,
            Item::Leaf(_) => false,
            _ => {
                at.push(None);
                continue;
            }
        };
        let i = index.entry((group, d.ancestors.clone())).or_default();
        at.push(Some((group, d.ancestors.clone(), *i)));
        *i += 1;
    }
    at.into_iter()
        .map(|a| {
            let (group, parent, i) = a?;
            let siblings = index[&(group, parent.clone())];
            let start = i / PAGE * PAGE;
            Some(((group, parent, start), (siblings - start).min(PAGE)))
        })
        .collect()
}

/// However many rows the table has, the layer holds its open groups (with
/// the closed ones a level opened by default shows) and a bounded cache of
/// pages: those the latest two Windows read, and up to `cache` more.
#[hegel::test(test_cases = 60)]
fn the_layer_holds_open_groups_and_a_bounded_cache_whatever_the_rows(tc: TestCase) {
    let groups = tc.draw(gs::integers::<usize>().min_value(1).max_value(200));
    let per = tc.draw(
        gs::integers::<usize>()
            .min_value(1)
            .max_value(3000 / groups),
    );
    let mut rows = many(groups, per);
    // Some rows of their own group in the second field, so the inner
    // groups can be many too.
    for (i, row) in rows.iter_mut().enumerate() {
        if tc.draw(gs::weighted_booleans(0.3)) {
            row.1[1] = Value::Text(format!("u{i}"));
        }
    }
    let levels = tc.draw(gs::integers::<usize>().min_value(1).max_value(2));
    let shape = tc.draw(shape());
    let first = depth(&tc, levels);
    let cache = tc.draw(gs::integers::<u64>().max_value(300));
    let chunk = tc.draw(gs::integers::<u64>().min_value(1).max_value(50));
    let mut fake = Fake::new(rows.clone());
    let mut layer = GroupingLayer::<u64>::with_cache(cache);
    layer.request_view(opening(levels, &shape, Expand::to(first)));
    drive(&tc, &mut layer, &mut fake);
    let opened = Opened {
        depth: first,
        except: vec![],
    };
    let want = model(&rows, levels, &shape, &opened);
    // The groups the layout needs whatever is shown: those at a level
    // the depth opens.
    let needed = (want.iter())
        .filter(|d| matches!(&d.item, Item::Group(p) if opened.opens(p)))
        .count() as u64;
    let pages = pages(&want);
    let read = |range: Range<u64>| {
        let mut seen: Vec<&(bool, Vec<Path>, u64)> = Vec::new();
        let mut n = 0;
        for (page, size) in pages[range.start as usize..range.end as usize]
            .iter()
            .flatten()
        {
            if !seen.contains(&page) {
                seen.push(page);
                n += size;
            }
        }
        n
    };
    let count = want.len() as u64;
    let mut before = 0..0;
    let mut start = 0;
    while start < count {
        let range = start..(start + chunk).min(count);
        let mut items = layer.window(Axis::Rows, range.clone());
        for _ in 0..20 {
            if items.len() as u64 == range.end - range.start {
                break;
            }
            drive(&tc, &mut layer, &mut fake);
            items = layer.window(Axis::Rows, range.clone());
        }
        assert!(!items.is_empty());
        let lately = read(before.clone()) + read(range.clone());
        assert!(
            layer.held() <= needed + cache + lately,
            "holds {} for {needed} open, a cache of {cache} and {lately} read lately, over {} rows",
            layer.held(),
            rows.len()
        );
        before = range;
        start += items.len() as u64;
    }
}

/// With a small cache, the pages a Key was read in are let go of, yet the
/// layer still locates every item where a Window put it, asking for the
/// pages it needs again.
#[hegel::test(test_cases = 60)]
fn items_are_located_after_their_pages_are_let_go(tc: TestCase) {
    let groups = tc.draw(gs::integers::<usize>().min_value(1).max_value(150));
    let per = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let rows = many(groups, per);
    let levels = tc.draw(gs::integers::<usize>().min_value(1).max_value(2));
    let shape = tc.draw(shape());
    let first = depth(&tc, levels);
    let mut fake = Fake::new(rows.clone());
    let mut layer = GroupingLayer::<u64>::with_cache(tc.draw(gs::integers::<u64>().max_value(20)));
    layer.request_view(opening(levels, &shape, Expand::to(first)));
    drive(&tc, &mut layer, &mut fake);
    let got = everything(&tc, &mut layer, &mut fake, &mut Vec::new());
    let want = model(
        &rows,
        levels,
        &shape,
        &Opened {
            depth: first,
            except: vec![],
        },
    );
    assert_eq!(got, want);
    let keys: Vec<Key> = (0..want.len() as u64)
        .map(|i| {
            let mut item = layer.window(Axis::Rows, i..i + 1);
            for _ in 0..20 {
                if !item.is_empty() {
                    break;
                }
                drive(&tc, &mut layer, &mut fake);
                item = layer.window(Axis::Rows, i..i + 1);
            }
            item[0].0
        })
        .collect();
    let order = tc.draw(gs::permutations((0..keys.len()).collect::<Vec<_>>()));
    for i in order {
        let located = locate(&tc, &mut layer, &mut fake, keys[i]);
        assert_eq!(located, Located::Found(i as u64), "{:?}", want[i].item);
    }
}
