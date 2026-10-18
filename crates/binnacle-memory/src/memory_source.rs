use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Range;

use binnacle_core::{
    Axis, Blanks, ChangeNotice, Condition, ConditionKind, Distinct, DistinctQuery, DistinctValue,
    FilterSpec, FindDirection, FindMatch, FindQuery, Grid, Key, Located, Position, Searched,
    SortDirection, SortSpec, Stamped, Support, TextQuery, TextSupport, Totals, Value, ViewSpec,
};
use regex::{Regex, RegexBuilder};

use crate::{FindCount, FindStep, InMemoryTableSource, PumpSource, SortKeys, Viewed};

/// A Sort Spec named a column with no sort-key closure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unsortable {
    pub column: Key,
}

impl fmt::Display for Unsortable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "column {:?} has no sort keys", self.column)
    }
}

impl std::error::Error for Unsortable {}

type Extract<R, T> = Box<dyn Fn(&R) -> Option<T>>;
type Keep<R> = Box<dyn Fn(&R) -> bool>;
type ExtractBytes<R> = Box<dyn for<'r> Fn(&'r R) -> Option<Cow<'r, [u8]>>>;
type ExtractText<R> = Box<dyn for<'r> Fn(&'r R) -> Option<Cow<'r, str>>>;

enum Column<R> {
    I64(Extract<R, i64>),
    F64(Extract<R, f64>),
    Bytes(ExtractBytes<R>),
}

impl<R> Column<R> {
    fn extract<'r>(&self, rows: &'r [R]) -> SortKeys<'r> {
        match self {
            Column::I64(f) => SortKeys::I64(rows.iter().map(f).collect()),
            Column::F64(f) => SortKeys::F64(rows.iter().map(f).collect()),
            Column::Bytes(f) => SortKeys::Bytes(rows.iter().map(f).collect()),
        }
    }

    /// The sort key of `row` written with `Display` (bytes as UTF-8, lossily).
    fn display<'r>(&self, row: &'r R) -> Option<Cow<'r, str>> {
        match self {
            Column::I64(f) => f(row).map(|v| v.to_string().into()),
            Column::F64(f) => f(row).map(|v| v.to_string().into()),
            Column::Bytes(f) => f(row).map(|v| match v {
                Cow::Borrowed(b) => String::from_utf8_lossy(b),
                Cow::Owned(b) => String::from_utf8_lossy(&b).into_owned().into(),
            }),
        }
    }
}

/// How a Text Query matches a value.
enum Matcher {
    Contains(String),
    Pattern(Regex),
}

impl Matcher {
    /// Plain text matched case-sensitively is a substring search; anything
    /// else is a regex, the text escaped unless it is one. `None` for an
    /// invalid regex.
    fn new(query: &TextQuery) -> Option<Matcher> {
        if query.case_sensitive && !query.regex {
            return Some(Matcher::Contains(query.text.clone()));
        }
        let pattern = if query.regex {
            Cow::Borrowed(query.text.as_str())
        } else {
            Cow::Owned(regex::escape(&query.text))
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!query.case_sensitive)
            .build()
            .ok()?;
        Some(Matcher::Pattern(regex))
    }

    fn is_match(&self, text: &str) -> bool {
        match self {
            Matcher::Contains(needle) => text.contains(needle.as_str()),
            Matcher::Pattern(regex) => regex.is_match(text),
        }
    }
}

/// The quick filter in force: its query, the columns it searches that have
/// a text form, and its matcher.
struct Quick {
    query: TextQuery,
    columns: Vec<Key>,
    matcher: Matcher,
}

/// How a column condition tests a value.
enum Test {
    Text(Matcher),
    /// The texts listed, and whether blanks are.
    OneOf(HashSet<String>, bool),
}

/// A column condition in force and its test.
struct Kept {
    condition: Condition,
    test: Test,
}

impl Kept {
    /// `condition` ready to test, or `None` for an invalid regex.
    fn new(condition: &Condition) -> Option<Kept> {
        let test = match &condition.kind {
            ConditionKind::Text(query) => Test::Text(Matcher::new(query)?),
            ConditionKind::OneOf(values) => {
                let texts = (values.iter())
                    .filter_map(|v| match v {
                        Value::Text(t) => Some(t.clone()),
                        Value::Blank => None,
                    })
                    .collect();
                Test::OneOf(texts, values.contains(&Value::Blank))
            }
            _ => return None,
        };
        Some(Kept {
            condition: condition.clone(),
            test,
        })
    }

