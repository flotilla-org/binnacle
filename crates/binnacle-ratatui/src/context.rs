//! The render context: everything [`render_with`](crate::render_with) takes
//! beside the Snapshot and the Content.

use std::cell::Cell;
use std::time::Duration;

use crate::{BorderStyle, ColorSupport, PendingMarks, Theme};

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
    /// When toggled groups' disclosure marks went pending; without it none
    /// draws pending.
    pub marks: Option<&'a PendingMarks>,
    /// Where cells ask for a later frame (see
    /// [`CellCanvas::request_frame_after`](crate::CellCanvas::request_frame_after)).
    pub next_frame: Option<&'a NextFrame>,
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
            marks: None,
            next_frame: None,
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

    pub fn marks(self, marks: &'a PendingMarks) -> Self {
        RenderContext {
            marks: Some(marks),
            ..self
        }
    }

    pub fn next_frame(self, next_frame: &'a NextFrame) -> Self {
        RenderContext {
            next_frame: Some(next_frame),
            ..self
        }
    }
}

/// The soonest frame time any cell asked for while drawing, as
/// [`CellCanvas::request_frame_after`](crate::CellCanvas::request_frame_after)
/// gives it. A host that redraws only on input or while
/// [`animating`](crate::animating) passes one in the render context and
/// also redraws once its [`NextFrame::take`] time comes, so a relative time
/// moves on.
#[derive(Debug, Default)]
pub struct NextFrame(Cell<Option<Duration>>);

impl NextFrame {
    pub fn new() -> Self {
        NextFrame::default()
    }

    /// The soonest frame time asked for since the last take, cleared.
    pub fn take(&self) -> Option<Duration> {
        self.0.take()
    }

    pub(crate) fn request(&self, at: Duration) {
        let due = self.0.get().map_or(at, |due| due.min(at));
        self.0.set(Some(due));
    }
}
