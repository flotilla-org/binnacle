//! The tables example's table, shared with its render snapshot test: a
//! Described Table replayed from a recorded rows stream, each column drawn
//! through a Value Renderer under its title, the nested columns hidden and
//! the key and state columns pinned leading.

use std::path::Path;

use binnacle_core::{Axis, AxisConfig, Edge, Grid, Key, ViewSpec, Viewport};
use binnacle_described::{Decoder, DescribedSource, Encoding, Message, Received, Scope};
use binnacle_format::{Timestamp, Value};
use binnacle_ratatui::{CellRenderer, Content, ValueRenderer, fit_width};

/// The widest a column starts.
const MAX_WIDTH: u32 = 40;

pub struct Tables {
    pub source: DescribedSource,
    /// The instant relative times count from at the frame's time zero.
    pub origin: Timestamp,
}

impl Tables {
    /// The table recorded at `path`, a `.ndjson` or `.cborseq` rows stream,
    /// replayed into a source as its host would feed it.
    pub fn load(path: &Path, origin: Timestamp) -> Result<Tables, String> {
        let encoding = match path.extension().and_then(|e| e.to_str()) {
            Some("ndjson") => Encoding::Ndjson,
            Some("cborseq") => Encoding::CborSeq,
            _ => return Err(format!("{}: not .ndjson or .cborseq", path.display())),
        };
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("table");
        let table = stem.split('-').nth(1).unwrap_or(stem);
        let mut source = DescribedSource::new(table, Scope::new("flotilla"));
        // The host would open the watch this asks for; a replay has it.
        let _ = source.poll_request();
        let mut decoder = Decoder::new(encoding);
        decoder.feed(&bytes);
        while let Some(message) = decoder.next().or_else(|| decoder.finish()) {
            let message = message.map_err(|e| format!("{}: {e}", path.display()))?;
            let _ = source.receive(message);
        }
        Ok(Tables { source, origin })
    }

    /// A live table: a source over `table` in `scope`, its rows still to
    /// come, as its host feeds them ([`Tables::apply`]).
    #[allow(dead_code)]
    pub fn live(table: &str, scope: Scope, origin: Timestamp) -> Tables {
        Tables {
            source: DescribedSource::new(table, scope),
            origin,
        }
    }

    /// Feeds `message` to the source and tells `grid` what changed: the
    /// columns' support, any View Spec the source applies now, and the
    /// Change Notices. Returns whether it was the first `synced`, when the
    /// host lays the Grid out afresh ([`Tables::grid`]).
    #[allow(dead_code)]
    pub fn apply(&mut self, grid: &mut Grid, message: Message) -> bool {
        let first = !self.source.status().synced && matches!(message, Message::Synced { .. });
        let described = matches!(message, Message::Table(_));
        let Received::Applied(notices) = self.source.receive(message) else {
            return false;
        };
        if described {
            self.source.declare(grid);
        }
        if let Some((spec, totals)) = self.source.poll_view() {
            grid.view_changed(spec, totals);
        }
        for (axis, notice) in notices {
            grid.notify(axis, notice);
        }
        first
    }

    /// A Grid over the table: each column as wide as its widest cell (up
    /// to 40), the nested columns hidden and the key and state columns
    /// pinned leading.
    pub fn grid(&mut self) -> Grid {
        let mut grid = Grid::new(
            AxisConfig::new(self.source.count(Axis::Rows), 1.0).header(1.0),
            AxisConfig::new(self.source.count(Axis::Cols), 10.0)
                .header(6.0)
                .gap(1.0),
        );
        self.source.declare(&mut grid);
        let rows = self.source.row_keys().to_vec();
        let columns: Vec<Key> = self.source.columns().iter().map(|c| c.key()).collect();
        for &col in &columns {
            let width = fit_width(self, &rows, col).clamp(4, MAX_WIDTH);
            grid.set_size(Axis::Cols, col, width as f32);
        }
        let column = |key| self.source.column(key).unwrap();
        let pinned: Vec<Key> = (columns.iter().copied())
            .filter(|&c| column(c).field().has_role("key"))
            .chain((columns.iter().copied()).filter(|&c| column(c).field().has_role("state")))
            .collect();
        grid.set_pins(Axis::Cols, Edge::Leading, &pinned);
        let nested: Vec<Key> = (columns.iter().copied())
            .filter(|&c| column(c).nested())
            .collect();
        grid.request_view(ViewSpec::default().with_hidden(Axis::Cols, nested));
        grid
    }

    /// Column `col`'s Value Renderer, its relative times counting from
    /// `origin`.
    pub fn renderer(&self, col: Key) -> ValueRenderer<impl Fn(Key, Key) -> Value + '_> {
        let column = self.source.column(col);
        // A Summary Item has no cells of its own: it draws blank.
        let renderer = ValueRenderer::new(self.origin, move |row: Key, col: Key| {
            (self.source.cell(row, col).cloned()).unwrap_or_else(|| Value::Text(String::new()))
        })
        .role(column.map(|c| c.role()).unwrap_or_default());
        match column.and_then(|c| c.unit()) {
            Some(unit) => renderer.unit(unit),
            None => renderer,
        }
    }

    /// Answers the Grid's Effects, then fills its Windows, outlines and all.
    pub fn fill(&mut self, grid: &mut Grid, viewport: Viewport) {
        binnacle_memory::pump(grid, &mut self.source, |_, _| {});
        for (axis, range) in grid.begin_frame(viewport) {
            let window = self.source.window(axis, range.clone());
            grid.set_window(axis, range.start, &window.keys);
            if let Some(outlines) = window.outlines {
                grid.set_outlines(axis, range.start, &outlines);
            }
        }
    }

    /// The status line: how many rows show of how many came, "of more"
    /// for a table over the cap, and the values that did not match their
    /// type.
    pub fn status(&self) -> String {
        let shown = self.source.count(Axis::Rows);
        let came = self.source.row_count();
        let status = self.source.status();
        let mut line = match (status.partial, self.source.totals().rows) {
            (true, _) => format!("{} of more rows", thousands(came as u64)),
            (false, Some(total)) if total != shown => {
                format!("{} of {} rows", thousands(shown), thousands(total))
            }
            _ => format!("{} rows", thousands(shown)),
        };
        if !status.synced {
            line.push_str(", loading");
        }
        if self.source.mismatches() > 0 {
            line.push_str(&format!(", {} invalid", self.source.mismatches()));
        }
        line
    }
}

/// `n` with a comma between each three digits.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl Content for Tables {
    /// The Stable Text copy and find read: dates absolute.
    fn cell(&self, row: Key, col: Key) -> String {
        self.renderer(col).text(row, col)
    }

    /// The field's title, else its name, and its unit.
    fn column_header(&self, col: Key) -> String {
        let Some(column) = self.source.column(col) else {
            return String::new();
        };
        match column.unit() {
            Some(unit) => format!("{} ({unit})", column.header()),
            None => column.header().to_string(),
        }
    }

    fn column_renderer(&self, col: Key) -> Box<dyn CellRenderer + '_> {
        Box::new(self.renderer(col))
    }
}
