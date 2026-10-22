use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Range;

use binnacle_core::{
    Axis, Blanks, ChangeNotice, Compared, Comparison, Condition, ConditionKind, Distinct,
    DistinctQuery, DistinctValue, FilterSpec, FindDirection, FindMatch, FindQuery, Grid, Key,
    Located, Position, Scalar, Searched, SortDirection, SortSpec, Stamped, Support, TextQuery,
    TextSupport, Totals, Value, ViewSpec,
};
use regex::{Regex, RegexBuilder};

use crate::{
    FindCount, FindStep, InMemoryTableSource, MovePolicy, Moved, PumpSource, SortKeys, Viewed,
};

/// A Sort Spec named a column the table has but no sort-key closure for;
/// `column` is the Key it named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unsortable {
    pub column: Key,
}

impl fmt::Display for Unsortable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "item {:?} has no sort keys", self.column)
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
    /// Milliseconds since the Unix epoch, sorting as integers.
    Date(Extract<R, i64>),
    Bytes(ExtractBytes<R>),
}

impl<R> Column<R> {
    fn extract<'r>(&self, rows: &'r [R]) -> SortKeys<'r> {
        match self {
            Column::I64(f) | Column::Date(f) => SortKeys::I64(rows.iter().map(f).collect()),
            Column::F64(f) => SortKeys::F64(rows.iter().map(f).collect()),
            Column::Bytes(f) => SortKeys::Bytes(rows.iter().map(f).collect()),
        }
    }

    /// The value of `row` comparison conditions compare: `None` for a blank
    /// or a byte string.
    fn scalar(&self, row: &R) -> Option<Scalar> {
        match self {
            Column::I64(f) => f(row).map(|v| Scalar::Number(v as f64)),
            Column::F64(f) => f(row).map(Scalar::Number),
            Column::Date(f) => f(row).map(Scalar::Date),
            Column::Bytes(_) => None,
        }
    }

    /// The kind of value comparison conditions on the column compare.
    fn compared(&self) -> Option<Compared> {
        match self {
            Column::I64(_) | Column::F64(_) => Some(Compared::Numbers),
            Column::Date(_) => Some(Compared::Dates),
            Column::Bytes(_) => None,
        }
    }

    /// The sort key of `row` written with `Display` (bytes as UTF-8, lossily).
    fn display<'r>(&self, row: &'r R) -> Option<Cow<'r, str>> {
        match self {
            Column::I64(f) => f(row).map(|v| v.to_string().into()),
            Column::F64(f) => f(row).map(|v| v.to_string().into()),
            Column::Date(f) => f(row).map(|v| Scalar::Date(v).to_string().into()),
            Column::Bytes(f) => f(row).map(|v| match v {
                Cow::Borrowed(b) => String::from_utf8_lossy(b),
                Cow::Owned(b) => String::from_utf8_lossy(&b).into_owned().into(),
            }),
        }
    }
}

/// Why a [`MemorySource`] can't apply or find `query`: the last line of the
/// `regex` crate's message for an invalid pattern, such as "unclosed group",
/// or `None` if it can. A host shows it on the control the query came from,
/// as the Table Source's error report.
pub fn query_error(query: &TextQuery) -> Option<String> {
    let error = Matcher::compile(query).err()?.to_string();
    let last = error.lines().last().unwrap_or_default().trim();
    Some(last.strip_prefix("error: ").unwrap_or(last).to_string())
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
        Matcher::compile(query).ok()
    }

    fn compile(query: &TextQuery) -> Result<Matcher, regex::Error> {
        if query.case_sensitive && !query.regex {
            return Ok(Matcher::Contains(query.text.clone()));
        }
        let pattern = if query.regex {
            Cow::Borrowed(query.text.as_str())
        } else {
            Cow::Owned(regex::escape(&query.text))
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!query.case_sensitive)
            .build()?;
        Ok(Matcher::Pattern(regex))
    }

    fn is_match(&self, text: &str) -> bool {
        match self {
            Matcher::Contains(needle) => text.contains(needle.as_str()),
            Matcher::Pattern(regex) => regex.is_match(text),
        }
    }
}

/// The kind of value `comparison` compares with: that of its scalars, or
/// none for a between mixing numbers and dates.
fn compared(comparison: &Comparison) -> Option<Compared> {
    let kind = |s: &Scalar| match s {
        Scalar::Number(_) => Compared::Numbers,
        Scalar::Date(_) => Compared::Dates,
    };
    match comparison {
        Comparison::Between(low, high) => (kind(low) == kind(high)).then(|| kind(low)),
        Comparison::Equal(s)
        | Comparison::NotEqual(s)
        | Comparison::Less(s)
        | Comparison::AtMost(s)
        | Comparison::Greater(s)
        | Comparison::AtLeast(s) => Some(kind(s)),
    }
}

/// A quick filter in force: its query and its matcher.
struct Quick {
    query: TextQuery,
    matcher: Matcher,
}

/// How a column condition tests a value.
enum Test {
    Text(Matcher),
    /// The texts listed, and whether blanks are.
    OneOf(HashSet<String>, bool),
    Compare(Comparison),
}

