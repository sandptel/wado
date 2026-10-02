//! Later wins: how a repeated section or an `include`d file combines with what came before.

use serde_json::Value;

/// Objects merge key by key, recursively; a list node (`schema::LIST_NODES`) appends, so an
/// include adds autostart entries rather than replacing them; anything else is replaced.
pub fn deep(into: &mut Value, from: Value) {
    match (into, from) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                match a.get_mut(&k) {
                    Some(Value::Array(old)) if crate::schema::LIST_NODES.contains(&k.as_str()) => {
                        if let Value::Array(new) = v {
                            old.extend(new);
                        }
                    }
                    Some(slot) => deep(slot, v),
                    None => {
                        a.insert(k, v);
                    }
                }
            }
        }
        (slot, v) => *slot = v,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn later_wins_per_key() {
        let mut a = json!({"s": {"x": 1, "y": 2}});
        super::deep(&mut a, json!({"s": {"y": 3}}));
        assert_eq!(a, json!({"s": {"x": 1, "y": 3}}));
    }
}
