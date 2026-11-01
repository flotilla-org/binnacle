//! A table printed: the Layout applied through a headless Grid, then the
//! rows in the source's order, as aligned text, TSV, JSON or JSON lines.

use std::io::{self, Write};
use std::str::FromStr;

use crate::width::width;
use binnacle_core::{Axis, AxisConfig, Edge, Grid, Key};
use binnacle_format::{
    Align, FixedOffset, FormatContext, FormatOptions, Role, TimeStyle, Timestamp, Value, format,
};
use binnacle_layout::{JsonId, Layout};
use serde_json::Value as Json;

use super::CliTable;

/// How [`print`] writes a table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Format {
    /// Columns under their titles, aligned by display width, two spaces
    /// apart.
    #[default]
    Aligned,
    /// A header of field names, then raw values separated by tabs.
    Tsv,
    /// One JSON array of objects keyed by field name. A number JSON can't
    /// hold is a string: `"NaN"`, `"inf"` or `"-inf"`, so a numeric
    /// column may hold strings.
    Json,
    /// One JSON object per line, written as [`Format::Json`] writes each.
    Jsonl,
}

impl FromStr for Format {
    type Err = String;

    fn from_str(s: &str) -> Result<Format, String> {
        match s {
            "aligned" | "text" | "table" => Ok(Format::Aligned),
            "tsv" => Ok(Format::Tsv),
            "json" => Ok(Format::Json),
            "jsonl" | "ndjson" => Ok(Format::Jsonl),
            other => Err(format!(
                "unknown format `{other}`: aligned, tsv, json or jsonl"
            )),
        }
    }
}

/// Prints `source` as `layout` presents it: a headless Grid declares the
/// source, restores the Layout and pumps until no Effect remains; then
/// every row, in the source's order, under the columns the Grid draws
/// (leading pins, the rest, trailing pins), less the hidden. A partial
/// table ([`CliTable::partial`]) prints the rows that came, then a line
/// saying so ([`partial_note`], with the host's `hint`) to `note`, the
/// host's stderr, so `out` stays the table's alone.
pub fn print<T: CliTable>(
    source: &mut T,
    layout: &Layout<JsonId, JsonId>,
    format: Format,
    out: &mut impl Write,
    note: &mut impl Write,
    hint: Option<&str>,
) -> io::Result<()> {
    let grid = headless(source, layout);
    let count = source.count(Axis::Rows);
    let rows = source.window(Axis::Rows, 0..count).keys;
    let cols = columns(source, &grid);
    print_rows(source, &rows, &cols, format, out)?;
    if let Some(came) = source.partial() {
        out.flush()?;
        writeln!(note, "{}", partial_note(count, came, hint))?;
    }
    Ok(())
}

/// What [`print`] says of a partial table of which `read` rows came and
/// `shown` show (fewer under a filter). The library doesn't know the host's
/// flags, so the advice on reading more is the host's `hint`, if it has
/// one, added after the note.
pub fn partial_note(shown: u64, read: u64, hint: Option<&str>) -> String {
    let rows = match shown == read {
        true => format!("showing {} rows read", super::thousands(read)),
        false => format!(
            "showing {} of {} rows read",
            super::thousands(shown),
            super::thousands(read)
        ),
    };
    match hint {
        Some(hint) => format!("{rows}; the table has more; {hint}"),
        None => format!("{rows}; the table has more"),
    }
}

/// A Grid over `source` with `layout` restored and every Effect answered.
pub(crate) fn headless<T: CliTable>(source: &mut T, layout: &Layout<JsonId, JsonId>) -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(source.count(Axis::Rows), 1.0).header(1.0),
        AxisConfig::new(source.count(Axis::Cols), 10.0)
            .header(1.0)
            .gap(1.0),
    );
    source.declare(&mut grid);
    layout.restore(&mut grid, &*source);
    binnacle_memory::pump(&mut grid, source, |_, _| {});
    grid
}

