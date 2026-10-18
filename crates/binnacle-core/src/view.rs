use std::collections::BTreeMap;

use crate::effect::Effect;
use crate::grid::Grid;
use crate::pointer::Modifiers;
use crate::sort::{HeaderSort, SortSpec};
use crate::types::{Axis, Key, Position};

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

/// What a condition asks of the values in its item.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionKind {
    /// The value matches the Text Query, which searches the condition's item.
    Text(TextQuery),
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

    /// This condition, or with `not` its opposite.
    pub fn not(mut self, not: bool) -> Self {
        self.not = not;
        self
    }

    /// A Text condition with no text, which asks for nothing.
    fn is_empty(&self) -> bool {
        match &self.kind {
            ConditionKind::Text(query) => query.text.is_empty(),
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
/// order them, each named by Keys on the other axis.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct AxisView {
    pub filter: FilterSpec,
    pub sort: SortSpec,
}

impl AxisView {
    pub fn is_empty(&self) -> bool {
        self.filter.is_empty() && self.sort.is_empty()
    }
}

/// The declared view a Table Source should present for the whole table, one
/// part per axis. The default is the Table Source's base view: nothing
/// filtered, unsorted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ViewSpec {
    pub rows: AxisView,
    pub cols: AxisView,
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

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.cols.is_empty()
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
}

impl Default for Support {
    fn default() -> Self {
        Support {
            sort: true,
            text: Some(TextSupport::default()),
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
}

/// How the header of an item shows the conditions read in it: `filtered`
/// while the applied View Spec has one, `pending` while the latest request
/// would change them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HeaderFilter {
    pub filtered: bool,
    pub pending: bool,
}

/// Where a condition stands between the applied View Spec and the latest
/// request, as [`crate::Snapshot::conditions`] lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConditionState {
    /// Applied, and still requested.
    Applied,
    /// Requested, not yet applied.
    Adding,
    /// Applied, no longer requested.
    Removing,
}

/// The Grid's view state: the View Spec the Table Source last applied and the
/// one the user last asked for while answers are still due.
#[derive(Clone, Debug, Default)]
pub(crate) struct ViewState {
    pub applied: ViewSpec,
    /// The latest request, kept while any `view_applied` is still due.
    pub requested: Option<ViewSpec>,
    due: u32,
    /// Per axis viewed, the items of the other axis whose support differs
    /// from the default (sparse, set by the host).
    support: [BTreeMap<Key, Support>; 2],
    /// Per axis, its quick filter's support, if it has one.
    pub quick_filter: [Option<TextSupport>; 2],
    /// The unfiltered counts from the last `view_applied`.
    pub totals: Totals,
}

impl ViewState {
    /// How the header of `key` shows the sort of the other axis.
    pub fn header(&self, header_axis: Axis, key: Key) -> HeaderSort {
        let sorted = header_axis.other();
        let place = |spec: &ViewSpec| spec.axis(sorted).sort.find(key).map(|(i, d)| (d, i));
        let applied = place(&self.applied);
        let pending = self.requested.as_ref().is_some_and(|r| place(r) != applied);
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
        let pending = self.requested.as_ref().is_some_and(|r| on(r) != applied);
        HeaderFilter {
            filtered: !applied.is_empty(),
            pending,
        }
    }

    /// The conditions of `axis`: the latest request's, each `Applied` or
    /// `Adding`, then the applied ones it drops, `Removing`.
    pub fn conditions(&self, axis: Axis) -> Vec<(Condition, ConditionState)> {
        let mut applied: Vec<&Condition> = self
            .applied
            .axis(axis)
            .filter
            .conditions
            .conditions()
            .iter()
            .collect();
        let Some(requested) = &self.requested else {
            return applied
                .into_iter()
                .map(|c| (c.clone(), ConditionState::Applied))
                .collect();
        };
        let mut out = Vec::new();
        for c in requested.axis(axis).filter.conditions.conditions() {
            let state = match applied.iter().position(|a| *a == c) {
                Some(i) => {
                    applied.remove(i);
                    ConditionState::Applied
                }
                None => ConditionState::Adding,
            };
            out.push((c.clone(), state));
        }
        out.extend(
            applied
                .into_iter()
                .map(|c| (c.clone(), ConditionState::Removing)),
        );
        out
    }

    pub fn support(&self, axis: Axis, key: Key) -> Support {
        self.support[axis.index()]
            .get(&key)
            .copied()
            .unwrap_or_default()
    }
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

    /// The Table Source's answer to a `ViewRequested`, or its own news: the
    /// View Spec it now applies, which may be a part of the request, or the
    /// previous spec if it rejected it. Headers show the applied spec; the
    /// pending flags clear once every request has had its answer.
    ///
    /// `totals` gives each axis's unfiltered count, where known.
    pub fn view_applied(&mut self, spec: ViewSpec, totals: Totals) {
        self.view.applied = spec;
        self.view.totals = totals;
        self.view.due = self.view.due.saturating_sub(1);
        if self.view.due == 0 {
            self.view.requested = None;
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

    /// The View Spec the latest request asks for, else the applied one.
    fn latest_view(&self) -> &ViewSpec {
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
            ViewEdit::Replace | ViewEdit::ShownChanged { .. } => {}
        }
        self.request(spec, edit);
    }

    fn request(&mut self, spec: ViewSpec, edit: ViewEdit) {
        self.view.requested = Some(spec.clone());
        self.view.due += 1;
        self.pending_damage.selection = true;
        self.push(Effect::ViewRequested { spec, edit });
    }

    /// A Sort header click on column header `position`: cycles the rows'
    /// sort by that column. Rows never sort columns.
    pub(crate) fn sort_click(&mut self, axis: Axis, position: Position, modifiers: Modifiers) {
        if axis != Axis::Cols {
            return;
        }
        let Some(key) = self.axis(axis).measure.key_at(position) else {
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
