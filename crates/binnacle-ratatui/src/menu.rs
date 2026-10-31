//! The header menu: a popup under the focused column's header (or under the
//! focused row) with Pin left and Pin right (Pin top and Pin bottom for a
//! row), or Unpin on a pinned item, and Freeze up to here; then Group rows
//! by a column the rows group by, or Ungroup rows once grouped, and while the
//! Table Source groups, Slice by a column that takes conditions; then, where the
//! axis hides, Hide, Show only these and Choose, the axis's hidden items
//! inline, numbered, and Show all, each shown again in its place. Hide and
//! Show only act on the picks or the selection as well as the focused item,
//! and past the axis's hide cap suggest a filter.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use binnacle_core::{
    AggregateValue, Axis, Depth, Edge, Grid, Grouping, ItemKind, Key, Position, Snapshot,
    VALUES_FIELD,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::filter::{grouped, label};
use crate::width::{fit, put_text, width as text_width};
use crate::{Content, Theme};

const WIDTH: u16 = 40;
/// How many hidden items show at once; the list scrolls past them.
const LIST_ROWS: usize = 8;

/// Hides the focused item of `axis` ([`crate::Control::Hide`]): `false` if
/// the axis does not hide, nothing there is focused, or the axis's hidden
/// items would pass its cap.
pub fn hide_focused(grid: &mut Grid, axis: Axis) -> bool {
    let snapshot = grid.snapshot();
    let key = snapshot.focus(axis).and_then(|f| f.key);
    match key.filter(|_| snapshot.hide_support(axis)) {
        Some(key) => grid.hide(axis, &[key]).is_ok(),
        None => false,
    }
}

/// What a hide or "show only" did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hid {
    /// Asked the Table Source to hide this many items.
    Hidden(usize),
    /// Nothing hidden: the axis's hidden items would number more than its
    /// cap, so the host suggests a filter ([`cap_hint`]).
    Capped(usize),
    /// Nothing to hide, or the axis does not hide.
    Nothing,
}

/// The items of `axis` that Hide and "show only" act on: the picks if there
/// are any, else the items the selection spans, else the focused item. A
/// selection comes as runs of Positions, since the Grid knows Keys only in
/// its Window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    Keys(Vec<Key>),
    Runs(Vec<Range<Position>>),
}

impl Chosen {
    /// What `snapshot` has chosen on `axis`, if anything.
    pub fn of(snapshot: &Snapshot, axis: Axis) -> Option<Chosen> {
        let picked = snapshot.picked(axis);
        if !picked.is_empty() {
            return Some(Chosen::Keys(picked.to_vec()));
        }
        if let Some(range) = snapshot.selection_range()
            && range.len(axis) > 1
        {
            return Some(Chosen::Runs(range.span(axis).to_vec()));
        }
        let key = snapshot.focus(axis).and_then(|f| f.key)?;
        Some(Chosen::Keys(vec![key]))
    }

