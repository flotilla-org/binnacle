use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::effect::Effect;
use crate::grid::Grid;
use crate::grouping::{Aggregate, AggregateValue, Depth, GRAND_TOTAL, Grouping, Level, Placement};
use crate::pointer::Modifiers;
use crate::sort::{HeaderSort, SortSpec};
use crate::types::{Axis, Edge, Key, Position};

/// The items of the other axis a Text Query searches.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Searched {
    /// Every item the Table Source shows, whichever they are when it
    /// applies the query: the Grid asks again when they change.
    #[default]
    Shown,
    /// These items only, shown or not.
    Keys(Vec<Key>),
}

/// Text to look for in the cells of some items, as the Table Source writes
/// their values (its own text form): anywhere in a value, ignoring case,
/// unless `case_sensitive`; as a pattern in the source's own syntax if
/// `regex`. A value with no text (a blank) never matches.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TextQuery {
    pub text: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub searches: Searched,
}

impl TextQuery {
    /// Looks for `text`, ignoring case, in every shown item.
    pub fn new(text: impl Into<String>) -> Self {
        TextQuery {
            text: text.into(),
            case_sensitive: false,
            regex: false,
            searches: Searched::Shown,
        }
    }

    pub fn case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }

    pub fn regex(mut self, regex: bool) -> Self {
        self.regex = regex;
        self
    }

    pub fn searching(mut self, searches: Searched) -> Self {
        self.searches = searches;
        self
    }
}

/// A value as a condition lists it: text in the Table Source's own text
/// form, or a blank (no value at all), which is not the same as empty text.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Value {
    Text(String),
    Blank,
}

impl Value {
    /// The value whose text form is `text`, or a blank for `None`.
    pub fn of(text: Option<&str>) -> Self {
        match text {
            Some(t) => Value::Text(t.to_owned()),
            None => Value::Blank,
        }
    }
}

/// A number or a date a comparison condition compares values with. A date
/// is an instant, in milliseconds since the Unix epoch (UTC); a date
/// without a time of day is its midnight, UTC.
#[derive(Clone, Copy, Debug)]
pub enum Scalar {
    Number(f64),
    Date(i64),
}

impl Scalar {
    /// How this compares with `other`: `None` for a NaN, or a number and a
    /// date.
    pub fn order(&self, other: &Scalar) -> Option<Ordering> {
        match (self, other) {
            (Scalar::Number(a), Scalar::Number(b)) => a.partial_cmp(b),
            (Scalar::Date(a), Scalar::Date(b)) => Some(a.cmp(b)),
            _ => None,
        }
    }

    /// Midnight, UTC, on day `day` of month `month` (1 to 12) of `year`.
    pub fn date(year: i64, month: u32, day: u32) -> Self {
        // Days from civil, after Howard Hinnant's algorithm.
        let y = if month <= 2 { year - 1 } else { year };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (i64::from(month) + 9) % 12;
        let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Scalar::Date((era * 146_097 + doe - 719_468) * 86_400_000)
    }
}

/// Two scalars are the same if they have the same kind and bits, so a
/// condition holding NaN still equals itself.
impl PartialEq for Scalar {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Scalar::Number(a), Scalar::Number(b)) => a.to_bits() == b.to_bits(),
            (Scalar::Date(a), Scalar::Date(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Scalar {}

impl Hash for Scalar {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Scalar::Number(n) => (0u8, n.to_bits()).hash(state),
            Scalar::Date(ms) => (1u8, ms).hash(state),
        }
    }
}

/// A number as Rust writes it; a date as `YYYY-MM-DD`, with `THH:MM` (and
/// seconds and milliseconds, where not zero) and `Z` past midnight UTC.
impl fmt::Display for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = match self {
            Scalar::Number(n) => return write!(f, "{n}"),
            Scalar::Date(ms) => *ms,
        };
        // Civil from days, after Howard Hinnant's algorithm.
        let (days, time) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        write!(f, "{year:04}-{month:02}-{day:02}")?;
        if time != 0 {
            let (h, m) = (time / 3_600_000, time / 60_000 % 60);
            let (s, milli) = (time / 1000 % 60, time % 1000);
            write!(f, "T{h:02}:{m:02}")?;
            match (s, milli) {
                (0, 0) => {}
                (s, 0) => write!(f, ":{s:02}")?,
                (s, milli) => write!(f, ":{s:02}.{milli:03}")?,
            }
            write!(f, "Z")?;
        }
        Ok(())
    }
}

/// How a comparison condition compares a value with its scalars. Between
/// takes both ends; a low end past the high one keeps nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Comparison {
    Equal(Scalar),
    NotEqual(Scalar),
    Less(Scalar),
    AtMost(Scalar),
    Greater(Scalar),
    AtLeast(Scalar),
    Between(Scalar, Scalar),
}

impl Comparison {
    /// Whether an item whose value is `value` passes: never a blank, a NaN,
    /// or a value of another kind than the scalars.
    pub fn passes(&self, value: Option<Scalar>) -> bool {
        use Ordering::*;
        let Some(value) = value else { return false };
        let with =
            |s: &Scalar, wanted: &[Ordering]| value.order(s).is_some_and(|o| wanted.contains(&o));
        match self {
            Comparison::Equal(s) => with(s, &[Equal]),
            Comparison::NotEqual(s) => with(s, &[Less, Greater]),
            Comparison::Less(s) => with(s, &[Less]),
            Comparison::AtMost(s) => with(s, &[Less, Equal]),
            Comparison::Greater(s) => with(s, &[Greater]),
            Comparison::AtLeast(s) => with(s, &[Greater, Equal]),
            Comparison::Between(low, high) => {
                with(low, &[Greater, Equal]) && with(high, &[Less, Equal])
            }
        }
    }
}

