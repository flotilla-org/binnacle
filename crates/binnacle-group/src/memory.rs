//! The In-Memory Table Source under the grouping layer: an aggregate backend
//! over rows held in memory, and [`GroupedMemory`], a `MemorySource` that
//! groups.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

use binnacle_core::{
    Aggregate, AggregateValue, Axis, AxisView, ChangeNotice, Compared, Comparison, Condition,
    ConditionKind, ConditionTree, Distinct, DistinctQuery, FilterSpec, FindDirection, FindMatch,
    FindQuery, GRAND_TOTAL, Grouping, ItemKind, Key, Level, Located, MoveRefusal, Outline,
    Placement, Placements, Position, Scalar, SortColumn, SortDirection, SortSpec, Support,
    TextSupport, Totals, Value, ViewSpec,
};
use binnacle_memory::{
    FindCount, FindStep, MemorySource, MovePolicy, Moved, PumpSource, TextMatcher, Viewed,
};

use crate::layer::{GroupingLayer, Summary, SummaryId, settle};
use crate::query::{
    AggregateBackend, Answer, Batch, Body, GroupRow, Path, Scope, Stamp, Want, having,
};

/// An aggregate as the In-Memory Table Source computes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Figure {
    Count(u64),
    Number(f64),
    /// An aggregate this source does not compute, or of no values.
    Blank,
}

impl Figure {
    /// The figure as a number to sort by; `None` for a blank.
    pub fn number(self) -> Option<f64> {
        match self {
            Figure::Count(n) => Some(n as f64),
            Figure::Number(n) => Some(n),
            Figure::Blank => None,
        }
    }
}

/// One group of rows, and the groups of the next level within it.
#[derive(Default)]
struct Bucket {
    value: Option<Value>,
    /// The rows in the group, as indices into the rows in view.
    rows: Vec<usize>,
    /// The first of them.
    first: usize,
    leaves: Vec<Key>,
    kids: Vec<Bucket>,
    index: HashMap<Value, usize>,
    figures: Vec<Figure>,
    /// On a pivot, the rows of a row group under each column group, by its
    /// path, until their figures are worked out into `cells`.
    crossed: HashMap<Path, Vec<usize>>,
    cells: Vec<(Path, Vec<Figure>)>,
}

impl Bucket {
    /// The group of `value` among this one's kids, made if new.
    fn kid(&mut self, value: Value, row: usize) -> &mut Bucket {
        let at = match self.index.get(&value) {
            Some(&at) => at,
            None => {
                self.index.insert(value.clone(), self.kids.len());
                self.kids.push(Bucket {
                    value: Some(value),
                    first: row,
                    ..Bucket::default()
                });
                self.kids.len() - 1
            }
        };
        &mut self.kids[at]
    }
}

/// The standard aggregate `value` of the rows `rows`: `value(field, i)` is
/// row `i`'s value in a field and `number(field, i)` its number there.
fn figure(
    aggregate: &AggregateValue,
    rows: &[usize],
    value: &impl Fn(Key, usize) -> Value,
    number: &impl Fn(Key, usize) -> Option<f64>,
) -> Figure {
    let Some(field) = aggregate.field else {
        return match aggregate.aggregate.as_str() {
            "count" => Figure::Count(rows.len() as u64),
            _ => Figure::Blank,
        };
    };
    let numbers = || rows.iter().filter_map(|&i| number(field, i));
    let present = || {
        rows.iter()
            .map(|&i| value(field, i))
            .filter(|v| *v != Value::Blank)
    };
    let fold = |f: fn(f64, f64) -> f64| numbers().reduce(f).map_or(Figure::Blank, Figure::Number);
    match aggregate.aggregate.as_str() {
        "count" => Figure::Count(present().count() as u64),
        "distinct count" => Figure::Count(present().collect::<HashSet<_>>().len() as u64),
        "sum" => fold(|a, b| a + b),
        "minimum" => fold(f64::min),
        "maximum" => fold(f64::max),
        "average" => {
            let (sum, n) = numbers().fold((0.0, 0u64), |(s, n), x| (s + x, n + 1));
            match n {
                0 => Figure::Blank,
                n => Figure::Number(sum / n as f64),
            }
        }
        _ => Figure::Blank,
    }
}