    /// How many items it names.
    pub fn len(&self) -> u64 {
        match self {
            Chosen::Keys(keys) => keys.len() as u64,
            Chosen::Runs(runs) => runs.iter().map(|r| r.end - r.start).sum(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn keys(
        self,
        axis: Axis,
        mut keys_at: impl FnMut(Axis, Range<Position>) -> Vec<Key>,
    ) -> Vec<Key> {
        match self {
            Chosen::Keys(keys) => keys,
            Chosen::Runs(runs) => runs.into_iter().flat_map(|r| keys_at(axis, r)).collect(),
        }
    }
}

/// Hides the chosen items of `axis` ([`Chosen`], [`crate::Control::Hide`]).
/// `keys_at` gives the Keys at a run of Positions, from the Table Source;
/// it is asked only for a selection within the axis's hide cap.
pub fn hide_chosen(
    grid: &mut Grid,
    axis: Axis,
    keys_at: impl FnMut(Axis, Range<Position>) -> Vec<Key>,
) -> Hid {
    let snapshot = grid.snapshot();
    let Some(chosen) = Chosen::of(&snapshot, axis).filter(|_| snapshot.hide_support(axis)) else {
        return Hid::Nothing;
    };
    let cap = snapshot.hide_cap(axis);
    if snapshot.hidden(axis).len() as u64 + chosen.len() > cap as u64 {
        return Hid::Capped(cap);
    }
    let keys = chosen.keys(axis, keys_at);
    hide(grid, axis, &keys)
}

/// Shows only the chosen items of `axis`: hides every other item shown at
/// that moment, so items arriving later are shown. `keys_at` gives the Keys
/// at a run of Positions; it is asked only within the axis's hide cap.
pub fn show_only_chosen(
    grid: &mut Grid,
    axis: Axis,
    keys_at: impl FnMut(Axis, Range<Position>) -> Vec<Key>,
) -> Hid {
    let snapshot = grid.snapshot();
    let Some(chosen) = Chosen::of(&snapshot, axis).filter(|_| snapshot.hide_support(axis)) else {
        return Hid::Nothing;
    };
    let cap = snapshot.hide_cap(axis) as u64;
    let others = snapshot.count(axis).saturating_sub(chosen.len());
    if snapshot.hidden(axis).len() as u64 + others > cap {
        return Hid::Capped(cap as usize);
    }
    let mut keys_at = keys_at;
    let count = snapshot.count(axis);
    // A selection's others lie between its runs; picks are within the cap of
    // all there is.
    let (keep, gaps) = match chosen {
        Chosen::Keys(keys) => (keys, std::iter::once(0..count).collect()),
        Chosen::Runs(runs) => {
            let mut gaps = Vec::new();
            let mut at = 0;
            for r in runs {
                gaps.push(at..r.start);
                at = r.end;
            }
            gaps.push(at..count);
            (Vec::new(), gaps)
        }
    };
    let others: Vec<Key> = (gaps.into_iter())
        .filter(|g| g.start < g.end)
        .flat_map(|g| keys_at(axis, g))
        .filter(|k| !keep.contains(k))
        .collect();
    hide(grid, axis, &others)
}

fn hide(grid: &mut Grid, axis: Axis, keys: &[Key]) -> Hid {
    match grid.hide(axis, keys) {
        Ok(()) if keys.is_empty() => Hid::Nothing,
        Ok(()) => Hid::Hidden(keys.len()),
        Err(capped) => Hid::Capped(capped.cap),
    }
}

/// What to tell the user when a hide on `axis` passed its cap: to filter
/// instead, since a filter scales.
pub fn cap_hint(axis: Axis, cap: usize) -> String {
    let noun = match (axis, cap) {
        (Axis::Cols, 1) => "column",
        (Axis::Cols, _) => "columns",
        (Axis::Rows, 1) => "row",
        (Axis::Rows, _) => "rows",
    };
    format!("Can't hide over {} {noun}: filter", grouped(cap as u64))
}

/// The header menu of a column or row, and the grouping menu of a level.
/// [`HeaderMenu::open`] drops it under the focused item's header with the
/// cursor on its first line; [`HeaderMenu::open_hidden`], on an axis whose
/// Table Source hides, with it on the first hidden item;
/// [`HeaderMenu::open_level`] under the corner, for a level named there.
/// While open it takes the keys: Up and Down move the cursor, Enter does
/// what it is on, `l` and `r` pin a column left and right (`t` and `b` a row
/// to the top and bottom), `u` unpins it (offered wherever it is pinned,
/// clipped or not), `f` freezes up to it, `g` groups the rows by the column
/// (or ungroups them once grouped), `p` adds it as a column level, `h` hides
/// the chosen items ([`Chosen`]), `o` shows only them, `c` asks for the
/// chooser ([`HeaderMenu::take_chooser`]), `s` for a slicer on the column
/// ([`HeaderMenu::take_slicer`]), `1` to `9` show that hidden item,
/// `a` shows them all, Esc closes. The grouping lines: `x` removes the
/// level, `m` moves it to the other axis, `-` collapses to it, `+` expands
/// through it, `v` opens the values to add and `n` the columns to group by,
/// each listed under it. Pinning, unpinning, freezing, hiding and each
/// grouping change close it; showing and opening a list leave it open, and
/// so does a hide past the cap, which says to filter instead.
///
/// The host may add [`RowItem`]s to a row's menu ([`HeaderMenu::set_row_items`],
/// or per row [`HeaderMenu::set_row_items_fn`]); choosing one queues a
/// [`RowChoice`] for the host ([`HeaderMenu::take_row_choice`]).
#[derive(Clone, Debug, Default)]
pub struct HeaderMenu {
    open: Option<Open>,
    /// The axis whose chooser the menu asked for, until the host takes it.
    chooser: Option<Axis>,
    /// The column the menu asked to slice by, until the host takes it.
    slicer: Option<Key>,
    /// The base fields the grouping may use, as the host lists them.
    fields: Vec<Key>,
    /// The items the host adds to a row's menu, asked per row.
    row_items: RowItems,
    /// The host item the user chose, until the host takes it.
    row_choice: Option<RowChoice>,
}

/// An item a host adds to a row's menu ([`HeaderMenu::set_row_items`]): a
/// name the host knows it by, a label, and a key that picks it while the
/// menu is open. A key that a built-in row line uses is silently ignored,
/// and not drawn. Built with [`RowItem::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RowItem {
    pub name: String,
    pub label: String,
    pub key: Option<char>,
}

impl RowItem {
    /// An item named `name` and drawn as `label`, with no key.
    pub fn new(name: impl Into<String>, label: impl Into<String>) -> Self {
        RowItem {
            name: name.into(),
            label: label.into(),
            key: None,
        }
    }

    /// The item, picked by `key` while the menu is open.
    pub fn key(mut self, key: char) -> Self {
        self.key = Some(key);
        self
    }
}

/// The host item the user chose on a row's menu: its name and the Key of
/// the row whose menu it is (never the chosen set's).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RowChoice {
    pub name: String,
    pub row: Key,
}

/// The row a row's menu is open on, as [`HeaderMenu::set_row_items_fn`]
/// asks for its items: a plain row, or on a grouped axis a Leaf, or a
/// Summary Item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RowTarget {
    /// A row of the data, by its Key.
    Row(Key),
    /// A Group Item, a Subtotal or the Grand Total, by its Key.
    Summary(Key, ItemKind),
}

impl RowTarget {
    /// The row's Key, whatever it is.
    pub fn key(self) -> Key {
        match self {
            RowTarget::Row(key) | RowTarget::Summary(key, _) => key,
        }
    }
}

type RowItemsFn = dyn Fn(RowTarget) -> Vec<RowItem> + Send + Sync;

/// The host's items per row; none until it sets them.
#[derive(Clone)]
struct RowItems(Arc<RowItemsFn>);

impl Default for RowItems {
    fn default() -> Self {
        RowItems(Arc::new(|_| Vec::new()))
    }
}