/// What a condition asks of the values in its item.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionKind {
    /// The value matches the Text Query, which searches the condition's item.
    Text(TextQuery),
    /// The value is one of these, exactly (a blank only if they list
    /// [`Value::Blank`]). None listed keeps nothing.
    OneOf(Vec<Value>),
    /// The value, a number or a date, compares so. A blank, a NaN, or a
    /// value of the other kind never passes, so `not` keeps it.
    Compare(Comparison),
    /// An opaque condition in the Table Source's own syntax, such as a SQL
    /// fragment, passed to it as it is. binnacle never reads it.
    Native(String),
}

/// One condition on the items of an axis, read in one item of the other
/// axis (a column, for the rows): the items whose value there satisfies
/// `kind` are kept, or with `not` the items whose value does not.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Condition {
    pub key: Key,
    pub kind: ConditionKind,
    pub not: bool,
}

impl Condition {
    /// Keeps the items whose value in `key` matches `query`. The query
    /// searches `key` alone, whatever it searched before.
    pub fn text(key: Key, query: TextQuery) -> Self {
        Condition {
            key,
            kind: ConditionKind::Text(query.searching(Searched::Keys(vec![key]))),
            not: false,
        }
    }

    /// Keeps the items whose value in `key` is one of `values`.
    pub fn one_of(key: Key, values: Vec<Value>) -> Self {
        Condition {
            key,
            kind: ConditionKind::OneOf(values),
            not: false,
        }
    }

    /// Keeps the items whose value in `key` passes `comparison`.
    pub fn compare(key: Key, comparison: Comparison) -> Self {
        Condition {
            key,
            kind: ConditionKind::Compare(comparison),
            not: false,
        }
    }

    /// Keeps the items `text`, in the Table Source's own syntax, keeps:
    /// a Native condition, read in `key`.
    pub fn native(key: Key, text: impl Into<String>) -> Self {
        Condition {
            key,
            kind: ConditionKind::Native(text.into()),
            not: false,
        }
    }

    /// This condition, or with `not` its opposite.
    pub fn not(mut self, not: bool) -> Self {
        self.not = not;
        self
    }

    /// A Text or Native condition with no text, which asks for nothing.
    /// Other kinds always ask for something, even OneOf with no values.
    fn is_empty(&self) -> bool {
        match &self.kind {
            ConditionKind::Text(query) => query.text.is_empty(),
            ConditionKind::Native(text) => text.is_empty(),
            ConditionKind::OneOf(_) | ConditionKind::Compare(_) => false,
        }
    }
}

/// How the conditions of a Filter Spec combine.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionTree {
    /// Keeps the items every condition keeps: all of them, when empty.
    All(Vec<Condition>),
}

impl Default for ConditionTree {
    fn default() -> Self {
        ConditionTree::All(Vec::new())
    }
}

impl From<Vec<Condition>> for ConditionTree {
    fn from(conditions: Vec<Condition>) -> Self {
        ConditionTree::All(conditions)
    }
}

impl ConditionTree {
    /// Every condition in the tree, in order.
    pub fn conditions(&self) -> &[Condition] {
        match self {
            ConditionTree::All(conditions) => conditions,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.conditions().is_empty()
    }

    /// The conditions read in item `key`, in order.
    pub fn on(&self, key: Key) -> impl Iterator<Item = &Condition> {
        self.conditions().iter().filter(move |c| c.key == key)
    }

    fn list_mut(&mut self) -> &mut Vec<Condition> {
        match self {
            ConditionTree::All(conditions) => conditions,
        }
    }

    /// Where the first condition equal to `condition` sits.
    fn find(&self, condition: &Condition) -> Option<usize> {
        self.conditions().iter().position(|c| c == condition)
    }
}

/// The filter part of one axis of a View Spec: which of its items the Table
/// Source keeps, those the quick filter and every condition keep. Empty keeps
/// every item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct FilterSpec {
    /// The quick filter: keeps the items with a match for the query in any
    /// item it searches.
    pub quick: Option<TextQuery>,
    pub conditions: ConditionTree,
}

impl FilterSpec {
    pub fn is_empty(&self) -> bool {
        self.quick.is_none() && self.conditions.is_empty()
    }

    pub fn with_quick(mut self, quick: Option<TextQuery>) -> Self {
        self.quick = quick;
        self
    }

    pub fn with_conditions(mut self, conditions: ConditionTree) -> Self {
        self.conditions = conditions;
        self
    }
}

/// One axis's part of a View Spec: which of its items to keep and how to
/// order them, each named by Keys on the other axis, and which of its own
/// items the user hid.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AxisView {
    pub filter: FilterSpec,
    pub sort: SortSpec,
    /// The hidden Keys, in the order they were hidden: items of this axis
    /// the Table Source leaves out, apart from the filter, so clearing the
    /// filter never shows them. A source answers with the Keys its data
    /// still has.
    pub hidden: Vec<Key>,
}

impl AxisView {
    pub fn is_empty(&self) -> bool {
        self.filter.is_empty() && self.sort.is_empty() && self.hidden.is_empty()
    }
}