/// Orders `a` and `b` as a sort column does: blanks last either way.
fn by_figure(a: Figure, b: Figure, direction: SortDirection) -> Ordering {
    match (a.number(), b.number()) {
        (Some(a), Some(b)) => match direction {
            SortDirection::Ascending => a.total_cmp(&b),
            SortDirection::Descending => b.total_cmp(&a),
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// The aggregate backend over a table in memory: every group of the rows in
/// view, worked out once with its aggregates, answering any request about
/// them.
///
/// The rows come sorted by the levels' fields first, so groups first come
/// in the order of their first row, and Leaves in row order. Siblings are
/// then ordered by the scope's Sort Spec, one rule at every level: by a
/// sort column on the level's own field, the order the rows gave them; by
/// a sort column on a value's field, that value's aggregate (the first
/// such value); unsorted, or on a tie, the order the rows gave them.
pub struct MemoryAggregates {
    stamp: Stamp,
    scope: Scope,
    levels: usize,
    root: Bucket,
    /// On a pivot, the column groups, by the column levels.
    cols: Bucket,
    /// Where each Leaf is: its group's path and its index there.
    places: HashMap<Key, (Path, u64)>,
    /// The rows grouped, the Leaves of groups left out included: the only
    /// rows an edit refigures over.
    grouped: HashSet<Key>,
}

impl MemoryAggregates {
    /// The groups of `rows`, the Leaf Keys in view order, for `batch`'s
    /// scope: `value(field, i)` is row `i`'s value in `field`, which groups
    /// it and which counts count, and `number(field, i)` its number there,
    /// which sums and averages take. The slicers, the scope's filter but its
    /// comparisons read after aggregating ([`having`]), the hidden Keys and
    /// the sort are the caller's to have applied to `rows`; those
    /// comparisons leave out the groups whose figures fail them here.
    pub fn new(
        batch: &Batch,
        rows: &[Key],
        value: impl Fn(Key, usize) -> Value,
        number: impl Fn(Key, usize) -> Option<f64>,
    ) -> Self {
        let scope = &batch.scope;
        let levels = scope.levels.len();
        let (mut root, mut cols) = bucket(rows, scope, &value);
        let mut aggregates = MemoryAggregates {
            stamp: batch.stamp,
            scope: scope.clone(),
            levels,
            root: Bucket::default(),
            cols: Bucket::default(),
            places: HashMap::new(),
            grouped: rows.iter().copied().collect(),
        };
        finish(&mut root, 0, scope, &value, &number);
        finish_columns(&mut cols, 0, scope, &value, &number);
        aggregates.root = root;
        aggregates.cols = cols;
        let mut path = Vec::new();
        place(
            &aggregates.root,
            &scope.levels,
            &mut path,
            &mut aggregates.places,
        );
        aggregates
    }

    /// Works out every group's aggregates again over `rows`, as
    /// [`MemoryAggregates::new`] takes them, after the user edited some,
    /// keeping the groups and their Leaves as they were: a group the rows
    /// no longer fill has the aggregates of no rows, and one they would
    /// newly form is not shown until the next View Spec. Only the rows
    /// grouped then count: one an edit brought past the filter waits for
    /// the next View Spec too, and a group whose figures now fail a
    /// comparison stays.
    pub fn refigure(
        &mut self,
        rows: &[Key],
        value: impl Fn(Key, usize) -> Value,
        number: impl Fn(Key, usize) -> Option<f64>,
    ) {
        let kept: Vec<usize> = (0..rows.len())
            .filter(|&i| self.grouped.contains(&rows[i]))
            .collect();
        let keys: Vec<Key> = kept.iter().map(|&i| rows[i]).collect();
        let value = |field, j: usize| value(field, kept[j]);
        let number = |field, j: usize| number(field, kept[j]);
        // Every group shown keeps its place, so no figure leaves one out now.
        let scope = Scope {
            filter: FilterSpec::default(),
            ..self.scope.clone()
        };
        let (mut fresh, _) = bucket(&keys, &scope, &value);
        finish(&mut fresh, 0, &scope, &value, &number);
        let empty = self.empty();
        fn copy(old: &mut Bucket, new: Option<&Bucket>, empty: &[Figure]) {
            old.figures = new.map_or_else(|| empty.to_vec(), |b| b.figures.clone());
            for kid in &mut old.kids {
                let value = kid.value.clone().unwrap_or(Value::Blank);
                let new = new.and_then(|b| b.index.get(&value).map(|&i| &b.kids[i]));
                copy(kid, new, empty);
            }
        }
        copy(&mut self.root, Some(&fresh), &empty);
    }

    /// The Leaf Keys of the group at `path`, in order.
    pub fn leaves(&self, path: &Path) -> Option<&[Key]> {
        Some(&self.bucket(path)?.leaves)
    }

    /// The path of the group holding the Leaf `key`.
    pub fn path_of(&self, key: Key) -> Option<&Path> {
        self.places.get(&key).map(|(path, _)| path)
    }

    /// Whether the Leaf `key` no longer belongs in the group holding it:
    /// `value` gives its value now in each level's field.
    pub fn regrouped(&self, key: Key, value: impl Fn(Key) -> Value) -> bool {
        (self.path_of(key)).is_some_and(|path| path.iter().any(|(f, v)| value(*f) != *v))
    }

    /// The Group Item Keys above the item with `key`, outermost first, open
    /// or not: a Leaf's group and its ancestors, a Subtotal's group and its
    /// ancestors, a Group Item's ancestors.
    pub fn ancestors(&self, layer: &mut GroupingLayer<Figure>, key: Key) -> Vec<Key> {
        let path = match layer.identity(key) {
            Some(SummaryId::Group(path)) => &path[..path.len().saturating_sub(1)],
            Some(SummaryId::Subtotal(path)) => &path[..],
            Some(SummaryId::GrandTotal | SummaryId::ValueTotal(_)) => return Vec::new(),
            None => match self.path_of(key) {
                Some(path) => &path[..],
                None => return Vec::new(),
            },
        }
        .to_vec();
        (1..=path.len())
            .map(|d| layer.key_of(SummaryId::Group(path[..d].to_vec())))
            .collect()
    }

    /// Where among the rows not grouped a move of the Leaves `keys` to
    /// before Position `to` of the grouped `axis` lands, or why it is
    /// refused: `base` gives a Leaf's Position among those rows.
    pub fn place_move(
        &self,
        layer: &GroupingLayer<Figure>,
        axis: Axis,
        keys: &[Key],
        to: Position,
        mut base: impl FnMut(Key) -> Option<Position>,
    ) -> Result<Position, MoveRefusal> {
        if keys.iter().any(|k| k.0 & crate::SUMMARY_KEY_BIT != 0) {
            return Err(MoveRefusal::Summary);
        }
        let between = MoveRefusal::BetweenGroups;
        let paths: Option<Vec<&Path>> = keys.iter().map(|&k| self.path_of(k)).collect();
        let path = (paths.as_deref())
            .and_then(|p| p.first().filter(|first| p.iter().all(|q| q == *first)))
            .ok_or(between)?;
        let index = layer.place_move(axis, path, to).ok_or(between)?;
        let leaves = self.leaves(path).unwrap_or_default();
        let to = match (leaves.get(index as usize), leaves.last()) {
            (Some(&k), _) => base(k),
            (None, Some(&last)) => base(last).map(|p| p + 1),
            (None, None) => None,
        };
        to.ok_or(MoveRefusal::BetweenGroups)
    }

    /// The aggregates of no rows.
    fn empty(&self) -> Vec<Figure> {
        let none = |_: Key, _: usize| Value::Blank;
        let rows: &[usize] = &[];
        (self.scope.values.iter())
            .map(|v| figure(v, rows, &none, &|_, _| None))
            .collect()
    }

    /// The stamp of the batch these groups were worked out for.
    pub fn stamp(&self) -> Stamp {
        self.stamp
    }

    fn bucket(&self, path: &Path) -> Option<&Bucket> {
        walk(&self.root, path)
    }

    fn row(&self, bucket: &Bucket, depth: usize) -> GroupRow<Figure> {
        GroupRow {
            value: bucket.value.clone().unwrap_or(Value::Blank),
            children: match depth < self.levels {
                true => bucket.kids.len() as u64,
                false => bucket.leaves.len() as u64,
            },
            aggregates: bucket.figures.clone(),
            cells: bucket.cells.clone(),
        }
    }
}

/// The groups of `rows` by the scope's levels, every group holding its
/// rows' indices and Leaf Keys in order, and on a pivot the column groups
/// by the column levels, each row group holding the rows it shares with
/// each column group.
fn bucket(rows: &[Key], scope: &Scope, value: &impl Fn(Key, usize) -> Value) -> (Bucket, Bucket) {
    let mut root = Bucket::default();
    let mut cols = Bucket::default();
    for (i, &row) in rows.iter().enumerate() {
        // On a pivot: each column group the row is in, outermost first.
        let mut across: Vec<Path> = Vec::new();
        let mut col = &mut cols;
        col.rows.push(i);
        for &field in &scope.columns {
            let v = value(field, i);
            let mut path = across.last().cloned().unwrap_or_default();
            path.push((field, v.clone()));
            across.push(path);
            col = col.kid(v, i);
            col.rows.push(i);
        }
        let mut bucket = &mut root;
        bucket.rows.push(i);
        for &field in &scope.levels {
            bucket = bucket.kid(value(field, i), i);
            bucket.rows.push(i);
            for path in &across {
                bucket.crossed.entry(path.clone()).or_default().push(i);
            }
        }
        bucket.leaves.push(row);
    }
    (root, cols)
}

fn walk<'b>(root: &'b Bucket, path: &Path) -> Option<&'b Bucket> {
    path.iter()
        .try_fold(root, |b, (_, v)| b.index.get(v).map(|&i| &b.kids[i]))
}

/// Orders two groups' values as ascending typed values do: by the number
/// `number` gives their first rows where both have one, else by text, with
/// blanks last.
fn by_value(
    field: Key,
    a: &Bucket,
    b: &Bucket,
    number: &impl Fn(Key, usize) -> Option<f64>,
) -> Ordering {
    match (number(field, a.first), number(field, b.first)) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        _ => match (&a.value, &b.value) {
            (Some(Value::Blank), Some(Value::Blank)) => Ordering::Equal,
            (Some(Value::Blank), _) => Ordering::Greater,
            (_, Some(Value::Blank)) => Ordering::Less,
            (x, y) => x.cmp(y),
        },
    }
}

