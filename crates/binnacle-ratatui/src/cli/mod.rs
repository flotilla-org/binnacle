//! A CLI host for any Table Source ([`CliTable`]), described tables first:
//! print a table as aligned text, TSV, JSON or JSON lines ([`print`]), take
//! the usual flags ([`Flags`]), or open the full widget inline below the
//! prompt, or full screen ([`Presentation`]), and hand back what the user
//! picked ([`run_inline`]). Each applies a [`binnacle_layout::Layout`]
//! (`binnacle_ratatui::Layout` is the scrollbar layout) through a Grid, so
//! a table prints as it shows.

mod flags;
mod inline;
mod print;

#[cfg(feature = "described")]
mod described;

use std::ops::Range;

use binnacle_core::{Axis, ChangeNotice, Grid, Key, Position, Totals, ViewSpec};
use binnacle_format::{Role, Value};
use binnacle_group::Window;
use binnacle_layout::{IdMap, JsonId};
use binnacle_memory::PumpSource;

#[cfg(feature = "clap")]
pub use flags::FlagArgs;
pub use flags::{FlagError, Flags, parse_where};
pub use inline::{InlineOptions, Live, Presentation, inline_height, run_inline, run_inline_with};
pub use print::{Format, partial_note, print, print_rows};

/// What a table gives the CLI host, over the Table Source the Grid pumps.
/// Rows and columns are named by JSON IDs ([`IdMap`]), as a Layout names
/// them.
pub trait CliTable: PumpSource + IdMap<JsonId, JsonId> {
    /// How many items `axis` shows now.
    fn count(&self, axis: Axis) -> u64;

    /// The Keys at `range` on `axis`, and their outlines if it is grouped.
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Window;

    /// Declares to `grid` what each column supports.
    fn declare(&mut self, grid: &mut Grid);

    /// Every column's Key, shown or hidden, in the source's order.
    fn columns(&self) -> Vec<Key>;

    /// Column `col`'s header: its title.
    fn header(&self, col: Key) -> String;

    /// Column `col`'s field name, as flags and machine output name it.
    fn field(&self, col: Key) -> String;

    /// The Typed Value where row `row` meets column `col`; a Summary Item's
    /// cells are null.
    fn value(&self, row: Key, col: Key) -> Value;

    /// Column `col`'s unit, if any.
    fn unit(&self, col: Key) -> Option<String>;

    /// The role column `col`'s values draw with.
    fn role(&self, col: Key) -> Role;

    /// Column `col`'s type, as an error names it: `string`, say.
    fn type_name(&self, col: Key) -> String;

    /// How many rows came, when the table has more than came (a source
    /// over its row cap): what prints and shows is only those. `None`, the
    /// default, when every row came.
    fn partial(&self) -> Option<u64> {
        None
    }

    /// The View Spec the source applies now and the totals, once, if a
    /// change in its data changed it (a column that left, say), for the
    /// Grid to hold. None unless overridden.
    fn view_changed(&mut self) -> Option<(ViewSpec, Totals)> {
        None
    }
}

/// What the user chose in the inline table: the picked rows' Keys if any
/// were picked, in the order they show (any picked rows the view leaves
/// out follow, in Key order), else the focused row's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub rows: Vec<Key>,
}

/// How the inline table ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    /// Enter, with a row to choose.
    Chosen(Selection),
    /// Esc or `q`, the end of the events or of the live feed, or Enter
    /// with no row to choose: nothing chosen.
    Left,
    /// Ctrl-C (taken as SIGINT), or SIGINT, SIGTERM or SIGHUP while the
    /// viewport was open: nothing chosen, the terminal restored.
    Interrupted { signal: i32 },
}

impl Exit {
    /// What the user chose, if anything.
    pub fn selection(self) -> Option<Selection> {
        match self {
            Exit::Chosen(selection) => Some(selection),
            _ => None,
        }
    }

    /// The status a CLI exits with after an interrupt, 128 and the
    /// signal's number (130 after Ctrl-C); `None` otherwise.
    pub fn status(&self) -> Option<i32> {
        match self {
            Exit::Interrupted { signal } => Some(128 + signal),
            _ => None,
        }
    }
}

/// `n` with a comma between each three digits.
pub(crate) fn thousands(n: u64) -> String {
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

/// The viewport's count for a partial table of which `read` rows came and
/// `shown` show: both numbers once a filter has dropped some.
pub(crate) fn partial_status(shown: u64, read: u64) -> String {
    match shown == read {
        true => format!("{} of more rows", thousands(read)),
        false => format!(
            "{} shown of {} read, more on the server",
            thousands(shown),
            thousands(read)
        ),
    }
}

/// Whether the CLI opens the table inline or prints it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Interactive {
    /// Inline when stdin and stdout are both terminals.
    #[default]
    Auto,
    /// `--interactive`.
    Always,
    /// `--no-interactive`.
    Never,
}

impl Interactive {
    /// Whether to open inline, given whether stdin and stdout are
    /// terminals.
    pub fn decide(self, stdin_terminal: bool, stdout_terminal: bool) -> bool {
        match self {
            Interactive::Auto => stdin_terminal && stdout_terminal,
            Interactive::Always => true,
            Interactive::Never => false,
        }
    }

    /// [`Interactive::decide`] for this process's stdin and stdout.
    pub fn now(self) -> bool {
        use std::io::IsTerminal;
        self.decide(
            std::io::stdin().is_terminal(),
            std::io::stdout().is_terminal(),
        )
    }
}

/// Applies `notices` from the source to `grid`, after the columns' support
/// and any View Spec the source applies now.
pub(crate) fn notify(
    grid: &mut Grid,
    source: &mut impl CliTable,
    notices: Vec<(Axis, ChangeNotice)>,
) {
    if notices.is_empty() {
        return;
    }
    source.declare(grid);
    if let Some((spec, totals)) = source.view_changed() {
        grid.view_changed(spec, totals);
    }
    for (axis, notice) in notices {
        grid.notify(axis, notice);
    }
}