/// The declared view a Table Source should present for the whole table, one
/// part per axis and the whole-table grouping. The default is the Table
/// Source's base view: nothing filtered, unsorted, ungrouped.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ViewSpec {
    pub rows: AxisView,
    pub cols: AxisView,
    /// How the table is grouped; `None` for a plain table.
    pub grouping: Option<Grouping>,
}

impl ViewSpec {
    pub fn axis(&self, axis: Axis) -> &AxisView {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        }
    }

    pub fn axis_mut(&mut self, axis: Axis) -> &mut AxisView {
        match axis {
            Axis::Rows => &mut self.rows,
            Axis::Cols => &mut self.cols,
        }
    }

    /// This spec with `axis` sorted by `sort`.
    pub fn with_sort(mut self, axis: Axis, sort: SortSpec) -> Self {
        self.axis_mut(axis).sort = sort;
        self
    }

    /// This spec with `axis` filtered by `filter`.
    pub fn with_filter(mut self, axis: Axis, filter: FilterSpec) -> Self {
        self.axis_mut(axis).filter = filter;
        self
    }

    /// This spec with the items of `axis` keyed by `hidden` hidden.
    pub fn with_hidden(mut self, axis: Axis, hidden: Vec<Key>) -> Self {
        self.axis_mut(axis).hidden = hidden;
        self
    }

    /// This spec grouped by `grouping`, or plain with `None`.
    pub fn with_grouping(mut self, grouping: Option<Grouping>) -> Self {
        self.grouping = grouping;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.cols.is_empty() && self.grouping.is_none()
    }
}

/// The named cause of a requested View Spec, carried with it as a hint: a
/// Table Source must be correct from the spec alone.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ViewEdit {
    /// A header click cycled the sort of `axis` by `key`, an item of the
    /// other axis: ascending, descending, then off. `multi` (Shift) kept the
    /// rest of the sort.
    CycleSort { axis: Axis, key: Key, multi: bool },
    /// The quick filter of `axis` was set to `query`, or cleared.
    SetQuickFilter {
        axis: Axis,
        query: Option<TextQuery>,
    },
    /// The host asked for a whole View Spec ([`Grid::request_view`]), say to
    /// restore a saved view.
    Replace,
    /// Items were shown or hidden on `axis`, and a Text Query searches the
    /// shown ones: the same spec, to apply again.
    ShownChanged { axis: Axis },
    /// The user asked for the same spec to be applied again
    /// ([`Grid::reapply_view`]), so rows their edits left unmatched are
    /// filtered.
    Reapply,
    /// `condition` was added to the conditions of `axis`.
    AddCondition { axis: Axis, condition: Condition },
    /// The first condition of `axis` equal to `condition` was removed.
    RemoveCondition { axis: Axis, condition: Condition },
    /// The first condition of `axis` equal to `from` became `to`, in place.
    ReplaceCondition {
        axis: Axis,
        from: Condition,
        to: Condition,
    },
    /// The items of `axis` with `keys` were hidden, after those hidden
    /// already.
    Hide { axis: Axis, keys: Vec<Key> },
    /// The hidden items of `axis` with `keys` were shown again.
    Show { axis: Axis, keys: Vec<Key> },
    /// `axis` was grouped by `field`, a base field, as its innermost level.
    /// A plain table becomes grouped with one value, the count.
    Group { axis: Axis, field: Key },
    /// Every level of `axis` was removed; with none left on either axis and
    /// no slicers the table is plain again.
    Ungroup { axis: Axis },
    /// `condition` was added to the slicers, which filter before
    /// aggregating. A plain table gains a grouping with no levels, counting.
    AddSlicer { condition: Condition },
    /// The first slicer equal to `condition` was removed; with no slicers
    /// and no levels left the table is plain again.
    RemoveSlicer { condition: Condition },
    /// The first slicer equal to `from` became `to`, in place.
    ReplaceSlicer { from: Condition, to: Condition },
    /// The group of `axis` headed by the Group Item `key` was opened: the
    /// Key joins or leaves `expand.except`.
    Expand { axis: Axis, key: Key },
    /// The group of `axis` headed by the Group Item `key` was closed.
    Collapse { axis: Axis, key: Key },
    /// The group headed by `key` and every group under it the Grid knows of
    /// were opened.
    ExpandAll { axis: Axis, key: Key },
    /// `axis` was opened to `depth` levels and its exceptions cleared.
    ExpandTo { axis: Axis, depth: Depth },
}

/// How a Table Source matches a Text Query: `regex` if it takes patterns as
/// well as plain text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TextSupport {
    pub regex: bool,
}

impl TextSupport {
    pub fn new(regex: bool) -> Self {
        TextSupport { regex }
    }
}

/// The unfiltered count of each axis, where the Table Source knows it, as it
/// gives them with [`Grid::view_applied`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Totals {
    pub rows: Option<u64>,
    pub cols: Option<u64>,
}

impl Totals {
    /// Neither total known.
    pub fn unknown() -> Self {
        Totals::default()
    }

    pub fn axis(&self, axis: Axis) -> Option<u64> {
        match axis {
            Axis::Rows => self.rows,
            Axis::Cols => self.cols,
        }
    }

    pub fn with(mut self, axis: Axis, total: Option<u64>) -> Self {
        match axis {
            Axis::Rows => self.rows = total,
            Axis::Cols => self.cols = total,
        }
        self
    }
}