/// Works out the column totals of `bucket` (at `depth` of the column
/// levels) and all under it, and orders its groups by the column Sort Spec:
/// a sort column on the level's field by value, one on the Grand Total row
/// by the first value's figure; unsorted, or on a tie, by ascending value.
fn finish_columns(
    bucket: &mut Bucket,
    depth: usize,
    scope: &Scope,
    value: &impl Fn(Key, usize) -> Value,
    number: &impl Fn(Key, usize) -> Option<f64>,
) {
    bucket.figures = (scope.values.iter())
        .map(|v| figure(v, &bucket.rows, value, number))
        .collect();
    bucket.rows = Vec::new();
    for kid in &mut bucket.kids {
        finish_columns(kid, depth + 1, scope, value, number);
    }
    let Some(&field) = scope.columns.get(depth) else {
        return;
    };
    bucket.kids.sort_by(|a, b| {
        let sorted = (scope.col_sort.columns().iter()).filter_map(|c| {
            let order = match c.key {
                k if k == field => by_value(field, a, b, number),
                GRAND_TOTAL if !scope.values.is_empty() => {
                    return Some(by_figure(a.figures[0], b.figures[0], c.direction));
                }
                _ => return None,
            };
            Some(match c.direction {
                SortDirection::Ascending => order,
                SortDirection::Descending => order.reverse(),
            })
        });
        (sorted.chain([by_value(field, a, b, number)]))
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal)
    });
    bucket.index = (bucket.kids.iter().enumerate())
        .map(|(i, k)| (k.value.clone().unwrap_or(Value::Blank), i))
        .collect();
}

/// Works out the aggregates of `bucket` (at `depth`) and all under it, and
/// orders its groups by the scope's sort.
fn finish(
    bucket: &mut Bucket,
    depth: usize,
    scope: &Scope,
    value: &impl Fn(Key, usize) -> Value,
    number: &impl Fn(Key, usize) -> Option<f64>,
) {
    bucket.figures = (scope.values.iter())
        .map(|v| figure(v, &bucket.rows, value, number))
        .collect();
    bucket.rows = Vec::new();
    bucket.cells = (std::mem::take(&mut bucket.crossed).into_iter())
        .map(|(path, rows)| {
            let figures = (scope.values.iter()).map(|v| figure(v, &rows, value, number));
            (path, figures.collect())
        })
        .collect();
    for kid in &mut bucket.kids {
        finish(kid, depth + 1, scope, value, number);
    }
    let Some(&field) = scope.levels.get(depth) else {
        return;
    };
    let having: Vec<(usize, &Condition)> = (scope.filter.conditions.conditions().iter())
        .filter_map(|c| Some((having(c, &scope.values)?, c)))
        .collect();
    if !having.is_empty() {
        let before = bucket.kids.len();
        bucket
            .kids
            .retain(|k| having.iter().all(|&(i, c)| passes(c, k.figures[i])));
        if bucket.kids.len() != before {
            reindex(bucket);
        }
    }
    // Each sort column that orders this level's groups, as the index of
    // the value whose aggregate it sorts by; `None` for the level's own
    // field, which keeps the order the rows gave.
    let by: Vec<(usize, SortDirection)> = (scope.sort.columns().iter())
        .take_while(|c| c.key != field)
        .filter_map(|c| {
            let i = (scope.values.iter()).position(|v| v.field == Some(c.key))?;
            Some((i, c.direction))
        })
        .collect();
    if by.is_empty() {
        return;
    }
    bucket.kids.sort_by(|a, b| {
        (by.iter())
            .map(|&(i, d)| by_figure(a.figures[i], b.figures[i], d))
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal)
    });
    reindex(bucket);
}

/// Finds each group of `bucket` by its value again, after they moved.
fn reindex(bucket: &mut Bucket) {
    bucket.index = (bucket.kids.iter().enumerate())
        .map(|(i, k)| (k.value.clone().unwrap_or(Value::Blank), i))
        .collect();
}

/// Whether a group whose figure is `figure` passes `condition`, a
/// comparison read after aggregating: never a blank.
fn passes(condition: &Condition, figure: Figure) -> bool {
    let ConditionKind::Compare(comparison) = &condition.kind else {
        return true;
    };
    let scalar = |n: f64| match comparison {
        Comparison::Equal(s)
        | Comparison::NotEqual(s)
        | Comparison::Less(s)
        | Comparison::AtMost(s)
        | Comparison::Greater(s)
        | Comparison::AtLeast(s)
        | Comparison::Between(s, _) => match s {
            Scalar::Number(_) => Scalar::Number(n),
            Scalar::Date(_) => Scalar::Date(n as i64),
        },
    };
    comparison.passes(figure.number().map(scalar)) != condition.not
}

/// Records where each Leaf under `bucket` (at `path`) is.
fn place(bucket: &Bucket, levels: &[Key], path: &mut Path, places: &mut HashMap<Key, (Path, u64)>) {
    for (i, &key) in bucket.leaves.iter().enumerate() {
        places.insert(key, (path.clone(), i as u64));
    }
    for kid in &bucket.kids {
        let field = levels[path.len()];
        path.push((field, kid.value.clone().unwrap_or(Value::Blank)));
        place(kid, levels, path, places);
        path.pop();
    }
}

/// One item of the grouped rows as if every group were open, in drawn
/// order, for searching what closed groups hide.
#[derive(Clone, Debug, PartialEq)]
pub struct Item<'a> {
    pub key: Key,
    pub kind: ItemKind,
    /// The Group Item Keys above it, outermost first.
    pub ancestors: Vec<Key>,
    /// Whether a closed group hides it: it is not among the items shown.
    pub hidden: bool,
    /// A Group Item's or Subtotal's level field and value.
    pub group: Option<(Key, &'a Value)>,
    /// A Summary Item's aggregates, one per value; empty for a Leaf.
    pub figures: &'a [Figure],
}

