//! The In-Memory Table Source under the grouping layer: an aggregate backend
//! over rows held in memory, and [`GroupedMemory`], a `MemorySource` that
//! groups.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

use binnacle_core::{
    AggregateValue, Axis, ChangeNotice, Distinct, DistinctQuery, FindDirection, FindMatch,
    FindQuery, Key, Located, Outline, Position, SortColumn, SortDirection, SortSpec, Support,
    TextSupport, Totals, Value, ViewSpec,
};
use binnacle_memory::{FindStep, MemorySource, Moved, PumpSource, Viewed};

use crate::layer::{GroupingLayer, Summary, settle};
use crate::query::{AggregateBackend, Answer, Batch, Body, GroupRow, Path, Scope, Stamp, Want};

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
    leaves: Vec<Key>,
    kids: Vec<Bucket>,
    index: HashMap<Value, usize>,
    figures: Vec<Figure>,
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
    let present = || rows.iter().map(|&i| value(field, i)).filter(|v| *v != Value::Blank);
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
    levels: usize,
    root: Bucket,
    /// Where each Leaf is: its group's path and its index there.
    places: HashMap<Key, (Path, u64)>,
}

impl MemoryAggregates {
    /// The groups of `rows`, the Leaf Keys in view order, for `batch`'s
    /// scope: `value(field, i)` is row `i`'s value in `field`, which groups
    /// it and which counts count, and `number(field, i)` its number there,
    /// which sums and averages take. The scope's filter, hidden Keys and
    /// sort are the caller's to have applied to `rows`.
    pub fn new(
        batch: &Batch,
        rows: &[Key],
        value: impl Fn(Key, usize) -> Value,
        number: impl Fn(Key, usize) -> Option<f64>,
    ) -> Self {
        let scope = &batch.scope;
        let levels = scope.levels.len();
        let mut root = Bucket::default();
        for i in 0..rows.len() {
            let mut bucket = &mut root;
            bucket.rows.push(i);
            for &field in &scope.levels {
                let v = value(field, i);
                let at = match bucket.index.get(&v) {
                    Some(&at) => at,
                    None => {
                        bucket.index.insert(v.clone(), bucket.kids.len());
                        bucket.kids.push(Bucket {
                            value: Some(v),
                            ..Bucket::default()
                        });
                        bucket.kids.len() - 1
                    }
                };
                bucket = &mut bucket.kids[at];
                bucket.rows.push(i);
            }
            bucket.leaves.push(rows[i]);
        }
        let mut aggregates = MemoryAggregates {
            stamp: batch.stamp,
            levels,
            root: Bucket::default(),
            places: HashMap::new(),
        };
        finish(&mut root, 0, scope, &value, &number);
        aggregates.root = root;
        let mut path = Vec::new();
        place(&aggregates.root, &scope.levels, &mut path, &mut aggregates.places);
        aggregates
    }

    /// The stamp of the batch these groups were worked out for.
    pub fn stamp(&self) -> Stamp {
        self.stamp
    }

    fn bucket(&self, path: &Path) -> Option<&Bucket> {
        path.iter()
            .try_fold(&self.root, |b, (_, v)| b.index.get(v).map(|&i| &b.kids[i]))
    }