    /// Whether the condition keeps a row whose text in its column is `text`.
    fn keeps(&self, text: Option<&str>) -> bool {
        let matched = match &self.test {
            Test::Text(matcher) => text.is_some_and(|t| matcher.is_match(t)),
            Test::OneOf(texts, blanks) => text.map_or(*blanks, |t| texts.contains(t)),
        };
        matched != self.condition.not
    }
}

/// An In-Memory Table Source for rows of type `R` in a host's slice, with a
/// closure per sortable column extracting its sort key from a row. It never
/// holds the rows: each sort borrows them.
///
/// A column's text form, which a quick filter and its Text conditions match,
/// is its text closure ([`MemorySource::text_column`]) or else its sort key
/// written with `Display`. A column with neither never matches, nor does a
/// blank (`None`), so a `not` condition keeps it. Regexes are the `regex`
/// crate's.
pub struct MemorySource<R> {
    table: InMemoryTableSource,
    columns: HashMap<Key, Column<R>>,
    texts: HashMap<Key, ExtractText<R>>,
    /// The host's own filter in force, kept to place rows the base data gains.
    keep: Option<Keep<R>>,
    /// The quick filter in force, ANDed with `keep`.
    quick: Option<Quick>,
    /// The column conditions in force, ANDed with the quick filter.
    conditions: Vec<Kept>,
}

impl<R> MemorySource<R> {
    /// A source over `rows` base rows and `cols` base columns, none of them
    /// sortable until given a closure.
    pub fn new(rows: usize, cols: usize) -> Self {
        MemorySource {
            table: InMemoryTableSource::new(rows, cols),
            columns: HashMap::new(),
            texts: HashMap::new(),
            keep: None,
            quick: None,
            conditions: Vec::new(),
        }
    }

    /// See [`InMemoryTableSource::with_keys`].
    pub fn with_keys(mut self, axis: Axis, keys: Vec<Key>) -> Self {
        self.table = self.table.with_keys(axis, keys);
        self
    }

    /// Makes the column with Key `column` sort by an integer; `None` is null.
    pub fn i64_column(mut self, column: Key, f: impl Fn(&R) -> Option<i64> + 'static) -> Self {
        self.columns.insert(column, Column::I64(Box::new(f)));
        self
    }

    /// Makes the column with Key `column` sort by a float; `None` is null.
    pub fn f64_column(mut self, column: Key, f: impl Fn(&R) -> Option<f64> + 'static) -> Self {
        self.columns.insert(column, Column::F64(Box::new(f)));
        self
    }

