//! Phone, tablet or desktop — one rule for both clients, so a layout or a hint meant for one
//! form never lands on another.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    #[default]
    Phone,
    Tablet,
    Desktop,
}

impl Form {
    /// From the logical screen size and whether the pointer is a finger. A short side under
    /// 600 is a phone; a larger touch screen a tablet; anything with a mouse a desktop.
    pub fn of(w: f64, h: f64, touch: bool) -> Self {
        if w.min(h) < 600.0 {
            Self::Phone
        } else if touch {
            Self::Tablet
        } else {
            Self::Desktop
        }
    }
    pub fn touch(self) -> bool {
        self != Self::Desktop
    }
}

#[cfg(test)]
mod tests {
    use super::Form;
    #[test]
    fn forms() {
        assert_eq!(Form::of(390.0, 844.0, true), Form::Phone);
        assert_eq!(Form::of(1180.0, 820.0, true), Form::Tablet);
        assert_eq!(Form::of(1440.0, 900.0, false), Form::Desktop);
        assert_eq!(
            Form::of(500.0, 900.0, false),
            Form::Phone,
            "a narrow window is laid out as a phone"
        );
    }
}
