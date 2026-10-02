//! Deserialize helpers for shapes KDL produces naturally.

use serde::{Deserialize, Deserializer};

/// `url "a"` and `url "a" "b"` both mean a list — KDL has no one-element-array syntax.
pub fn one_or_many<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(s) => vec![s],
        OneOrMany::Many(v) => v,
    })
}