/// The columns `grid` draws over `source`, in drawn order: the leading
/// pins, the rest in the source's order, the trailing pins; the hidden
/// left out.
pub(crate) fn columns<T: CliTable>(source: &mut T, grid: &Grid) -> Vec<Key> {
    let count = source.count(Axis::Cols);
    let shown = source.window(Axis::Cols, 0..count).keys;
    let pinned = |edge| -> Vec<Key> {
        (grid.pins(Axis::Cols, edge))
            .filter(|k| shown.contains(k))
            .collect()
    };
    let (leading, trailing) = (pinned(Edge::Leading), pinned(Edge::Trailing));
    let rest = (shown.iter().copied()).filter(|k| !leading.contains(k) && !trailing.contains(k));
    leading
        .iter()
        .copied()
        .chain(rest)
        .chain(trailing.iter().copied())
        .collect()
}

/// Prints `rows` under `cols` of `source` in `format`.
pub fn print_rows<T: CliTable>(
    source: &T,
    rows: &[Key],
    cols: &[Key],
    format: Format,
    out: &mut impl Write,
) -> io::Result<()> {
    match format {
        Format::Aligned => {
            for line in aligned(source, rows, cols) {
                writeln!(out, "{line}")?;
            }
        }
        Format::Tsv => {
            let names: Vec<String> = cols.iter().map(|&c| escape(&source.field(c))).collect();
            writeln!(out, "{}", names.join("\t"))?;
            for &row in rows {
                let cells: Vec<String> = (cols.iter())
                    .map(|&c| escape(&raw(&source.value(row, c))))
                    .collect();
                writeln!(out, "{}", cells.join("\t"))?;
            }
        }
        Format::Json => {
            write!(out, "[")?;
            for (i, &row) in rows.iter().enumerate() {
                let sep = if i == 0 { "\n" } else { ",\n" };
                write!(out, "{sep}{}", object(source, row, cols))?;
            }
            let end = if rows.is_empty() { "]" } else { "\n]" };
            writeln!(out, "{end}")?;
        }
        Format::Jsonl => {
            for &row in rows {
                writeln!(out, "{}", object(source, row, cols))?;
            }
        }
    }
    Ok(())
}

/// The table as aligned text: a line of titles, then a line per row, each
/// cell formatted in plain text (a null as `-`) and cut at its first line
/// break, widths by display width, columns two spaces apart, an end-aligned
/// cell to the right, and no trailing spaces. Dates are absolute, in UTC,
/// so what stays in the scrollback never goes stale.
pub(crate) fn aligned<T: CliTable>(source: &T, rows: &[Key], cols: &[Key]) -> Vec<String> {
    let options = FormatOptions {
        plain: true,
        time: Some(TimeStyle::Absolute),
        ..FormatOptions::default()
    };
    let zone = FixedOffset(0);
    let ctx = FormatContext::new(Timestamp::UNIX_EPOCH, &zone);
    let mut lines: Vec<Vec<(String, Align)>> = Vec::new();
    for &row in rows {
        lines.push(
            (cols.iter())
                .map(|&c| {
                    let value = cut(source.value(row, c));
                    let unit = source.unit(c);
                    let f = format(&value, unit.as_deref(), source.role(c), &options, &ctx);
                    (f.text, f.align)
                })
                .collect(),
        );
    }
    // A title is the server's text too: it goes through the formatter, so
    // no control character reaches the terminal. It aligns as most of its
    // column's cells do, ignoring nulls.
    let header: Vec<(String, Align)> = (cols.iter().enumerate())
        .map(|(i, &c)| {
            let title = Value::Text(first_line(&source.header(c)).to_owned());
            let f = format(&title, None, Role::None, &options, &ctx);
            let cells =
                (rows.iter().zip(&lines)).map(|(&row, line)| (source.value(row, c), line[i].1));
            let align = majority(cells);
            (f.text, align)
        })
        .collect();
    lines.insert(0, header);
    let widths: Vec<usize> = (0..cols.len())
        .map(|i| {
            lines
                .iter()
                .map(|l| width(&l[i].0) as usize)
                .max()
                .unwrap_or(0)
        })
        .collect();
    (lines.into_iter())
        .map(|cells| {
            let mut line = String::new();
            for (i, (text, align)) in cells.into_iter().enumerate() {
                if i > 0 {
                    line.push_str("  ");
                }
                let blank = " ".repeat(widths[i].saturating_sub(width(&text) as usize));
                match align {
                    Align::End => line.push_str(&format!("{blank}{text}")),
                    Align::Start => line.push_str(&format!("{text}{blank}")),
                }
            }
            line.trim_end().to_owned()
        })
        .collect()
}