    fn row(&self, bucket: &Bucket, depth: usize) -> GroupRow<Figure> {
        GroupRow {
            value: bucket.value.clone().unwrap_or(Value::Blank),
            children: match depth < self.levels {
                true => bucket.kids.len() as u64,
                false => bucket.leaves.len() as u64,
            },
            aggregates: bucket.figures.clone(),
        }
    }
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
    for kid in &mut bucket.kids {
        finish(kid, depth + 1, scope, value, number);
    }
    let Some(&field) = scope.levels.get(depth) else {
        return;
    };
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
    bucket.index = (bucket.kids.iter().enumerate())
        .map(|(i, k)| (k.value.clone().unwrap_or(Value::Blank), i))
        .collect();
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

/// A [`MemorySource`] that groups its rows under the grouping layer. While
/// the rows are grouped, the base rows are sorted by the levels' fields
/// first (in the direction the user sorts them, ascending otherwise) and
/// then by the user's sort, and the levels' fields leave the columns.
pub struct GroupedMemory<R> {
    source: MemorySource<R>,
    layer: GroupingLayer<Figure>,
    aggregates: Option<MemoryAggregates>,
    /// The fields hidden from the columns because the rows group by them.
    lifted: Vec<Key>,
}

impl<R> GroupedMemory<R> {
    pub fn new(source: MemorySource<R>) -> Self {
        GroupedMemory {
            source,
            layer: GroupingLayer::new(),
            aggregates: None,
            lifted: Vec::new(),
        }
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

    /// The rows may group by a column that sorts and has a text form.
    pub fn support(&self, axis: Axis, key: Key) -> Support {
        let support = self.source.support(axis, key);
        let group = axis == Axis::Rows && support.sort && support.text.is_some();
        support.with_group(group)
    }

    /// Presents the table as `spec` asks, grouping included; answers with
    /// the spec applied, the totals (the rows' unknown while grouped) and
    /// the Change Notices: the rows' `Refiltered` whenever their grouping
    /// or the rows under it change.
    pub fn view(&mut self, rows: &[R], spec: &ViewSpec) -> Viewed {
        let levels: Vec<Key> = (spec.grouping.iter())
            .flat_map(|g| g.row_levels.iter().filter_map(|l| l.field_key()))
            .collect();
        let levels: Vec<Key> = match levels.iter().all(|&f| self.support(Axis::Rows, f).group) {
            true => levels,
            false => Vec::new(),
        };
        let mut base = spec.clone();
        base.grouping = None;
        self.lifted = (levels.iter())
            .filter(|f| !spec.cols.hidden.contains(f))
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
        applied.grouping = match levels.is_empty() {
            true => None,
            false => spec.grouping.clone(),
        };
        let scope_before = self.layer.grouped(Axis::Rows);
        self.layer.request_view(applied.clone());
        if rows_changed && scope_before && self.layer.grouped(Axis::Rows) {
            self.layer.refresh();
        }
        self.settle(rows);
        let mut layered = Vec::new();
        while let Some(viewed) = self.layer.poll_viewed() {
            layered.extend(viewed.notices);
        }
        let grouped = self.layer.grouped(Axis::Rows);
        if grouped || was_grouped {
            notices.retain(|(a, _)| *a != Axis::Rows);
        }
        if grouped {
            notices.extend(layered.into_iter().filter(|(a, _)| *a == Axis::Rows));
        } else if was_grouped {
            let count = self.source.count(Axis::Rows);
            notices.push((Axis::Rows, ChangeNotice::Refiltered(count)));
        }
        notices.sort_by_key(|(a, _)| a.index());
        let mut totals = self.source.totals();
        if grouped {
            totals.rows = None;
        }
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
        Some(!self.source.grouped(axis) && self.source.source.movable(axis))
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Result<Moved, String> {
        if self.source.grouped(axis) {
            return Err("Can't move while grouped".into());
        }
        let before = self.source.source.view_spec();
        let Some(notice) = self.source.source.move_items(axis, keys, to) else {
            return Err("Can't move while sorted: clear the sort first".into());
        };
        let after = self.source.source.view_spec();
        Ok(Moved {
            notices: vec![(axis, notice)],
            view: (after != before).then(|| (after, self.source.source.totals())),
        })
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
        self.source
            .source
            .find_next(self.rows, query, from, direction)
    }
}

impl<R> GroupedMemory<R> {
    /// The unfiltered count of each axis, as the last view gave it.
    pub fn totals(&self) -> Totals {
        let mut totals = self.source.totals();
        if self.layer.grouped(Axis::Rows) {
            totals.rows = None;
        }
        totals
    }
}