/// A condition in force and its test.
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
            ConditionKind::Compare(comparison) => Test::Compare(*comparison),
            _ => return None,
        };
        Some(Kept {
            condition: condition.clone(),
            test,
        })
    }

    /// Whether the condition keeps an item whose cell where it meets the
    /// condition's Key is `cell`.
    fn keeps<R>(&self, cell: Cell<'_, R>) -> bool {
        let matched = match &self.test {
            Test::Text(matcher) => cell.text().is_some_and(|t| matcher.is_match(&t)),
            Test::OneOf(texts, blanks) => cell.text().map_or(*blanks, |t| texts.contains(&*t)),
            Test::Compare(comparison) => comparison.passes(cell.scalar()),
        };
        matched != self.condition.not
    }
}

/// Where an item meets a condition's item: a column of a row, or no row.
struct Cell<'a, R> {
    source: &'a MemorySource<R>,
    column: Key,
    row: Option<&'a R>,
}

impl<'a, R> Cell<'a, R> {
    fn text(&self) -> Option<Cow<'a, str>> {
        self.source.text(self.column, self.row?)
    }

    fn scalar(&self) -> Option<Scalar> {
        self.source.columns.get(&self.column)?.scalar(self.row?)
    }
}

/// One axis's filter in force: its quick filter and its conditions.
#[derive(Default)]
struct Filter {
    quick: Option<Quick>,
    conditions: Vec<Kept>,
}

impl Filter {
    /// The parts of `wanted` this source applies: a quick filter or
    /// condition with an invalid regex, or of a kind it does not apply, is
    /// left out.
    fn new(wanted: &FilterSpec) -> Filter {
        let mut filter = Filter::default();
        filter.take(wanted);
        filter
    }

    /// Takes what this source applies of `wanted`: a quick filter with an
    /// invalid regex leaves the one in force, a condition with one (or of a
    /// kind it does not apply) is left out. Returns whether the filter
    /// changed.
    fn take(&mut self, wanted: &FilterSpec) -> bool {
        let mut changed = false;
        let query = wanted.quick.as_ref();
        if query != self.quick.as_ref().map(|q| &q.query) {
            let quick = query.map(|q| {
                Matcher::new(q).map(|matcher| Quick {
                    query: q.clone(),
                    matcher,
                })
            });
            if let Some(None) = quick {
                // An invalid regex: the quick filter in force stays.
            } else {
                self.quick = quick.flatten();
                changed = true;
            }
        }
        let wanted = wanted.conditions.conditions();
        if !(wanted.iter()).eq(self.conditions.iter().map(|k| &k.condition)) {
            let kept: Vec<Kept> = wanted.iter().filter_map(Kept::new).collect();
            changed |= !(kept.iter().map(|k| &k.condition))
                .eq(self.conditions.iter().map(|k| &k.condition));
            self.conditions = kept;
        }
        changed
    }

    fn is_some(&self) -> bool {
        self.quick.is_some() || !self.conditions.is_empty()
    }

    /// Whether its quick filter searches the shown items of the other axis,
    /// so hiding them changes what it keeps.
    fn searches_shown(&self) -> bool {
        (self.quick.as_ref()).is_some_and(|q| q.query.searches == Searched::Shown)
    }

    fn spec(&self) -> FilterSpec {
        let quick = self.quick.as_ref().map(|q| q.query.clone());
        let conditions: Vec<Condition> = (self.conditions.iter())
            .map(|k| k.condition.clone())
            .collect();
        FilterSpec::default()
            .with_quick(quick)
            .with_conditions(conditions.into())
    }
}

/// A filter on the columns read against the rows: the rows its quick
/// filter searches and the row each condition reads, by Logical Index.
struct RowsRead {
    searched: Vec<usize>,
    reads: Vec<Option<usize>>,
}

