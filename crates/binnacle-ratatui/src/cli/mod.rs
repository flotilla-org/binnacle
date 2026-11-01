//! A CLI host for any Table Source ([`CliTable`]), described tables first:
//! print a table as aligned text, TSV, JSON or JSON lines ([`print`]), take
//! the usual flags ([`Flags`]), or open the full widget inline below the
//! prompt, or full screen ([`Presentation`]), and hand back what the user
//! picked ([`run_inline`]). Each applies a [`binnacle_layout::Layout`]
//! (`binnacle_ratatui::Layout` is the scrollbar layout) through a Grid, so
//! a table prints as it shows.
//!
//! Everything but the viewport lives in `binnacle-cli`, which has no
//! ratatui in its tree, and is re-exported here.

mod inline;

pub use binnacle_cli::*;
pub use inline::{InlineOptions, Live, Presentation, inline_height, run_inline, run_inline_with};