/// Which side a column's title aligns on: the side most of its non-null
/// cells do, start on a tie. `cells` are the values and their alignments.
fn majority(cells: impl IntoIterator<Item = (Value, Align)>) -> Align {
    let (mut end, mut start) = (0, 0);
    for (value, align) in cells {
        match (value, align) {
            (Value::Null, _) => {}
            (_, Align::End) => end += 1,
            (_, Align::Start) => start += 1,
        }
    }
    if end > start {
        Align::End
    } else {
        Align::Start
    }
}

/// The alignment of each of `cols`' titles over `rows`, as [`aligned`]'s.
pub(crate) fn title_aligns<T: CliTable>(source: &T, rows: &[Key], cols: &[Key]) -> Vec<Align> {
    let options = FormatOptions {
        plain: true,
        time: Some(TimeStyle::Absolute),
        ..FormatOptions::default()
    };
    let zone = FixedOffset(0);
    let ctx = FormatContext::new(Timestamp::UNIX_EPOCH, &zone);
    (cols.iter())
        .map(|&c| {
            let unit = source.unit(c);
            majority(rows.iter().map(|&row| {
                let value = cut(source.value(row, c));
                let f = format(&value, unit.as_deref(), source.role(c), &options, &ctx);
                (source.value(row, c), f.align)
            }))
        })
        .collect()
}

/// `text` up to its first line break.
fn first_line(text: &str) -> &str {
    text.split(['\n', '\r']).next().unwrap_or_default()
}

/// `value` cut at its first line break.
fn cut(value: Value) -> Value {
    match value {
        Value::Text(text) => Value::Text(first_line(&text).to_owned()),
        Value::Invalid { raw, expected } => Value::Invalid {
            raw: first_line(&raw).to_owned(),
            expected,
        },
        Value::Category { value, label } => Value::Category {
            value: first_line(&value).to_owned(),
            label: label.map(|l| first_line(&l).to_owned()),
        },
        other => other,
    }
}

/// A value's raw text: text as it is, numbers as written, a date in RFC
/// 3339 (UTC, nanoseconds), a null empty, bytes in hex, a tree as JSON.
fn raw(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Number(x) => number(*x),
        Value::Text(text) => text.clone(),
        Value::Bytes(bytes) => hex(bytes),
        Value::DateTime(at) => at.to_rfc3339(),
        Value::Category { value, .. } => value.clone(),
        Value::Nested(json) => json.to_string(),
        Value::Invalid { raw, .. } => raw.clone(),
    }
}

/// A float as written: the shortest text that reads back as it, and
/// `NaN`, `inf` and `-inf`.
fn number(x: f64) -> String {
    match x {
        x if x.is_nan() => "NaN".into(),
        f64::INFINITY => "inf".into(),
        f64::NEG_INFINITY => "-inf".into(),
        x => x.to_string(),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `text` with tabs, line breaks and backslashes escaped, for TSV, and any
/// other control character as `\xNN` (or `\u{NNNN}` past ASCII), so none
/// reaches a terminal.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if c.is_control() => out.push_str(&format!("\\u{{{:04x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// A value as JSON: integers exact, a date in RFC 3339, a tree as itself,
/// an invalid value as its raw text, bytes in hex, a non-finite number as
/// `"NaN"`, `"inf"` or `"-inf"`.
fn json(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(i) => Json::from(*i),
        Value::UInt(u) => Json::from(*u),
        Value::Number(x) => serde_json::Number::from_f64(*x)
            .map(Json::Number)
            .unwrap_or_else(|| Json::String(number(*x))),
        Value::Text(text) => Json::String(text.clone()),
        Value::Bytes(bytes) => Json::String(hex(bytes)),
        Value::DateTime(at) => Json::String(at.to_rfc3339()),
        Value::Category { value, .. } => Json::String(value.clone()),
        Value::Nested(tree) => tree.clone(),
        Value::Invalid { raw, .. } => Json::String(raw.clone()),
    }
}

/// Row `row` as a JSON object of `cols`, keyed by field name, in column
/// order.
fn object<T: CliTable>(source: &T, row: Key, cols: &[Key]) -> String {
    let fields: Vec<String> = (cols.iter())
        .map(|&c| {
            let name = Json::String(source.field(c)).to_string();
            format!("{name}:{}", json(&source.value(row, c)))
        })
        .collect();
    format!("{{{}}}", fields.join(","))
}