/// How the items of an axis may be viewed by one item of the other axis,
/// as its Table Source declares with [`Grid::set_support`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Support {
    /// The axis may be sorted by the item.
    pub sort: bool,
    /// The axis may be filtered by Text conditions on the item, and how they
    /// match; `None` if not. Contains without regex by default.
    pub text: Option<TextSupport>,
    /// The axis may be filtered by OneOf conditions on the item, the Table
    /// Source listing its distinct values. Not by default.
    pub one_of: bool,
    /// The axis may be filtered by comparison conditions on the item, and
    /// on which kind of value; `None` (the default) if not.
    pub compare: Option<Compared>,
    /// The axis may be filtered by Native conditions on the item, in the
    /// Table Source's own syntax. Not by default.
    pub native: bool,
    /// The axis may be grouped by the item, a base field: a level of the
    /// axis. Not by default.
    pub group: bool,
    /// The other axis may be grouped by the item's values, a base field: a
    /// level of the other axis, as in a pivot. Not by default.
    pub pivot: bool,
    /// Where the Subtotals of a level grouping by the item may go, beyond
    /// off. Nowhere by default.
    pub subtotals: Placements,
}

/// Where Subtotals may be placed, beyond off, which every grouped Table
/// Source can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Placements {
    pub before: bool,
    pub after: bool,
}

impl Placements {
    /// Before and after both.
    pub const BOTH: Placements = Placements {
        before: true,
        after: true,
    };

    /// Whether `placement` is one of these, or off.
    pub fn allows(self, placement: Placement) -> bool {
        match placement {
            Placement::Before => self.before,
            Placement::After => self.after,
            Placement::Off => true,
        }
    }
}

/// The kind of value comparison conditions on an item compare: its values
/// and the scalars both numbers, or both dates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Compared {
    Numbers,
    Dates,
}

impl Default for Support {
    fn default() -> Self {
        Support {
            sort: true,
            text: Some(TextSupport::default()),
            one_of: false,
            compare: None,
            native: false,
            group: false,
            pivot: false,
            subtotals: Placements::default(),
        }
    }
}

impl Support {
    pub fn with_sort(mut self, sort: bool) -> Self {
        self.sort = sort;
        self
    }

    pub fn with_text(mut self, text: Option<TextSupport>) -> Self {
        self.text = text;
        self
    }

    pub fn with_one_of(mut self, one_of: bool) -> Self {
        self.one_of = one_of;
        self
    }

    pub fn with_compare(mut self, compare: Option<Compared>) -> Self {
        self.compare = compare;
        self
    }

    pub fn with_native(mut self, native: bool) -> Self {
        self.native = native;
        self
    }

    pub fn with_group(mut self, group: bool) -> Self {
        self.group = group;
        self
    }

    pub fn with_pivot(mut self, pivot: bool) -> Self {
        self.pivot = pivot;
        self
    }

    pub fn with_subtotals(mut self, subtotals: Placements) -> Self {
        self.subtotals = subtotals;
        self
    }
}

/// How the header of an item shows the conditions read in it: `filtered`
/// while the applied View Spec has one, `pending` while the latest request
/// would change them, and `not_applied` while one the user asked for is
/// left out of the applied spec with no answer due.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HeaderFilter {
    pub filtered: bool,
    pub pending: bool,
    pub not_applied: bool,
}

/// Where a condition stands between the applied View Spec and the latest
/// request, as [`crate::Snapshot::conditions`] lists it; likewise a slicer,
/// a level or a value of the grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConditionState {
    /// Applied, and still requested.
    Applied,
    /// Requested, not yet applied.
    Adding,
    /// Applied, no longer requested.
    Removing,
    /// Requested, with no answer due, but left out of the applied spec: the
    /// Table Source rejected it, failed, stopped applying it, or never
    /// could (a restored layout asking more than it supports).
    NotApplied,
}

/// The most hidden Keys an axis takes, unless its Table Source declares
/// fewer ([`Grid::set_hide_cap`]). The View Spec carries them whole, so
/// past this many a filter is the tool that scales.
pub const HIDE_CAP: usize = 1000;

/// A hide refused because the axis's hidden Keys would number more than its
/// cap: nothing was hidden. Adapters suggest a filter instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HideCapped {
    pub cap: usize,
}

/// The Grid's view state: the View Spec the Table Source last applied and the
/// one the user last asked for while answers are still due, or while the
/// source leaves part of it out.
#[derive(Clone, Debug, Default)]
pub(crate) struct ViewState {
    pub applied: ViewSpec,
    /// The latest request, kept while any `view_applied` is still due, then
    /// while the applied spec leaves out part of it.
    pub requested: Option<ViewSpec>,
    due: u32,
    /// Per axis viewed, the items of the other axis whose support differs
    /// from the default (sparse, set by the host).
    support: [BTreeMap<Key, Support>; 2],
    /// Per axis viewed, the aggregates the Table Source offers over each
    /// item of the other axis that has any (sparse, set by the host).
    aggregates: [BTreeMap<Key, Vec<Aggregate>>; 2],
    /// Per axis, its quick filter's support, if it has one.
    pub quick_filter: [Option<TextSupport>; 2],
    /// Per axis, whether the Table Source hides its items.
    pub hides: [bool; 2],
    /// Per axis, the most hidden Keys the Table Source takes, if it declared
    /// fewer than [`HIDE_CAP`].
    pub hide_caps: [Option<usize>; 2],
    /// The unfiltered counts from the last `view_applied`.
    pub totals: Totals,
}

impl ViewState {
    /// The most hidden Keys `axis` takes.
    pub fn hide_cap(&self, axis: Axis) -> usize {
        self.hide_caps[axis.index()].unwrap_or(HIDE_CAP)
    }