impl fmt::Debug for RowItems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RowItems(..)")
    }
}

/// The keys the row menu's own lines and hidden-item numbers use. A host
/// item's key that is one of these is ignored: the built-in wins. The test
/// at the foot of this file guards the string; a new row line needs its key here.
const ROW_MENU_KEYS: &str = "tbufghoca123456789";

#[derive(Clone, Debug)]
struct Open {
    /// The item's own axis (the columns for a column's menu), or the
    /// level's.
    axis: Axis,
    target: Target,
    /// An index into [`entries`].
    cursor: usize,
    /// Why the last hide hid nothing.
    hint: Option<String>,
    /// Whether the values to add, and the columns to group by, are listed.
    adding: bool,
    others: bool,
}

impl Open {
    fn new(axis: Axis, target: Target) -> Self {
        Open {
            axis,
            target,
            cursor: 0,
            hint: None,
            adding: false,
            others: false,
        }
    }
}

/// What a menu is for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    /// An item of the axis, by its Key, and on the rows what it is.
    Item(Key, Option<RowTarget>),
    /// A level of the axis, by its field (`None` for the values
    /// pseudo-level).
    Level(Option<Key>),
}

/// What a line of the menu does.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Pin(Edge),
    Unpin,
    Freeze,
    /// Groups the axis by the field, one level more.
    Group(Axis, Key),
    Ungroup,
    Slice,
    RemoveLevel(Axis, Key),
    MoveLevel(Axis, Option<Key>),
    /// Opens the axis's groups down to the level at this index.
    CollapseTo(Axis, u32, Key),
    ExpandThrough(Axis, u32, Key),
    RemoveValue(AggregateValue),
    /// Lists the values to add, or stops listing them.
    Adding,
    AddValue(AggregateValue),
    /// Lists the columns no level uses, or stops listing them.
    Others,
    Hide,
    ShowOnly,
    Choose,
    Show(Key),
    ShowAll,
    /// The host's item at this index in the row's items.
    Host(usize),
}

impl Entry {
    /// The key that picks it, if any, but for a hidden item's number. `g`
    /// ungroups the rows while the menu offers that, else groups them by
    /// the item.
    fn key(&self, axis: Axis, item: Option<Key>, ungroups: bool) -> Option<char> {
        Some(match (self, axis) {
            (Entry::Pin(Edge::Leading), Axis::Cols) => 'l',
            (Entry::Pin(Edge::Trailing), Axis::Cols) => 'r',
            (Entry::Pin(Edge::Leading), Axis::Rows) => 't',
            (Entry::Pin(Edge::Trailing), Axis::Rows) => 'b',
            (Entry::Unpin, _) => 'u',
            (Entry::Freeze, _) => 'f',
            (Entry::Group(Axis::Rows, f), _) if Some(*f) == item && !ungroups => 'g',
            (Entry::Group(Axis::Cols, f), _) if Some(*f) == item => 'p',
            (Entry::Ungroup, _) => 'g',
            (Entry::RemoveLevel(..), _) => 'x',
            (Entry::MoveLevel(..), _) => 'm',
            (Entry::CollapseTo(..), _) => '-',
            (Entry::ExpandThrough(..), _) => '+',
            (Entry::Adding, _) => 'v',
            (Entry::Others, _) => 'n',
            (Entry::Slice, _) => 's',
            (Entry::Hide, _) => 'h',
            (Entry::ShowOnly, _) => 'o',
            (Entry::Choose, _) => 'c',
            (Entry::Show(_) | Entry::ShowAll, _) => 'a',
            _ => return None,
        })
    }

    /// Whether it is listed under Add a value or Group by another column.
    fn listed(&self, item: Option<Key>) -> bool {
        match self {
            Entry::AddValue(_) => true,
            Entry::Group(_, f) => Some(*f) != item,
            _ => false,
        }
    }
}

/// The grouping the latest request asked for, else the one applied.
fn grouping<'s>(snapshot: &Snapshot<'s>) -> Option<&'s Grouping> {
    let view = snapshot.requested_view().unwrap_or(snapshot.applied_view());
    view.grouping.as_ref()
}

