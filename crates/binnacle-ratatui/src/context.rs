//! The render context: everything [`render_with`](crate::render_with) takes
//! beside the Snapshot and the Content.

use std::time::Duration;

use crate::{BorderStyle, ColorSupport, Theme};

/// How [`render_with`](crate::render_with) draws: the Theme, the Border
/// Style, the current time, the terminal's colour capability, whether to draw
/// scrollbars and the message for a table with nothing to show.
#[derive(Clone, Debug)]
pub struct RenderContext<'a> {
    pub theme: &'a Theme,
    pub border: BorderStyle,
    /// The time of this frame, from any fixed origin the host picks.
    /// Animation reads it, so a frame drawn at the same time is the same.
    pub now: Duration,
    pub colors: ColorSupport,
    /// Whether to draw scrollbars (see [`layout`](crate::layout)).
    pub scrollbars: bool,
    /// Centred in the body when an axis has no items: "No rows" unless set.
    pub empty: &'a str,
}

impl<'a> RenderContext<'a> {
    /// `theme` with header-only lines and scrollbars, at time zero, in full
    /// RGB colour.
    pub fn new(theme: &'a Theme) -> Self {
        RenderContext {
            theme,
            border: BorderStyle::default(),
            now: Duration::ZERO,
            colors: ColorSupport::TrueColor,
            scrollbars: true,
            empty: "No rows",
        }
    }

    pub fn border(self, border: BorderStyle) -> Self {
        RenderContext { border, ..self }
    }

    pub fn at(self, now: Duration) -> Self {
        RenderContext { now, ..self }
    }

    pub fn colors(self, colors: ColorSupport) -> Self {
        RenderContext { colors, ..self }
    }

    pub fn scrollbars(self, scrollbars: bool) -> Self {
        RenderContext { scrollbars, ..self }
    }

    pub fn empty(self, empty: &'a str) -> Self {
        RenderContext { empty, ..self }
    }
}
