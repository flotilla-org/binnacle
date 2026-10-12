//! Turning crossterm mouse events into Grid pointer events, with the click
//! count the adapter computes itself (crossterm reports none).

use std::time::{Duration, Instant};

use binnacle_core::{Button, Modifiers, PointerEvent, PointerKind};
use binnacle_ratatui::Mouse;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

fn event(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers,
    }
}

const AREA: Rect = Rect {
    x: 2,
    y: 1,
    width: 30,
    height: 10,
};

#[test]
fn presses_moves_and_releases_become_pointer_events_relative_to_the_area() {
    let mut mouse = Mouse::default();
    let now = Instant::now();
    let none = KeyModifiers::NONE;
    let down = mouse.event(
        &event(
            MouseEventKind::Down(MouseButton::Left),
            7,
            4,
            KeyModifiers::SHIFT,
        ),
        AREA,
        now,
    );
    assert_eq!(
        down,
        Some(PointerEvent::down(5.0, 3.0).modifiers(Modifiers::SHIFT))
    );
    let drag = mouse.event(
        &event(MouseEventKind::Drag(MouseButton::Left), 1, 0, none),
        AREA,
        now,
    );
    assert_eq!(drag, Some(PointerEvent::moved(-1.0, -1.0)));
    let moved = mouse.event(&event(MouseEventKind::Moved, 9, 9, none), AREA, now);
    assert_eq!(moved.map(|e| e.kind), Some(PointerKind::Move));
    let up = mouse.event(
        &event(
            MouseEventKind::Up(MouseButton::Right),
            9,
            9,
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ),
        AREA,
        now,
    );
    assert_eq!(
        up,
        Some(
            PointerEvent::up(7.0, 8.0)
                .button(Button::Secondary)
                .modifiers(Modifiers::CTRL | Modifiers::ALT)
        )
    );
    let wheel = mouse.event(&event(MouseEventKind::ScrollDown, 9, 9, none), AREA, now);
    assert_eq!(wheel, None, "the wheel is `wheel`'s business");
}

#[test]
fn quick_presses_on_the_same_cell_with_the_same_button_count_up() {
    let mut mouse = Mouse::default();
    let t0 = Instant::now();
    let left = MouseEventKind::Down(MouseButton::Left);
    let mut press = |kind, column, at: Duration| {
        mouse
            .event(&event(kind, column, 3, KeyModifiers::NONE), AREA, t0 + at)
            .map(|e| e.clicks)
    };
    let ms = Duration::from_millis;
    assert_eq!(press(left, 5, ms(0)), Some(1));
    assert_eq!(press(left, 5, ms(200)), Some(2));
    assert_eq!(press(left, 5, ms(400)), Some(3));
    assert_eq!(press(left, 6, ms(500)), Some(1), "another cell");
    assert_eq!(
        press(MouseEventKind::Down(MouseButton::Right), 6, ms(600)),
        Some(1),
        "another button"
    );
    assert_eq!(press(left, 6, ms(700)), Some(1));
    assert_eq!(press(left, 6, ms(1200)), Some(1), "too slow");
}
