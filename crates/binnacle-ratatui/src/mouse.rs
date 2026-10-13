use std::time::{Duration, Instant};

use binnacle_core::{Button, Modifiers, PointerEvent};
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

/// Turns crossterm mouse events into Grid pointer events relative to the
/// Grid's area. crossterm reports no click count, so this counts presses of
/// the same button on the same cell within [`Mouse::interval`] of each other.
/// The wheel is left to [`crate::wheel`].
#[derive(Clone, Debug)]
pub struct Mouse {
    /// The longest pause between presses that still counts as one multi-click.
    pub interval: Duration,
    last: Option<(Instant, MouseButton, u16, u16, u32)>,
}

impl Default for Mouse {
    fn default() -> Self {
        Mouse {
            interval: Duration::from_millis(400),
            last: None,
        }
    }
}

impl Mouse {
    /// The pointer event for `event` with the Grid drawn at `area`, at time
    /// `now`, if it is a press, a release or a move.
    pub fn event(&mut self, event: &MouseEvent, area: Rect, now: Instant) -> Option<PointerEvent> {
        let x = f32::from(event.column) - f32::from(area.x);
        let y = f32::from(event.row) - f32::from(area.y);
        let base = match event.kind {
            MouseEventKind::Down(b) => {
                let clicks = match self.last {
                    Some((t, lb, col, row, n))
                        if lb == b
                            && (col, row) == (event.column, event.row)
                            && now.saturating_duration_since(t) < self.interval =>
                    {
                        n.saturating_add(1)
                    }
                    _ => 1,
                };
                self.last = Some((now, b, event.column, event.row, clicks));
                PointerEvent::down(x, y).button(button(b)).clicks(clicks)
            }
            MouseEventKind::Up(b) => PointerEvent::up(x, y).button(button(b)),
            MouseEventKind::Drag(_) | MouseEventKind::Moved => PointerEvent::moved(x, y),
            _ => return None,
        };
        Some(base.modifiers(modifiers(event.modifiers)))
    }
}

fn button(b: MouseButton) -> Button {
    match b {
        MouseButton::Left => Button::Primary,
        MouseButton::Right => Button::Secondary,
        MouseButton::Middle => Button::Middle,
    }
}

fn modifiers(m: KeyModifiers) -> Modifiers {
    Modifiers {
        shift: m.contains(KeyModifiers::SHIFT),
        ctrl: m.contains(KeyModifiers::CONTROL),
        alt: m.contains(KeyModifiers::ALT),
        meta: m.intersects(KeyModifiers::SUPER | KeyModifiers::META),
    }
}