impl RowsRead {
    /// What `filter` reads of `rows` base rows in `table`: a quick filter
    /// searching the shown rows searches every row not hidden, filtered out
    /// or not.
    fn new(table: &mut InMemoryTableSource, filter: &Filter, rows: usize) -> RowsRead {
        let searched = match filter.quick.as_ref().map(|q| &q.query.searches) {
            None => Vec::new(),
            Some(Searched::Shown) => (0..rows)
                .filter(|&r| !table.is_hidden(Axis::Rows, r))
                .collect(),
            Some(Searched::Keys(keys)) => (keys.iter())
                .filter_map(|&k| table.logical_index(Axis::Rows, k))
                .collect(),
        };
        let reads = (filter.conditions.iter())
            .map(|k| table.logical_index(Axis::Rows, k.condition.key))
            .collect();
        RowsRead { searched, reads }
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
///
/// Both axes filter: the rows by their columns' text forms, the columns by
/// their rows' (a column meets a row in the cell of that column's text
/// form). Both axes hide, apart from the filter. A quick filter searching
/// the shown items of the other axis searches every one not hidden, whether
/// filtered out or not: filtering one axis never changes what the other's
/// quick filter searches, while hiding does.
pub struct MemorySource<R> {
    table: InMemoryTableSource,
    columns: HashMap<Key, Column<R>>,
    texts: HashMap<Key, ExtractText<R>>,
    /// The host's own filter on the rows in force, kept to place rows the
    /// base data gains.
    keep: Option<Keep<R>>,
    /// Each axis's quick filter and conditions in force; the rows' ANDed
    /// with `keep`.
    filters: [Filter; 2],
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
            filters: Default::default(),
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

    /// Makes the column with Key `column` a date, in milliseconds since the
    /// Unix epoch (UTC), sorting by it and compared as a date; `None` is
    /// null. Without a text closure its text form is the date as
    /// [`Scalar`] writes it.
    pub fn date_column(mut self, column: Key, f: impl Fn(&R) -> Option<i64> + 'static) -> Self {
        self.columns.insert(column, Column::Date(Box::new(f)));
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

    /// The Value a OneOf condition on `column` names `row` by: its text form,
    /// or `Blank` without one, as [`MemorySource::distinct`] lists it.
    pub fn value(&self, column: Key, row: &R) -> Value {
        Value::of(self.text(column, row).as_deref())
    }

    /// Column `column`'s number in `row`: an integer or float column's
    /// value, `None` for a blank or any other column. Aggregates such as a
    /// sum work on these.
    pub fn number(&self, column: Key, row: &R) -> Option<f64> {
        match self.columns.get(&column)?.scalar(row)? {
            Scalar::Number(n) => Some(n),
            Scalar::Date(_) => None,
        }
    }

    /// Column `column`'s text form of `row`, if it has one and `row` is not
    /// blank there.
    fn text<'r>(&self, column: Key, row: &'r R) -> Option<Cow<'r, str>> {
        match self.texts.get(&column) {
            Some(f) => f(row),
            None => self.columns.get(&column)?.display(row),
        }
    }

    /// Whether column `column` has a text form.
    fn has_text(&self, column: Key) -> bool {
        self.texts.contains_key(&column) || self.columns.contains_key(&column)
    }

    /// `axis`'s `filter` less its conditions on items of the other axis the
    /// table does not have, which it leaves out of its answer.
    /// Comparisons with scalars of another kind than the item's values are
    /// left out too.
    fn present_filter(&mut self, axis: Axis, filter: &FilterSpec) -> FilterSpec {
        let conditions: Vec<Condition> = (filter.conditions.conditions().iter())
            .filter(|c| self.table.logical_index(axis.other(), c.key).is_some())
            .cloned()
            .collect();
        let conditions: Vec<Condition> = (conditions.into_iter())
            .filter(|c| match &c.kind {
                ConditionKind::Compare(comparison) => {
                    (self.compared(axis, c.key)).is_some_and(|k| compared(comparison) == Some(k))
                }
                _ => true,
            })
            .collect();
        filter.clone().with_conditions(conditions.into())
    }

    /// Presents the table as `spec` asks: the rows sorted and filtered by
    /// its rows part, extracting each sort column's keys and each searched
    /// column's text from `rows`, and the columns sorted (see
    /// [`MemorySource::support`]) and filtered by its columns part, reading
    /// the rows' sort keys and text forms. Each axis hides the items its
    /// hidden Keys name; Keys no item has are dropped from the spec
    /// [`MemorySource::view_spec`] gives. Returns the Change Notices the
    /// axes need: the rows' first, `Refiltered` if their filter or hidden
    /// items changed, or the columns their quick filter searches, else
    /// `Reordered` if the sort moved them; then the columns' `Refiltered`
    /// likewise, else their `Reordered` if their sort moved them. An axis
    /// left in the same order is told nothing. Any View
    /// Spec, even the one in force, applies each axis's filter again to its
    /// unmatched items.
    /// A sort term naming a column or row the table does not have is left
    /// out, as hidden Keys no item has are: a restored layout may name one.
    /// If a sort names a column with no closure, nothing changes. A quick filter with an invalid regex is not applied:
    /// the one in force stays. A condition with an invalid regex, of a kind
    /// this source does not apply, or on an item the table does not have, is
    /// left out.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn view(
        &mut self,
        rows: &[R],
        spec: &ViewSpec,
    ) -> Result<Vec<(Axis, ChangeNotice)>, Unsortable> {
        let by = self.present(Axis::Rows, &spec.cols.sort);
        let resort = !by.is_empty() || &by != self.table.sort_spec(Axis::Cols);
        let by_rows = match resort {
            true => Some(self.sorting_rows(&by)),
            false => None,
        };
        let present = self.present(Axis::Cols, &spec.rows.sort);
        let sorted = self.sort(rows, &present)?;
        // An axis the spec leaves in the same order is told nothing.
        let resorted = by_rows.and_then(|by_rows| {
            let keys = by_rows.iter().map(|&i| self.row_keys(&rows[i])).collect();
            self.table.resort(Axis::Cols, &by, keys)
        });
        let hid = Axis::ALL.map(|a| {
            let hidden = &spec.axis(a).hidden;
            self.table.apply_hidden(a, hidden).is_some()
        });
        let mut notices = Vec::new();
        let filters = Axis::ALL.map(|a| self.present_filter(a, &spec.axis(a).filter));
        let changed = self.filters[0].take(&filters[0])
            || hid[0]
            || (hid[1] && self.filters[0].searches_shown());
        let notice = match changed || self.table.has_unmatched(Axis::Rows) {
            true => Some(self.refilter(rows)),
            false => sorted,
        };
        notices.extend(notice.map(|n| (Axis::Rows, n)));
        let changed = self.filters[1].take(&filters[1])
            || hid[1]
            || (hid[0] && self.filters[1].searches_shown());
        if changed || self.table.has_unmatched(Axis::Cols) {
            notices.push((Axis::Cols, self.refilter_columns(rows)));
        } else if let Some(resorted) = resorted {
            notices.push((Axis::Cols, resorted));
        }
        Ok(notices)
    }