impl MemoryAggregates {
    /// Every item of the grouped rows, open or not, in the order the layer
    /// draws them, with its Keys as the layer gives them and whether the
    /// groups `layer` has closed hide it.
    pub fn items(&self, layer: &mut GroupingLayer<Figure>) -> Vec<Item<'_>> {
        let shape = layer.shape().clone();
        let total = Item {
            key: GRAND_TOTAL,
            kind: ItemKind::GrandTotal,
            ancestors: Vec::new(),
            hidden: false,
            group: None,
            figures: &self.root.figures,
        };
        let mut out = Vec::new();
        if shape.grand_total == Placement::Before {
            out.push(total.clone());
        }
        let levels = self.scope.levels.clone();
        let mut walk = Walk {
            layer,
            shape: &shape,
            levels: &levels,
            path: Vec::new(),
            ancestors: Vec::new(),
            out: &mut out,
        };
        walk.bucket(&self.root, false);
        if shape.grand_total == Placement::After {
            out.push(total);
        }
        out
    }
}

/// A walk through every group for [`MemoryAggregates::items`].
struct Walk<'w, 'a> {
    layer: &'w mut GroupingLayer<Figure>,
    shape: &'w crate::layer::Shape,
    levels: &'w [Key],
    path: Path,
    ancestors: Vec<Key>,
    out: &'w mut Vec<Item<'a>>,
}

impl<'a> Walk<'_, 'a> {
    fn bucket(&mut self, bucket: &'a Bucket, hidden: bool) {
        let depth = self.path.len();
        let Some(&field) = self.levels.get(depth) else {
            self.out.extend(bucket.leaves.iter().map(|&key| Item {
                key,
                kind: ItemKind::Leaf,
                ancestors: self.ancestors.clone(),
                hidden,
                group: None,
                figures: &[],
            }));
            return;
        };
        for kid in &bucket.kids {
            let value = kid.value.as_ref().unwrap_or(&Value::Blank);
            self.path.push((field, value.clone()));
            let key = self.layer.key_of(SummaryId::Group(self.path.clone()));
            let summary = |kind, key, ancestors| Item {
                key,
                kind,
                ancestors,
                hidden,
                group: Some((field, value)),
                figures: &kid.figures,
            };
            self.out
                .push(summary(ItemKind::Group, key, self.ancestors.clone()));
            let shows = self.layer.layout().opens(depth, key);
            let subtotal = self.layer.key_of(SummaryId::Subtotal(self.path.clone()));
            self.ancestors.push(key);
            let mut sub = summary(ItemKind::Subtotal, subtotal, self.ancestors.clone());
            sub.hidden = hidden || !shows;
            let placed = self.shape.subtotal(depth);
            if placed == Placement::Before {
                self.out.push(sub.clone());
            }
            self.bucket(kid, hidden || !shows);
            if placed == Placement::After {
                self.out.push(sub);
            }
            self.ancestors.pop();
            self.path.pop();
        }
    }
}

/// The part of `items` in `range`.
fn slice<'a, T>(items: &'a [T], range: &Range<u64>) -> &'a [T] {
    let end = (range.end as usize).min(items.len());
    let start = (range.start as usize).min(end);
    &items[start..end]
}

impl AggregateBackend<Figure> for MemoryAggregates {
    fn answer(&mut self, batch: &Batch) -> Vec<Answer<Figure>> {
        let mut answers = Vec::new();
        for request in &batch.requests {
            let answer = |path: Path, start: u64, body: Body<Figure>| Answer {
                stamp: batch.stamp,
                id: Some(request.id),
                path,
                start,
                body,
            };
            match request.want {
                Want::Locate(key) => {
                    let (path, start) = self.places.get(&key).cloned().unwrap_or_default();
                    let found = self.places.contains_key(&key);
                    answers.push(answer(path, start, Body::Located { key, found }));
                }
                Want::Total if self.bucket(&request.path).is_none() => {
                    let body = Body::Total {
                        aggregates: self.empty(),
                    };
                    answers.push(answer(request.path.clone(), 0, body));
                }
                Want::Columns => {
                    let Some(bucket) = walk(&self.cols, &request.path) else {
                        continue;
                    };
                    let depth = request.path.len() + 1;
                    let body = Body::Columns {
                        total: bucket.kids.len() as u64,
                        groups: (slice(&bucket.kids, &request.range).iter())
                            .map(|b| GroupRow {
                                cells: Vec::new(),
                                children: match depth < batch.scope.columns.len() {
                                    true => b.kids.len() as u64,
                                    false => 0,
                                },
                                ..self.row(b, depth)
                            })
                            .collect(),
                    };
                    answers.push(answer(request.path.clone(), request.range.start, body));
                }
                want => {
                    let Some(bucket) = self.bucket(&request.path) else {
                        continue;
                    };
                    let body = match want {
                        Want::Groups => Body::Groups {
                            total: bucket.kids.len() as u64,
                            groups: (slice(&bucket.kids, &request.range).iter())
                                .map(|b| self.row(b, request.path.len() + 1))
                                .collect(),
                        },
                        Want::Total => Body::Total {
                            aggregates: bucket.figures.clone(),
                        },
                        _ => Body::Leaves {
                            keys: slice(&bucket.leaves, &request.range).to_vec(),
                        },
                    };
                    answers.push(answer(request.path.clone(), request.range.start, body));
                }
            }
        }
        answers
    }
}

/// The Keys of a Window, with their outlines where the axis is grouped.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Window {
    pub keys: Vec<Key>,
    pub outlines: Option<Vec<Outline>>,
}

/// How a host writes a Summary Item's figure in a column, as its cells
/// show it, for find to match.
type FigureText = Box<dyn Fn(Key, Figure) -> Option<String>>;

/// A match of grouped find: its place among every cell searched, the cell,
/// and whether a closed group hides it.
type Found = (u64, FindMatch, bool);

/// A [`MemorySource`] that groups its rows under the grouping layer. While
/// the rows are grouped, the base rows are sorted by the levels' fields
/// first (in the direction the user sorts them, ascending otherwise) and
/// then by the user's sort, and the levels' fields leave the columns.
///
/// Find searches every item as if every group were open, counting the
/// matches closed groups hide: a Leaf's cells as the base source matches
/// them, and a Group Item's label (its value) at its level's field. A
/// Summary Item's figures match only once the host says how its cells
/// write them ([`GroupedMemory::find_figures`]). Leaves move within their
/// group, and the user's edit moves nothing until the next View Spec.
pub struct GroupedMemory<R> {
    source: MemorySource<R>,
    layer: GroupingLayer<Figure>,
    aggregates: Option<MemoryAggregates>,
    /// The fields hidden from the columns because the rows group by them.
    lifted: Vec<Key>,
    /// The View Spec last applied.
    spec: ViewSpec,
    /// Whether the user edited a row since the rows were last grouped, so
    /// the next View Spec groups them again.
    edited: bool,
    figure_text: Option<FigureText>,
}

