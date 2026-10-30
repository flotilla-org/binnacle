//! Shared test support: the recorded fixtures decoded, and a Described
//! Table source as a scenario's Table Source.

#![allow(dead_code)]

use std::fmt::Write;
use std::ops::Range;
use std::path::PathBuf;

use binnacle_core::{
    Axis, AxisConfig, Distinct, DistinctQuery, FindDirection, FindMatch, FindQuery, Grid, Key,
    Located, Outline, Position, Support, TextSupport, ViewSpec, Viewport,
};
use binnacle_described::{
    Decoder, DescribedSource, Encoding, HostRequest, Message, Received, Scope,
};
use binnacle_format::{FixedOffset, FormatContext, FormatOptions, Timestamp, Value, format_stable};
use binnacle_memory::{FindStep, Moved, PumpSource, Viewed};
use binnacle_testkit::{Scenario, ScenarioSource};

/// A fixture's path: the recorded ones in `tests/data`, the made ones in
/// `tests/fixtures`.
pub fn path(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let made = root.join("fixtures").join(name);
    match made.exists() {
        true => made,
        false => root.join("data").join(name),
    }
}

/// The recorded fixture names with `prefix` and one of `extensions`.
pub fn fixtures(prefix: &str, extensions: &[&str]) -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with(prefix) && extensions.iter().any(|x| n.ends_with(x)))
        .collect();
    names.sort();
    names
}

pub fn encoding(name: &str) -> Encoding {
    match name.rsplit('.').next() {
        Some("ndjson") => Encoding::Ndjson,
        Some("cborseq") => Encoding::CborSeq,
        _ => panic!("{name} is not a rows stream"),
    }
}

/// Every message of the fixture `name`, by its extension's encoding.
pub fn messages(name: &str) -> Vec<Message> {
    decode(encoding(name), &std::fs::read(path(name)).unwrap())
}

/// Every message in `bytes`.
pub fn decode(encoding: Encoding, bytes: &[u8]) -> Vec<Message> {
    let mut decoder = Decoder::new(encoding);
    decoder.feed(bytes);
    let mut messages = Vec::new();
    while let Some(message) = decoder.next() {
        messages.push(message.unwrap());
    }
    if let Some(last) = decoder.finish() {
        messages.push(last.unwrap());
    }
    messages
}

/// The table a fixture's name says: `rows-hosts.ndjson` is `hosts`.
pub fn table(name: &str) -> &str {
    let stem = name.split('.').next().unwrap();
    let stem = stem.split_once('-').map_or(stem, |(_, rest)| rest);
    stem.split('-').next().unwrap()
}

/// A source fed every message of fixture `name`, after it asked to open
/// its watch.
pub fn replay(name: &str) -> DescribedSource {
    let mut source = DescribedSource::new(table(name), Scope::new("flotilla"));
    assert!(matches!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
    for message in messages(name) {
        let _: Received = source.receive(message);
    }
    source
}

/// A Described Table source as a scenario's Table Source.
pub struct Table(pub DescribedSource);

impl ScenarioSource for Table {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.0.window(axis, range).keys
    }

    fn outlines(&mut self, axis: Axis, keys: &[Key]) -> Option<Vec<Outline>> {
        let first = keys.first()?;
        let Located::Found(at) = self.0.locate(axis, *first) else {
            return None;
        };
        self.0.window(axis, at..at + keys.len() as u64).outlines
    }
}

impl PumpSource for Table {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.0.view(spec)
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.0.support(axis, key)
    }

    fn quick_filter(&mut self, axis: Axis) -> Option<TextSupport> {
        self.0.quick_filter(axis)
    }

    fn hide_support(&mut self, axis: Axis) -> bool {
        self.0.hide_support(axis)
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        self.0.distinct(axis, key, query)
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.0.locate(axis, key)
    }

    fn find_support(&mut self) -> Option<TextSupport> {
        self.0.find_support()
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        self.0.find_next(query, from, direction)
    }

    fn ancestors(&mut self, axis: Axis, key: Key) -> Vec<Key> {
        self.0.ancestors(axis, key)
    }

    fn fits(&mut self, axis: Axis, key: Key) -> bool {
        self.0.fits(axis, key)
    }

    fn movable(&mut self, axis: Axis) -> Option<bool> {
        self.0.movable(axis)
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Result<Moved, String> {
        self.0.move_items(axis, keys, to)
    }
}

