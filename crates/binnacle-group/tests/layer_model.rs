//! The grouping layer driven directly, sans-IO, against a naive model that
//! groups the whole table in memory. A fake aggregate backend answers each
//! batch out of order, in parts, short (the rest asked for again), with
//! more than asked, some answers late, and some after a newer View Spec has
//! superseded them.

use std::collections::HashMap;

use binnacle_core::{
    Axis, ChangeNotice, Grouping, ItemKind, Key, Level, Located, OpenState, Outline, Value,
    ViewSpec,
};
use binnacle_group::{Answer, Batch, Body, GroupRow, GroupingLayer, Path, SUMMARY_KEY_BIT, Want};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const FIELDS: [Key; 2] = [Key(10), Key(11)];

/// A row of the table, in the order the backend shows them: its Leaf Key
/// and its value in each field.
type Row = (Key, [Value; 2]);

#[hegel::composite]
fn value(tc: &TestCase) -> Value {
    let text = tc.draw(gs::sampled_from(vec!["a", "b", "c", ""]));
    match text {
        "" => Value::Blank,
        t => Value::Text(t.to_string()),
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

fn spec(levels: usize) -> ViewSpec {
    let mut grouping = Grouping::rows_by(FIELDS[0]);
    if levels == 2 {
        grouping.row_levels.push(Level::field(FIELDS[1]));
    }
    ViewSpec::default().with_grouping(Some(grouping))
}

/// An item as the model and the test describe it: Group Items by path,
/// Leaves by Key, with the outline's ancestors as paths.
#[derive(Clone, Debug, PartialEq)]
struct Described {
    item: Result<Path, Key>,
    kind: ItemKind,
    depth: u32,
    open: OpenState,
    ancestors: Vec<Path>,
    group: Option<(Key, Value)>,
}

fn path_of(row: &Row, depth: usize) -> Path {
    (0..depth).map(|d| (FIELDS[d], row.1[d].clone())).collect()
}

fn prefixes(path: &Path) -> Vec<Path> {
    (1..=path.len()).map(|d| path[..d].to_vec()).collect()
}

/// The naive model: every group in order of its first row, each Group
/// Item followed by what is under it, everything open.
fn model(rows: &[Row], levels: usize) -> Vec<Described> {
    fn walk(rows: &[&Row], path: Path, levels: usize, out: &mut Vec<Described>) {
        let depth = path.len();
        if depth == levels {
            for row in rows {
                out.push(Described {
                    item: Err(row.0),
                    kind: ItemKind::Leaf,
                    depth: depth as u32,
                    open: OpenState::CantOpen,
                    ancestors: prefixes(&path),
                    group: None,
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
            out.push(Described {
                item: Ok(child.clone()),
                kind: ItemKind::Group,
                depth: depth as u32,
                open: OpenState::Open,
                ancestors: prefixes(&path),
                group: Some((FIELDS[depth], value.clone())),
            });
            let under: Vec<&Row> = rows
                .iter()
                .filter(|r| r.1[depth] == value)
                .copied()
                .collect();
            walk(&under, child, levels, out);
        }
    }
    let mut out = Vec::new();
    walk(
        &rows.iter().collect::<Vec<_>>(),
        Vec::new(),
        levels,
        &mut out,
    );
    out
}

/// A fake backend over `rows`, worked out naively per request, which
/// mangles how it answers as `tc` draws.
struct Fake {
    rows: Vec<Row>,
    /// Answers held back, delivered at a later batch.
    late: Vec<Answer<u64>>,
}

impl Fake {
    fn under(&self, path: &Path) -> Vec<&Row> {
        (self.rows.iter())
            .filter(|r| path.iter().enumerate().all(|(d, (_, v))| r.1[d] == *v))
            .collect()
    }

    fn groups(&self, path: &Path, levels: usize) -> Vec<GroupRow<u64>> {
        let depth = path.len();
        let under = self.under(path);
        let mut order: Vec<Value> = Vec::new();
        for row in &under {
            if !order.contains(&row.1[depth]) {
                order.push(row.1[depth].clone());
            }
        }
        order
            .into_iter()
            .map(|value| {
                let mut child = path.clone();
                child.push((FIELDS[depth], value.clone()));
                let rows = self.under(&child);
                let children = match depth + 1 < levels {
                    true => self.groups(&child, levels).len(),
                    false => rows.len(),
                };
                GroupRow {
                    value,
                    children: children as u64,
                    aggregates: vec![rows.len() as u64],
                }
            })
            .collect()
    }

    /// The whole answer to each request, split in parts as `tc` draws.
    fn answer(&mut self, tc: &TestCase, batch: &Batch) -> Vec<Answer<u64>> {
        let levels = batch.scope.levels.len();
        let mut answers = Vec::new();
        for request in &batch.requests {
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
            if parts.len() > 1 && tc.draw(gs::weighted_booleans(0.3)) {
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
    let item = match key.0 & SUMMARY_KEY_BIT {
        0 => Err(key),
        _ => Ok(path(key).expect("a Group Item the layer knows")),
    };
    Described {
        item,
        kind: outline.kind,
        depth: outline.depth,
        open: outline.open,
        ancestors: outline
            .ancestors
            .iter()
            .map(|&k| path(k).unwrap())
            .collect(),
        group: outline.group.as_ref().map(|g| (g.field, g.value.clone())),
    }
}

#[hegel::test(test_cases = 300)]
fn the_layer_shows_what_a_whole_table_grouping_shows(tc: TestCase) {
    let rows = tc.draw(rows());
    let levels = tc.draw(gs::integers::<usize>().min_value(1).max_value(2));
    let mut fake = Fake {
        rows: rows.clone(),
        late: Vec::new(),
    };
    let mut layer = GroupingLayer::<u64>::new();

    // A spec the backend answers late, superseded before it settles.
    if tc.draw(gs::booleans()) {
        layer.request_view(spec(3 - levels));
        let stale = layer.poll_batch().unwrap();
        let answers = fake.answer(&tc, &stale);
        fake.late.extend(answers);
    }
    layer.request_view(spec(levels));
    drive(&tc, &mut layer, &mut fake);

    let want = model(&rows, levels);
    let mut viewed = Vec::new();
    while let Some(v) = layer.poll_viewed() {
        viewed.push(v);
    }
    let last = viewed.last().expect("the layer settles");
    assert_eq!(last.applied, spec(levels));
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

    // Keys are unique, and Summary Item Keys carry the summary bit.
    let mut seen = HashMap::new();
    for (i, (key, outline)) in got.iter().enumerate() {
        assert_eq!(seen.insert(*key, i), None, "Key {key:?} twice");
        assert_eq!(key.0 & SUMMARY_KEY_BIT != 0, outline.kind.is_summary());
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
    let mut fake = Fake {
        rows: rows.clone(),
        late: Vec::new(),
    };
    let mut layer = GroupingLayer::<u64>::new();
    layer.request_view(spec(1));
    drive(&tc, &mut layer, &mut fake);
    let want = model(&rows, 1);
    let i = tc.draw(gs::integers::<usize>().max_value(rows.len() - 1));
    let key = rows[i].0;
    let at = want.iter().position(|d| d.item == Err(key)).unwrap();
    let located = locate(&tc, &mut layer, &mut fake, key);
    assert_eq!(located, Located::Found(at as u64));
    let gone = locate(&tc, &mut layer, &mut fake, Key(1));
    assert_eq!(gone, Located::Gone { suggest: None }, "no row has Key 1");
}

#[hegel::test(test_cases = 100)]
fn summary_keys_stay_the_same_across_a_refresh_and_a_new_spec(tc: TestCase) {
    let rows = tc.draw(rows());
    let mut fake = Fake {
        rows,
        late: Vec::new(),
    };
    let mut layer = GroupingLayer::<u64>::new();
    let keys = |layer: &mut GroupingLayer<u64>, fake: &mut Fake| {
        drive(&tc, layer, fake);
        let count = layer.count(Axis::Rows).unwrap();
        let mut got = layer.window(Axis::Rows, 0..count);
        for _ in 0..20 {
            if got.len() as u64 == count {
                break;
            }
            drive(&tc, layer, fake);
            got = layer.window(Axis::Rows, 0..count);
        }
        got.into_iter().map(|(k, _)| k).collect::<Vec<_>>()
    };
    layer.request_view(spec(1));
    let first = keys(&mut layer, &mut fake);
    layer.refresh();
    assert_eq!(keys(&mut layer, &mut fake), first, "after a refresh");
    layer.request_view(spec(2));
    keys(&mut layer, &mut fake);
    layer.request_view(spec(1));
    assert_eq!(
        keys(&mut layer, &mut fake),
        first,
        "after grouping by two levels and back"
    );
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