impl<R> GroupedMemory<R> {
    pub fn new(source: MemorySource<R>) -> Self {
        GroupedMemory {
            source,
            layer: GroupingLayer::new(),
            aggregates: None,
            lifted: Vec::new(),
            spec: ViewSpec::default(),
            edited: false,
            figure_text: None,
        }
    }

    /// Lets find match a Summary Item's figures, as `text` writes the
    /// figure of the value over a column in that column's cells (`None`
    /// for one drawn blank). By default they don't: aggregates are
    /// computed, not data.
    pub fn find_figures(mut self, text: impl Fn(Key, Figure) -> Option<String> + 'static) -> Self {
        self.figure_text = Some(Box::new(text));
        self
    }

    pub fn source(&self) -> &MemorySource<R> {
        &self.source
    }

    /// The source, to change data through. Changes to the rows while they
    /// are grouped need [`GroupedMemory::refresh`] after.
    pub fn source_mut(&mut self) -> &mut MemorySource<R> {
        &mut self.source
    }

    /// Whether `axis` is grouped now.
    pub fn grouped(&self, axis: Axis) -> bool {
        self.layer.grouped(axis)
    }

    /// The rows may group by a column that sorts and has a text form, with
    /// Subtotals before or after, pivot by it (the columns grouped by it),
    /// and slice by any column as they filter by it.
    pub fn support(&self, axis: Axis, key: Key) -> Support {
        let support = self.source.support(axis, key);
        let group = axis == Axis::Rows && support.sort && support.text.is_some();
        let subtotals = match group {
            true => Placements::BOTH,
            false => Placements::default(),
        };
        (support.with_group(group).with_pivot(group)).with_subtotals(subtotals)
    }

    /// The aggregates a value over column `key` may take while the rows are
    /// grouped: count and distinct count over a column with a text form,
    /// and the rest of the standard ones over a column of numbers.
    pub fn aggregates(&self, axis: Axis, key: Key) -> Vec<Aggregate> {
        let support = self.source.support(axis, key);
        match (axis, support.compare) {
            (Axis::Rows, Some(Compared::Numbers)) => Aggregate::numbers(),
            (Axis::Rows, _) if support.text.is_some() => Aggregate::counts(),
            _ => Vec::new(),
        }
    }

    /// The parts of `grouping` this source applies: the levels on the rows
    /// it may group by, those on the columns it may pivot by, with their
    /// Subtotals, the values pseudo-level, and the values with aggregates
    /// it offers. Slicers are the base source's to apply.
    fn applicable(&self, grouping: &Grouping) -> Grouping {
        let mut applied = grouping.clone();
        for axis in Axis::ALL {
            applied.levels_mut(axis).retain(|l| match l {
                Level::Field { field, subtotal } => {
                    let support = self.support(Axis::Rows, *field);
                    let may = match axis {
                        Axis::Rows => support.group,
                        Axis::Cols => support.pivot,
                    };
                    may && support.subtotals.allows(*subtotal)
                }
                Level::Values => true,
                _ => false,
            });
        }
        applied.values.retain(|v| match v.field {
            None => v.aggregate == "count" && v.params.is_empty(),
            Some(field) => (self.aggregates(Axis::Rows, field).iter())
                .any(|a| a.name == v.aggregate && a.params.len() == v.params.len()),
        });
        applied
    }

    /// Presents the table as `spec` asks, grouping included; answers with
    /// the spec applied, the totals (the rows' unknown while grouped) and
    /// the Change Notices: the rows' `Refiltered` whenever their grouping
    /// or the rows under it change, and `Collapsed` or `Expanded` when one
    /// group opens or closes.
    ///
    /// On a pivot the columns come from the data too: the base columns
    /// leave, any change to the pivot columns comes as `Refiltered`, and
    /// the column axis's Sort Spec orders them by a column level's field or
    /// by the Grand Total row; other column sorts, filters and hidden Keys
    /// are not applied.
    pub fn view(&mut self, rows: &[R], spec: &ViewSpec) -> Viewed {
        let grouping = spec.grouping.as_ref().map(|g| self.applicable(g));
        let fields = |axis| -> Vec<Key> {
            (grouping.iter())
                .flat_map(|g| g.levels(axis).iter().filter_map(|l| l.field_key()))
                .collect()
        };
        let (levels, columns) = (fields(Axis::Rows), fields(Axis::Cols));
        let pivot = grouping.as_ref().is_some_and(Grouping::pivot);
        let values = match &grouping {
            Some(g) if !levels.is_empty() || pivot => g.values.as_slice(),
            _ => &[],
        };
        let slicers = grouping.as_ref().map_or(&[][..], |g| &g.slicers);
        // The base rows take the conditions on the Leaves, then the slicers;
        // the comparisons on a value's field wait for the figures.
        let conditions = spec.rows.filter.conditions.conditions();
        let (later, leaves): (Vec<&Condition>, Vec<&Condition>) =
            conditions.iter().partition(|c| having(c, values).is_some());
        let mut base = spec.clone();
        base.grouping = None;
        base.rows.filter.conditions =
            ConditionTree::All(leaves.iter().copied().chain(slicers).cloned().collect());
        if pivot {
            base.cols = AxisView::default();
        }
        self.lifted = (levels.iter())
            .filter(|f| !spec.cols.hidden.contains(f) && !pivot)
            .copied()
            .collect();
        base.cols.hidden.extend(&self.lifted);
        let mut sort: Vec<SortColumn> = (levels.iter())
            .map(|&f| match spec.rows.sort.find(f) {
                Some((i, _)) => spec.rows.sort.columns()[i],
                None => SortColumn::ascending(f),
            })
            .collect();
        sort.extend((spec.rows.sort.columns().iter()).filter(|c| !levels.contains(&c.key)));
        base.rows.sort = SortSpec::new(sort);

        let was_grouped = self.layer.grouped(Axis::Rows);
        let was_pivot = self.layer.pivot();
        let mut notices = self.source.view(rows, &base).unwrap_or_default();
        let rows_changed = notices.iter().any(|(a, _)| *a == Axis::Rows);
        let mut applied = self.source.view_spec();
        let base_sort = applied.rows.sort.clone();
        applied.rows.sort = SortSpec::new(
            (spec.rows.sort.columns().iter())
                .filter(|c| base_sort.find(c.key).is_some())
                .copied()
                .collect(),
        );
        applied.cols.hidden.retain(|k| !self.lifted.contains(k));
        // The base applied an ordered part of what it was given: the Leaves'
        // conditions, then the slicers.
        let mut taken = applied
            .rows
            .filter
            .conditions
            .conditions()
            .iter()
            .peekable();
        let mut took = |c: &Condition| taken.next_if(|t| *t == c).is_some();
        let leaves: Vec<&Condition> = leaves.into_iter().filter(|c| took(c)).collect();
        let slicers: Vec<Condition> = slicers.iter().filter(|c| took(c)).cloned().collect();
        applied.rows.filter.conditions = ConditionTree::All(
            (conditions.iter())
                .filter(|c| later.contains(c) || leaves.contains(c))
                .cloned()
                .collect(),
        );
        applied.grouping = grouping
            .map(|mut g| {
                g.slicers = slicers;
                g
            })
            .filter(|g| !levels.is_empty() || pivot || !g.slicers.is_empty());
        if pivot {
            let sorts = |c: &&SortColumn| columns.contains(&c.key) || c.key == GRAND_TOTAL;
            applied.cols = AxisView::default();
            applied.cols.sort = SortSpec::new(
                spec.cols
                    .sort
                    .columns()
                    .iter()
                    .filter(sorts)
                    .copied()
                    .collect(),
            );
        }
        let scope_before = self.layer.grouped(Axis::Rows);
        self.layer.request_view(applied.clone());
        let edited = std::mem::take(&mut self.edited);
        if (rows_changed || edited) && scope_before && self.layer.grouped(Axis::Rows) {
            self.layer.refresh();
        }
        self.settle(rows);
        let mut layered = Vec::new();
        while let Some(viewed) = self.layer.poll_viewed() {
            layered.extend(viewed.notices);
            // The layer prunes `expand` as the levels change.
            if applied.grouping.is_some() {
                applied.grouping = viewed.applied.grouping;
            }
        }
        let now = Axis::ALL.map(|a| self.layer.grouped(a));
        let was = [was_grouped, was_pivot];
        for axis in Axis::ALL {
            let i = axis.index();
            if now[i] || was[i] {
                notices.retain(|(a, _)| *a != axis);
            }
            if now[i] {
                notices.extend(layered.iter().filter(|(a, _)| *a == axis).cloned());
            } else if was[i] {
                let count = self.source.count(axis);
                notices.push((axis, ChangeNotice::Refiltered(count)));
            }
        }
        notices.sort_by_key(|(a, _)| a.index());
        let mut totals = self.source.totals();
        if now[0] {
            totals.rows = None;
        }
        if now[1] {
            totals.cols = None;
        }
        self.spec = applied.clone();
        Viewed {
            applied,
            totals,
            notices,
        }
    }

