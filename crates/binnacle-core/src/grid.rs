use std::collections::VecDeque;
use std::ops::Range;

use crate::axis::AxisState;
use crate::effect::{Action, Effect, Stamped};
use crate::snapshot::{Damage, Snapshot};
use crate::types::{Axis, Generation, Key, Position, Viewport};

/// How one axis starts out: its count, default item size and header band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisConfig {
    pub count: u64,
    pub default_size: f32,
    pub header: f32,
}

impl AxisConfig {
    /// `default_size` is in the host's unit and must be positive and finite. Sizes
    /// below the `f32` spacing at the viewport's far edge cannot be laid out
    /// reliably: the Strip ends where offsets stop advancing, short of the edge.
    pub fn new(count: u64, default_size: f32) -> Self {
        AxisConfig {
            count,
            default_size,
            header: 0.0,
        }
    }

    /// The thickness of the header band leading this axis (see [`Snapshot::header`]).
    pub fn header(self, thickness: f32) -> Self {
        AxisConfig {
            header: thickness,
            ..self
        }
    }
}

/// The Windows [`Grid::begin_frame`] asks the host to fill, at most one per axis.
/// An axis whose current Window already covers what is needed is absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowRequests([Option<(Axis, Range<Position>)>; 2]);

impl WindowRequests {
    pub fn get(&self, axis: Axis) -> Option<Range<Position>> {
        self.0[axis.index()].as_ref().map(|(_, r)| r.clone())
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }
}

impl IntoIterator for WindowRequests {
    type Item = (Axis, Range<Position>);
    type IntoIter = std::iter::Flatten<std::array::IntoIter<Option<(Axis, Range<Position>)>, 2>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().flatten()
    }
}

/// The view controller for one table.
pub struct Grid {
    axes: [AxisState; 2],
    pub(crate) generation: Generation,
    effects: VecDeque<Stamped>,
    pending_damage: Damage,
    pub(crate) damage: Damage,
}

impl Grid {
    pub fn new(rows: AxisConfig, cols: AxisConfig) -> Self {
        let axis = |c: AxisConfig| {
            assert!(
                c.default_size.is_finite() && c.default_size > 0.0,
                "default size must be positive and finite, got {}",
                c.default_size
            );
            AxisState::new(c.count, c.default_size, c.header.max(0.0))
        };
        Grid {
            axes: [axis(rows), axis(cols)],
            generation: 0,
            effects: VecDeque::new(),
            pending_damage: Damage::ALL,
            damage: Damage::NONE,
        }
    }

    pub(crate) fn axis(&self, axis: Axis) -> &AxisState {
        &self.axes[axis.index()]
    }

    fn axis_mut(&mut self, axis: Axis) -> &mut AxisState {
        &mut self.axes[axis.index()]
    }

    /// Starts a frame: takes the viewport, advances the generation and returns the
    /// Windows the host must fill with [`Grid::set_window`] before taking the
    /// Snapshot.
    pub fn begin_frame(&mut self, viewport: Viewport) -> WindowRequests {
        self.generation += 1;
        let mut requests = WindowRequests::default();
        for axis in Axis::ALL {
            let extent = viewport.extent(axis);
            let st = self.axis_mut(axis);
            if st.extent != extent {
                st.extent = extent;
                self.pending_damage.layout = true;
            }
            let st = self.axis(axis);
            let needed = st.needed();
            if !st.window_covers(&needed) {
                requests.0[axis.index()] = Some((axis, needed));
            }
        }
        requests
    }

    /// Fills the Window on `axis` with the Keys for Positions `start..`. Keys
    /// past the count are ignored; Positions left unfilled draw as loading.
    pub fn set_window(&mut self, axis: Axis, start: Position, keys: &[Key]) {
        if self.axis_mut(axis).set_window(start, keys) {
            self.pending_damage.positions = true;
        }
    }

    pub fn action(&mut self, action: Action) {
        match action {
            Action::TypeText(text) => self.push(Effect::TextTyped { text }),
        }
    }

    fn push(&mut self, effect: Effect) {
        self.effects.push_back(Stamped {
            generation: self.generation,
            effect,
        });
    }

    /// Takes the oldest queued Effect, if any.
    pub fn poll_effect(&mut self) -> Option<Stamped> {
        self.effects.pop_front()
    }

    /// Lays out the frame and returns the Snapshot, whose damage covers every
    /// change since the previous Snapshot.
    pub fn snapshot(&mut self) -> Snapshot<'_> {
        for st in &mut self.axes {
            st.layout();
        }
        self.damage = std::mem::replace(&mut self.pending_damage, Damage::NONE);
        Snapshot { grid: self }
    }
}
