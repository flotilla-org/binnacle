//! A Rust host embeds a `BinnacleGrid`: it builds the handle from a core
//! Grid, pumps it with `binnacle_memory::pump`, and passes on the Effects
//! the pump did not answer to the native layer, which polls them through C.

use binnacle_core::{
    Axis, AxisConfig, ChangeNotice, Effect, Grid, Key, Located, PointerEvent, ViewSpec, Viewport,
};
use binnacle_ffi::*;
use binnacle_memory::{PumpSource, Viewed, pump};

/// Applies every View Spec it is asked for; locates nothing.
struct Applying;

impl PumpSource for Applying {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        Viewed {
            applied: spec.clone(),
            notices: vec![(Axis::Rows, ChangeNotice::Reordered)],
            ..Viewed::default()
        }
    }

    fn locate(&mut self, _axis: Axis, _key: Key) -> Located {
        Located::Gone { suggest: None }
    }
}

fn action(kind: u32) -> BinnacleAction {
    BinnacleAction {
        kind,
        ..BinnacleAction::default()
    }
}

fn poll(grid: *mut BinnacleGrid) -> Option<BinnacleEffect> {
    let mut out = BinnacleEffect::default();
    // SAFETY: `grid` is live and `out` is writable.
    (unsafe { binnacle_grid_poll_effect(grid, &mut out) } == 1).then_some(out)
}

#[test]
fn passed_on_effects_are_polled_before_newer_ones_in_order() {
    let mut grid = Grid::new(
        AxisConfig::new(3, 1.0).header(1.0),
        AxisConfig::new(2, 4.0).header(2.0),
    );
    for (axis, range) in grid.begin_frame(Viewport::new(10.0, 4.0)) {
        let keys: Vec<Key> = range.clone().map(Key).collect();
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot();
    let handle = Box::into_raw(BinnacleGrid::from_grid(grid));
    // SAFETY: `handle` is live until freed below, and each borrow ends
    // before the next C call.
    let host = unsafe { &mut *handle };

    // Focusing a cell and activating it are for the host, not the pump.
    host.grid_mut().pointer(PointerEvent::down(3.0, 2.5));
    host.grid_mut().pointer(PointerEvent::up(3.0, 2.5));
    // SAFETY: `handle` is live and `host` is not used across the call.
    assert_eq!(
        unsafe { binnacle_grid_action(handle, &action(BINNACLE_ACTION_ACTIVATE)) },
        BINNACLE_ACTION_DONE
    );
    // SAFETY: as above.
    let host = unsafe { &mut *handle };
    host.pump_with(|grid, pass_on| {
        pump(grid, &mut Applying, |_, stamped| {
            if !matches!(
                stamped.effect,
                Effect::ViewRequested { .. } | Effect::Locate { .. }
            ) {
                pass_on(stamped);
            }
        })
    });

    // A newer Effect, queued after the pump ran.
    let text = "x";
    let mut typed = action(BINNACLE_ACTION_TYPE_TEXT);
    typed.args.type_text = BinnacleText {
        data: text.as_ptr(),
        len: text.len(),
    };
    // SAFETY: `handle` is live; `text` outlives the call.
    unsafe { binnacle_grid_action(handle, &typed) };

    let polled: Vec<(u32, u64, u64)> = std::iter::from_fn(|| poll(handle))
        .map(|e| (e.kind, e.row, e.col))
        .collect();
    assert_eq!(
        polled,
        vec![
            (BINNACLE_EFFECT_FOCUS_CHANGED, 1, 0),
            (BINNACLE_EFFECT_ACTIVATED, 1, 0),
            (BINNACLE_EFFECT_TEXT_TYPED, 0, 0),
        ]
    );
    // SAFETY: `handle` came from `from_grid` and is not used again.
    unsafe { binnacle_grid_free(handle) };
}