    /// Makes the column with Key `column` sort by a byte string, borrowed from
    /// the row or computed; `None` is null.
    pub fn bytes_column(
        mut self,
        column: Key,
        f: impl for<'r> Fn(&'r R) -> Option<Cow<'r, [u8]>> + 'static,
    ) -> Self {
        self.columns.insert(column, Column::Bytes(Box::new(f)));
        self
    }

    /// Gives the column with Key `column` the text form `f`, which a quick
    /// filter matches; `None` is a blank, which never matches.
    pub fn text_column(
        mut self,
        column: Key,
        f: impl for<'r> Fn(&'r R) -> Option<Cow<'r, str>> + 'static,
    ) -> Self {
        self.texts.insert(column, Box::new(f));
        self
    }

    /// Column `column`'s text form of `row`, if it has one and `row` is not
    /// blank there.
    fn text<'r>(&self, column: Key, row: &'r R) -> Option<Cow<'r, str>> {
        match self.texts.get(&column) {
            Some(f) => f(row),
            None => self.columns.get(&column)?.display(row),
        }
    }

    /// Presents the rows as `spec`'s rows part asks, extracting each sort
    /// column's keys and each searched column's text from `rows`, and returns
    /// the Change Notice the rows need: `Refiltered` if the filter changed.
    /// If a sort column has no closure, nothing changes. A quick filter with
    /// an invalid regex is not applied: the one in force stays. A condition
    /// with an invalid regex, or of a kind this source does not apply, is
    /// left out.
    /// The columns part is not applied: [`MemorySource::view_spec`] leaves
    /// out what is not.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn view(&mut self, rows: &[R], spec: &ViewSpec) -> Result<ChangeNotice, Unsortable> {
        let sorted = self.sort(rows, &spec.rows.sort)?;
        let filter = &spec.rows.filter;
        let query = filter.quick.as_ref();
        let mut changed = false;
        if query != self.quick.as_ref().map(|q| &q.query) {
            let quick = query.map(|q| self.quick(q));
            if let Some(None) = quick {
                // An invalid regex: the quick filter in force stays.
            } else {
                self.quick = quick.flatten();
                changed = true;
            }
        }
        let wanted = filter.conditions.conditions();
        if !(wanted.iter()).eq(self.conditions.iter().map(|k| &k.condition)) {
            let kept: Vec<Kept> = wanted.iter().filter_map(Kept::new).collect();
            changed |= !(kept.iter().map(|k| &k.condition))
                .eq(self.conditions.iter().map(|k| &k.condition));
            self.conditions = kept;
        }
        Ok(if changed { self.refilter(rows) } else { sorted })
    }

    /// `query` ready to match, or `None` for an invalid regex.
    fn quick(&self, query: &TextQuery) -> Option<Quick> {
        let columns = match &query.searches {
            Searched::Shown => self.window(Axis::Cols, 0..self.count(Axis::Cols)),
            Searched::Keys(keys) => keys.clone(),
        };
        let columns = (columns.into_iter())
            .filter(|c| self.texts.contains_key(c) || self.columns.contains_key(c))
            .collect();
        Some(Quick {
            query: query.clone(),
            columns,
            matcher: Matcher::new(query)?,
        })
    }

    fn sort(&mut self, rows: &[R], spec: &SortSpec) -> Result<ChangeNotice, Unsortable> {
        let columns = spec
            .columns()
            .iter()
            .map(|c| self.columns.get(&c.key).ok_or(Unsortable { column: c.key }))
            .collect::<Result<Vec<_>, _>>()?;
        let keys = columns.into_iter().map(|c| c.extract(rows)).collect();
        Ok(self.table.apply_sort(spec, keys))
    }

    /// Whether a filter is in force: the host's, the quick filter or a
    /// condition.
    fn filtered(&self) -> bool {
        self.keep.is_some() || self.quick.is_some() || !self.conditions.is_empty()
    }

    /// Whether the filters in force keep `row`.
    fn keeps(&self, row: &R) -> bool {
        self.keep.as_ref().is_none_or(|keep| keep(row))
            && self.passes(self.quick.as_ref(), &self.conditions, row)
    }

    /// Whether `quick` and every one of `conditions` keep `row`.
    fn passes(&self, quick: Option<&Quick>, conditions: &[Kept], row: &R) -> bool {
        quick.is_none_or(|q| {
            (q.columns.iter()).any(|&c| self.text(c, row).is_some_and(|t| q.matcher.is_match(&t)))
        }) && (conditions.iter()).all(|k| k.keeps(self.text(k.condition.key, row).as_deref()))
    }

    /// The distinct values of column `column` in `rows` as `query` asks:
    /// those containing its search (ignoring case, as a Text Query would),
    /// in rows the host's filter and `query.narrow` keep, with how many rows
    /// hold each and how many are blank, in the column's sort-key order
    /// (ties, and columns with only a text closure, by text), up to its
    /// limit. A part of `narrow` with an invalid regex is left out.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn distinct(&self, rows: &[R], column: Key, query: &DistinctQuery) -> Distinct {
        let quick = (query.narrow.quick.as_ref()).and_then(|q| self.quick(q));
        let conditions: Vec<Kept> = (query.narrow.conditions.conditions().iter())
            .filter_map(Kept::new)
            .collect();
        let search = (!query.search.is_empty())
            .then(|| Matcher::new(&TextQuery::new(query.search.clone())))
            .flatten();
        let keys = self.columns.get(&column).map(|c| c.extract(rows));
        let mut blanks = 0;
        // Each value's count and the row with its least sort key.
        let mut seen: HashMap<Cow<'_, str>, (u64, usize)> = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            let kept = self.keep.as_ref().is_none_or(|keep| keep(row))
                && self.passes(quick.as_ref(), &conditions, row);
            if !kept {
                continue;
            }
            let Some(text) = self.text(column, row) else {
                blanks += 1;
                continue;
            };
            if search.as_ref().is_some_and(|m| !m.is_match(&text)) {
                continue;
            }
            let entry = seen.entry(text).or_insert((0, i));
            entry.0 += 1;
            let less = |k: &SortKeys| k.compare(i, entry.1, SortDirection::Ascending).is_lt();
            if keys.as_ref().is_some_and(less) {
                entry.1 = i;
            }
        }
        let mut values: Vec<(Cow<'_, str>, (u64, usize))> = seen.into_iter().collect();
        values.sort_by(|(a, (_, i)), (b, (_, j))| {
            let by_key = keys.as_ref().map_or(Ordering::Equal, |k| {
                k.compare(*i, *j, SortDirection::Ascending)
            });
            by_key.then_with(|| a.cmp(b))
        });
        let partial = values.len() > query.limit;
        values.truncate(query.limit);
        let listed = query.search.is_empty() && blanks > 0;
        Distinct {
            values: (values.into_iter())
                .map(|(text, (count, _))| DistinctValue {
                    text: text.into_owned(),
                    count: Some(count),
                })
                .collect(),
            blanks: listed.then_some(Blanks::Counted(blanks)),
            partial,
            narrowed: true,
        }
    }

    /// Applies the filters in force to `rows` again, in the sort in force;
    /// returns the `Refiltered` the rows need.
    fn refilter(&mut self, rows: &[R]) -> ChangeNotice {
        if self.filtered() {
            let keep = rows.iter().map(|r| self.keeps(r)).collect();
            self.table.apply_filter(keep)
        } else {
            self.table.clear_filter()
        }
    }

    /// Keeps only the rows for which `keep` is true, and the quick filter
    /// in force, in sort order, and returns the `Refiltered` the rows need.
    /// The sort is not redone. The filter stays in force for rows the base
    /// data gains or changes.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn filter(&mut self, rows: &[R], keep: impl Fn(&R) -> bool + 'static) -> ChangeNotice {
        self.keep = Some(Box::new(keep));
        self.refilter(rows)
    }

    /// Removes the filter [`MemorySource::filter`] set, keeping the quick
    /// filter, and returns the `Refiltered` the rows need.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn clear_filter(&mut self, rows: &[R]) -> ChangeNotice {
        self.keep = None;
        self.refilter(rows)
    }

    /// Rows were appended to the host's base data, now `rows`, keyed by
    /// `keys` if the rows have host Keys (empty otherwise). They take their
    /// place in the sort in force and the filter in force drops those it
    /// rejects. See [`InMemoryTableSource::append_rows`] for the notice.
    ///
    /// # Panics
    ///
    /// If `rows` is shorter than before, or as [`InMemoryTableSource::append_rows`].
    pub fn appended(&mut self, rows: &[R], keys: &[Key]) -> ChangeNotice {
        let old = self.table.base_rows();
        let n = rows
            .len()
            .checked_sub(old)
            .expect("rows were appended, not removed");
        let keep: Vec<bool> = self.keep_flags(&rows[old..]);
        let sort_keys = self.sort_keys(rows);
        self.table.append_rows(n, keys, sort_keys, &keep)
    }

    /// The value in `column` of base row `row` (a Logical Index into `rows`)
    /// changed. Returns `ContentChanged` for the row's Position, `Reordered`
    /// if the rows are sorted by that column, `Refiltered` if the filter now keeps
    /// or drops the row where it did not before, or `None` if the row is
    /// filtered out either way.
    ///
    /// # Panics
    ///
    /// If `row` is not a row of `rows`, or `rows` is not as long as the base
    /// row count.
    pub fn edited(&mut self, rows: &[R], row: usize, column: Key) -> Option<ChangeNotice> {
        if self.filtered() && self.keeps(&rows[row]) != self.table.row_position(row).is_some() {
            return Some(self.reorder(rows));
        }
        if self.sort_spec().find(column).is_some() {
            self.resort(rows);
            return Some(ChangeNotice::Reordered);
        }
        let p = self.table.row_position(row)?;
        Some(ChangeNotice::ContentChanged(p..p + 1))
    }

    /// The host's base data changed in some other way (rows inserted or
    /// removed anywhere, or replaced): it is now `rows`, keyed by `keys` (by
    /// Logical Index if `None`). The sort and filter in force are applied
    /// again and the rows need the `Reset` returned.
    ///
    /// # Panics
    ///
    /// As [`InMemoryTableSource::replace_rows`].
    pub fn replaced(&mut self, rows: &[R], keys: Option<Vec<Key>>) -> ChangeNotice {
        let keep = self.keep_flags(rows);
        let sort_keys = self.sort_keys(rows);
        self.table.replace_rows(rows.len(), keys, sort_keys, keep)
    }

    /// The keep flags of `rows` while a filter is in force, else none.
    fn keep_flags(&self, rows: &[R]) -> Vec<bool> {
        if self.filtered() {
            rows.iter().map(|r| self.keeps(r)).collect()
        } else {
            Vec::new()
        }
    }

    /// The sort keys of `rows` for the Sort Spec in force.
    fn sort_keys<'r>(&self, rows: &'r [R]) -> Vec<SortKeys<'r>> {
        // Every spec column had a closure when it was applied.
        self.sort_spec()
            .columns()
            .iter()
            .map(|c| self.columns[&c.key].extract(rows))
            .collect()
    }

    /// Sorts `rows` again by the Sort Spec in force.
    fn resort(&mut self, rows: &[R]) {
        let spec = self.sort_spec().clone();
        let _ = self.sort(rows, &spec);
    }

    /// Applies the sort and filter in force to `rows` again; returns the
    /// Change Notice the rows need (`Refiltered` while a filter is in force).
    fn reorder(&mut self, rows: &[R]) -> ChangeNotice {
        if !self.sort_spec().is_empty() {
            self.resort(rows);
        }
        if self.filtered() {
            self.refilter(rows)
        } else {
            ChangeNotice::Reset(self.count(Axis::Rows))
        }
    }

    /// The Sort Spec the rows are in, as last applied.
    fn sort_spec(&self) -> &SortSpec {
        self.table.sort_spec()
    }

    /// The View Spec the source applies: the rows' sort, quick filter and
    /// conditions, as last applied.
    pub fn view_spec(&self) -> ViewSpec {
        let quick = self.quick.as_ref().map(|q| q.query.clone());
        let conditions: Vec<Condition> = self
            .conditions
            .iter()
            .map(|k| k.condition.clone())
            .collect();
        let filter = FilterSpec::default()
            .with_quick(quick)
            .with_conditions(conditions.into());
        ViewSpec::default()
            .with_sort(Axis::Rows, self.sort_spec().clone())
            .with_filter(Axis::Rows, filter)
    }

    /// The unfiltered count of each axis: every base row and column.
    pub fn totals(&self) -> Totals {
        Totals {
            rows: Some(self.table.base_rows() as u64),
            cols: Some(self.count(Axis::Cols)),
        }
    }

    /// The rows sort by the columns given a closure, and filter by Text
    /// conditions, regex included, and OneOf conditions on the columns with
    /// a text form; columns sort and filter by nothing.
    pub fn support(&self, axis: Axis, key: Key) -> Support {
        let rows = axis == Axis::Rows;
        let text = rows && (self.texts.contains_key(&key) || self.columns.contains_key(&key));
        Support::default()
            .with_sort(rows && self.columns.contains_key(&key))
            .with_text(text.then_some(TextSupport::new(true)))
            .with_one_of(text)
    }

    /// The rows have a quick filter, regex included; the columns have none.
    pub fn quick_filter(&self, axis: Axis) -> Option<TextSupport> {
        (axis == Axis::Rows).then_some(TextSupport::new(true))
    }

    /// The source finds, regex included.
    pub fn find_support(&self) -> Option<TextSupport> {
        Some(TextSupport::new(true))
    }

    /// The columns `query` searches that have a text form, as (Position,
    /// Key) in column order.
    fn searched(&self, query: &TextQuery) -> Vec<(Position, Key)> {
        let shown = self.window(Axis::Cols, 0..self.count(Axis::Cols));
        (0..)
            .zip(shown)
            .filter(|(_, c)| match &query.searches {
                Searched::Shown => true,
                Searched::Keys(keys) => keys.contains(c),
            })
            .filter(|(_, c)| self.texts.contains_key(c) || self.columns.contains_key(c))
            .collect()
    }

    /// Whether the cell of base row `row` in column `col` matches.
    fn found(&self, matcher: &Matcher, row: &R, col: Key) -> bool {
        self.text(col, row).is_some_and(|t| matcher.is_match(&t))
    }

    /// Which cells of the block `row_keys` by `col_keys` match `query`, row
    /// by row: what find marks. A cell in a column the query does not search,
    /// or of a row or column the source does not have, is not marked; nor is
    /// any cell for an invalid regex.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn find_marks(
        &mut self,
        rows: &[R],
        row_keys: &[Key],
        col_keys: &[Key],
        query: &FindQuery,
    ) -> Vec<bool> {
        let mut marks = vec![false; row_keys.len() * col_keys.len()];
        let Some(matcher) = Matcher::new(&query.query) else {
            return marks;
        };
        let searched: Vec<Key> = self
            .searched(&query.query)
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        for (i, &r) in row_keys.iter().enumerate() {
            let Some(row) = self.table.logical_index(Axis::Rows, r) else {
                continue;
            };
            for (j, c) in col_keys.iter().enumerate() {
                marks[i * col_keys.len() + j] =
                    searched.contains(c) && self.found(&matcher, &rows[row], *c);
            }
        }
        marks
    }

    /// The match of `query` next after `from` (or before it, for
    /// [`FindDirection::Previous`]) in the view's order: by rows, row
    /// Positions then the searched columns in column order; by columns, the
    /// transpose. Past the last match it wraps to the first. A `from` the
    /// view does not show, or `None`, steps from before the start (after
    /// the end, going back). The answer carries its index among every match.
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
        let order = self.find_order(query)?;
        let n = order.len();
        let from = from.and_then(|m| {
            let mut at = |axis| match self.table.locate(axis, m.key(axis)) {
                Located::Found(p) => Some(p),
                Located::Gone { .. } => None,
            };
            Some(order.ordinal(at(Axis::Rows)?, at(Axis::Cols)?))
        });
        let hit = |i: u64| self.found(&order.matcher, &rows[order.row(i)], order.col(i).1);
        // Where the scan starts, and the order it visits every cell in.
        let found = match direction {
            FindDirection::Next => {
                let start = from.map_or(0, |(_, after)| after);
                (start..n)
                    .chain(0..start)
                    .find(|&i| hit(i))
                    .map(|i| (i, i < start))
            }
            FindDirection::Previous => {
                let end = from.map_or(n, |(before, _)| before);
                (0..end)
                    .rev()
                    .chain((end..n).rev())
                    .find(|&i| hit(i))
                    .map(|i| (i, i >= end))
            }
        };
        let (i, wrapped) = found?;
        let index = (0..i).filter(|&j| hit(j)).count() as u64;
        Some(FindStep {
            at: FindMatch::new(order.row_key(i), order.col(i).1),
            index: Some(index),
            wrapped,
        })
    }

    /// How many cells match `query`, all counted.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn find_count(&self, rows: &[R], query: &FindQuery) -> FindCount {
        let total = self.find_order(query).map_or(0, |order| {
            (0..order.len())
                .filter(|&i| self.found(&order.matcher, &rows[order.row(i)], order.col(i).1))
                .count() as u64
        });
        FindCount {
            total,
            partial: false,
        }
    }

    /// The cells `query` searches, in its order, and its matcher; `None` for
    /// an invalid regex.
    fn find_order(&self, query: &FindQuery) -> Option<FindOrder> {
        let rows = self.count(Axis::Rows);
        Some(FindOrder {
            matcher: Matcher::new(&query.query)?,
            logical: self.table.logical_window(Axis::Rows, 0..rows),
            row_keys: self.window(Axis::Rows, 0..rows),
            cols: self.searched(&query.query),
            by: query.by,
        })
    }

    /// Runs [`crate::pump`] with this source over the host's `rows`: View
    /// Specs are answered by sorting them (or rejected if a column has no
    /// closure), `Locate` by the Remap, column moves by moving the columns.
    /// `seen` gets every Effect drained.
    pub fn pump(&mut self, rows: &[R], grid: &mut Grid, seen: impl FnMut(&mut Grid, Stamped)) {
        crate::pump(grid, &mut self.over(rows), seen);
    }

    /// This source with the host's `rows`, as [`crate::pump`] needs it: the
    /// answers [`MemorySource::pump`] gives, for hosts that drive the pump
    /// themselves.
    pub fn over<'a>(&'a mut self, rows: &'a [R]) -> impl PumpSource + 'a {
        Over { source: self, rows }
    }

    /// See [`InMemoryTableSource::move_columns`]. Sorting names columns by
    /// Key, so it is unaffected by the column order.
    pub fn move_columns(&mut self, keys: &[Key], to: Position) -> ChangeNotice {
        self.table.move_columns(keys, to)
    }

    pub fn count(&self, axis: Axis) -> u64 {
        self.table.count(axis)
    }

    pub fn window(&self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.table.window(axis, range)
    }

    pub fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.table.locate(axis, key)
    }
}