    /// Starts the groups afresh from the rows, after a change to the data.
    pub fn refresh(&mut self, rows: &[R]) {
        self.layer.refresh();
        self.settle(rows);
        while self.layer.poll_viewed().is_some() {}
    }

    fn settle(&mut self, rows: &[R]) {
        let mut backend = Backend {
            source: &self.source,
            rows,
            aggregates: &mut self.aggregates,
        };
        settle(&mut self.layer, &mut backend);
    }

    pub fn count(&self, axis: Axis) -> u64 {
        match self.layer.count(axis) {
            Some(count) => count,
            None => self.source.count(axis),
        }
    }

    /// The Keys at `range` on `axis`, and their outlines if it is grouped.
    pub fn window(&mut self, rows: &[R], axis: Axis, range: Range<Position>) -> Window {
        if !self.layer.grouped(axis) {
            return Window {
                keys: self.source.window(axis, range),
                outlines: None,
            };
        }
        let want = range.end.min(self.count(axis)).saturating_sub(range.start) as usize;
        let mut items = self.layer.window(axis, range.clone());
        // Each pass asks for the first page it lacks.
        while items.len() < want {
            self.settle(rows);
            let more = self.layer.window(axis, range.clone());
            if more.len() <= items.len() {
                break;
            }
            items = more;
        }
        let (keys, outlines) = items.into_iter().unzip();
        Window {
            keys,
            outlines: Some(outlines),
        }
    }

    pub fn locate(&mut self, rows: &[R], axis: Axis, key: Key) -> Located {
        if !self.layer.grouped(axis) {
            return self.source.locate(axis, key);
        }
        loop {
            if let Some(located) = self.layer.locate(axis, key) {
                return located;
            }
            self.settle(rows);
        }
    }