    /// How the header of `key` shows the sort of the other axis.
    pub fn header(&self, header_axis: Axis, key: Key) -> HeaderSort {
        let sorted = header_axis.other();
        let place = |spec: &ViewSpec| spec.axis(sorted).sort.find(key).map(|(i, d)| (d, i));
        let applied = place(&self.applied);
        let pending =
            self.pending() && self.requested.as_ref().is_some_and(|r| place(r) != applied);
        HeaderSort { applied, pending }
    }

    /// How the header of `key` shows the conditions read in it.
    pub fn header_filter(&self, header_axis: Axis, key: Key) -> HeaderFilter {
        let filtered = header_axis.other();
        let on = |spec: &ViewSpec| -> Vec<Condition> {
            spec.axis(filtered)
                .filter
                .conditions
                .on(key)
                .cloned()
                .collect()
        };
        let applied = on(&self.applied);
        let differs = self.requested.as_ref().is_some_and(|r| on(r) != applied);
        HeaderFilter {
            filtered: !applied.is_empty(),
            pending: differs && self.pending(),
            not_applied: differs && !self.pending(),
        }
    }

    /// Whether a `view_applied` is still due.
    pub fn pending(&self) -> bool {
        self.due > 0
    }

    /// The conditions of `axis`: the latest request's, each `Applied` or
    /// `Adding`, then the applied ones it drops, `Removing`.
    pub fn conditions(&self, axis: Axis) -> Vec<(Condition, ConditionState)> {
        let of = |spec: &ViewSpec| spec.axis(axis).filter.conditions.conditions().to_vec();
        self.listed(of, |a, b| a == b)
    }

    /// The slicers, listed as [`ViewState::conditions`] lists conditions.
    pub fn slicers(&self) -> Vec<(Condition, ConditionState)> {
        let of = |spec: &ViewSpec| {
            (spec.grouping.iter())
                .flat_map(|g| g.slicers.clone())
                .collect()
        };
        self.listed(of, |a, b| a == b)
    }

    /// The levels of `axis`, listed as [`ViewState::conditions`] lists
    /// conditions; a requested level is matched with the applied level on
    /// its field, and is not applied while they differ (in its Subtotal).
    pub fn levels(&self, axis: Axis) -> Vec<(Level, ConditionState)> {
        let of = |spec: &ViewSpec| {
            (spec.grouping.iter())
                .flat_map(|g| g.levels(axis).to_vec())
                .collect()
        };
        self.listed(of, |a: &Level, b: &Level| a.field_key() == b.field_key())
    }

    /// The values, listed as [`ViewState::conditions`] lists conditions.
    pub fn values(&self) -> Vec<(AggregateValue, ConditionState)> {
        let of = |spec: &ViewSpec| {
            (spec.grouping.iter())
                .flat_map(|g| g.values.clone())
                .collect()
        };
        self.listed(of, |a, b| a == b)
    }

    /// The parts `of` a View Spec, the latest request's first, each matched
    /// with an applied part the `same` way: `Applied` where they are equal,
    /// else `Adding` while an answer is due and `NotApplied` once none is;
    /// then the applied parts left unmatched, `Removing`.
    fn listed<T: Clone + PartialEq>(
        &self,
        of: impl Fn(&ViewSpec) -> Vec<T>,
        same: impl Fn(&T, &T) -> bool,
    ) -> Vec<(T, ConditionState)> {
        let mut applied = of(&self.applied);
        let Some(requested) = &self.requested else {
            return (applied.into_iter())
                .map(|c| (c, ConditionState::Applied))
                .collect();
        };
        let mut out = Vec::new();
        for c in of(requested) {
            let matched = applied.iter().position(|a| same(a, &c));
            let equal = matched.map(|i| applied.remove(i)).is_some_and(|a| a == c);
            let state = match equal {
                true => ConditionState::Applied,
                false if self.pending() => ConditionState::Adding,
                false => ConditionState::NotApplied,
            };
            out.push((c, state));
        }
        out.extend(applied.into_iter().map(|c| (c, ConditionState::Removing)));
        out
    }

    pub fn support(&self, axis: Axis, key: Key) -> Support {
        self.support[axis.index()]
            .get(&key)
            .copied()
            .unwrap_or_default()
    }

    pub fn aggregates(&self, axis: Axis, key: Key) -> &[Aggregate] {
        self.aggregates[axis.index()]
            .get(&key)
            .map_or(&[], Vec::as_slice)
    }
}

/// The grouping of `spec`, made one with no levels counting each group if
/// the table is plain.
fn counting(spec: &mut ViewSpec) -> &mut Grouping {
    spec.grouping.get_or_insert_with(|| Grouping {
        values: vec![AggregateValue::count()],
        ..Grouping::default()
    })
}

/// Makes `spec` a plain table if its grouping has no levels and no slicers.
fn plain_if_bare(spec: &mut ViewSpec) {
    let bare =
        |g: &Grouping| g.row_levels.is_empty() && g.col_levels.is_empty() && g.slicers.is_empty();
    if spec.grouping.as_ref().is_some_and(bare) {
        spec.grouping = None;
    }
}