/// The menu's lines for `open`, in order, as the web's menu lists them.
/// An item's menu: Pin at the leading edge, or Unpin wherever it is pinned;
/// Pin at the trailing edge unless pinned; Freeze unless pinned at the
/// trailing edge; the grouping lines of a column (below); Ungroup rows once
/// the rows are grouped; on a column that takes conditions while the source
/// groups (the rows are grouped or sliced, or a column shown groups them),
/// Slice by it; then, on an axis that hides, Hide, Show only and Choose,
/// each hidden item and Show all. A level's menu: its grouping lines alone.
///
/// The grouping lines of a column of the table: Group rows by it and Add it
/// as a column level where the source supports them and no level uses it,
/// and while grouped Add a value, listing the aggregates the source offers
/// over it. Of a pivot's column, the lines of the column level it is a
/// group of (a value's column, the innermost field level). Of a level:
/// Remove level and Move to the other axis (where the source supports
/// it), Collapse to it, and Expand through it unless it is a pivot's
/// innermost; the values pseudo-level only moves. Then Remove value for
/// each value while there are two or more, Add a value (a count, and the
/// aggregates the source offers over `fields`), and Group by another column
/// with the `fields` no level uses. Aggregates that take parameters are
/// left out: the menu has nowhere to ask for them.
fn entries(snapshot: &Snapshot, open: &Open, fields: &[Key]) -> Vec<Entry> {
    let axis = open.axis;
    let grouping = grouping(snapshot);
    let levels = |axis| grouping.map_or(&[][..], |g| g.levels(axis));
    let used = |field| {
        [Axis::Rows, Axis::Cols]
            .iter()
            .any(|&a| levels(a).iter().any(|l| l.field_key() == Some(field)))
    };
    let pivot = !levels(Axis::Cols).is_empty();
    let values = grouping.map_or(&[][..], |g| &g.values[..]);
    let addable = |field: Key| -> Vec<Entry> {
        (snapshot.aggregates(Axis::Rows, field).iter())
            .filter(|a| a.params.is_empty())
            .map(|a| AggregateValue::new(Some(field), &a.name))
            .filter(|v| !values.contains(v))
            .map(Entry::AddValue)
            .collect()
    };
    let mut entries = Vec::new();
    // Whether the level's lines follow, and the level they are for: none
    // for a pivot's Grand Total, the values pseudo-level's `None`.
    let (lines, level) = match open.target {
        Target::Item(key, _) => {
            let pinned_at = |edge| (snapshot.pinned(axis, edge).iter()).any(|i| i.key == Some(key));
            let trailing = pinned_at(Edge::Trailing);
            match pinned_at(Edge::Leading) || trailing {
                true => entries.push(Entry::Unpin),
                false => entries.extend([Entry::Pin(Edge::Leading), Entry::Pin(Edge::Trailing)]),
            }
            if !trailing {
                entries.push(Entry::Freeze);
            }
            match axis == Axis::Cols && pivot {
                // The column level the pivot's column is a group of.
                true => {
                    let at = snapshot
                        .strip(Axis::Cols)
                        .iter()
                        .find(|i| i.key == Some(key));
                    let outline = at.and_then(|i| snapshot.outline(Axis::Cols, i.position));
                    let level = match outline.and_then(|o| o.group.as_ref()).map(|g| g.field) {
                        Some(VALUES_FIELD) => levels(Axis::Cols)
                            .iter()
                            .rev()
                            .find_map(|l| l.field_key())
                            .map(Some),
                        field => field.map(Some),
                    };
                    (true, level)
                }
                false if axis == Axis::Cols => {
                    let support = snapshot.support(Axis::Rows, key);
                    if support.group && !used(key) {
                        entries.push(Entry::Group(Axis::Rows, key));
                    }
                    if support.pivot && !used(key) {
                        entries.push(Entry::Group(Axis::Cols, key));
                    }
                    let adding = addable(key);
                    if grouping.is_some() && !adding.is_empty() {
                        entries.push(Entry::Adding);
                        if open.adding {
                            entries.extend(adding);
                        }
                    }
                    (false, None)
                }
                false => (false, None),
            }
        }
        Target::Level(field) => (true, Some(field)),
    };
    if lines {
        let levels = levels(axis);
        let at = level.and_then(|field| levels.iter().position(|l| l.field_key() == field));
        let other = axis.other();
        match (level.flatten(), at) {
            (Some(field), Some(at)) => {
                let support = snapshot.support(Axis::Rows, field);
                entries.push(Entry::RemoveLevel(axis, field));
                if (other == Axis::Cols && support.pivot) || (other == Axis::Rows && support.group)
                {
                    entries.push(Entry::MoveLevel(axis, Some(field)));
                }
                entries.push(Entry::CollapseTo(axis, at as u32, field));
                // A pivot's innermost groups are its items: nothing under them to open.
                if !(pivot && at + 1 == levels.len()) {
                    entries.push(Entry::ExpandThrough(axis, at as u32 + 1, field));
                }
            }
            (None, Some(_)) => entries.push(Entry::MoveLevel(axis, None)),
            _ => {}
        }
        if values.len() > 1 {
            entries.extend(values.iter().cloned().map(Entry::RemoveValue));
        }
        let count = AggregateValue::count();
        let mut adding: Vec<Entry> = match values.contains(&count) {
            true => Vec::new(),
            false => vec![Entry::AddValue(count)],
        };
        adding.extend(fields.iter().flat_map(|&f| addable(f)));
        if !adding.is_empty() {
            entries.push(Entry::Adding);
            if open.adding {
                entries.extend(adding);
            }
        }
        let others: Vec<Entry> = (fields.iter().filter(|&&f| !used(f)))
            .flat_map(|&f| {
                let support = snapshot.support(Axis::Rows, f);
                [(support.group, Axis::Rows), (support.pivot, Axis::Cols)]
                    .into_iter()
                    .filter(|&(on, _)| on)
                    .map(move |(_, axis)| Entry::Group(axis, f))
            })
            .collect();
        if !others.is_empty() {
            entries.push(Entry::Others);
            if open.others {
                entries.extend(others);
            }
        }
    }
    if let Target::Item(key, _) = open.target {
        if grouping.is_some_and(|g| !g.levels(Axis::Rows).is_empty()) {
            entries.push(Entry::Ungroup);
        }
        let groups = grouping.is_some()
            || !snapshot.slicers().is_empty()
            || !snapshot.levels(Axis::Rows).is_empty()
            || (snapshot.cols().iter())
                .any(|c| c.key.is_some_and(|k| snapshot.support(Axis::Rows, k).group));
        let support = snapshot.support(Axis::Rows, key);
        let conditions = support.text.is_some() || support.one_of || support.compare.is_some();
        if axis == Axis::Cols && groups && conditions {
            entries.push(Entry::Slice);
        }
        if snapshot.hide_support(axis) {
            entries.extend([Entry::Hide, Entry::ShowOnly, Entry::Choose]);
            let hidden = snapshot.hidden(axis);
            entries.extend(hidden.iter().map(|&k| Entry::Show(k)));
            if !hidden.is_empty() {
                entries.push(Entry::ShowAll);
            }
        }
    }
    entries
}

