//! The value renderer: Typed Values drawn through the shared formatter, so a
//! terminal shows the same text as every other host.

use std::time::Duration;

use binnacle_core::Key;
use binnacle_format::{
    Align, FormatContext, FormatOptions, Formatted, Role, Style as Hints, Timestamp, Value, format,
};
use ratatui::style::Style;

use crate::renderer::{char_count, fit};
use crate::{CellCanvas, CellRenderer, Theme};

/// What starts an invalid value, in the Theme's `warning` style.
pub const WARNING_MARK: char = '!';

/// A column's Typed Values from a closure, formatted with its unit, role and
/// options. A null draws in the Theme's `dim` style and an invalid value in
/// its `warning` style after [`WARNING_MARK`]. Relative times count from
/// the frame's time ([`CellCanvas::now`]) past the clock's origin, so they
/// move on as frames are drawn.
pub struct ValueRenderer<F> {
    value: F,
    unit: Option<String>,
    role: Role,
    options: FormatOptions,
    origin: Timestamp,
    utc_offset: i32,
}

impl<F: Fn(Key, Key) -> Value> ValueRenderer<F> {
    pub fn new(value: F) -> Self {
        ValueRenderer {
            value,
            unit: None,
            role: Role::None,
            options: FormatOptions::default(),
            origin: Timestamp::UNIX_EPOCH,
            utc_offset: 0,
        }
    }

    /// The column's unit, a UCUM code.
    pub fn unit(self, unit: impl Into<String>) -> Self {
        ValueRenderer {
            unit: Some(unit.into()),
            ..self
        }
    }

    pub fn role(self, role: Role) -> Self {
        ValueRenderer { role, ..self }
    }

    pub fn options(self, options: FormatOptions) -> Self {
        ValueRenderer { options, ..self }
    }

    /// The UTC instant at the render context's time zero, and the local
    /// zone in seconds east of UTC. A host whose `now` counts from its
    /// start passes the instant it started.
    pub fn clock(self, origin: Timestamp, utc_offset: i32) -> Self {
        ValueRenderer {
            origin,
            utc_offset,
            ..self
        }
    }

    fn formatted(&self, row: Key, col: Key, now: Duration) -> Formatted {
        let ctx = FormatContext {
            now: self.origin.saturating_add(now),
            utc_offset: self.utc_offset,
        };
        let value = (self.value)(row, col);
        let mut f = format(&value, self.unit.as_deref(), self.role, &self.options, &ctx);
        if f.style.contains(Hints::WARNING) {
            f.text.insert(0, WARNING_MARK);
        }
        f
    }
}

/// The Theme's styles for `hints`, patched over each other.
pub fn hint_style(theme: &Theme, hints: Hints) -> Style {
    [
        (Hints::DIM, theme.dim),
        (Hints::PENDING, theme.loading),
        (Hints::WARNING, theme.warning),
    ]
    .into_iter()
    .filter(|&(flag, _)| hints.contains(flag))
    .fold(Style::default(), |style, (_, patch)| style.patch(patch))
}

impl<F: Fn(Key, Key) -> Value> CellRenderer for ValueRenderer<F> {
    fn draw(&self, row: Key, col: Key, canvas: &mut CellCanvas) {
        let f = self.formatted(row, col, canvas.now());
        let text = fit(&f.text, canvas.width());
        let x = match f.align {
            Align::Start => 0,
            Align::End => canvas.width().saturating_sub(char_count(&text)),
        };
        let y = canvas.visible_y().start;
        let style = hint_style(canvas.theme(), f.style);
        canvas.print(x, y, &text, style);
    }

    fn preferred_width(&self, row: Key, col: Key) -> u32 {
        char_count(&self.formatted(row, col, Duration::ZERO).text)
    }
}