/// What the user asked for, `wanted`, once the Table Source has answered with
/// `applied`: the source's sort and hidden Keys (pruned of those its data
/// lost), its conditions plus the wanted ones it left out, and the wanted
/// quick filter if there is one, else its own. Likewise its grouping, with
/// the wanted levels, values and slicers it left out.
fn kept(wanted: ViewSpec, applied: &ViewSpec) -> ViewSpec {
    let mut kept = applied.clone();
    for axis in Axis::ALL {
        let wanted = &wanted.axis(axis).filter;
        let filter = &mut kept.axis_mut(axis).filter;
        if wanted.quick.is_some() {
            filter.quick = wanted.quick.clone();
        }
        let have = filter.conditions.conditions();
        if let Some(list) = merged(wanted.conditions.conditions(), have, |a, b| a == b) {
            filter.conditions = ConditionTree::All(list);
        }
    }
    if let Some(wanted) = wanted.grouping {
        let mut grouping = kept.grouping.clone().unwrap_or_else(|| Grouping {
            row_levels: Vec::new(),
            col_levels: Vec::new(),
            values: Vec::new(),
            slicers: Vec::new(),
            ..wanted.clone()
        });
        let mut changed = false;
        for axis in Axis::ALL {
            let field = |a: &Level, b: &Level| a.field_key() == b.field_key();
            if let Some(list) = merged(wanted.levels(axis), grouping.levels(axis), field) {
                *grouping.levels_mut(axis) = list;
                changed = true;
            }
        }
        if let Some(list) = merged(&wanted.values, &grouping.values, |a, b| a == b) {
            grouping.values = list;
            changed = true;
        }
        if let Some(list) = merged(&wanted.slicers, &grouping.slicers, |a, b| a == b) {
            grouping.slicers = list;
            changed = true;
        }
        if changed {
            kept.grouping = Some(grouping);
        }
    }
    kept
}

/// The `wanted` parts in their order, then the `applied` ones matched to
/// none of them the `same` way, if some wanted part has no equal applied
/// one; `None` when the applied parts stand. In the wanted order, so a chip
/// the source left out stays in place.
fn merged<T: Clone + PartialEq>(
    wanted: &[T],
    applied: &[T],
    same: impl Fn(&T, &T) -> bool,
) -> Option<Vec<T>> {
    let mut extra = applied.to_vec();
    let mut missing = false;
    for w in wanted {
        match extra.iter().position(|a| same(a, w)) {
            Some(i) => missing |= extra.remove(i) != *w,
            None => missing = true,
        }
    }
    missing.then(|| wanted.iter().cloned().chain(extra).collect())
}

impl Grid {
    /// What the Table Source supports when viewing `axis` by `key`, an item
    /// of the other axis. Every item has [`Support::default`] until set
    /// otherwise.
    pub fn set_support(&mut self, axis: Axis, key: Key, support: Support) {
        let declared = &mut self.view.support[axis.index()];
        if support == Support::default() {
            declared.remove(&key);
        } else {
            declared.insert(key, support);
        }
    }

    /// The aggregates the Table Source offers when grouping `axis` with a
    /// value over `key`, a base field (an item of the other axis): none
    /// until set. A value with no field, a count, needs no declaring.
    pub fn set_aggregates(&mut self, axis: Axis, key: Key, aggregates: Vec<Aggregate>) {
        let declared = &mut self.view.aggregates[axis.index()];
        if aggregates.is_empty() {
            declared.remove(&key);
        } else {
            declared.insert(key, aggregates);
        }
    }

    /// The Table Source's answer to a `ViewRequested`, or its own news: the
    /// View Spec it now applies, which may be a part of the request, or the
    /// previous spec if it rejected it. Headers show the applied spec; the
    /// pending flags clear once every request has had its answer.
    ///
    /// `totals` gives each axis's unfiltered count, where known.
    ///
    /// Once no answer is due, what the user asked for and the source left out
    /// stays requested, shown as not applied (a red chip, say) until the user
    /// removes it or the source applies it: an invalid regex, a condition the
    /// source stopped supporting. A removal the source refused puts the
    /// condition back, and the source's sort stands.
    pub fn view_applied(&mut self, spec: ViewSpec, totals: Totals) {
        self.view.due = self.view.due.saturating_sub(1);
        self.view_changed(spec, totals);
    }

    /// The Table Source's news, not an answer to a request: the View Spec it
    /// applies now, or new totals, pushed when what it can apply changes.
    /// Answers still due stay due. A condition the user asked for that the
    /// new spec leaves out stays, shown as not applied.
    pub fn view_changed(&mut self, spec: ViewSpec, totals: Totals) {
        let wanted = self.latest_view().clone();
        let after = |spec: &ViewSpec, axis| {
            (spec.grouping.as_ref()).is_some_and(|g| g.grand_total_after(axis))
        };
        for axis in Axis::ALL {
            // A Grand Total placed after is pinned when it comes, and the
            // pin goes with it; unpinned, it stays so while it stays after.
            match (after(&self.view.applied, axis), after(&spec, axis)) {
                (false, true) => self.pin_before(axis, Edge::Trailing, &[GRAND_TOTAL], None),
                (true, false) => self.unpin(axis, GRAND_TOTAL),
                _ => {}
            }
        }
        self.view.applied = spec;
        self.view.totals = totals;
        if self.view.due == 0 {
            let kept = kept(wanted, &self.view.applied);
            self.view.requested = (kept != self.view.applied).then_some(kept);
        }
        self.pending_damage.selection = true;
    }

    /// Whether `axis` has a quick filter, and how it matches: hidden by the
    /// adapters when `None`, the default.
    pub fn set_quick_filter_support(&mut self, axis: Axis, support: Option<TextSupport>) {
        self.view.quick_filter[axis.index()] = support;
    }

