use crate::types::Generation;

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
}
