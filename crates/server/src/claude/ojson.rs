//! JSON that keeps its keys in the order they were written.
//!
//! `wado claude hooks install` edits the person's own `~/.claude/settings.json`. `serde_json`'s
//! map sorts keys (its `preserve_order` feature would change every map in the workspace), so
//! reading and writing through it would reorder their whole file and bury the two lines that
//! matter in a diff of noise. This is just enough of a value to read, edit and write it back in
//! place.

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

#[derive(Debug, Clone, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(serde_json::Number),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    pub fn obj() -> J {
        J::Obj(Vec::new())
    }

    pub fn str(s: &str) -> J {
        J::Str(s.to_string())
    }

    pub fn num(n: u64) -> J {
        J::Num(n.into())
    }

    pub fn get(&self, k: &str) -> Option<&J> {
        match self {
            J::Obj(m) => m.iter().find(|(key, _)| key == k).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, k: &str) -> Option<&mut J> {
        match self {
            J::Obj(m) => m.iter_mut().find(|(key, _)| key == k).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Set `k`, in its place if it is there, else at the end.
    pub fn set(&mut self, k: &str, v: J) {
        if let J::Obj(m) = self {
            match m.iter_mut().find(|(key, _)| key == k) {
                Some(slot) => slot.1 = v,
                None => m.push((k.to_string(), v)),
            }
        }
    }

    pub fn remove(&mut self, k: &str) {
        if let J::Obj(m) = self {
            m.retain(|(key, _)| key != k);
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            J::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_empty_container(&self) -> bool {
        matches!(self, J::Arr(a) if a.is_empty()) || matches!(self, J::Obj(o) if o.is_empty())
    }

    pub fn parse(text: &str) -> serde_json::Result<J> {
        serde_json::from_str(text)
    }

    /// Two-space indented, as Claude Code writes it, with a final newline.
    pub fn pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default() + "\n"
    }
}

impl Serialize for J {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            J::Null => s.serialize_unit(),
            J::Bool(b) => s.serialize_bool(*b),
            J::Num(n) => n.serialize(s),
            J::Str(v) => s.serialize_str(v),
            J::Arr(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            J::Obj(m) => {
                let mut map = s.serialize_map(Some(m.len()))?;
                for (k, v) in m {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

struct V;

impl<'de> Visitor<'de> for V {
    type Value = J;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_unit<E>(self) -> Result<J, E> {
        Ok(J::Null)
    }
    fn visit_none<E>(self) -> Result<J, E> {
        Ok(J::Null)
    }
    fn visit_bool<E>(self, v: bool) -> Result<J, E> {
        Ok(J::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<J, E> {
        Ok(J::Num(v.into()))
    }
    fn visit_u64<E>(self, v: u64) -> Result<J, E> {
        Ok(J::Num(v.into()))
    }
    fn visit_f64<E>(self, v: f64) -> Result<J, E> {
        Ok(serde_json::Number::from_f64(v).map_or(J::Null, J::Num))
    }
    fn visit_str<E>(self, v: &str) -> Result<J, E> {
        Ok(J::Str(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<J, E> {
        Ok(J::Str(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<J, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element()? {
            out.push(v);
        }
        Ok(J::Arr(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<J, A::Error> {
        let mut out = Vec::new();
        while let Some((k, v)) = map.next_entry::<String, J>()? {
            out.push((k, v));
        }
        Ok(J::Obj(out))
    }
}

impl<'de> Deserialize<'de> for J {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<J, D::Error> {
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_come_back_in_the_order_they_went_in() {
        let text = "{\n  \"zeta\": 1,\n  \"alpha\": {\n    \"y\": [\n      true,\n      null,\n      1.5\n    ],\n    \"b\": \"x\"\n  }\n}\n";
        let j = J::parse(text).unwrap();
        assert_eq!(j.pretty(), text);
        let mut j = j;
        j.set("alpha", J::str("now"));
        j.set("new", J::num(2));
        assert_eq!(
            j.pretty(),
            "{\n  \"zeta\": 1,\n  \"alpha\": \"now\",\n  \"new\": 2\n}\n"
        );
    }
}
