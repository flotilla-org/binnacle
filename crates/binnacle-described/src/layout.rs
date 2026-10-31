//! A Described Table's Layout: its IDs, its default presentation, and the
//! search that keeps Nested columns in reach of filters and find.

use binnacle_core::{Axis, FindQuery, Key, Searched, ViewSpec};
use binnacle_layout::view::AxisView;
use binnacle_layout::{EdgePins, IdMap, JsonId, Layout, Pins, ViewSpec as LayoutView};
use serde_json::Value as Json;

use crate::source::{Column, DescribedSource};
use crate::wire::Wire;

/// A row is named by its primary-key tuple as a JSON array, a column by its
/// field's name as a JSON string. Group Items and Summary Items have no ID
/// here, so a saved Layout leaves them out. Nor do the rows of a table with
/// no primary key, whose Keys name only where each row came in its stream,
/// or a row whose tuple JSON can't carry exactly (a NaN or infinity, or an
/// integer past `i64` and `u64`), as its ID would name nothing, or another
/// row.
impl IdMap<JsonId, JsonId> for DescribedSource {
    fn row_key(&self, id: &JsonId) -> Option<Key> {
        let Json::Array(items) = id.value() else {
            return None;
        };
        if !self.keyed() {
            return None;
        }
        let tuple: Vec<Wire> = items.iter().map(Wire::from_json).collect();
        DescribedSource::row_key(self, &tuple)
    }

    fn col_key(&self, id: &JsonId) -> Option<Key> {
        self.column_key(id.value().as_str()?)
    }

    fn row_id(&self, key: Key) -> Option<JsonId> {
        if !self.keyed() {
            return None;
        }
        let wire: Wire = ciborium::from_reader(self.row_identity(key)?).ok()?;
        let id = JsonId::new(wire.to_json())?;
        (IdMap::row_key(self, &id) == Some(key)).then_some(id)
    }

    fn col_id(&self, key: Key) -> Option<JsonId> {
        Some(JsonId::from(self.column(key)?.name()))
    }
}

impl DescribedSource {
    /// Whether the table has a primary key, so its rows have IDs.
    fn keyed(&self) -> bool {
        self.schema().is_some_and(|s| !s.primary_key.is_empty())
    }

    /// The Layout every Described Table starts from: the Nested columns
    /// (`object` and `array` fields) hidden, in schema order, and the
    /// columns with role `key`, then those with role `state`, pinned
    /// leading. A hidden column stays in the chooser and still feeds
    /// filters and find.
    pub fn default_layout(&self) -> Layout<JsonId, JsonId> {
        let name = |c: &Column| JsonId::from(c.name());
        let has = |c: &Column, role: &str| c.field().has_role(role);
        let columns = self.columns();
        let hidden = (columns.iter()).filter(|c| c.nested()).map(name).collect();
        let leading = (columns.iter().filter(|c| has(c, "key")))
            .chain(columns.iter().filter(|c| !has(c, "key") && has(c, "state")))
            .map(name)
            .collect();
        Layout {
            view: Some(LayoutView {
                cols: AxisView {
                    hidden,
                    ..AxisView::default()
                },
                ..LayoutView::default()
            }),
            pins: Some(Pins {
                cols: EdgePins {
                    leading,
                    ..EdgePins::default()
                },
                ..Pins::default()
            }),
            ..Layout::default()
        }
    }

    /// How the table is presented: the generic [`default_layout`], then
    /// the server's `default_view` for it, then the `user`'s Layout, each
    /// laid over the last with [`Layout::layered`]. A part a later Layout
    /// sets replaces the earlier one whole, Size Overrides merge per
    /// column, and the order layers per axis.
    ///
    /// A default view that sets `view` replaces the generic hidden columns,
    /// so it should list the Nested columns it wants hidden. One that sets
    /// `pins` replaces the generic pins the same way. A view that sets
    /// `order` but not `pins` keeps the generic pins, and as pinned columns
    /// lead, they lead its order; set `pins` (empty to unpin) to take the
    /// leading edge.
    ///
    /// A default view names columns by field name, so it loads as it is
    /// written; names the table lacks are skipped on restore, and stay in
    /// the document for [`Layout::keeping`] to put back on save:
    ///
    /// ```
    /// # use binnacle_core::{Axis, AxisConfig, Grid};
    /// # use binnacle_described::{DescribedSource, Field, FieldType, Message, Schema, Scope, TableMessage};
    /// # use binnacle_layout::{JsonId, Layout};
    /// # let mut source = DescribedSource::new("hosts", Scope::new("lab"));
    /// # source.receive(Message::Table(TableMessage {
    /// #     schema: Schema {
    /// #         fields: vec![Field::new("name", FieldType::String), Field::new("spec", FieldType::Object)],
    /// #         primary_key: vec!["name".into()],
    /// #         foreign_keys: Vec::new(),
    /// #     },
    /// #     incarnation: None,
    /// #     table: None,
    /// #     sequence: None,
    /// # }));
    /// # let mut grid = Grid::new(AxisConfig::new(0, 1.0), AxisConfig::new(2, 10.0));
    /// # let user: Option<Layout<JsonId, JsonId>> = None;
    /// let bytes = br#"{"version":2,"order":{"cols":["spec","nosuch","name"]}}"#;
    /// let view = Layout::<JsonId, JsonId>::from_json(bytes)?;
    /// source.presentation(Some(&view), user.as_ref()).restore(&mut grid, &source);
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    ///
    /// [`default_layout`]: DescribedSource::default_layout
    pub fn presentation(
        &self,
        default_view: Option<&Layout<JsonId, JsonId>>,
        user: Option<&Layout<JsonId, JsonId>>,
    ) -> Layout<JsonId, JsonId> {
        let layered = |under, over: Option<&Layout<JsonId, JsonId>>| match over {
            Some(over) => Layout::layered(under, over.clone()),
            None => under,
        };
        layered(layered(self.default_layout(), default_view), user)
    }

    /// `spec` with a quick filter on the rows that searches `Shown` made
    /// to search `Keys`: the columns shown and the Nested ones, hidden or
    /// not.
    pub(crate) fn widened(&self, spec: &ViewSpec) -> ViewSpec {
        let mut spec = spec.clone();
        let hidden = spec.axis(Axis::Cols).hidden.clone();
        if let Some(quick) = &mut spec.axis_mut(Axis::Rows).filter.quick
            && quick.searches == Searched::Shown
        {
            quick.searches = Searched::Keys(self.searched_keys(&hidden));
        }
        spec
    }

    /// `query` made to search the Nested columns as well as the shown.
    pub(crate) fn widened_find(&self, query: &FindQuery, hidden: &[Key]) -> FindQuery {
        let mut query = query.clone();
        if query.query.searches == Searched::Shown {
            query.query.searches = Searched::Keys(self.searched_keys(hidden));
        }
        query
    }

    fn searched_keys(&self, hidden: &[Key]) -> Vec<Key> {
        (self.columns().iter())
            .filter(|c| c.nested() || !hidden.contains(&c.key()))
            .map(Column::key)
            .collect()
    }

    /// `applied` with the quick filter as `asked` had it, where the source
    /// widened what it searches.
    pub(crate) fn unwidened(&self, asked: &ViewSpec, applied: &mut ViewSpec) {
        let asked = &asked.axis(Axis::Rows).filter.quick;
        let kept = &mut applied.axis_mut(Axis::Rows).filter.quick;
        if let (Some(asked), Some(kept)) = (asked, kept)
            && asked.searches == Searched::Shown
        {
            kept.searches = Searched::Shown;
        }
    }
}
