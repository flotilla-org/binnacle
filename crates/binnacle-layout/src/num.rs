//! Numbers as `JSON.stringify` writes them, so a document Rust writes reads
//! the same as one the web writes: a whole number without a fraction (`150`,
//! not `150.0`) and `-0` as `0`. A binary format such as CBOR keeps the
//! float as it is. Either reads an integer or a float.

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserializer, Serializer};

/// The largest whole number a double holds exactly, past which JavaScript
/// writes an exponent.
const EXACT: f64 = 9_007_199_254_740_992.0;

pub fn serialize<S: Serializer>(n: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    let n = *n;
    if serializer.is_human_readable() && n.fract() == 0.0 && n.abs() < EXACT {
        serializer.serialize_i64(n as i64)
    } else {
        serializer.serialize_f64(n)
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    deserializer.deserialize_any(Number)
}

struct Number;

impl Visitor<'_> for Number {
    type Value = f64;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number")
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
        Ok(v)
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
        Ok(v as f64)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
        Ok(v as f64)
    }
}

/// `[id, number]` pairs, the numbers as above.
pub mod pairs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize)]
    struct Out<'a, Id>(&'a Id, #[serde(with = "super")] f64);

    #[derive(Deserialize)]
    struct In<Id>(Id, #[serde(with = "super")] f64);

    pub fn serialize<Id: Serialize, S: Serializer>(
        pairs: &[(Id, f64)],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(pairs.iter().map(|(id, n)| Out(id, *n)))
    }

    pub fn deserialize<'de, Id: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<(Id, f64)>, D::Error> {
        let pairs = Vec::<In<Id>>::deserialize(deserializer)?;
        Ok(pairs.into_iter().map(|In(id, n)| (id, n)).collect())
    }
}
