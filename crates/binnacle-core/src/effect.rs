use crate::types::{Axis, Generation};

/// A request the Grid emits for its host or Table Source to act on. Effects
/// carry no continuations: answers come back later as ordinary inputs.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// The user typed text while the Grid had focus.
    TextTyped { text: String },
}

/// An Effect stamped with the generation of the Snapshot it was based on.
#[derive(Clone, Debug, PartialEq)]
pub struct Stamped {
    pub generation: Generation,
    pub effect: Effect,
}

/// A semantic input the host sends on the user's behalf, mapped from native keys
/// by the host's keymap.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    TypeText(String),
    /// Scrolls by a distance in the host's unit along each axis (a wheel or
    /// trackpad): `dx` across the columns, `dy` down the rows.
    Scroll {
        dx: f32,
        dy: f32,
    },
    /// Scrolls `axis` so its thumb sits at `fraction` of its travel (a
    /// scrollbar drag): 0 is the start, 1 the end.
    ScrollTo {
        axis: Axis,
        fraction: f64,
    },
    /// Scrolls `axis` by whole items, landing on an item start.
    ScrollBy {
        axis: Axis,
        items: i64,
    },
}
