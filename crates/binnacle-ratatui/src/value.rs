//! The value renderer: Typed Values drawn through the shared formatter, so a
//! terminal shows the same text as every other host.

use std::time::Duration;

use binnacle_core::Key;
use binnacle_format::{
    Align, FixedOffset, FormatContext, FormatOptions, Formatted, Role, Style as Hints, Timestamp,
    Value, Zone, format, format_stable,
};
use ratatui::style::Style;

use crate::renderer::{char_count, fit};
use crate::{CellCanvas, CellRenderer, Theme};

/// What starts an invalid value, in the Theme's `warning` style.
pub const WARNING_MARK: char = '!';

/// How often a cell showing a relative time asks to be drawn again.
const TICK: Duration = Duration::from_secs(1);

/// A column's Typed Values from a closure, formatted with its unit, role and
/// options. A null draws in the Theme's `dim` style and an invalid value in
/// its `warning` style after [`WARNING_MARK`]. Relative times count from
/// the frame's time ([`CellCanvas::now`]) past the origin, and a cell
/// showing one asks for a frame a second later
/// ([`CellCanvas::request_frame_after`]), so they move on. Absolute times
/// draw in the zone, UTC unless set.
pub struct ValueRenderer<F> {
    value: F,
    unit: Option<String>,
    role: Role,
    options: FormatOptions,
    origin: Timestamp,
    zone: Box<dyn Zone>,
}

impl<F: Fn(Key, Key) -> Value> ValueRenderer<F> {
    /// `origin` is the UTC instant at the render context's time zero: a
    /// host whose `now` counts from its start passes the instant it
    /// started.
    pub fn new(origin: Timestamp, value: F) -> Self {
        ValueRenderer {
            value,
            unit: None,
            role: Role::None,
            options: FormatOptions::default(),
            origin,
            zone: Box::new(FixedOffset(0)),
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

    /// The local zone absolute times draw in.
    pub fn zone(self, zone: impl Zone + 'static) -> Self {
        ValueRenderer {
            zone: Box::new(zone),
            ..self
        }
    }

    /// The cell's stable text, for copying, search and filters: its dates
    /// absolute (see [`format_stable`]). A [`Content::cell`](crate::Content::cell)
    /// for a column this draws returns it.
    pub fn text(&self, row: Key, col: Key) -> String {
        let ctx = FormatContext::new(self.origin, &*self.zone);
        let value = (self.value)(row, col);
        format_stable(&value, self.unit.as_deref(), self.role, &self.options, &ctx).text
    }

    fn formatted(&self, value: &Value, now: Duration) -> Formatted {
        let ctx = FormatContext::new(self.origin.saturating_add(now), &*self.zone);
        let mut f = format(value, self.unit.as_deref(), self.role, &self.options, &ctx);
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
        let value = (self.value)(row, col);
        let f = self.formatted(&value, canvas.now());
        let text = fit(&f.text, canvas.width());
        let x = match f.align {
            Align::Start => 0,
            Align::End => canvas.width().saturating_sub(char_count(&text)),
        };
        let y = canvas.visible_y().start;
        let style = hint_style(canvas.theme(), f.style);
        canvas.print(x, y, &text, style);
        if matches!(value, Value::DateTime(_)) && self.options.relative_dates(self.role) {
            canvas.request_frame_after(TICK);
        }
    }

    fn preferred_width(&self, row: Key, col: Key) -> u32 {
        char_count(&self.formatted(&(self.value)(row, col), Duration::ZERO).text)
    }
}