    /// The Summary Item with `key`, for drawing.
    pub fn summary(&self, key: Key) -> Option<Summary<'_, Figure>> {
        self.layer.summary(key)
    }

    /// The path of the group the Summary Item `key` heads or totals (empty
    /// for the Grand Total), shown or not, or of the group holding the Leaf
    /// `key` among those shown.
    pub fn path(&self, key: Key) -> Option<&Path> {
        self.layer.path(key)
    }

    /// The user edited the value in `column` of base row `row`, as
    /// [`MemorySource::edited`] takes it. While the rows are grouped
    /// nothing moves until the next View Spec, even a Leaf whose edit
    /// changed its group, which stays where it is, unmatched; the figures
    /// of every Summary Item are worked out again at once. Returns the
    /// Change Notices: `ContentChanged` for the Leaf and for each Summary
    /// Item whose figures were answered anew, as the layer places them.
    ///
    /// # Panics
    ///
    /// As [`MemorySource::edited`].
    pub fn edited(&mut self, rows: &[R], row: usize, column: Key) -> Vec<(Axis, ChangeNotice)> {
        let notices = self.source.edited(rows, row, column);
        if !self.layer.grouped(Axis::Rows) {
            return notices;
        }
        self.edited = true;
        let mut out: Vec<_> = notices
            .into_iter()
            .filter(|(a, _)| *a == Axis::Cols)
            .collect();
        if let Some(aggregates) = self.aggregates.as_mut() {
            let count = self.source.count(Axis::Rows);
            let keys = self.source.window(Axis::Rows, 0..count);
            let logical = self.source.logical_window(Axis::Rows, 0..count);
            let source = &self.source;
            let row = |i: usize| &rows[logical[i]];
            aggregates.refigure(
                &keys,
                |field, i| source.value(field, row(i)),
                |field, i| source.number(field, row(i)),
            );
        }
        self.layer.refigure();
        self.settle(rows);
        let key = self.source.base_key(Axis::Rows, row);
        if let Located::Found(p) = self.locate(rows, Axis::Rows, key) {
            out.push((Axis::Rows, ChangeNotice::ContentChanged(p..p + 1)));
        }
        while let Some(change) = self.layer.poll_change() {
            out.push(change);
        }
        out.sort_by_key(|(a, _)| a.index());
        out
    }

    /// Whether each item with `keys` on `axis` is unmatched: in view though
    /// the filter in force drops it (see [`MemorySource::unmatched`]), or a
    /// Leaf whose values no longer fit the group it is shown in, since the
    /// user's edit.
    pub fn unmatched(&mut self, rows: &[R], axis: Axis, keys: &[Key]) -> Vec<bool> {
        let mut flags = self.source.unmatched(axis, keys);
        if !self.layer.grouped(axis) {
            return flags;
        }
        for (flag, &key) in flags.iter_mut().zip(keys) {
            if key.0 & crate::SUMMARY_KEY_BIT != 0 {
                *flag = false;
                continue;
            }
            let Some(row) = self.source.logical_index(axis, key) else {
                continue;
            };
            let Some(aggregates) = self.aggregates.as_ref() else {
                continue;
            };
            *flag |= aggregates.regrouped(key, |field| self.source.value(field, &rows[row]));
        }
        flags
    }

    /// Whether the fields the Leaves are sorted by include one the rows do
    /// not group by.
    fn sorted_within(&self) -> bool {
        let levels = (self.spec.grouping.iter())
            .flat_map(|g| g.row_levels.iter().filter_map(|l| l.field_key()))
            .collect::<Vec<_>>();
        (self.spec.rows.sort.columns().iter()).any(|c| !levels.contains(&c.key))
    }

    /// Whether the items of `axis` can move now: grouped Leaves can, within
    /// their group, unless moves on a sorted axis are refused and the
    /// Leaves are sorted.
    pub fn movable(&self, axis: Axis) -> bool {
        match self.layer.grouped(axis) {
            false => self.source.movable(axis),
            true => self.source.move_policy() == MovePolicy::Bake || !self.sorted_within(),
        }
    }

    /// Moves the items with `keys` on `axis` to before Position `to`, as
    /// [`MemorySource::move_items`] does for a plain axis. Grouped Leaves
    /// move within their group; on Leaves sorted by a field the rows do not
    /// group by, the order shown is kept and that sort dropped (unless
    /// moves on a sorted axis are refused). Returns the Change Notices and
    /// the spec now applied if the sort changed, or why it was refused.
    pub fn move_items(
        &mut self,
        rows: &[R],
        axis: Axis,
        keys: &[Key],
        to: Position,
    ) -> Result<Moved, String> {
        let sorted = "Can't move while sorted: clear the sort first";
        if !self.layer.grouped(axis) {
            let before = self.source.view_spec();
            let notice = self.source.move_items(axis, keys, to).ok_or(sorted)?;
            let after = self.source.view_spec();
            return Ok(Moved {
                notices: vec![(axis, notice)],
                view: (after != before).then(|| (after, self.source.totals())),
            });
        }
        let refused = |r| match r {
            MoveRefusal::Summary => "Group rows and totals can't be moved".to_string(),
            _ => "Can't move between groups".to_string(),
        };
        let aggregates = self.aggregates.as_ref();
        let aggregates = aggregates.ok_or_else(|| refused(MoveRefusal::BetweenGroups))?;
        let source = &mut self.source;
        let base_to = aggregates
            .place_move(&self.layer, axis, keys, to, |k| {
                match source.locate(axis, k) {
                    Located::Found(p) => Some(p),
                    Located::Gone { .. } => None,
                }
            })
            .map_err(refused)?;
        let sorted_within = self.sorted_within();
        if sorted_within && self.source.move_policy() == MovePolicy::Refuse {
            return Err(sorted.into());
        }
        // The base rows are always sorted by the levels: the order shown is
        // baked, and the levels sort them again with the next View Spec.
        let policy = self.source.move_policy();
        self.source.set_move_policy(MovePolicy::Bake);
        self.source.move_items(axis, keys, base_to);
        self.source.set_move_policy(policy);
        let levels: Vec<Key> = (self.spec.grouping.iter())
            .flat_map(|g| g.row_levels.iter().filter_map(|l| l.field_key()))
            .collect();
        let view = sorted_within.then(|| {
            let kept = (self.spec.rows.sort.columns().iter())
                .filter(|c| levels.contains(&c.key))
                .copied()
                .collect();
            self.spec.rows.sort = SortSpec::new(kept);
            (self.spec.clone(), self.totals())
        });
        match &view {
            Some((spec, _)) => self.layer.request_view(spec.clone()),
            None => self.layer.refresh(),
        }
        self.settle(rows);
        while self.layer.poll_viewed().is_some() {}
        Ok(Moved {
            notices: vec![(axis, ChangeNotice::Reordered)],
            view,
        })
    }

    /// The Group Item Keys above the item with `key` on `axis`, outermost
    /// first, open or not; empty where `axis` is not grouped.
    pub fn ancestors(&mut self, axis: Axis, key: Key) -> Vec<Key> {
        match (self.layer.grouped(axis), self.aggregates.as_ref()) {
            (true, Some(aggregates)) => aggregates.ancestors(&mut self.layer, key),
            _ => Vec::new(),
        }
    }

    /// The text of the cell of the Summary Item `key` in `col` that find
    /// matches: its label at its level's field, its figure in a column a
    /// value aggregates once the host writes figures, else none.
    fn summary_text(&self, key: Key, col: Key) -> Option<String> {
        let summary = self.layer.summary(key)?;
        if let (ItemKind::Group, Some((field, value))) = (summary.kind, summary.path.last())
            && *field == col
        {
            return match value {
                Value::Text(t) => Some(t.clone()),
                Value::Blank => None,
            };
        }
        let text = self.figure_text.as_ref()?;
        let values = &self.spec.grouping.as_ref()?.values;
        let i = values.iter().position(|v| v.field == Some(col))?;
        text(col, *summary.aggregates.get(i)?)
    }

    /// Which cells of the block `row_keys` by `col_keys` match `query`, row
    /// by row, as [`MemorySource::find_marks`] says for a plain table: a
    /// Group Item's label at its level's field, and a Leaf's cells.
    pub fn find_marks(
        &mut self,
        rows: &[R],
        row_keys: &[Key],
        col_keys: &[Key],
        query: &FindQuery,
    ) -> Vec<bool> {
        if !self.layer.grouped(Axis::Rows) {
            return self.source.find_marks(rows, row_keys, col_keys, query);
        }
        let mut marks = self.source.find_marks(rows, row_keys, col_keys, query);
        let Some(matcher) = TextMatcher::new(&query.query) else {
            return marks;
        };
        let searched: Vec<Key> = (self.source.find_columns(&query.query).into_iter())
            .map(|(_, c)| c)
            .collect();
        for (i, &r) in row_keys.iter().enumerate() {
            if r.0 & crate::SUMMARY_KEY_BIT == 0 {
                continue;
            }
            for (j, &c) in col_keys.iter().enumerate() {
                let label =
                    (self.layer.path(r).and_then(|p| p.last())).is_some_and(|(f, _)| *f == c);
                let hit = (label || searched.contains(&c))
                    && self
                        .summary_text(r, c)
                        .is_some_and(|t| matcher.is_match(&t));
                marks[i * col_keys.len() + j] = hit;
            }
        }
        marks
    }

    /// Every cell of the grouped rows that matches `query`, in its order
    /// (as if every group were open: by rows, each row's label before its
    /// cells; by columns, the labels first), with its place among every
    /// cell searched and whether a closed group hides it; and the place of
    /// the cell `from`, if the rows hold it. `None` for an invalid regex.
    fn grouped_matches(
        &mut self,
        rows: &[R],
        query: &FindQuery,
        from: Option<FindMatch>,
    ) -> Option<(Vec<Found>, Option<u64>)> {
        let matcher = TextMatcher::new(&query.query)?;
        let cols: Vec<Key> = (self.source.find_columns(&query.query).into_iter())
            .map(|(_, c)| c)
            .collect();
        self.settle(rows);
        let Some(aggregates) = self.aggregates.as_ref() else {
            return Some((Vec::new(), None));
        };
        let items = aggregates.items(&mut self.layer);
        let (n, slots) = (items.len() as u64, cols.len() as u64 + 1);
        let place = |item: u64, slot: u64| match query.by {
            Axis::Rows => item * slots + slot,
            Axis::Cols => slot * n + item,
        };
        let from = from.and_then(|f| {
            let item = items.iter().position(|i| i.key == f.row)? as u64;
            let slot = match cols.iter().position(|&c| c == f.col) {
                Some(c) => c as u64 + 1,
                None => 0,
            };
            Some(place(item, slot))
        });
        let mut found = Vec::new();
        let source = &mut self.source;
        let text = self.figure_text.as_ref();
        let values = self
            .spec
            .grouping
            .as_ref()
            .map_or(&[][..], |g| &g.values[..]);
        for (i, item) in (0..).zip(&items) {
            let mut hit = |slot: u64, col: Key, t: Option<String>| {
                if t.is_some_and(|t| matcher.is_match(&t)) {
                    found.push((place(i, slot), FindMatch::new(item.key, col), item.hidden));
                }
            };
            if item.kind == ItemKind::Group
                && let Some((field, Value::Text(label))) = item.group
            {
                hit(0, field, Some(label.clone()));
            }
            for (slot, &col) in (1..).zip(&cols) {
                let t = match item.kind {
                    ItemKind::Leaf => source
                        .logical_index(Axis::Rows, item.key)
                        .and_then(|r| source.text(col, &rows[r]).map(|t| t.into_owned())),
                    _ => text.and_then(|text| {
                        let i = values.iter().position(|v| v.field == Some(col))?;
                        text(col, *item.figures.get(i)?)
                    }),
                };
                hit(slot, col, t);
            }
        }
        found.sort_by_key(|(at, ..)| *at);
        Some((found, from))
    }

    /// The match of `query` next after `from` (or before it), as
    /// [`MemorySource::find_next`] steps a plain table: on grouped rows,
    /// through every item as if every group were open, so a match a closed
    /// group hides is stepped to as well.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn find_next(
        &mut self,
        rows: &[R],
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        if !self.layer.grouped(Axis::Rows) {
            return self.source.find_next(rows, query, from, direction);
        }
        let (found, from) = self.grouped_matches(rows, query, from)?;
        let i = match (direction, from) {
            (FindDirection::Next, Some(at)) => found.iter().position(|(p, ..)| *p > at),
            (FindDirection::Next, None) => (!found.is_empty()).then_some(0),
            (FindDirection::Previous, Some(at)) => found.iter().rposition(|(p, ..)| *p < at),
            (FindDirection::Previous, None) => found.len().checked_sub(1),
        };
        let (i, wrapped) = match (i, direction) {
            (Some(i), _) => (i, false),
            (None, _) if found.is_empty() => return None,
            (None, FindDirection::Next) => (0, true),
            (None, FindDirection::Previous) => (found.len() - 1, true),
        };
        Some(FindStep {
            at: found[i].1,
            index: Some(i as u64),
            wrapped,
        })
    }

    /// How many cells match `query`, as [`MemorySource::find_count`] counts
    /// a plain table; on grouped rows, with those closed groups hide.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn find_count(&mut self, rows: &[R], query: &FindQuery) -> FindCount {
        if !self.layer.grouped(Axis::Rows) {
            return self.source.find_count(rows, query);
        }
        let found = (self.grouped_matches(rows, query, None)).map_or(Vec::new(), |(f, _)| f);
        FindCount {
            total: found.len() as u64,
            partial: false,
            hidden: found.iter().filter(|(.., hidden)| *hidden).count() as u64,
        }
    }

    /// Whether the table is a pivot now.
    pub fn pivot(&self) -> bool {
        self.layer.pivot()
    }

    /// The figure of a pivot's cell at the row `row` and the column `col`;
    /// `None` where no rows meet.
    pub fn cell(&self, row: Key, col: Key) -> Option<Figure> {
        self.layer.cell(row, col).copied()
    }

    /// This source with the host's `rows`, as [`binnacle_memory::pump`]
    /// needs it.
    pub fn over<'a>(&'a mut self, rows: &'a [R]) -> impl PumpSource + 'a {
        Over { source: self, rows }
    }
}