    /// Sets the quick filter of `axis` to `query`, or clears it with `None`
    /// or a query with no text, and requests the result.
    pub fn set_quick_filter(&mut self, axis: Axis, query: Option<TextQuery>) {
        let query = query.filter(|q| !q.text.is_empty());
        if self.latest_view().axis(axis).filter.quick == query {
            return;
        }
        self.edit_view(ViewEdit::SetQuickFilter { axis, query });
    }

    /// Adds `condition` to the conditions of `axis` and requests the result.
    /// A Text condition with no text asks for nothing.
    pub fn add_condition(&mut self, axis: Axis, condition: Condition) {
        if condition.is_empty() {
            return;
        }
        self.edit_view(ViewEdit::AddCondition { axis, condition });
    }

    /// Removes the first condition of `axis` equal to `condition`, if the
    /// latest request has one, and requests the result.
    pub fn remove_condition(&mut self, axis: Axis, condition: &Condition) {
        let tree = &self.latest_view().axis(axis).filter.conditions;
        if tree.find(condition).is_none() {
            return;
        }
        let condition = condition.clone();
        self.edit_view(ViewEdit::RemoveCondition { axis, condition });
    }

    /// Puts `to` in place of the first condition of `axis` equal to `from`
    /// and requests the result: adds `to` if there is no such condition,
    /// removes `from` if `to` asks for nothing, and asks for nothing if they
    /// are equal.
    pub fn replace_condition(&mut self, axis: Axis, from: &Condition, to: Condition) {
        let tree = &self.latest_view().axis(axis).filter.conditions;
        if tree.find(from).is_none() {
            self.add_condition(axis, to);
        } else if to.is_empty() {
            self.remove_condition(axis, from);
        } else if *from != to {
            let from = from.clone();
            self.edit_view(ViewEdit::ReplaceCondition { axis, from, to });
        }
    }

    /// Requests the whole of `spec`, say to restore a saved view, with the
    /// View Edit [`ViewEdit::Replace`].
    pub fn request_view(&mut self, spec: ViewSpec) {
        self.request(spec, ViewEdit::Replace);
    }

    /// Requests the latest View Spec again, with the View Edit
    /// [`ViewEdit::Reapply`]: the Table Source filters rows the user's edits
    /// left in view though they no longer match.
    pub fn reapply_view(&mut self) {
        self.edit_view(ViewEdit::Reapply);
    }

    /// Whether the Table Source hides items of `axis`: the adapters offer
    /// hiding only then. Not by default.
    pub fn set_hide_support(&mut self, axis: Axis, hides: bool) {
        self.view.hides[axis.index()] = hides;
    }

    /// The most hidden Keys the Table Source takes on `axis`, at most
    /// [`HIDE_CAP`] (the default): past it, hiding is refused and adapters
    /// suggest a filter, which scales.
    pub fn set_hide_cap(&mut self, axis: Axis, cap: usize) {
        self.view.hide_caps[axis.index()] = (cap < HIDE_CAP).then_some(cap);
    }

    /// Hides the items of `axis` with `keys`, adding those not hidden yet to
    /// its hidden Keys, and requests the result. Nothing happens while the
    /// Table Source can't hide `axis`. If the hidden Keys would then number
    /// more than the axis's hide cap, nothing is hidden and nothing asked
    /// for: the error carries the cap, so the adapter can suggest a filter.
    pub fn hide(&mut self, axis: Axis, keys: &[Key]) -> Result<(), HideCapped> {
        if !self.view.hides[axis.index()] {
            return Ok(());
        }
        let hidden = &self.latest_view().axis(axis).hidden;
        let mut new: Vec<Key> = Vec::new();
        for &key in keys {
            if !hidden.contains(&key) && !new.contains(&key) {
                new.push(key);
            }
        }
        let cap = self.view.hide_cap(axis);
        if hidden.len() + new.len() > cap {
            return Err(HideCapped { cap });
        }
        if !new.is_empty() {
            self.edit_view(ViewEdit::Hide { axis, keys: new });
        }
        Ok(())
    }

    /// Shows the hidden items of `axis` with `keys` again and requests the
    /// result. Keys not hidden are skipped.
    pub fn show(&mut self, axis: Axis, keys: &[Key]) {
        let hidden = &self.latest_view().axis(axis).hidden;
        let keys: Vec<Key> = hidden
            .iter()
            .copied()
            .filter(|k| keys.contains(k))
            .collect();
        if !keys.is_empty() {
            self.edit_view(ViewEdit::Show { axis, keys });
        }
    }

    /// Groups `axis` by `field`, a base field, as its innermost level, and
    /// requests the result. Nothing happens if the axis is grouped by it
    /// already or the Table Source can't group the axis by it.
    pub fn group_by(&mut self, axis: Axis, field: Key) {
        let grouped = self
            .latest_view()
            .grouping
            .as_ref()
            .is_some_and(|g| g.groups_by(axis, field));
        if grouped || !self.view.support(axis, field).group {
            return;
        }
        self.edit_view(ViewEdit::Group { axis, field });
    }

    /// Adds `condition` to the slicers, which filter the base fields before
    /// aggregating, and requests the result. A Text or Native condition
    /// with no text asks for nothing.
    pub fn add_slicer(&mut self, condition: Condition) {
        if condition.is_empty() {
            return;
        }
        self.edit_view(ViewEdit::AddSlicer { condition });
    }

    /// Removes the first slicer equal to `condition`, if the latest request
    /// has one, and requests the result.
    pub fn remove_slicer(&mut self, condition: &Condition) {
        if !self.slicers().contains(condition) {
            return;
        }
        let condition = condition.clone();
        self.edit_view(ViewEdit::RemoveSlicer { condition });
    }