impl HeaderMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Sets the base fields the grouping may use (columns of the table, by
    /// Key): the grouping menu offers values over them and groups by those
    /// no level uses. A pivot's columns are its groups, so only fields
    /// listed here are offered there.
    pub fn set_fields(&mut self, fields: Vec<Key>) {
        self.fields = fields;
    }

    /// The fields to offer: those set, else the columns in view while the
    /// columns are not grouped.
    fn fields(&self, snapshot: &Snapshot) -> Vec<Key> {
        if !self.fields.is_empty() {
            return self.fields.clone();
        }
        match grouping(snapshot).is_some_and(|g| !g.levels(Axis::Cols).is_empty()) {
            true => Vec::new(),
            false => snapshot
                .strip(Axis::Cols)
                .iter()
                .filter_map(|i| i.key)
                .collect(),
        }
    }

    /// Sets the items a plain row's menu lists after its own, below a
    /// separator: [`HeaderMenu::set_row_items_fn`] with `items` for a
    /// [`RowTarget::Row`] and none for a Summary Item.
    pub fn set_row_items(&mut self, items: Vec<RowItem>) {
        self.set_row_items_fn(move |target| match target {
            RowTarget::Row(_) => items.clone(),
            _ => Vec::new(),
        });
    }

    /// Sets the items a row's menu lists after its own, below a separator,
    /// asked of `items` for the row the menu is open on whenever the menu
    /// draws or takes a key. They show only when the menu is open on a row,
    /// and the menu lists them even where no built-in line applies. An
    /// item's `key` picks it, unless it is a key a built-in line of the row
    /// menu uses (`t`, `b`, `u`, `f`, `g`, `h`, `o`, `c`, `a` or a digit):
    /// the built-in wins, and the item is drawn without it. Of two items
    /// with one key, the first wins.
    pub fn set_row_items_fn(
        &mut self,
        items: impl Fn(RowTarget) -> Vec<RowItem> + Send + Sync + 'static,
    ) {
        self.row_items = RowItems(Arc::new(items));
    }

    /// The host item the user chose on a row's menu, once. A choice not taken
    /// before the next is replaced by it.
    pub fn take_row_choice(&mut self) -> Option<RowChoice> {
        self.row_choice.take()
    }

    /// The host items `open` lists: those of a row's menu.
    fn hosts(&self, open: &Open) -> Vec<RowItem> {
        match open.target {
            Target::Item(_, Some(row)) => (self.row_items.0)(row),
            _ => Vec::new(),
        }
    }

    /// Opens the menu of the focused item of `axis` (`Axis::Cols` for the
    /// focused column's): `false` if nothing there is focused.
    pub fn open(&mut self, snapshot: &Snapshot, axis: Axis) -> bool {
        let Some((key, at)) = snapshot
            .focus(axis)
            .and_then(|f| Some((f.key?, f.position)))
        else {
            return false;
        };
        let row = (axis == Axis::Rows).then(|| match snapshot.outline(axis, at).map(|o| o.kind) {
            None | Some(ItemKind::Leaf) => RowTarget::Row(key),
            Some(kind) => RowTarget::Summary(key, kind),
        });
        self.open = Some(Open::new(axis, Target::Item(key, row)));
        true
    }

    /// Opens the grouping menu of `axis`'s level by `field` (`None` for the
    /// values pseudo-level), as a right click on its name in the corner
    /// does: `false` if `axis` has no such level.
    pub fn open_level(&mut self, snapshot: &Snapshot, axis: Axis, field: Option<Key>) -> bool {
        let levels = grouping(snapshot).map_or(&[][..], |g| g.levels(axis));
        if !levels.iter().any(|l| l.field_key() == field) {
            return false;
        }
        self.open = Some(Open::new(axis, Target::Level(field)));
        true
    }

    /// Opens the menu with the cursor on the first hidden item (or Choose,
    /// with none), as [`HeaderMenu::open`] does: `false` as well if the axis
    /// does not hide.
    pub fn open_hidden(&mut self, snapshot: &Snapshot, axis: Axis) -> bool {
        if !snapshot.hide_support(axis) || !self.open(snapshot, axis) {
            return false;
        }
        let fields = self.fields(snapshot);
        if let Some(o) = &mut self.open {
            let entries = entries(snapshot, o, &fields);
            let choose = entries.iter().position(|e| *e == Entry::Choose);
            o.cursor = choose.map_or(0, |i| (i + 1).min(entries.len() - 1));
        }
        true
    }

    /// Closes the menu: `false` if it was not open.
    pub fn close(&mut self) -> bool {
        self.open.take().is_some()
    }

    /// The axis whose chooser the user picked from the menu, once: the host
    /// opens its [`crate::Chooser`] there.
    pub fn take_chooser(&mut self) -> Option<Axis> {
        self.chooser.take()
    }

    /// The column the user picked "Slice by this column" on, once: the host
    /// opens its [`crate::ColumnFilter`] there in slicer mode
    /// ([`crate::ColumnFilter::open_slicer`]).
    pub fn take_slicer(&mut self) -> Option<Key> {
        self.slicer.take()
    }

    /// Handles a key press while open: `true` if the menu took it. Every
    /// key is taken while it is open, so none reaches the table. `keys_at`
    /// gives the Keys at a run of Positions from the Table Source, for a
    /// hide of the selection ([`hide_chosen`]).
    pub fn key(
        &mut self,
        event: &KeyEvent,
        grid: &mut Grid,
        keys_at: impl FnMut(Axis, Range<Position>) -> Vec<Key>,
    ) -> bool {
        let Some(open) = self.open.clone() else {
            return false;
        };
        if event.kind == KeyEventKind::Release {
            return true;
        }
        let snapshot = grid.snapshot();
        let fields = self.fields(&snapshot);
        let mut entries = entries(&snapshot, &open, &fields);
        let hosts = self.hosts(&open);
        entries.extend((0..hosts.len()).map(Entry::Host));
        let Open { axis, cursor, .. } = open;
        let item = match open.target {
            Target::Item(key, _) => Some(key),
            Target::Level(_) => None,
        };
        let hidden: Vec<Key> = snapshot.hidden(axis).to_vec();
        let ungroups = entries.contains(&Entry::Ungroup);
        let Some(last) = entries.len().checked_sub(1) else {
            return match event.code {
                KeyCode::Esc => self.close(),
                _ => true,
            };
        };
        let cursor = cursor.min(last);
        let mut moved = cursor;
        let mut act = None;
        match event.code {
            KeyCode::Esc => return self.close(),
            KeyCode::Up => moved = cursor.checked_sub(1).unwrap_or(last),
            KeyCode::Down => moved = if cursor >= last { 0 } else { cursor + 1 },
            KeyCode::Enter => act = Some(entries[cursor].clone()),
            KeyCode::Char(c @ '1'..='9') if item.is_some() => {
                act = (hidden.get(c as usize - '1' as usize)).map(|&k| Entry::Show(k));
            }
            KeyCode::Char(c) => {
                act = (entries.iter())
                    .find(|e| {
                        !matches!(e, Entry::Show(_)) && e.key(axis, item, ungroups) == Some(c)
                    })
                    .cloned()
                    .or_else(|| {
                        (entries.iter())
                            .find(|e| match e {
                                Entry::Host(i) => {
                                    let key = hosts[*i].key;
                                    key == Some(c) && !ROW_MENU_KEYS.contains(c)
                                }
                                _ => false,
                            })
                            .cloned()
                    });
            }
            _ => {}
        }
        let mut closes = true;
        let hid = match act {
            Some(Entry::Pin(edge)) => {
                item.inspect(|&key| grid.pin(axis, edge, key));
                None
            }
            Some(Entry::Unpin) => {
                item.inspect(|&key| grid.unpin(axis, key));
                None
            }
            Some(Entry::Freeze) => {
                item.inspect(|&key| grid.freeze(axis, key));
                None
            }
            Some(Entry::Group(axis, field)) => {
                grid.group_by(axis, field);
                None
            }
            Some(Entry::Ungroup) => {
                grid.ungroup(Axis::Rows);
                None
            }
            Some(Entry::Slice) => {
                self.slicer = item;
                None
            }
            Some(Entry::RemoveLevel(axis, field)) => {
                grid.remove_level(axis, Some(field));
                None
            }
            Some(Entry::MoveLevel(axis, field)) => {
                grid.move_level(axis, field);
                None
            }
            Some(Entry::CollapseTo(axis, at, _) | Entry::ExpandThrough(axis, at, _)) => {
                grid.expand_to(axis, Depth::Levels(at));
                None
            }
            Some(Entry::RemoveValue(value)) => {
                grid.remove_value(&value);
                None
            }
            Some(Entry::AddValue(value)) => {
                grid.add_value(value);
                None
            }
            Some(Entry::Adding) => {
                closes = false;
                if let Some(o) = &mut self.open {
                    o.adding = !o.adding;
                }
                moved = entries
                    .iter()
                    .position(|e| *e == Entry::Adding)
                    .unwrap_or(moved);
                None
            }
            Some(Entry::Others) => {
                closes = false;
                if let Some(o) = &mut self.open {
                    o.others = !o.others;
                }
                moved = entries
                    .iter()
                    .position(|e| *e == Entry::Others)
                    .unwrap_or(moved);
                None
            }
            Some(Entry::Hide) => {
                closes = false;
                Some(hide_chosen(grid, axis, keys_at))
            }
            Some(Entry::ShowOnly) => {
                closes = false;
                Some(show_only_chosen(grid, axis, keys_at))
            }
            Some(Entry::Choose) => {
                self.chooser = Some(axis);
                None
            }
            Some(Entry::Show(k)) => {
                closes = false;
                grid.show(axis, &[k]);
                None
            }
            Some(Entry::ShowAll) => {
                closes = false;
                grid.show(axis, &hidden);
                None
            }
            Some(Entry::Host(i)) => {
                if let (Some(row), Some(chosen)) = (item, hosts.get(i)) {
                    self.row_choice = Some(RowChoice {
                        name: chosen.name.clone(),
                        row,
                    });
                }
                None
            }
            None => {
                closes = false;
                None
            }
        };
        if closes {
            self.open = None;
        }
        match hid {
            Some(Hid::Hidden(_)) => self.open = None,
            Some(Hid::Capped(cap)) => {
                if let Some(o) = &mut self.open {
                    o.hint = Some(cap_hint(axis, cap));
                }
            }
            _ => {}
        }
        if let Some(o) = &mut self.open {
            o.cursor = moved;
        }
        true
    }

    /// Draws the open menu in `area` (the table's), under the column's
    /// header or under the row (a level's under the corner), moved to fit:
    /// the pin lines and the grouping lines (scrolled to the cursor when
    /// they don't fit), then where the axis hides the actions (Hide and
    /// Show only naming how many items they reach), the hidden items
    /// numbered (scrolled to the cursor past [`LIST_ROWS`]), Show all, a
    /// hint after a hide past the cap, and the keys.
    pub fn render(
        &self,
        snapshot: &Snapshot,
        content: &impl Content,
        theme: &Theme,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let area = area.intersection(buf.area);
        let Some(o) = &self.open else {
            return;
        };
        // `label` names an item of the other axis than the one it is given.
        let name = |key: Key| label(content, o.axis.other(), key);
        let (noun, nouns) = match o.axis {
            Axis::Cols => ("column", "columns"),
            Axis::Rows => ("row", "rows"),
        };
        let entries = entries(snapshot, o, &self.fields(snapshot));
        let hosts = self.hosts(o);
        let item = match o.target {
            Target::Item(key, _) => Some(key),
            Target::Level(_) => None,
        };
        let hides = entries.contains(&Entry::Hide);
        let ungroups = entries.contains(&Entry::Ungroup);
        let hidden = if hides { snapshot.hidden(o.axis) } else { &[] };
        // The lines above the heading: the pin lines and the actions.
        let above = entries.len() - hidden.len() - usize::from(!hidden.is_empty());
        let (left, top) = match (item, o.axis) {
            (None, _) => (0.0, snapshot.header(Axis::Rows)),
            (Some(key), Axis::Cols) => {
                let x = (snapshot.headers(Axis::Cols))
                    .find(|h| h.key == Some(key))
                    .map_or(snapshot.header(Axis::Cols), |h| {
                        h.rect.x.max(snapshot.header(Axis::Cols))
                    });
                (x, snapshot.header(Axis::Rows))
            }
            (Some(key), Axis::Rows) => {
                let y = (snapshot.headers(Axis::Rows))
                    .find(|h| h.key == Some(key))
                    .map_or(0.0, |h| h.rect.y + h.rect.height);
                (
                    snapshot.header(Axis::Cols),
                    y.max(snapshot.header(Axis::Rows)),
                )
            }
        };
        let list = hidden.len().min(LIST_ROWS);
        // Borders, the lines above the heading, the heading, the list and
        // Show all, any hint, the keys.
        let rest = 2
            + u16::from(hides)
            + list as u16
            + u16::from(!hidden.is_empty())
            + u16::from(o.hint.is_some())
            + (1 + hosts.len() as u16) * u16::from(!hosts.is_empty())
            + 1;
        // The lines above the heading that fit, scrolled to the cursor.
        let room = usize::from(area.height.saturating_sub(rest)).max(1);
        let shown = above.min(room);
        let skip = match o.cursor < above {
            true => o.cursor.saturating_sub(shown - 1),
            false => 0,
        };
        let height = rest + shown as u16;
        let width = WIDTH.min(area.width);
        let height = height.min(area.height);
        if width < 8 || height < 4 {
            return;
        }
        let left = (left.max(0.0).ceil() as u16).min(area.width - width);
        let top = (top.max(0.0).ceil() as u16).min(area.height - height);
        let popup = Rect::new(area.x + left, area.y + top, width, height);
        let border = theme.body.patch(theme.border);
        let inner = usize::from(width - 2);
        let mut put = |row: u16, x: &mut u16, text: &str, style: Style| {
            if row >= popup.height {
                return;
            }
            put_text(buf, x, popup.y + row, popup.right(), text, style);
        };
        let chosen = Chosen::of(snapshot, o.axis).map_or(1, |c| c.len());
        let title = match (o.target, chosen) {
            (Target::Level(None), _) => " Values ".into(),
            (Target::Level(Some(field)), _) => {
                let level = match o.axis {
                    Axis::Cols => "Column level",
                    Axis::Rows => "Row level",
                };
                format!(" {level} {} ", content.column_header(field))
            }
            (Target::Item(key, _), 1) => format!(" {} {} ", capital(noun), name(key)),
            (Target::Item(..), n) => format!(" {} {nouns} chosen ", grouped(n)),
        };
        let title = fit(&title, u32::try_from(inner.saturating_sub(1)).unwrap_or(0));
        let rule = inner.saturating_sub(text_width(&title) as usize + 1);
        let mut x = popup.x;
        put(0, &mut x, "┌─", border);
        put(0, &mut x, &title, theme.body.add_modifier(Modifier::BOLD));
        put(0, &mut x, &format!("{}┐", "─".repeat(rule)), border);
        let mut row = 1;
        let mut line = |row: u16, text: &str, style: Style| {
            let mut x = popup.x;
            put(row, &mut x, "│", border);
            let text = fit(text, u32::try_from(inner).unwrap_or(0));
            let pad = inner.saturating_sub(text_width(&text) as usize);
            put(row, &mut x, &format!("{text}{}", " ".repeat(pad)), style);
            put(row, &mut x, "│", border);
        };
        // The cursor's line is reversed and marked, for terminals without colour.
        let at = |i: usize| match o.cursor == i {
            true => ('▸', theme.body.add_modifier(Modifier::REVERSED)),
            false => (' ', theme.body),
        };
        let these = match chosen {
            1 => format!("this {noun}"),
            n => format!("these {}", grouped(n)),
        };
        let field = |key: Key| content.column_header(key);
        let other = |axis: Axis| match axis {
            Axis::Cols => "columns",
            Axis::Rows => "rows",
        };
        for (i, entry) in entries.iter().enumerate().take(above).skip(skip) {
            let label = match entry {
                Entry::Pin(Edge::Leading) if o.axis == Axis::Cols => "Pin left".into(),
                Entry::Pin(Edge::Trailing) if o.axis == Axis::Cols => "Pin right".into(),
                Entry::Pin(Edge::Leading) => "Pin top".into(),
                Entry::Pin(Edge::Trailing) => "Pin bottom".into(),
                Entry::Unpin => "Unpin".into(),
                Entry::Freeze => "Freeze up to here".into(),
                Entry::Group(Axis::Rows, f) if Some(*f) == item => {
                    "Group rows by this column".into()
                }
                Entry::Group(Axis::Cols, f) if Some(*f) == item => "Add as a column level".into(),
                Entry::Group(Axis::Rows, f) => format!("Group rows by {}", field(*f)),
                Entry::Group(Axis::Cols, f) => format!("Add {} as a column level", field(*f)),
                Entry::Ungroup => "Ungroup rows".into(),
                Entry::Slice => "Slice by this column".into(),
                Entry::RemoveLevel(..) => "Remove level".into(),
                Entry::MoveLevel(axis, _) => format!("Move to {}", other(axis.other())),
                Entry::CollapseTo(_, _, f) => format!("Collapse to {}", field(*f)),
                Entry::ExpandThrough(_, _, f) => format!("Expand through {}", field(*f)),
                Entry::RemoveValue(v) => format!("Remove value {}", content.value_name(v)),
                Entry::Adding => format!("Add a value {}", if o.adding { "▾" } else { "▸" }),
                Entry::AddValue(v) => content.value_name(v),
                Entry::Others => format!(
                    "Group by another column {}",
                    if o.others { "▾" } else { "▸" }
                ),
                Entry::Hide => match chosen {
                    1 => format!("Hide {noun}"),
                    n => format!("Hide {} {nouns}", grouped(n)),
                },
                Entry::ShowOnly => format!("Show only {these}"),
                Entry::Choose => format!("Choose {nouns}…"),
                Entry::Show(_) | Entry::ShowAll | Entry::Host(_) => continue,
            };
            let (mark, style) = at(i);
            let key = entry.key(o.axis, item, ungroups).unwrap_or(' ');
            let indent = if entry.listed(item) { "  " } else { "" };
            line(row, &format!("{mark}{key} {indent}{label}"), style);
            row += 1;
        }
        if hides {
            let heading = match hidden.len() {
                0 => format!(" No hidden {nouns}"),
                n => format!(" Hidden {nouns} ({n}):"),
            };
            line(row, &heading, theme.body.patch(theme.row_number));
            row += 1;
        }
        // The window of the list that keeps the cursor in view.
        let first = (o.cursor.saturating_sub(above))
            .saturating_sub(list.saturating_sub(1))
            .min(hidden.len().saturating_sub(list));
        for (i, &key) in hidden.iter().enumerate().skip(first).take(list) {
            let number = if i < 9 {
                format!("{}", i + 1)
            } else {
                " ".into()
            };
            // A row's label is its number, so it is named a row.
            let named = match o.axis {
                Axis::Cols => name(key),
                Axis::Rows => format!("row {}", name(key)),
            };
            let (mark, style) = at(above + i);
            line(row, &format!("{mark}{number} {named}"), style);
            row += 1;
        }
        if !hidden.is_empty() {
            let (mark, style) = at(above + hidden.len());
            line(row, &format!("{mark}a Show all"), style);
            row += 1;
        }
        if !hosts.is_empty() {
            line(row, &"─".repeat(inner), border);
            row += 1;
            for (j, item) in hosts.iter().enumerate() {
                let (mark, style) = at(entries.len() + j);
                let key = item.key.filter(|c| !ROW_MENU_KEYS.contains(*c));
                line(
                    row,
                    &format!("{mark}{} {}", key.unwrap_or(' '), item.label),
                    style,
                );
                row += 1;
            }
        }
        if let Some(hint) = &o.hint {
            line(row, &format!(" {hint}"), theme.body.patch(theme.error));
            row += 1;
        }
        let keys = "↑↓ Enter  Esc close";
        line(row, &format!(" {keys}"), theme.body.patch(theme.row_number));
        row += 1;
        let mut x = popup.x;
        put(row, &mut x, &format!("└{}┘", "─".repeat(inner)), border);
    }
}

fn capital(noun: &str) -> String {
    let mut chars = noun.chars();
    chars
        .next()
        .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reserved_keys_hold_every_key_a_row_menu_line_uses() {
        let item = Some(Key(1));
        let lines = [
            Entry::Pin(Edge::Leading),
            Entry::Pin(Edge::Trailing),
            Entry::Unpin,
            Entry::Freeze,
            Entry::Ungroup,
            Entry::Hide,
            Entry::ShowOnly,
            Entry::Choose,
            Entry::Show(Key(2)),
            Entry::ShowAll,
        ];
        for line in lines {
            for ungroups in [false, true] {
                if let Some(c) = line.key(Axis::Rows, item, ungroups) {
                    assert!(ROW_MENU_KEYS.contains(c), "{line:?} uses {c}");
                }
            }
        }
        assert!(('1'..='9').all(|c| ROW_MENU_KEYS.contains(c)));
    }
}