/// Every cell a Find Query searches, numbered in its order: by rows, cell
/// `i` is in row `i / c` and searched column `i % c` (`c` searched
/// columns); by columns, the transpose.
struct FindOrder {
    matcher: Matcher,
    /// The Logical Index and Key of the row at each Position.
    logical: Vec<usize>,
    row_keys: Vec<Key>,
    /// The searched columns' Positions and Keys, in column order.
    cols: Vec<(Position, Key)>,
    by: Axis,
}

impl FindOrder {
    fn len(&self) -> u64 {
        (self.logical.len() * self.cols.len()) as u64
    }

    /// The row and searched-column indices of cell `i`.
    fn place(&self, i: u64) -> (usize, usize) {
        let i = i as usize;
        match self.by {
            Axis::Rows => (i / self.cols.len(), i % self.cols.len()),
            Axis::Cols => (i % self.logical.len(), i / self.logical.len()),
        }
    }

    /// The Logical Index of cell `i`'s row.
    fn row(&self, i: u64) -> usize {
        self.logical[self.place(i).0]
    }

    fn row_key(&self, i: u64) -> Key {
        self.row_keys[self.place(i).0]
    }

    fn col(&self, i: u64) -> (Position, Key) {
        self.cols[self.place(i).1]
    }

    /// Where the cell at row Position `row` and column Position `col` falls
    /// among the searched cells, which it need not be one of: how many
    /// come before it, and how many come before it or are it.
    fn ordinal(&self, row: Position, col: Position) -> (u64, u64) {
        let (rows, cols) = (self.logical.len() as u64, self.cols.len() as u64);
        let row = row.min(rows);
        let before = self.cols.partition_point(|&(p, _)| p < col) as u64;
        let through = self.cols.partition_point(|&(p, _)| p <= col) as u64;
        match self.by {
            Axis::Rows => (row * cols + before, row * cols + through),
            // Rows are numbered from 0 with none left out, so `row` is a count.
            Axis::Cols if through > before => (before * rows + row, before * rows + row + 1),
            Axis::Cols => (before * rows, before * rows),
        }
    }
}

/// A [`MemorySource`] with the rows it sorts, as the pump needs it.
struct Over<'a, R> {
    source: &'a mut MemorySource<R>,
    rows: &'a [R],
}

impl<R> PumpSource for Over<'_, R> {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        let notice = self.source.view(self.rows, spec).ok();
        Viewed {
            applied: self.source.view_spec(),
            totals: self.source.totals(),
            notices: notice.map(|n| (Axis::Rows, n)).into_iter().collect(),
        }
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.source.support(axis, key)
    }

    fn quick_filter(&mut self, axis: Axis) -> Option<TextSupport> {
        self.source.quick_filter(axis)
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        (axis == Axis::Rows).then(|| self.source.distinct(self.rows, key, query))
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.source.locate(axis, key)
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        (axis == Axis::Cols).then(|| self.source.move_columns(keys, to))
    }

    fn find_support(&mut self) -> Option<TextSupport> {
        self.source.find_support()
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