    /// Puts `to` in place of the first slicer equal to `from` and requests
    /// the result, as [`Grid::replace_condition`] does for conditions.
    pub fn replace_slicer(&mut self, from: &Condition, to: Condition) {
        if !self.slicers().contains(from) {
            self.add_slicer(to);
        } else if to.is_empty() {
            self.remove_slicer(from);
        } else if *from != to {
            let from = from.clone();
            self.edit_view(ViewEdit::ReplaceSlicer { from, to });
        }
    }

    /// The slicers of the latest request.
    fn slicers(&self) -> &[Condition] {
        (self.latest_view().grouping.as_ref()).map_or(&[], |g| &g.slicers)
    }

    /// Removes every level of `axis` and requests the result. Nothing
    /// happens if the axis is not grouped.
    pub fn ungroup(&mut self, axis: Axis) {
        let grouped = self
            .latest_view()
            .grouping
            .as_ref()
            .is_some_and(|g| !g.levels(axis).is_empty());
        if grouped {
            self.edit_view(ViewEdit::Ungroup { axis });
        }
    }

    /// The View Spec the latest request asks for, else the applied one.
    pub(crate) fn latest_view(&self) -> &ViewSpec {
        self.view.requested.as_ref().unwrap_or(&self.view.applied)
    }

    /// Items were shown or hidden on `axis`: requests the same spec again if
    /// a quick filter searches the shown ones.
    pub(crate) fn shown_changed(&mut self, axis: Axis) {
        let quick = &self.latest_view().axis(axis.other()).filter.quick;
        if quick
            .as_ref()
            .is_some_and(|q| q.searches == Searched::Shown)
        {
            self.edit_view(ViewEdit::ShownChanged { axis });
        }
    }

    /// Applies `edit` to the latest requested View Spec (or the applied one)
    /// and requests the result.
    fn edit_view(&mut self, edit: ViewEdit) {
        let mut spec = self.latest_view().clone();
        match &edit {
            ViewEdit::CycleSort { axis, key, multi } => {
                let sort = &mut spec.axis_mut(*axis).sort;
                *sort = sort.cycled(*key, *multi);
            }
            ViewEdit::SetQuickFilter { axis, query } => {
                spec.axis_mut(*axis).filter.quick = query.clone();
            }
            ViewEdit::AddCondition { axis, condition } => {
                let tree = &mut spec.axis_mut(*axis).filter.conditions;
                tree.list_mut().push(condition.clone());
            }
            ViewEdit::RemoveCondition { axis, condition } => {
                let tree = &mut spec.axis_mut(*axis).filter.conditions;
                if let Some(i) = tree.find(condition) {
                    tree.list_mut().remove(i);
                }
            }
            ViewEdit::ReplaceCondition { axis, from, to } => {
                let tree = &mut spec.axis_mut(*axis).filter.conditions;
                if let Some(i) = tree.find(from) {
                    tree.list_mut()[i] = to.clone();
                }
            }
            ViewEdit::Hide { axis, keys } => {
                spec.axis_mut(*axis).hidden.extend(keys);
            }
            ViewEdit::Show { axis, keys } => {
                spec.axis_mut(*axis).hidden.retain(|k| !keys.contains(k));
            }
            ViewEdit::Group { axis, field } => {
                counting(&mut spec)
                    .levels_mut(*axis)
                    .push(Level::field(*field));
            }
            ViewEdit::Ungroup { axis } => {
                if let Some(grouping) = &mut spec.grouping {
                    grouping.levels_mut(*axis).clear();
                }
                plain_if_bare(&mut spec);
            }
            ViewEdit::AddSlicer { condition } => {
                counting(&mut spec).slicers.push(condition.clone());
            }
            ViewEdit::RemoveSlicer { condition } => {
                if let Some(grouping) = &mut spec.grouping
                    && let Some(i) = grouping.slicers.iter().position(|c| c == condition)
                {
                    grouping.slicers.remove(i);
                }
                plain_if_bare(&mut spec);
            }
            ViewEdit::ReplaceSlicer { from, to } => {
                if let Some(grouping) = &mut spec.grouping
                    && let Some(c) = grouping.slicers.iter_mut().find(|c| *c == from)
                {
                    *c = to.clone();
                }
            }
            ViewEdit::Replace
            | ViewEdit::ShownChanged { .. }
            | ViewEdit::Reapply
            | ViewEdit::Expand { .. }
            | ViewEdit::Collapse { .. }
            | ViewEdit::ExpandAll { .. }
            | ViewEdit::ExpandTo { .. } => {}
        }
        self.request(spec, edit);
    }

    pub(crate) fn request(&mut self, spec: ViewSpec, edit: ViewEdit) {
        self.view.requested = Some(spec.clone());
        self.view.due += 1;
        self.pending_damage.selection = true;
        self.push(Effect::ViewRequested { spec, edit });
    }

    /// A Sort header click on the header of the item at `position` on
    /// `axis`: cycles the other axis's sort by that item (the rows' by a
    /// column, the columns' by a row).
    pub(crate) fn sort_click(&mut self, axis: Axis, position: Position, modifiers: Modifiers) {
        let Some(key) = self.axis(axis).key_at(position) else {
            return;
        };
        let sorted = axis.other();
        if !self.view.support(sorted, key).sort {
            return;
        }
        self.edit_view(ViewEdit::CycleSort {
            axis: sorted,
            key,
            multi: modifiers.shift,
        });
    }
}
