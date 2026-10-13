//! The Shimmer: a bright band sweeping across a pending state, after
//! flotilla-tui's shimmer module.

use std::f32::consts::PI;
use std::time::Duration;

use ratatui::style::{Color, Modifier, Style};

use crate::theme::nearest_ansi;
use crate::{ColorSupport, RenderContext};

/// How long the band takes to sweep across once.
const PERIOD: Duration = Duration::from_secs(2);

/// Half the band's width, in cells.
const HALF_WIDTH: f32 = 5.0;

/// A band sweeping from left to right across `width` cells once every
/// [`PERIOD`], starting and ending off either edge so it enters and leaves
/// smoothly. Its colour blends from the Theme's `shimmer_base` to its
/// `shimmer_highlight` at the band's centre. Without full RGB colour (or
/// when either Shimmer colour is not RGB) the band steps through dim,
/// normal and bold instead, in the highlight colour.
#[derive(Clone, Copy, Debug)]
pub struct Shimmer {
    /// The band's centre, in cells from `HALF_WIDTH` before the first cell.
    centre: f32,
    look: Look,
}

#[derive(Clone, Copy, Debug)]
enum Look {
    Blend {
        base: (u8, u8, u8),
        highlight: (u8, u8, u8),
    },
    Steps {
        fg: Option<Color>,
    },
}

impl Shimmer {
    /// The Shimmer across `width` cells at `ctx.now`, in `ctx.theme`'s
    /// colours as `ctx.colors` can show them.
    pub fn new(ctx: &RenderContext, width: u16) -> Shimmer {
        let travel = f32::from(width) + 2.0 * HALF_WIDTH;
        let phase = (ctx.now.as_secs_f64() % PERIOD.as_secs_f64()) / PERIOD.as_secs_f64();
        let (base, highlight) = (ctx.theme.shimmer_base, ctx.theme.shimmer_highlight);
        let look = match (ctx.colors, base, highlight) {
            (ColorSupport::TrueColor, Color::Rgb(r0, g0, b0), Color::Rgb(r1, g1, b1)) => {
                Look::Blend {
                    base: (r0, g0, b0),
                    highlight: (r1, g1, b1),
                }
            }
            (colors, _, highlight) => {
                let highlight = match colors {
                    ColorSupport::TrueColor => highlight,
                    ColorSupport::Ansi16 => nearest_ansi(highlight),
                };
                Look::Steps {
                    fg: (highlight != Color::Reset).then_some(highlight),
                }
            }
        };
        Shimmer {
            centre: phase as f32 * travel,
            look,
        }
    }

    /// The style to patch over cell `x` (from the left of the sweep): only
    /// its foreground and the dim and bold modifiers change.
    pub fn style(&self, x: u16) -> Style {
        let distance = (f32::from(x) + HALF_WIDTH - self.centre).abs();
        let t = if distance < HALF_WIDTH {
            0.5 * (1.0 + (PI * distance / HALF_WIDTH).cos())
        } else {
            0.0
        };
        match self.look {
            Look::Blend { base, highlight } => {
                let mix =
                    |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
                let fg = Color::Rgb(
                    mix(base.0, highlight.0),
                    mix(base.1, highlight.1),
                    mix(base.2, highlight.2),
                );
                Style::default().fg(fg)
            }
            Look::Steps { fg } => {
                let style = Style {
                    fg,
                    ..Style::default()
                };
                let both = Modifier::DIM | Modifier::BOLD;
                if t < 0.2 {
                    style
                        .add_modifier(Modifier::DIM)
                        .remove_modifier(Modifier::BOLD)
                } else if t < 0.6 {
                    style.remove_modifier(both)
                } else {
                    style
                        .add_modifier(Modifier::BOLD)
                        .remove_modifier(Modifier::DIM)
                }
            }
        }
    }
}
