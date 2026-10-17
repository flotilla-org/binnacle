//! Effects as plain tagged objects for `serde-wasm-bindgen`: Keys as `u64`
//! (a `bigint` in JS), Positions and generations as `f64`, axes and
//! directions as strings. `web/src/effects.ts` holds the matching union.

use binnacle_core::{Axis, Effect, Key, SortDirection, SortSpec, Stamped};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireAxis {
    Rows,
    Cols,
}

impl From<Axis> for WireAxis {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::Rows => WireAxis::Rows,
            Axis::Cols => WireAxis::Cols,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireDirection {
    Ascending,
    Descending,
}

impl From<SortDirection> for WireDirection {
    fn from(direction: SortDirection) -> Self {
        match direction {
            SortDirection::Ascending => WireDirection::Ascending,
            SortDirection::Descending => WireDirection::Descending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WireSortColumn {
    pub key: u64,
    pub direction: WireDirection,
}

pub fn sort_columns(spec: &SortSpec) -> Vec<WireSortColumn> {
    spec.columns()
        .iter()
        .map(|c| WireSortColumn {
            key: c.key.0,
            direction: c.direction.into(),
        })
        .collect()
}

/// One Effect with the generation it was stamped with.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WireStamped {
    pub generation: f64,
    #[serde(flatten)]
    pub effect: WireEffect,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum WireEffect {
    TextTyped {
        text: String,
    },
    Activated {
        row: u64,
        col: u64,
    },
    EditRequested {
        row: u64,
        col: u64,
    },
    EditCancelled {
        row: u64,
        col: u64,
    },
    FocusChanged {
        row: u64,
        col: u64,
    },
    SelectionChanged,
    SortRequested {
        spec: Vec<WireSortColumn>,
    },
    Locate {
        axis: WireAxis,
        key: u64,
    },
    SizeChanged {
        axis: WireAxis,
        key: u64,
        size: f32,
    },
    AutoSizeRequested {
        axis: WireAxis,
        key: u64,
    },
    BandSizeChanged {
        axis: WireAxis,
        thickness: f32,
    },
    BandAutoSizeRequested {
        axis: WireAxis,
    },
    MoveRequested {
        axis: WireAxis,
        keys: Vec<u64>,
        to: f64,
    },
}

impl From<Stamped> for WireStamped {
    fn from(stamped: Stamped) -> Self {
        WireStamped {
            generation: stamped.generation as f64,
            effect: stamped.effect.into(),
        }
    }
}

impl From<Effect> for WireEffect {
    fn from(effect: Effect) -> Self {
        let k = |key: Key| key.0;
        match effect {
            Effect::TextTyped { text } => WireEffect::TextTyped { text },
            Effect::Activated { row, col } => WireEffect::Activated {
                row: k(row),
                col: k(col),
            },
            Effect::EditRequested { row, col } => WireEffect::EditRequested {
                row: k(row),
                col: k(col),
            },
            Effect::EditCancelled { row, col } => WireEffect::EditCancelled {
                row: k(row),
                col: k(col),
            },
            Effect::FocusChanged { row, col } => WireEffect::FocusChanged {
                row: k(row),
                col: k(col),
            },
            Effect::SelectionChanged => WireEffect::SelectionChanged,
            Effect::SortRequested { spec } => WireEffect::SortRequested {
                spec: sort_columns(&spec),
            },
            Effect::Locate { axis, key } => WireEffect::Locate {
                axis: axis.into(),
                key: k(key),
            },
            Effect::SizeChanged { axis, key, size } => WireEffect::SizeChanged {
                axis: axis.into(),
                key: k(key),
                size,
            },
            Effect::AutoSizeRequested { axis, key } => WireEffect::AutoSizeRequested {
                axis: axis.into(),
                key: k(key),
            },
            Effect::BandSizeChanged { axis, thickness } => WireEffect::BandSizeChanged {
                axis: axis.into(),
                thickness,
            },
            Effect::BandAutoSizeRequested { axis } => {
                WireEffect::BandAutoSizeRequested { axis: axis.into() }
            }
            Effect::MoveRequested { axis, keys, to } => WireEffect::MoveRequested {
                axis: axis.into(),
                keys: keys.into_iter().map(k).collect(),
                to: to as f64,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use binnacle_core::SortColumn;
    use serde_json::json;

    use super::*;

    fn wire(effect: Effect) -> serde_json::Value {
        serde_json::to_value(WireStamped::from(Stamped {
            generation: 7,
            effect,
        }))
        .unwrap()
    }

    #[test]
    fn an_effect_is_tagged_with_its_type_and_generation() {
        assert_eq!(
            wire(Effect::FocusChanged {
                row: Key(3),
                col: Key(4)
            }),
            json!({ "generation": 7.0, "type": "FocusChanged", "row": 3, "col": 4 })
        );
        assert_eq!(
            wire(Effect::SelectionChanged),
            json!({ "generation": 7.0, "type": "SelectionChanged" })
        );
    }

    #[test]
    fn keys_keep_all_64_bits() {
        assert_eq!(
            wire(Effect::Locate {
                axis: Axis::Rows,
                key: Key(u64::MAX)
            }),
            json!({ "generation": 7.0, "type": "Locate", "axis": "rows", "key": u64::MAX })
        );
    }

    #[test]
    fn a_sort_request_lists_columns_with_their_directions() {
        let spec = SortSpec::new(vec![
            SortColumn::descending(Key(2)),
            SortColumn::ascending(Key(0)),
        ]);
        assert_eq!(
            wire(Effect::SortRequested { spec }),
            json!({ "generation": 7.0, "type": "SortRequested", "spec": [
                { "key": 2, "direction": "descending" },
                { "key": 0, "direction": "ascending" },
            ] })
        );
    }

    #[test]
    fn a_move_request_carries_keys_and_a_position() {
        assert_eq!(
            wire(Effect::MoveRequested {
                axis: Axis::Cols,
                keys: vec![Key(5), Key(9)],
                to: 2
            }),
            json!({ "generation": 7.0, "type": "MoveRequested", "axis": "cols", "keys": [5, 9], "to": 2.0 })
        );
    }

    #[test]
    fn sizes_and_band_effects_carry_their_numbers() {
        assert_eq!(
            wire(Effect::SizeChanged {
                axis: Axis::Cols,
                key: Key(1),
                size: 80.5
            }),
            json!({ "generation": 7.0, "type": "SizeChanged", "axis": "cols", "key": 1, "size": 80.5 })
        );
        assert_eq!(
            wire(Effect::BandSizeChanged {
                axis: Axis::Rows,
                thickness: 30.0
            }),
            json!({ "generation": 7.0, "type": "BandSizeChanged", "axis": "rows", "thickness": 30.0 })
        );
    }
}
