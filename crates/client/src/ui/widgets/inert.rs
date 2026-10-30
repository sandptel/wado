//! The `inert` attribute, written so that `false` really means "not inert".
//!
//! Dioxus 0.7 removes a `false` attribute only for the boolean attributes it knows, and `inert` is
//! not among them (`dioxus-interpreter-js`, `isBoolAttr`): `inert: false` is written as
//! `inert="false"`, and since the attribute's *presence* is what counts, the element is inert —
//! untappable and unfocusable — exactly when it should be live. `None` removes the attribute.
//!
//! ```ignore
//! section { inert: inert(covered), … }
//! ```

/// `Some("")` (inert) when `on`, else `None` (the attribute is removed).
pub fn inert(on: bool) -> Option<&'static str> {
    on.then_some("")
}

#[cfg(test)]
mod tests {
    use super::inert;

    #[test]
    fn false_removes_the_attribute() {
        assert_eq!(inert(false), None);
        assert_eq!(inert(true), Some(""));
    }
}