    /// Sorts the rows by `spec`: `None` if the rows shown stay in order.
    fn sort(&mut self, rows: &[R], spec: &SortSpec) -> Result<Option<ChangeNotice>, Unsortable> {
        let columns = spec
            .columns()
            .iter()
            .map(|c| self.columns.get(&c.key).ok_or(Unsortable { column: c.key }))
            .collect::<Result<Vec<_>, _>>()?;
        let keys = columns.into_iter().map(|c| c.extract(rows)).collect();
        Ok(self.table.resort(Axis::Rows, spec, keys))
    }

    /// `spec` less its terms on items of `axis` the table does not have.
    fn present(&mut self, axis: Axis, spec: &SortSpec) -> SortSpec {
        let columns = (spec.columns().iter())
            .filter(|c| self.table.logical_index(axis, c.key).is_some())
            .copied()
            .collect();
        SortSpec::new(columns)
    }

    /// The Logical Indices of the rows `spec` sorts the columns by, every
    /// one of which the table has.
    fn sorting_rows(&mut self, spec: &SortSpec) -> Vec<usize> {
        (spec.columns().iter())
            .filter_map(|c| self.table.logical_index(Axis::Rows, c.key))
            .collect()
    }

    /// Sorts the columns by `spec`, whose rows have the Logical Indices
    /// `by_rows`.
    fn sort_columns(&mut self, rows: &[R], spec: &SortSpec, by_rows: &[usize]) -> ChangeNotice {
        let keys = by_rows.iter().map(|&i| self.row_keys(&rows[i])).collect();
        self.table.apply_sort(Axis::Cols, spec, keys)
    }