/// The backend for one settle: the base rows in view, grouped once per
/// stamp.
struct Backend<'a, R> {
    source: &'a MemorySource<R>,
    rows: &'a [R],
    aggregates: &'a mut Option<MemoryAggregates>,
}

impl<R> AggregateBackend<Figure> for Backend<'_, R> {
    fn answer(&mut self, batch: &Batch) -> Vec<Answer<Figure>> {
        if self.aggregates.as_ref().map(|a| a.stamp()) != Some(batch.stamp) {
            let count = self.source.count(Axis::Rows);
            let keys = self.source.window(Axis::Rows, 0..count);
            let logical = self.source.logical_window(Axis::Rows, 0..count);
            let row = |i: usize| &self.rows[logical[i]];
            *self.aggregates = Some(MemoryAggregates::new(
                batch,
                &keys,
                |field, i| self.source.value(field, row(i)),
                |field, i| self.source.number(field, row(i)),
            ));
        }
        self.aggregates
            .as_mut()
            .map_or(Vec::new(), |a| a.answer(batch))
    }
}

struct Over<'a, R> {
    source: &'a mut GroupedMemory<R>,
    rows: &'a [R],
}

impl<R> PumpSource for Over<'_, R> {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.source.view(self.rows, spec)
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.source.support(axis, key)
    }

    fn quick_filter(&mut self, axis: Axis) -> Option<TextSupport> {
        self.source.source.quick_filter(axis)
    }

    fn hide_support(&mut self, axis: Axis) -> bool {
        self.source.source.hide_support(axis)
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        Some(self.source.source.distinct(self.rows, axis, key, query))
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.source.locate(self.rows, axis, key)
    }

    fn movable(&mut self, axis: Axis) -> Option<bool> {
        Some(self.source.movable(axis))
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Result<Moved, String> {
        self.source.move_items(self.rows, axis, keys, to)
    }

    fn ancestors(&mut self, axis: Axis, key: Key) -> Vec<Key> {
        self.source.ancestors(axis, key)
    }

    fn find_support(&mut self) -> Option<TextSupport> {
        self.source.source.find_support()
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        self.source.find_next(self.rows, query, from, direction)
    }
}

impl<R> GroupedMemory<R> {
    /// The unfiltered count of each axis, as the last view gave it.
    pub fn totals(&self) -> Totals {
        let mut totals = self.source.totals();
        if self.layer.grouped(Axis::Rows) {
            totals.rows = None;
        }
        if self.layer.grouped(Axis::Cols) {
            totals.cols = None;
        }
        totals
    }
}