/// A viewport showing up to 10 rows of 12 columns.
pub const VIEWPORT: Viewport = Viewport {
    width: 12.0 * 21.0,
    height: 11.0,
};

/// A scenario over `source`, every column 20 wide and each row 1 high,
/// its support declared.
pub fn scenario(mut source: DescribedSource) -> Scenario<Table> {
    let mut grid = Grid::new(
        AxisConfig::new(source.count(Axis::Rows), 1.0).header(1.0),
        AxisConfig::new(source.count(Axis::Cols), 20.0)
            .header(1.0)
            .gap(1.0),
    );
    source.declare(&mut grid);
    let mut scenario = Scenario::new(grid, Table(source));
    scenario.frame(VIEWPORT);
    scenario
}

/// The Stable Text of a cell as filters read it, a null as `∅`.
pub fn text(source: &DescribedSource, row: Key, col: Key) -> String {
    let column = source.column(col).unwrap();
    let zone = FixedOffset(0);
    let ctx = FormatContext::new(Timestamp::UNIX_EPOCH, &zone);
    let value = source.cell(row, col).cloned().unwrap_or(Value::Null);
    let options = FormatOptions::default();
    format_stable(&value, column.unit(), column.role(), &options, &ctx).text
}

/// What the Grid shows, after a frame: the counts, then a header line of
/// column titles and a line per row drawn, each cell its Stable Text, a
/// Group Item as its value and count, cells cut to 24 characters and
/// separated by ` | `.
pub fn picture(scenario: &mut Scenario<Table>) -> String {
    let snapshot = scenario.frame(VIEWPORT);
    let rows: Vec<(Position, Option<Key>)> = (snapshot.drawn(Axis::Rows))
        .map(|i| (i.position, i.key))
        .collect();
    let cols: Vec<Key> = snapshot.drawn(Axis::Cols).filter_map(|i| i.key).collect();
    let outlines: Vec<Option<Outline>> = (rows.iter())
        .map(|(p, _)| snapshot.outline(Axis::Rows, *p).cloned())
        .collect();
    let (row_count, col_count) = (snapshot.count(Axis::Rows), snapshot.count(Axis::Cols));
    let source = &scenario.source.0;
    let status = source.status();
    let totals = source.totals();
    let mut out = format!(
        "rows {row_count} (of {:?}), columns {col_count}, synced {}, partial {}, count {:?}, mismatches {}\n",
        totals.rows,
        status.synced,
        status.partial,
        status.count,
        source.mismatches()
    );
    let cut = |s: &str| s.chars().take(24).collect::<String>();
    let headers: Vec<String> = (cols.iter())
        .map(|&c| cut(source.column(c).unwrap().header()))
        .collect();
    let _ = writeln!(out, "{}", headers.join(" | "));
    for ((_, key), outline) in rows.iter().zip(outlines) {
        let Some(key) = *key else {
            let _ = writeln!(out, "(pending)");
            continue;
        };
        if let Some(group) = outline.as_ref().and_then(|o| o.group.as_ref()) {
            let label = match &group.value {
                binnacle_core::Value::Text(t) => t.as_str(),
                binnacle_core::Value::Blank => "(blank)",
            };
            let count = source
                .summary(key)
                .map(|s| format!(" ({} items, {:?})", s.children, s.aggregates));
            let _ = writeln!(out, "▸ {label}{}", count.unwrap_or_default());
            continue;
        }
        let cells: Vec<String> = (cols.iter()).map(|&c| cut(&text(source, key, c))).collect();
        let _ = writeln!(out, "{}", cells.join(" | "));
    }
    out
}

/// The texts of column `name` down the rows drawn, Group Items left out.
pub fn column_down(scenario: &mut Scenario<Table>, name: &str) -> Vec<String> {
    let snapshot = scenario.frame(VIEWPORT);
    let keys: Vec<Key> = snapshot.drawn(Axis::Rows).filter_map(|i| i.key).collect();
    let source = &scenario.source.0;
    let col = source.column_key(name).unwrap();
    (keys.into_iter())
        .filter(|&k| source.cell(k, col).is_some())
        .map(|k| text(source, k, col))
        .collect()
}
