//! The render context: everything [`render_with`](crate::render_with) takes
//! beside the Snapshot and the Content.

use std::time::Duration;

use crate::{ColorSupport, Theme};

/// How [`render_with`](crate::render_with) draws: the Theme, the Border
/// Style, the current time and the terminal's colour capability.
#[derive(Clone, Debug)]
pub struct RenderContext<'a> {
    pub theme: &'a Theme,
    pub border: BorderStyle,
    /// The time of this frame, from any fixed origin the host picks.
    /// Animation reads it, so a frame drawn at the same time is the same.
    pub now: Duration,
    pub colors: ColorSupport,
}

impl<'a> RenderContext<'a> {
    /// `theme` with header-only lines, at time zero, in full RGB colour.
    pub fn new(theme: &'a Theme) -> Self {
        RenderContext {
            theme,
            border: BorderStyle::default(),
            now: Duration::ZERO,
            colors: ColorSupport::TrueColor,
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
}

/// Which lines are drawn in the Gaps. Today there is one: handles in the
/// header band's Gaps only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BorderStyle {}

impl BorderStyle {
    /// Lines in the header band only: the handles a resize grabs.
    pub fn headers() -> Self {
        BorderStyle {}
    }
}
