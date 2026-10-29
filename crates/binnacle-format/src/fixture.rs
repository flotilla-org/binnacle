//! The shared fixture: one table of Typed Values that every host formats
//! alike (the Rust tests, vitest through wasm, the ratatui values example
//! and the web demo's Values page), with the text each cell formats to.

use serde_json::Value as Json;

use crate::{
    FixedOffset, FormatContext, FormatOptions, Formatted, Role, Style, TimeStyle, Timestamp, Value,
    fewest_decimals, format, from_tagged_json,
};

/// The fixture, as JSON.
pub const VALUES_JSON: &str = include_str!("../fixtures/values.json");
/// What each cell of [`VALUES_JSON`] formats to, one cell a line (see
/// [`Table::expected_text`]).
pub const VALUES_EXPECTED: &str = include_str!("../fixtures/values.expected.txt");

/// A column of the fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub name: String,
    pub unit: Option<String>,
    pub role: Role,
    /// With its decimals filled in from its Numbers when the fixture gives
    /// none.
    pub options: FormatOptions,
}

/// The fixture read: its columns, its rows (one value a column) and the
/// clock and zone its expected text was formatted at.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
    pub now: Timestamp,
    pub zone: FixedOffset,
}

impl Table {
    /// The shared fixture.
    pub fn values() -> Table {
        let json = serde_json::from_str(VALUES_JSON).expect("the fixture is JSON");
        Table::from_json(&json).expect("the fixture reads")
    }

    pub fn from_json(json: &Json) -> Result<Table, String> {
        let field = |key: &str| json.get(key).ok_or_else(|| format!("no {key}"));
        let now = field("now")?
            .as_str()
            .and_then(Timestamp::parse_rfc3339)
            .ok_or("bad now")?;
        let utc_offset = field("utcOffset")?
            .as_i64()
            .and_then(|o| i32::try_from(o).ok())
            .ok_or("bad utcOffset")?;
        let names: Vec<&Json> = field("columns")?
            .as_array()
            .ok_or("bad columns")?
            .iter()
            .collect();
        let rows = field("rows")?
            .as_array()
            .ok_or("bad rows")?
            .iter()
            .map(|row| {
                names
                    .iter()
                    .map(|c| {
                        let name = c.get("name").and_then(Json::as_str).unwrap_or_default();
                        from_tagged_json(row.get(name).unwrap_or(&Json::Null))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let columns = names
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let text = |key: &str| c.get(key).and_then(Json::as_str).map(str::to_owned);
                let opts = c.get("options");
                let opt = |key: &str| opts.and_then(|o| o.get(key));
                let numbers = rows.iter().filter_map(|row| match row[i] {
                    Value::Number(x) => Some(x),
                    _ => None,
                });
                let decimals = match opt("decimals") {
                    Some(d) => d
                        .as_u64()
                        .and_then(|d| u8::try_from(d).ok())
                        .ok_or("bad decimals")?,
                    None => fewest_decimals(numbers),
                };
                let time = match opt("time").and_then(Json::as_str) {
                    Some("relative") => Some(TimeStyle::Relative),
                    Some("absolute") => Some(TimeStyle::Absolute),
                    _ => None,
                };
                Ok(Column {
                    name: text("name").ok_or("a column without a name")?,
                    unit: text("unit"),
                    role: text("role").map_or(Role::None, |r| Role::from_name(&r)),
                    options: FormatOptions {
                        time,
                        decimals: Some(decimals),
                        null_glyph: opt("nullGlyph").and_then(Json::as_str).map(str::to_owned),
                        plain: opt("plain").and_then(Json::as_bool).unwrap_or(false),
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Table {
            columns,
            rows,
            now,
            zone: FixedOffset(utc_offset),
        })
    }

    /// The fixture's own clock and zone.
    pub fn ctx(&self) -> FormatContext<'_> {
        FormatContext::new(self.now, &self.zone)
    }

    /// The cell at `row`, `col` formatted at `ctx`.
    pub fn format(&self, row: usize, col: usize, ctx: &FormatContext) -> Formatted {
        let column = &self.columns[col];
        format(
            &self.rows[row][col],
            column.unit.as_deref(),
            column.role,
            &column.options,
            ctx,
        )
    }

    /// Every cell formatted at the fixture's own clock, one a line, row by
    /// row: `row⇥column⇥text⇥align⇥style`, the align `start` or `end` and
    /// the style its flags' names joined with `+`, or `-` for none.
    pub fn expected_text(&self) -> String {
        let mut out = String::new();
        for row in 0..self.rows.len() {
            for (col, column) in self.columns.iter().enumerate() {
                let f = self.format(row, col, &self.ctx());
                let align = match f.align {
                    crate::Align::Start => "start",
                    crate::Align::End => "end",
                };
                out += &format!(
                    "{row}\t{}\t{}\t{align}\t{}\n",
                    column.name,
                    f.text,
                    style_names(f.style)
                );
            }
        }
        out
    }
}

/// `dim+warning`, or `-` for no flags.
pub fn style_names(style: Style) -> String {
    let names: Vec<&str> = [
        (Style::DIM, "dim"),
        (Style::WARNING, "warning"),
        (Style::PENDING, "pending"),
    ]
    .into_iter()
    .filter(|&(flag, _)| style.contains(flag))
    .map(|(_, name)| name)
    .collect();
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join("+")
    }
}