    /// The sort keys of `row` across the base columns, by which the columns
    /// sort: integers if every column with a closure has an integer one,
    /// numbers if every one is numeric, else text (bytes as they are,
    /// numbers written with `Display`). A column without a closure is null.
    fn row_keys<'r>(&self, row: &'r R) -> SortKeys<'r> {
        let columns: Vec<Option<&Column<R>>> = (0..self.table.base_len(Axis::Cols))
            .map(|c| self.columns.get(&self.table.base_key(Axis::Cols, c)))
            .collect();
        let mut present = columns.iter().flatten();
        if (present.clone()).all(|c| matches!(c, Column::I64(_) | Column::Date(_))) {
            let key = |c: &Option<&Column<R>>| match c {
                Some(Column::I64(f) | Column::Date(f)) => f(row),
                _ => None,
            };
            return SortKeys::I64(columns.iter().map(key).collect());
        }
        if present.all(|c| !matches!(c, Column::Bytes(_))) {
            let key = |c: &Option<&Column<R>>| match c {
                Some(Column::I64(f) | Column::Date(f)) => f(row).map(|v| v as f64),
                Some(Column::F64(f)) => f(row),
                _ => None,
            };
            return SortKeys::F64(columns.iter().map(key).collect());
        }
        let key = |c: &Option<&Column<R>>| match c {
            Some(Column::Bytes(f)) => f(row),
            Some(c) => c
                .display(row)
                .map(|t| Cow::Owned(t.into_owned().into_bytes())),
            None => None,
        };
        SortKeys::Bytes(columns.iter().map(key).collect())
    }

    /// Whether a filter is in force on the rows: the host's, the quick
    /// filter or a condition.
    fn filtered(&self) -> bool {
        self.keep.is_some() || self.filters[0].is_some()
    }

    /// The columns with a text form `query` searches: a quick filter
    /// searching the shown columns searches every one not hidden, filtered
    /// out or not.
    fn searched_columns(&self, query: &TextQuery) -> Vec<Key> {
        let columns = match &query.searches {
            Searched::Shown => (0..self.table.base_len(Axis::Cols))
                .filter(|&i| !self.table.is_hidden(Axis::Cols, i))
                .map(|i| self.table.base_key(Axis::Cols, i))
                .collect(),
            Searched::Keys(keys) => keys.clone(),
        };
        (columns.into_iter())
            .filter(|&c| self.has_text(c))
            .collect()
    }

    /// The keep flag of every row in `rows`: whether the filters in force
    /// keep it.
    fn row_flags(&self, rows: &[R]) -> Vec<bool> {
        let searched = self.row_search(&self.filters[0]);
        let filter = &self.filters[0];
        (rows.iter())
            .map(|r| self.kept_by_host(r) && self.passes(filter, &searched, r))
            .collect()
    }

    /// Whether the filters in force keep `row`.
    fn keeps(&self, row: &R) -> bool {
        let searched = self.row_search(&self.filters[0]);
        self.kept_by_host(row) && self.passes(&self.filters[0], &searched, row)
    }

    fn kept_by_host(&self, row: &R) -> bool {
        self.keep.as_ref().is_none_or(|keep| keep(row))
    }

    /// The columns the quick filter of the rows' `filter` searches.
    fn row_search(&self, filter: &Filter) -> Vec<Key> {
        let quick = filter.quick.as_ref();
        quick.map_or(Vec::new(), |q| self.searched_columns(&q.query))
    }

    /// Whether the rows' `filter`, its quick filter searching `searched`,
    /// keeps `row`.
    fn passes(&self, filter: &Filter, searched: &[Key], row: &R) -> bool {
        filter.quick.as_ref().is_none_or(|q| {
            (searched.iter()).any(|&c| self.text(c, row).is_some_and(|t| q.matcher.is_match(&t)))
        }) && (filter.conditions.iter()).all(|k| {
            k.keeps(Cell {
                source: self,
                column: k.condition.key,
                row: Some(row),
            })
        })
    }

    /// Whether the columns' `filter`, reading `read` of `rows`, keeps
    /// `column`.
    fn column_passes(&self, filter: &Filter, read: &RowsRead, rows: &[R], column: Key) -> bool {
        let text = |row: Option<usize>| row.and_then(|r| self.text(column, &rows[r]));
        filter.quick.as_ref().is_none_or(|q| {
            (read.searched.iter()).any(|&r| text(Some(r)).is_some_and(|t| q.matcher.is_match(&t)))
        }) && (filter.conditions.iter().zip(&read.reads)).all(|(k, &row)| {
            k.keeps(Cell {
                source: self,
                column,
                row: row.map(|r| &rows[r]),
            })
        })
    }

    /// The keep flag of every base column: whether the columns' filter in
    /// force keeps it, reading `rows`.
    fn column_flags(&mut self, rows: &[R]) -> Vec<bool> {
        let read = RowsRead::new(&mut self.table, &self.filters[1], rows.len());
        (0..self.table.base_len(Axis::Cols))
            .map(|i| {
                let column = self.table.base_key(Axis::Cols, i);
                self.column_passes(&self.filters[1], &read, rows, column)
            })
            .collect()
    }

    /// Applies the columns' filter in force again, reading `rows`; returns
    /// the `Refiltered` the columns need.
    fn refilter_columns(&mut self, rows: &[R]) -> ChangeNotice {
        if self.filters[1].is_some() {
            let keep = self.column_flags(rows);
            self.table.apply_filter(Axis::Cols, keep)
        } else {
            self.table.clear_filter(Axis::Cols)
        }
    }

    /// The rows changed, so the columns' filter in force may keep other
    /// columns: those whose flag changed come or go at once. Returns the
    /// columns' `Refiltered` if any did.
    fn recheck_columns(&mut self, rows: &[R]) -> Option<(Axis, ChangeNotice)> {
        if !self.filters[1].is_some() {
            return None;
        }
        let keep = self.column_flags(rows);
        let notice = self.table.refilter_items(Axis::Cols, &keep)?;
        Some((Axis::Cols, notice))
    }

    /// Column `column`'s text changed in one row, so the columns' filter in
    /// force may keep it or not: it comes or goes at once, or, with `defer`
    /// (the user's edit), stays unmatched while in view. Returns the
    /// columns' `Refiltered` if it came or went.
    fn recheck_column(
        &mut self,
        rows: &[R],
        column: Key,
        defer: bool,
    ) -> Option<(Axis, ChangeNotice)> {
        if !self.filters[1].is_some() {
            return None;
        }
        let i = self.table.logical_index(Axis::Cols, column)?;
        let read = RowsRead::new(&mut self.table, &self.filters[1], rows.len());
        let keep = self.column_passes(&self.filters[1], &read, rows, column);
        let notice = match defer {
            true => self.table.defer_item(Axis::Cols, i, keep),
            false => self.table.refilter_item(Axis::Cols, i, keep),
        };
        notice.map(|n| (Axis::Cols, n))
    }

    /// The distinct values `key`, an item of the other axis, holds across
    /// the items of `axis` in `rows`, as `query` asks: those containing its
    /// search (ignoring case, as a Text Query would), in the items the
    /// filters of `query.narrow` (and, for the rows, the host's filter)
    /// keep, with how many items hold each and how many are blank, up to its
    /// limit. A part of `narrow` with an invalid regex is left out.
    ///
    /// On the rows, `key` is a column and the values come in its sort-key
    /// order (ties, and columns with only a text closure, by text); on the
    /// columns, `key` is a row and the values come in text order.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn distinct(
        &mut self,
        rows: &[R],
        axis: Axis,
        key: Key,
        query: &DistinctQuery,
    ) -> Distinct {
        match axis {
            Axis::Rows => self.distinct_rows(rows, key, query),
            Axis::Cols => self.distinct_columns(rows, key, query),
        }
    }

    fn distinct_rows(&self, rows: &[R], column: Key, query: &DistinctQuery) -> Distinct {
        let narrow = Filter::new(&query.narrow);
        let searched = self.row_search(&narrow);
        let search = (!query.search.is_empty())
            .then(|| Matcher::new(&TextQuery::new(query.search.clone())))
            .flatten();
        let keys = self.columns.get(&column).map(|c| c.extract(rows));
        let mut blanks = 0;
        // Each value's count and the row with its least sort key.
        let mut seen: HashMap<Cow<'_, str>, (u64, usize)> = HashMap::new();
        for (i, row) in rows.iter().enumerate() {
            if self.table.is_hidden(Axis::Rows, i)
                || !(self.kept_by_host(row) && self.passes(&narrow, &searched, row))
            {
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
        let counted = values.into_iter().map(|(text, (count, _))| (text, count));
        distinct(counted, blanks, query)
    }

    fn distinct_columns(&mut self, rows: &[R], row: Key, query: &DistinctQuery) -> Distinct {
        let narrow = Filter::new(&query.narrow);
        let read = RowsRead::new(&mut self.table, &narrow, rows.len());
        let row = self.table.logical_index(Axis::Rows, row).map(|r| &rows[r]);
        let search = (!query.search.is_empty())
            .then(|| Matcher::new(&TextQuery::new(query.search.clone())))
            .flatten();
        let mut blanks = 0;
        let mut seen: HashMap<Cow<'_, str>, u64> = HashMap::new();
        for i in 0..self.table.base_len(Axis::Cols) {
            let column = self.table.base_key(Axis::Cols, i);
            if self.table.is_hidden(Axis::Cols, i)
                || !self.column_passes(&narrow, &read, rows, column)
            {
                continue;
            }
            let Some(text) = row.and_then(|r| self.text(column, r)) else {
                blanks += 1;
                continue;
            };
            if search.as_ref().is_some_and(|m| !m.is_match(&text)) {
                continue;
            }
            *seen.entry(text).or_insert(0) += 1;
        }
        let mut values: Vec<(Cow<'_, str>, u64)> = seen.into_iter().collect();
        values.sort();
        distinct(values.into_iter(), blanks, query)
    }

    /// Applies the rows' filters in force to `rows` again, in the sort in
    /// force; returns the `Refiltered` the rows need.
    fn refilter(&mut self, rows: &[R]) -> ChangeNotice {
        if self.filtered() {
            let keep = self.row_flags(rows);
            self.table.apply_filter(Axis::Rows, keep)
        } else {
            self.table.clear_filter(Axis::Rows)
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
    /// rejects (see [`InMemoryTableSource::append_rows`] for the rows'
    /// notice). The columns' filter in force reads them too: columns it now
    /// keeps or drops come or go at once, with a `Refiltered` after the
    /// rows' notice.
    ///
    /// # Panics
    ///
    /// If `rows` is shorter than before, or as [`InMemoryTableSource::append_rows`].
    pub fn appended(&mut self, rows: &[R], keys: &[Key]) -> Vec<(Axis, ChangeNotice)> {
        let old = self.table.base_len(Axis::Rows);
        let n = rows
            .len()
            .checked_sub(old)
            .expect("rows were appended, not removed");
        let keep: Vec<bool> = self.keep_flags(&rows[old..]);
        let sort_keys = self.sort_keys(rows);
        let notice = self.table.append_rows(n, keys, sort_keys, &keep);
        let mut notices = vec![(Axis::Rows, notice)];
        notices.extend(self.recheck_columns(rows));
        notices
    }

    /// The value in `column` of base row `row` (a Logical Index into `rows`)
    /// changed in the data itself, as from a live feed: the filters in
    /// force take it at once. For the rows, returns `ContentChanged` for the
    /// row's Position, `Reordered` if the rows are sorted by that column,
    /// `Refiltered` if the filter now keeps or drops the row where it did
    /// not before, or nothing if the row is filtered out either way; then
    /// the columns' `Reordered` if they are sorted by the row, and their
    /// `Refiltered` if the columns' filter now keeps or drops the column. An
    /// unmatched row or column is matched again, or leaves.
    ///
    /// # Panics
    ///
    /// If `row` is not a row of `rows`, or `rows` is not as long as the base
    /// row count.
    pub fn changed(&mut self, rows: &[R], row: usize, column: Key) -> Vec<(Axis, ChangeNotice)> {
        let refiltered = match self.filtered() {
            true => (self.table).refilter_item(Axis::Rows, row, self.keeps(&rows[row])),
            false => None,
        };
        let mut notices = self.content_changed(rows, row, column, refiltered);
        notices.extend(self.recheck_column(rows, column, false));
        notices
    }

    /// The user edited the value in `column` of base row `row`. As
    /// [`MemorySource::changed`], except that a row or column in view stays
    /// even if the filter in force now drops it: it is unmatched until the
    /// next View Spec (or, for a row, host filter) and
    /// [`MemorySource::unmatched`] flags it. One out of view is filtered as
    /// for [`MemorySource::changed`].
    ///
    /// # Panics
    ///
    /// As [`MemorySource::changed`].
    pub fn edited(&mut self, rows: &[R], row: usize, column: Key) -> Vec<(Axis, ChangeNotice)> {
        let refiltered = match self.filtered() {
            true => (self.table).defer_item(Axis::Rows, row, self.keeps(&rows[row])),
            false => None,
        };
        let mut notices = self.content_changed(rows, row, column, refiltered);
        notices.extend(self.recheck_column(rows, column, true));
        notices
    }

    /// The Change Notices for a value change in `column` of base row `row`,
    /// after any `refiltered` the filter gave, re-sorting either axis if
    /// sorted by it.
    fn content_changed(
        &mut self,
        rows: &[R],
        row: usize,
        column: Key,
        refiltered: Option<ChangeNotice>,
    ) -> Vec<(Axis, ChangeNotice)> {
        let notice = if self.sort_spec().find(column).is_some() {
            self.resort(rows);
            Some(refiltered.unwrap_or(ChangeNotice::Reordered))
        } else if refiltered.is_some() {
            refiltered
        } else {
            (self.table.position(Axis::Rows, row)).map(|p| ChangeNotice::ContentChanged(p..p + 1))
        };
        let key = self.table.base_key(Axis::Rows, row);
        let by_row = self.table.sort_spec(Axis::Cols).find(key).is_some();
        let cols = by_row.then(|| (Axis::Cols, self.resort_columns(rows)));
        notice
            .map(|n| (Axis::Rows, n))
            .into_iter()
            .chain(cols)
            .collect()
    }

    /// Whether each item with `keys` on `axis` is unmatched: in view though
    /// the filter in force drops it, since the user's edit. Hosts ask for
    /// the items shown, to draw them marked.
    pub fn unmatched(&mut self, axis: Axis, keys: &[Key]) -> Vec<bool> {
        self.table.unmatched(axis, keys)
    }

    /// The host's base data changed in some other way (rows inserted or
    /// removed anywhere, or replaced): it is now `rows`, keyed by `keys` (by
    /// Logical Index if `None`). The sort and filters in force are applied
    /// again: the rows need the `Reset` returned first. Rows the columns
    /// sort by that left are dropped from the columns' Sort Spec, and sorted
    /// columns are sorted again and need the `Reordered` returned, then the
    /// `Refiltered` if the columns' filter now keeps or drops columns.
    ///
    /// # Panics
    ///
    /// As [`InMemoryTableSource::replace_rows`].
    pub fn replaced(&mut self, rows: &[R], keys: Option<Vec<Key>>) -> Vec<(Axis, ChangeNotice)> {
        let keep = self.keep_flags(rows);
        let sort_keys = self.sort_keys(rows);
        let reset = self.table.replace_rows(rows.len(), keys, sort_keys, keep);
        let mut notices = vec![(Axis::Rows, reset)];
        let by = self.table.sort_spec(Axis::Cols).clone();
        if !by.is_empty() {
            let spec = self.present(Axis::Rows, &by);
            let by_rows = self.sorting_rows(&spec);
            notices.push((Axis::Cols, self.sort_columns(rows, &spec, &by_rows)));
        }
        notices.extend(self.recheck_columns(rows));
        notices
    }

    /// The keep flags of `rows` while a filter is in force, else none.
    fn keep_flags(&self, rows: &[R]) -> Vec<bool> {
        match self.filtered() {
            true => self.row_flags(rows),
            false => Vec::new(),
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

    /// Sorts the columns again by the rows' values, by the columns' Sort
    /// Spec in force.
    fn resort_columns(&mut self, rows: &[R]) -> ChangeNotice {
        let spec = self.table.sort_spec(Axis::Cols).clone();
        let by_rows = self.sorting_rows(&spec);
        self.sort_columns(rows, &spec, &by_rows)
    }

    /// The Sort Spec the rows are in, as last applied.
    fn sort_spec(&self) -> &SortSpec {
        self.table.sort_spec(Axis::Rows)
    }

    /// The View Spec the source applies: each axis's sort, quick filter,
    /// conditions and hidden Keys, as last applied, less hidden Keys no item
    /// has.
    pub fn view_spec(&self) -> ViewSpec {
        ViewSpec::default()
            .with_sort(Axis::Rows, self.sort_spec().clone())
            .with_filter(Axis::Rows, self.filters[0].spec())
            .with_hidden(Axis::Rows, self.table.hidden(Axis::Rows).to_vec())
            .with_sort(Axis::Cols, self.table.sort_spec(Axis::Cols).clone())
            .with_filter(Axis::Cols, self.filters[1].spec())
            .with_hidden(Axis::Cols, self.table.hidden(Axis::Cols).to_vec())
    }

    /// The unfiltered count of each axis: every base row and column.
    pub fn totals(&self) -> Totals {
        Totals {
            rows: Some(self.table.base_len(Axis::Rows) as u64),
            cols: Some(self.table.base_len(Axis::Cols) as u64),
        }
    }

    /// The rows sort by the columns given a closure, and filter by Text
    /// conditions, regex included, and OneOf conditions on the columns with
    /// a text form. The columns sort by any row while some column has a
    /// closure, comparing each column's sort key in that row (see
    /// [`MemorySource::view`]), and filter by Text and OneOf conditions on
    /// any row, reading the text forms. The rows compare numbers on integer
    /// and float columns and dates on date columns; the columns compare a
    /// row's values when every column with a closure is numeric, or every
    /// one a date.
    pub fn support(&self, axis: Axis, key: Key) -> Support {
        let text = axis == Axis::Cols || self.has_text(key);
        let sort = match axis {
            Axis::Rows => self.columns.contains_key(&key),
            Axis::Cols => !self.columns.is_empty(),
        };
        Support::default()
            .with_sort(sort)
            .with_text(text.then_some(TextSupport::new(true)))
            .with_one_of(text)
            .with_compare(self.compared(axis, key))
    }

    /// The kind of value comparison conditions on `key`, an item of the
    /// other axis, compare on `axis`: a column's, for the rows; for the
    /// columns, the kind every column with a closure shares, if any.
    fn compared(&self, axis: Axis, key: Key) -> Option<Compared> {
        match axis {
            Axis::Rows => self.columns.get(&key)?.compared(),
            Axis::Cols => {
                let mut kinds = self.columns.values().map(Column::compared);
                let first = kinds.next()?;
                kinds.all(|k| k == first).then_some(first)?
            }
        }
    }

    /// Both axes hide.
    pub fn hide_support(&self, _axis: Axis) -> bool {
        true
    }

    /// Both axes have a quick filter, regex included.
    pub fn quick_filter(&self, _axis: Axis) -> Option<TextSupport> {
        Some(TextSupport::new(true))
    }

    /// The source finds, regex included.
    pub fn find_support(&self) -> Option<TextSupport> {
        Some(TextSupport::new(true))
    }

    /// The columns shown that `query` searches and that have a text form,
    /// as (Position, Key) in column order.
    fn searched(&self, query: &TextQuery) -> Vec<(Position, Key)> {
        let shown = self.window(Axis::Cols, 0..self.count(Axis::Cols));
        (0..)
            .zip(shown)
            .filter(|(_, c)| match &query.searches {
                Searched::Shown => true,
                Searched::Keys(keys) => keys.contains(c),
            })
            .filter(|&(_, c)| self.has_text(c))
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
    /// closure), `Locate` by the Remap, moves by moving the items.
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

    /// See [`InMemoryTableSource::move_items`]. Sorting names items by Key,
    /// so it is unaffected by the order of the other axis.
    pub fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        self.table.move_items(axis, keys, to)
    }

    /// See [`InMemoryTableSource::movable`].
    pub fn movable(&self, axis: Axis) -> bool {
        self.table.movable(axis)
    }

    /// See [`InMemoryTableSource::set_move_policy`].
    pub fn with_move_policy(mut self, policy: MovePolicy) -> Self {
        self.table.set_move_policy(policy);
        self
    }

    pub fn count(&self, axis: Axis) -> u64 {
        self.table.count(axis)
    }

    pub fn window(&self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.table.window(axis, range)
    }

    /// See [`InMemoryTableSource::logical_window`].
    pub fn logical_window(&self, axis: Axis, range: Range<Position>) -> Vec<usize> {
        self.table.logical_window(axis, range)
    }

    pub fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.table.locate(axis, key)
    }
}

/// A Distinct answer from `values`, each text with how many items hold it
/// in the order to list them, and `blanks` blank items, cut to `query`'s
/// limit. Blanks are listed only when not searching.
fn distinct<'t>(
    values: impl Iterator<Item = (Cow<'t, str>, u64)>,
    blanks: u64,
    query: &DistinctQuery,
) -> Distinct {
    let mut values: Vec<DistinctValue> = values
        .map(|(text, count)| DistinctValue {
            text: text.into_owned(),
            count: Some(count),
        })
        .collect();
    let partial = values.len() > query.limit;
    values.truncate(query.limit);
    let listed = query.search.is_empty() && blanks > 0;
    Distinct {
        values,
        blanks: listed.then_some(Blanks::Counted(blanks)),
        partial,
        narrowed: true,
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
        let notices = self.source.view(self.rows, spec).unwrap_or_default();
        Viewed {
            applied: self.source.view_spec(),
            totals: self.source.totals(),
            notices,
        }
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.source.support(axis, key)
    }

    fn quick_filter(&mut self, axis: Axis) -> Option<TextSupport> {
        self.source.quick_filter(axis)
    }

    fn hide_support(&mut self, axis: Axis) -> bool {
        self.source.hide_support(axis)
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        Some(self.source.distinct(self.rows, axis, key, query))
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.source.locate(axis, key)
    }

    fn movable(&mut self, axis: Axis) -> Option<bool> {
        Some(self.source.movable(axis))
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Result<Moved, String> {
        let before = self.source.view_spec();
        let Some(notice) = self.source.move_items(axis, keys, to) else {
            return Err("Can't move while sorted: clear the sort first".into());
        };
        let after = self.source.view_spec();
        Ok(Moved {
            notices: vec![(axis, notice)],
            view: (after != before).then(|| (after, self.source.totals())),
        })
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
